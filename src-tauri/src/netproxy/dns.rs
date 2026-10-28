//! 校内域名的 DNS 覆盖。
//!
//! 部分校内域名只有校内 DNS 才能解析出正确的（校内）地址，因此对
//! `campus-network.json` 里 `hosts` 指定的域名先问校内 DNS；问不到
//! （不在校园网、没有路由、超时）就退回系统解析器，行为与覆盖前一致。
//!
//! 这里只发 A/AAAA 查询并解析应答，够用即可 —— 为它引入一个完整的 DNS
//! 客户端库并不划算，reqwest 侧也只需要一个 [`Resolve`] 实现。

use crate::netproxy::model;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::Mutex,
    time::{Duration, Instant},
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const DNS_PORT: u16 = 53;
/// 校内 DNS 没有回应时的单次等待上限。测试里调短，免得为了等超时拖慢用例。
#[cfg(not(test))]
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const QUERY_TIMEOUT: Duration = Duration::from_millis(200);
/// 校内 DNS 认定不可用后，这段时间内不再重复等待它。
const DOWN_TTL: Duration = Duration::from_secs(60);
const QTYPE_A: u16 = 1;
const QTYPE_AAAA: u16 = 28;

/// 校内 DNS 是否「刚刚不可用」。它是整条链路的状态，不按域名区分，
/// 否则离线时每个校内域名都要先白等一次超时。
static CAMPUS_DNS_DOWN: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) struct CampusResolver;

impl Resolve for CampusResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            if model::is_campus_domain(&host) {
                if let Ok(addrs) = campus_lookup(&host).await {
                    if !addrs.is_empty() {
                        return Ok(Box::new(addrs.into_iter().map(socket_addr)) as Addrs);
                    }
                }
            }
            system_lookup(&host).await
        })
    }
}

/// 系统解析器（getaddrinfo）。与覆盖前的行为一致。
async fn system_lookup(host: &str) -> Result<Addrs, BoxError> {
    let host = host.to_owned();
    let addrs = tokio::task::spawn_blocking(move || {
        (host.as_str(), 0)
            .to_socket_addrs()
            .map(|addrs| addrs.collect::<Vec<_>>())
    })
    .await
    .map_err(|error| -> BoxError { Box::new(error) })??;
    Ok(Box::new(addrs.into_iter()))
}

/// 问校内 DNS。调用方必须先确认这是校内域名。
async fn campus_lookup(host: &str) -> Result<Vec<IpAddr>, String> {
    let servers = model::campus_dns_servers();
    if servers.is_empty() {
        return Err("未配置校内 DNS".into());
    }
    if campus_dns_is_down() {
        return Err("校内 DNS 刚被判定不可用".into());
    }
    let host = host.to_owned();
    let servers = servers.to_vec();
    let result = tokio::task::spawn_blocking(move || query_all(&servers, &host))
        .await
        .map_err(|_| "校内 DNS 查询任务异常结束".to_owned())?;
    match result {
        Ok(addrs) => {
            mark_campus_dns_up();
            Ok(addrs)
        }
        Err(error) => {
            mark_campus_dns_down();
            Err(error)
        }
    }
}

fn campus_dns_is_down() -> bool {
    let mut state = CAMPUS_DNS_DOWN
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(at) = state.as_ref() {
        if at.elapsed() < DOWN_TTL {
            return true;
        }
    }
    *state = None;
    false
}

/// 校内 DNS 当前是否可用（据最近的观测）。
///
/// 还没观测过时按「可用」处理：在校内网的用户第一次请求就能用上校内 DNS，
/// 不必先等一次探测；一旦失败会记住 60 秒，期间不再让它拖慢解析。
pub(crate) fn campus_dns_available() -> bool {
    !model::campus_dns_servers().is_empty() && !campus_dns_is_down()
}

fn mark_campus_dns_down() {
    *CAMPUS_DNS_DOWN
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(Instant::now());
}

fn mark_campus_dns_up() {
    *CAMPUS_DNS_DOWN
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
}

/// 按配置顺序问每台 DNS；全都拿不到地址才算失败。
fn query_all(servers: &[IpAddr], host: &str) -> Result<Vec<IpAddr>, String> {
    let mut last = String::from("校内 DNS 无应答");
    for server in servers {
        match query_dual(SocketAddr::new(*server, DNS_PORT), host) {
            Ok(addrs) => return Ok(addrs),
            Err(error) => last = error,
        }
    }
    Err(last)
}

/// A 与 AAAA 并发查询：只要有一族能解析出地址就算成功，另一族失败不影响。
fn query_dual(server: SocketAddr, host: &str) -> Result<Vec<IpAddr>, String> {
    let (a, aaaa) = std::thread::scope(|scope| {
        let a = scope.spawn(|| query(server, host, QTYPE_A));
        let aaaa = scope.spawn(|| query(server, host, QTYPE_AAAA));
        (
            a.join().unwrap_or_else(|_| Err("A 查询线程异常结束".into())),
            aaaa.join()
                .unwrap_or_else(|_| Err("AAAA 查询线程异常结束".into())),
        )
    });
    let mut addrs = Vec::new();
    let mut error = None;
    for result in [a, aaaa] {
        match result {
            Ok(mut found) => addrs.append(&mut found),
            Err(reason) => error = error.or(Some(reason)),
        }
    }
    if addrs.is_empty() {
        return Err(error.unwrap_or_else(|| "校内 DNS 未返回任何地址".into()));
    }
    Ok(addrs)
}

/// 一次 UDP 查询。截断（TC）或应答异常都算失败，交给系统解析器兜底。
fn query(server: SocketAddr, host: &str, qtype: u16) -> Result<Vec<IpAddr>, String> {
    let socket = UdpSocket::bind(unspecified_for(server))
        .map_err(|_| format!("无法为校内 DNS 建立本地 socket（{server}）"))?;
    socket
        .set_read_timeout(Some(QUERY_TIMEOUT))
        .map_err(|error| error.to_string())?;
    let id = query_id();
    let request = build_query(id, host, qtype)?;
    socket
        .send_to(&request, server)
        .map_err(|_| format!("校内 DNS {server} 不可达"))?;

    // 收到别人的包就继续等，直到超时。
    let mut buffer = [0u8; 1024];
    loop {
        let (len, from) = socket
            .recv_from(&mut buffer)
            .map_err(|_| format!("校内 DNS {server} 无应答"))?;
        if from != server {
            continue;
        }
        return parse_response(&buffer[..len], id);
    }
}

fn unspecified_for(server: SocketAddr) -> SocketAddr {
    match server {
        SocketAddr::V4(_) => SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::V6(_) => SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
    }
}

/// 端口留 0：连接层会按协议补全。
fn socket_addr(addr: IpAddr) -> SocketAddr {
    SocketAddr::new(addr, 0)
}

/// 用系统时间做查询 ID：这里不需要密码学强度，只需要不撞上过期应答。
fn query_id() -> u16 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| (elapsed.subsec_nanos() & 0xFFFF) as u16)
        .unwrap_or(0x1F1F)
}

fn build_query(id: u16, host: &str, qtype: u16) -> Result<Vec<u8>, String> {
    let mut packet = Vec::with_capacity(20 + host.len());
    packet.extend_from_slice(&id.to_be_bytes());
    // RD=1：期望递归查询；其余标志清零。
    packet.extend_from_slice(&0x0100u16.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // ANCOUNT/NSCOUNT/ARCOUNT
    packet.extend_from_slice(&encode_name(host)?);
    packet.extend_from_slice(&qtype.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes()); // QCLASS=IN
    Ok(packet)
}

fn encode_name(host: &str) -> Result<Vec<u8>, String> {
    let host = host.trim_end_matches('.');
    if host.is_empty() {
        return Err("域名不能为空".into());
    }
    let mut encoded = Vec::with_capacity(host.len() + 1);
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(format!("域名标签非法：{host}"));
        }
        encoded.push(label.len() as u8);
        encoded.extend_from_slice(label.as_bytes());
    }
    encoded.push(0); // 根标签
    Ok(encoded)
}

/// 解析应答，取出 A/AAAA 地址。CNAME 只是跳过 —— 其目标记录通常也在同一应答里。
fn parse_response(packet: &[u8], id: u16) -> Result<Vec<IpAddr>, String> {
    if packet.len() < 12 {
        return Err("DNS 应答过短".into());
    }
    if u16::from_be_bytes([packet[0], packet[1]]) != id {
        return Err("DNS 应答 ID 不匹配".into());
    }
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    if flags & 0x8000 == 0 {
        return Err("收到的不是 DNS 应答".into());
    }
    if flags & 0x0200 != 0 {
        // 截断应答不可信，交给系统解析器。
        return Err("DNS 应答被截断".into());
    }
    if flags & 0x000F != 0 {
        // NXDOMAIN 等也是「DNS 可达」的正常结果，只是没有地址。
        return Ok(Vec::new());
    }
    let questions = read_u16(packet, 4)? as usize;
    let answers = read_u16(packet, 6)? as usize;

    let mut cursor = 12;
    for _ in 0..questions {
        cursor = skip_name(packet, cursor)?;
        cursor = cursor.checked_add(4).ok_or("DNS 应答越界")?;
        if cursor > packet.len() {
            return Err("DNS 应答越界".into());
        }
    }

    let mut found = Vec::new();
    for _ in 0..answers {
        cursor = skip_name(packet, cursor)?;
        // 记录头：TYPE(2) CLASS(2) TTL(4) RDLENGTH(2)，紧跟名字之后。
        let kind = read_u16(packet, cursor)?;
        let class = read_u16(packet, cursor + 2)?;
        let length = read_u16(packet, cursor + 8)? as usize;
        let start = cursor.checked_add(10).ok_or("DNS 应答越界")?;
        let end = start.checked_add(length).ok_or("DNS 应答越界")?;
        if end > packet.len() {
            return Err("DNS 应答越界".into());
        }
        if class == 1 {
            // A/AAAA 之外的记录（含 CNAME）忽略。
            match (kind, length) {
                (QTYPE_A, 4) => {
                    let mut octets = [0u8; 4];
                    octets.copy_from_slice(&packet[start..end]);
                    found.push(IpAddr::V4(Ipv4Addr::from(octets)));
                }
                (QTYPE_AAAA, 16) => {
                    let mut octets = [0u8; 16];
                    octets.copy_from_slice(&packet[start..end]);
                    found.push(IpAddr::V6(Ipv6Addr::from(octets)));
                }
                _ => {}
            }
        }
        cursor = end;
    }
    Ok(found)
}

fn read_u16(packet: &[u8], at: usize) -> Result<u16, String> {
    let bytes = packet.get(at..at + 2).ok_or("DNS 应答越界")?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

/// 跳过域名：普通标签按长度前进，压缩指针固定 2 字节。
fn skip_name(packet: &[u8], mut at: usize) -> Result<usize, String> {
    loop {
        let Some(&length) = packet.get(at) else {
            return Err("DNS 应答越界".into());
        };
        if length & 0xC0 == 0xC0 {
            // 指针：本字段到此为止，被指向的名字不在这里展开。
            let end = at.checked_add(2).ok_or("DNS 应答越界")?;
            if end > packet.len() {
                return Err("DNS 应答越界".into());
            }
            return Ok(end);
        }
        if length == 0 {
            return Ok(at + 1);
        }
        at = at
            .checked_add(1 + length as usize)
            .ok_or("DNS 应答越界")?;
        if at > packet.len() {
            return Err("DNS 应答越界".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "authserver.szu.edu.cn";

    /// 手工构造应答：问题段 + 若干 A/AAAA 记录，记录名用压缩指针指回问题段。
    fn response(id: u16, qtype: u16, records: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.extend_from_slice(&id.to_be_bytes());
        packet.extend_from_slice(&0x8180u16.to_be_bytes()); // QR + RD + RA + NOERROR
        packet.extend_from_slice(&1u16.to_be_bytes()); // QDCOUNT
        packet.extend_from_slice(&(records.len() as u16).to_be_bytes());
        packet.extend_from_slice(&[0, 0, 0, 0]); // NSCOUNT/ARCOUNT
        packet.extend_from_slice(&encode_name(NAME).unwrap());
        packet.extend_from_slice(&qtype.to_be_bytes());
        packet.extend_from_slice(&1u16.to_be_bytes());
        for (kind, rdata) in records {
            packet.extend_from_slice(&[0xC0, 0x0C]); // 指回问题段的名字
            packet.extend_from_slice(&kind.to_be_bytes());
            packet.extend_from_slice(&1u16.to_be_bytes()); // CLASS=IN
            packet.extend_from_slice(&60u32.to_be_bytes());
            packet.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            packet.extend_from_slice(rdata);
        }
        packet
    }

    fn v6_bytes() -> Vec<u8> {
        "2001:250::1".parse::<Ipv6Addr>().unwrap().octets().to_vec()
    }

    #[test]
    fn parses_a_and_aaaa_answers() {
        let packet = response(
            7,
            QTYPE_A,
            &[
                (QTYPE_A, vec![172, 21, 1, 9]),
                (QTYPE_AAAA, v6_bytes()),
                // CNAME 之类的记录必须被忽略
                (5, vec![0xC0, 0x0C]),
            ],
        );
        let addrs = parse_response(&packet, 7).unwrap();
        assert_eq!(addrs.len(), 2);
        assert!(addrs.contains(&IpAddr::V4(Ipv4Addr::new(172, 21, 1, 9))));
        assert!(addrs.contains(&"2001:250::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn rejects_mismatched_or_malformed_answers() {
        let packet = response(7, QTYPE_A, &[(QTYPE_A, vec![10, 0, 0, 1])]);
        assert!(parse_response(&packet, 8).is_err(), "ID 不匹配要报错");
        assert!(parse_response(&packet[..8], 7).is_err(), "过短的包要报错");

        let mut truncated = packet.clone();
        truncated[2] |= 0x02; // TC=1
        assert!(parse_response(&truncated, 7).is_err());

        let mut request = packet.clone();
        request[2] &= 0x7F; // 清掉 QR，变成请求
        assert!(parse_response(&request, 7).is_err());
    }

    #[test]
    fn nxdomain_means_reachable_but_empty() {
        let mut packet = response(9, QTYPE_A, &[]);
        packet[3] = 3; // RCODE=NXDOMAIN
        assert!(parse_response(&packet, 9).unwrap().is_empty());
    }

    #[test]
    fn builds_a_well_formed_query() {
        let packet = build_query(0x1234, "szu.edu.cn", QTYPE_A).unwrap();
        assert_eq!(&packet[..2], &[0x12, 0x34]);
        assert_eq!(u16::from_be_bytes([packet[4], packet[5]]), 1);
        assert_eq!(&packet[12..], b"\x03szu\x03edu\x02cn\x00\x00\x01\x00\x01");
        assert!(build_query(1, "", QTYPE_A).is_err());
        assert!(build_query(1, "a..b", QTYPE_A).is_err());
    }

    #[test]
    fn queries_a_local_responder_end_to_end() {
        // 用本地 UDP socket 充当即 DNS，避免用例碰真实网络。
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let responder = std::thread::spawn(move || {
            // 两个家族各来一次，顺序不定，按请求里的 QTYPE 回。
            for _ in 0..2 {
                let mut buffer = [0u8; 512];
                let (len, from) = server.recv_from(&mut buffer).unwrap();
                let id = u16::from_be_bytes([buffer[0], buffer[1]]);
                let qtype = u16::from_be_bytes([buffer[len - 4], buffer[len - 3]]);
                let record = if qtype == QTYPE_A {
                    (QTYPE_A, vec![172, 21, 1, 9])
                } else {
                    (QTYPE_AAAA, v6_bytes())
                };
                server.send_to(&response(id, qtype, &[record]), from).unwrap();
            }
        });

        let addrs = query_dual(address, NAME).unwrap();
        responder.join().unwrap();
        assert!(addrs.contains(&IpAddr::V4(Ipv4Addr::new(172, 21, 1, 9))));
        assert!(addrs.contains(&"2001:250::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn unanswered_query_fails_instead_of_hanging() {
        // 拿一个刚释放的端口：没人应答（或立刻回 ICMP 不可达），必须报错。
        let port = {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.local_addr().unwrap()
        };
        let error = query_dual(port, NAME).unwrap_err();
        assert!(!error.is_empty());
    }

    #[test]
    fn query_all_reports_every_server_failure() {
        let server = SocketAddr::from(([127, 0, 0, 1], 1));
        let error = query_all(&[server.ip()], NAME).unwrap_err();
        assert!(!error.is_empty());
    }

    #[test]
    fn shipped_config_points_campus_domains_at_the_campus_dns() {
        // 校内域名要走校内 DNS；公网域名不受影响。
        assert!(model::is_campus_domain(NAME));
        assert!(!model::is_campus_domain("github.com"));
        assert!(
            !model::campus_dns_servers().is_empty(),
            "campus-network.json 应配置校内 DNS"
        );
    }

    #[tokio::test]
    async fn non_campus_names_still_go_through_the_system_resolver() {
        // localhost 由 hosts 文件解析，不需要网络；它同时验证了
        // 「非校内域名必须原样交给系统解析器」这条兜底路径。
        let addrs = CampusResolver
            .resolve("localhost".parse::<Name>().unwrap())
            .await
            .unwrap()
            .collect::<Vec<_>>();
        assert!(
            addrs.iter().any(|addr| addr.ip().is_loopback()),
            "localhost 应解析到回环地址，实际：{addrs:?}"
        );
    }
}