//! 前端资源（图片等）的统一取回模块。
//!
//! WebView 直接加载第三方图片可能被 CORS、Referer 防盗链或证书策略拦截，
//! 因此所有需要"绕过前端跨域限制"的资源都必须走这里：
//!
//! - 网络出口只使用 [`crate::netproxy`] 提供的客户端构建器（代理、校园 DNS、overlay）；
//! - 重定向手动跟随并逐跳重新校验，最多 5 跳；
//! - 不携带 Cookie、Referer，避免把用户的登录态发给第三方图床；
//! - 限制响应大小，由调用方决定后续的图片格式校验与缓存策略。
//!
//! 两种访问策略：
//! - [`ResourceAccess::Trusted`]：公告、更新日志等可信内容源，允许私有地址与内网域名；
//! - [`ResourceAccess::Public`]：任意第三方图片，只允许公网 http(s) 地址，
//!   拒绝回环、内网、保留域名与带认证信息的 URL。

use reqwest::blocking::Client;
use reqwest::Url;
use std::collections::HashMap;
use std::io::Read;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Mutex, OnceLock};

const MAX_URL_LENGTH: usize = 4096;
const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceAccess {
    Trusted,
    Public,
}

#[derive(Clone, Copy, Debug)]
pub struct ResourceRequest<'a> {
    pub url: &'a str,
    pub access: ResourceAccess,
    pub accept: &'a str,
    pub limit: usize,
    /// 调用方传入的 User-Agent（通常带应用版本），避免在此处读取编译期版本。
    pub user_agent: &'a str,
}

#[derive(Clone, Debug)]
pub struct ResourceResponse {
    pub bytes: Vec<u8>,
    pub content_type: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceErrorKind {
    Invalid,
    Denied,
    TooLarge,
    Network,
    Http,
    Unavailable,
}

#[derive(Clone, Debug)]
pub struct ResourceError {
    pub kind: ResourceErrorKind,
    pub message: String,
}

impl ResourceError {
    fn new(kind: ResourceErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::Invalid, message)
    }

    pub fn denied(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::Denied, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::Network, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::TooLarge, message)
    }

    pub fn http(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::Http, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ResourceErrorKind::Unavailable, message)
    }
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ResourceError {}

/// 校验并规范化一个前端资源 URL。
pub fn validate_url(value: &str, access: ResourceAccess) -> Result<Url, ResourceError> {
    if value.len() > MAX_URL_LENGTH
        || value.chars().any(char::is_control)
        || value.trim() != value
    {
        return Err(ResourceError::invalid("资源地址格式无效"));
    }
    let url = Url::parse(value).map_err(|_| ResourceError::invalid("资源地址格式无效"))?;
    let authority = value
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || authority.is_some_and(|part| part.contains('@'))
    {
        return Err(ResourceError::invalid(
            "仅支持不含用户名和密码的 HTTP / HTTPS 地址",
        ));
    }
    if access == ResourceAccess::Public
        && !is_public_host(url.host_str().unwrap_or_default())
    {
        return Err(ResourceError::denied(
            "只允许加载公网 HTTP / HTTPS 图片地址",
        ));
    }
    Ok(url)
}

/// 公网图片校验：拒绝回环、内网、链路本地、保留地址与常见内网域名。
///
/// 注意：这里只检查 URL 字面量，不做 DNS 解析；自定义域名解析到内网 IP 的情况
/// 无法在这里拦截。调用方不应把该模块用于不受信任的服务器端抓取场景。
pub fn is_public_host(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    let host = host.trim_end_matches('.').trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return false;
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        return is_public_ipv4(ip);
    }
    if let Ok(ip) = host.parse::<Ipv6Addr>() {
        return is_public_ipv6(ip);
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return false;
    }
    if [
        ".local",
        ".localdomain",
        ".lan",
        ".home",
        ".internal",
        ".intranet",
        ".corp",
        ".private",
        ".arpa",
        ".invalid",
        ".test",
        ".example",
        ".onion",
    ]
    .iter()
    .any(|suffix| host.ends_with(suffix))
    {
        return false;
    }
    if matches!(host, "example.com" | "example.net" | "example.org") {
        return false;
    }
    // 单标签主机名基本只可能是本地/内网主机。
    host.contains('.')
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    let segments = ip.segments();
    !(ip.is_loopback()
        || ip.is_unspecified()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] & 0xff00) == 0xff00
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
        || segments[0] == 0x2002
        || (segments[0] == 0x0064 && segments[1] == 0xff9b))
}

fn client(user_agent: &str) -> Result<Client, ResourceError> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, Client>>> = OnceLock::new();
    let clients = CLIENTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = clients.lock() {
        if let Some(client) = map.get(user_agent) {
            return Ok(client.clone());
        }
    }
    let client = build_client(user_agent)?;
    if let Ok(mut map) = clients.lock() {
        map.insert(user_agent.to_string(), client.clone());
    }
    Ok(client)
}

fn build_client(user_agent: &str) -> Result<Client, ResourceError> {
    #[cfg(target_os = "android")]
    crate::platform_tls::initialize_platform_verifier().map_err(ResourceError::unavailable)?;
    crate::netproxy::blocking_client_builder()
        .user_agent(user_agent)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .cookie_store(false)
        .referer(false)
        // 重定向必须逐跳重新校验，不能交给 reqwest 自动跟随。
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|error| ResourceError::unavailable(format!("无法初始化资源代理网络客户端: {error}")))
}

/// 同步取回一个前端资源。调用方通常运行在 `spawn_blocking` 中。
pub fn fetch_blocking(request: ResourceRequest<'_>) -> Result<ResourceResponse, ResourceError> {
    let client = client(request.user_agent)?;
    let mut target = validate_url(request.url, request.access)?;
    let mut hops = 0usize;

    loop {
        let response = client
            .get(target.clone())
            .header(reqwest::header::ACCEPT, request.accept)
            .send()
            .map_err(|_| ResourceError::network("请求失败或超时，请检查网络"))?;
        let status = response.status();

        if matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
            if hops >= MAX_REDIRECTS {
                return Err(ResourceError::http("资源重定向次数过多"));
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| ResourceError::invalid("资源重定向缺少地址"))?;
            let next = target
                .join(location)
                .map_err(|_| ResourceError::invalid("资源重定向地址无效"))?;
            target = validate_url(next.as_str(), request.access)?;
            hops += 1;
            continue;
        }

        if !status.is_success() {
            return Err(ResourceError::http(format!(
                "服务器返回 HTTP {}",
                status.as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|size| size > request.limit as u64)
        {
            return Err(ResourceError::too_large("资源超过大小限制"));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        response
            .take(request.limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ResourceError::network("读取资源响应失败或超时"))?;
        if bytes.len() > request.limit {
            return Err(ResourceError::too_large("资源超过大小限制"));
        }
        if bytes.is_empty() {
            return Err(ResourceError::invalid("资源响应为空"));
        }
        return Ok(ResourceResponse {
            bytes,
            content_type,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trusted_access_allows_private_hosts() {
        for url in [
            "http://127.0.0.1/a.png",
            "http://172.30.1.2/a.png",
            "http://intranet/a.png",
            "http://pic.local/a.png",
        ] {
            assert!(validate_url(url, ResourceAccess::Trusted).is_ok(), "{url}");
        }
    }

    #[test]
    fn public_access_rejects_private_and_reserved_hosts() {
        for url in [
            "http://127.0.0.1/a.png",
            "http://10.0.0.1/a.png",
            "http://192.168.1.2/a.png",
            "http://169.254.1.1/a.png",
            "http://localhost/a.png",
            "http://pic.local/a.png",
            "http://intranet/a.png",
            "http://example.com/a.png",
            "http://[::1]/a.png",
            "http://[fd00::1]/a.png",
            "http://[::ffff:127.0.0.1]/a.png",
        ] {
            assert!(
                validate_url(url, ResourceAccess::Public).is_err(),
                "should reject {url}"
            );
        }
    }

    #[test]
    fn public_access_allows_public_hosts() {
        for url in [
            "https://www.uooc.net.cn/a.png",
            "http://assets.uooconline.com/a.png",
            "https://avatars.githubusercontent.com/u/1",
            "https://8.8.8.8/a.png",
        ] {
            assert!(validate_url(url, ResourceAccess::Public).is_ok(), "{url}");
        }
    }

    #[test]
    fn unsafe_schemes_and_credentials_are_rejected() {
        for url in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "https://user:pass@a.com/a.png",
            "https://a.com@b.com/a.png",
            "/relative.png",
            "//a.com/a.png",
            " https://a.com/a.png",
        ] {
            assert!(validate_url(url, ResourceAccess::Public).is_err(), "{url}");
        }
    }
}
