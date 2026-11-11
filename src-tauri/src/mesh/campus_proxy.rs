//! 校园网出口代理：只在开启「校园网访问」的在校设备上运行。
//!
//! 它是应用层的最小 HTTP 代理（`CONNECT` + absolute-form），因为 EasyTier 在
//! `no_tun` 模式下无法把子网路由给客户端，而项目又不使用 VpnService / TUN。
//! 代理只接受校内目标（`campus-network.json` 的白名单），不会变成通用出口。
//!
//! 监听端口在虚拟网上，只有同一账号的设备能连到；代理本身不带认证。

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

use super::node::Node;
use super::CAMPUS_PROXY_PORT;
use crate::netproxy::model;

const MAX_HEADER: usize = 16 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) async fn serve(node: Arc<Node>) -> Result<JoinHandle<()>, String> {
    let mut listener = node.bind_overlay(CAMPUS_PROXY_PORT).await?;
    Ok(tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    tokio::spawn(async move {
                        if let Err(error) = handle(stream).await {
                            log::debug!("校园网出口请求失败（{peer}）: {error}");
                        }
                    });
                }
                Err(error) => {
                    log::warn!("校园网出口监听结束: {error}");
                    break;
                }
            }
        }
    }))
}

async fn handle(mut stream: easytier_core::gateway::DataPlaneTcpStream) -> Result<(), String> {
    if !super::campus_access_enabled() {
        return respond(&mut stream, 403, "校园网访问未开启").await;
    }
    let (head, rest) = read_head(&mut stream).await?;
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_owned();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_owned();
    let target = parts.next().unwrap_or_default().to_owned();
    let version = parts.next().unwrap_or("HTTP/1.1").to_owned();

    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = split_authority(&target) else {
            return respond(&mut stream, 400, "无效的目标地址").await;
        };
        if !model::is_campus_target(&host) {
            return respond(&mut stream, 403, "只允许访问校内资源").await;
        }
        let upstream = connect(&host, port).await?;
        stream
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await
            .map_err(|error| error.to_string())?;
        let mut upstream = upstream;
        let _ = copy_bidirectional(&mut stream, &mut upstream).await;
        return Ok(());
    }

    let url = reqwest::Url::parse(&target).map_err(|_| "代理只接受绝对 URL".to_string())?;
    let host = url.host_str().ok_or_else(|| "目标缺少主机名".to_string())?;
    if !model::is_campus_target(host) {
        return respond(&mut stream, 403, "只允许访问校内资源").await;
    }
    let port = url.port_or_known_default().unwrap_or(80);
    let mut upstream = connect(host, port).await?;

    let mut request = format!("{method} {} {version}\r\n", request_target(&url)).into_bytes();
    let mut content_length = 0usize;
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("proxy-connection:") || lower.starts_with("proxy-authorization:") {
            continue;
        }
        if let Some(value) = lower.strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
        request.extend_from_slice(line.as_bytes());
        request.extend_from_slice(b"\r\n");
    }
    request.extend_from_slice(b"\r\n");
    upstream
        .write_all(&request)
        .await
        .map_err(|error| error.to_string())?;
    if !rest.is_empty() {
        upstream
            .write_all(&rest)
            .await
            .map_err(|error| error.to_string())?;
    }
    if content_length > rest.len() {
        let mut remainder = vec![0u8; content_length - rest.len()];
        stream
            .read_exact(&mut remainder)
            .await
            .map_err(|error| error.to_string())?;
        upstream
            .write_all(&remainder)
            .await
            .map_err(|error| error.to_string())?;
    }
    upstream.flush().await.map_err(|error| error.to_string())?;
    let _ = copy_bidirectional(&mut stream, &mut upstream).await;
    Ok(())
}

async fn read_head(
    stream: &mut easytier_core::gateway::DataPlaneTcpStream,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(position) = find_header_end(&buf) {
            let rest = buf.split_off(position);
            return Ok((buf, rest));
        }
        if buf.len() > MAX_HEADER {
            return Err("请求头过大".into());
        }
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("连接提前关闭".into());
        }
        buf.extend_from_slice(&chunk[..read]);
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn split_authority(target: &str) -> Option<(String, u16)> {
    let (host, port) = target.rsplit_once(':')?;
    let port = port.parse().ok()?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return None;
    }
    Some((host.to_owned(), port))
}

fn request_target(url: &reqwest::Url) -> String {
    let mut path = url.path().to_owned();
    if path.is_empty() {
        path.push('/');
    }
    match url.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    }
}

async fn connect(host: &str, port: u16) -> Result<TcpStream, String> {
    let addrs = crate::netproxy::resolve_overlay_target(host).await?;
    let mut last_error = None;
    for addr in addrs {
        match tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect(SocketAddr::new(addr, port)),
        )
        .await
        {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(error)) => last_error = Some(error.to_string()),
            Err(_) => last_error = Some("连接超时".into()),
        }
    }
    Err(last_error.unwrap_or_else(|| "无法解析目标地址".into()))
}

async fn respond(
    stream: &mut easytier_core::gateway::DataPlaneTcpStream,
    status: u16,
    message: &str,
) -> Result<(), String> {
    let body = message.as_bytes();
    let response = format!(
        "HTTP/1.1 {status} {message}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    let _ = stream.write_all(body).await;
    let _ = stream.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_end_is_found_after_the_terminator() {
        assert_eq!(find_header_end(b"GET / HTTP/1.1\r\nA: b\r\n\r\nrest"), Some(24));
        assert!(find_header_end(b"GET / HTTP/1.1\r\n").is_none());
    }

    #[test]
    fn authority_parsing_handles_ports_and_ipv6_brackets() {
        assert_eq!(
            split_authority("net.szu.edu.cn:443"),
            Some(("net.szu.edu.cn".to_owned(), 443))
        );
        assert_eq!(
            split_authority("[2001:da8::1]:80"),
            Some(("2001:da8::1".to_owned(), 80))
        );
        assert!(split_authority("no-port").is_none());
    }

    #[test]
    fn request_target_keeps_the_query() {
        assert_eq!(
            request_target(&reqwest::Url::parse("http://a.example/x?y=1").unwrap()),
            "/x?y=1"
        );
        assert_eq!(
            request_target(&reqwest::Url::parse("http://a.example").unwrap()),
            "/"
        );
    }
}
