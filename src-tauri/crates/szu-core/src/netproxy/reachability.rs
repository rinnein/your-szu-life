//! 本机此刻能否**直接**到达校园网（不含 easytier overlay）。
//!
//! 「校内优化」的决策依据不能只是区域身份：开着 SecureLink 一类外部 VPN 时
//! 人在校外，校内地址与校内 DNS 却是通的。这里给上层一个同步可读、带 60 秒
//! 缓存的可达性：区域判定为校内直接算可达，否则用 `alwaysDirect` 里的校园网
//! 门户地址做一次 TCP 探测。
//!
//! 探测只用系统解析器，绝不问校内 DNS —— 「要不要问校内 DNS」由可达性决定，
//! 可达性再依赖校内 DNS 就成环了。

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::model;

/// 探测结果的有效期。
const PROBE_TTL: Duration = Duration::from_secs(60);
/// 单个探测目标的连接超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Probe {
    reachable: bool,
    at: Instant,
}

static PROBE: Mutex<Option<Probe>> = Mutex::new(None);
static INFLIGHT: AtomicBool = AtomicBool::new(false);

/// 测试注入的可达性；生产构建里不存在。它优先于区域判定与探测缓存，
/// 这样依赖 crate 的测试不会被并行的校园网用例改写的全局区域标志带偏。
#[cfg(any(test, feature = "test-support"))]
static TEST_OVERRIDE: Mutex<Option<bool>> = Mutex::new(None);

/// 本机此刻能否直接到达校园网。
///
/// 区域判定为校内时直接返回 true（校内没登录门户也仍然有校内路由）；否则读
/// 探测缓存，过期就异步补一次并沿用上一次结果。从未探测过按「不可达」处理：
/// 公网出口是安全默认，探测结果几秒内就会补上。
pub fn campus_reachable() -> bool {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(value) = *TEST_OVERRIDE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
    {
        return value;
    }
    // 区域判定为校内时不必再探测，省掉一次无意义的连接。
    if super::dns::is_in_campus_network() {
        return true;
    }
    cached_probe().unwrap_or(false)
}

/// 读探测缓存；过期时异步补一次，并沿用上一次结果。
fn cached_probe() -> Option<bool> {
    let (cached, stale) = {
        let guard = PROBE.lock().unwrap_or_else(|error| error.into_inner());
        match guard.as_ref() {
            Some(probe) => (Some(probe.reachable), probe.at.elapsed() >= PROBE_TTL),
            None => (None, true),
        }
    };
    if stale {
        refresh();
    }
    cached
}

/// 缓存过期时异步补一次探测。启动与校园网状态发布时调用，让缓存保持新鲜。
pub fn refresh() {
    if fresh() || INFLIGHT.swap(true, Ordering::AcqRel) {
        return;
    }
    #[cfg(any(test, feature = "test-support"))]
    {
        // 测试里一律由 `set_reachable` 注入，不发真实探测。
        let _ = probe_campus_portals;
        INFLIGHT.store(false, Ordering::Release);
        return;
    }
    #[cfg(not(any(test, feature = "test-support")))]
    {
        tauri::async_runtime::spawn(async move {
            let reachable = probe_campus_portals().await;
            if let Ok(mut guard) = PROBE.lock() {
                *guard = Some(Probe {
                    reachable,
                    at: Instant::now(),
                });
            }
            INFLIGHT.store(false, Ordering::Release);
        });
    }
}

fn fresh() -> bool {
    PROBE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .is_some_and(|probe| probe.at.elapsed() < PROBE_TTL)
}

/// 依次尝试 `alwaysDirect` 里的校园网门户；任一能建立 TCP 连接即可达。
///
/// 域名条目只接受解析到内网/回环地址的结果：门户域名万一在公网也能解析并
/// 响应，连上它并不能证明本机够得着校园网。
async fn probe_campus_portals() -> bool {
    for (host, port) in model::campus_probe_targets() {
        let addrs = match host.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => match super::dns::resolve_system(host).await {
                Ok(addrs) => addrs,
                Err(_) => continue,
            },
        };
        for addr in addrs.into_iter().filter(|addr| is_sentinel_address(*addr)) {
            let connect = tokio::net::TcpStream::connect((addr, *port));
            if tokio::time::timeout(PROBE_TIMEOUT, connect)
                .await
                .is_ok_and(|result| result.is_ok())
            {
                return true;
            }
        }
    }
    false
}

/// 只有内网地址才能证明「本机路由进了校园网」。
fn is_sentinel_address(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(addr) => addr.is_private() || addr.is_loopback(),
        IpAddr::V6(addr) => addr.is_unique_local() || addr.is_loopback(),
    }
}

/// 仅供测试注入可达性（`None` 恢复「区域判定 + 探测」）。依赖 crate 的测试经
/// `szu-core/test-support` feature 使用它，生产构建里不存在。
#[cfg(any(test, feature = "test-support"))]
pub fn set_reachable(reachable: Option<bool>) {
    let mut guard = TEST_OVERRIDE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *guard = reachable;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injected_reachability_wins_over_the_region_flag() {
        let _guard = super::super::dns::TEST_ENV_LOCK.lock().unwrap();
        crate::netproxy::set_in_campus_network(true);
        set_reachable(Some(false));
        assert!(!campus_reachable());
        set_reachable(Some(true));
        crate::netproxy::set_in_campus_network(false);
        assert!(campus_reachable());
        set_reachable(None);
        assert!(!campus_reachable());
    }

    #[test]
    fn only_internal_addresses_count_as_campus_evidence() {
        for addr in ["172.30.255.42", "192.168.1.1", "10.0.0.1", "127.0.0.1", "::1", "fd00::1"] {
            assert!(
                is_sentinel_address(addr.parse().unwrap()),
                "{addr} 是内网地址，连上它就是校内路由的证据"
            );
        }
        for addr in ["210.39.4.1", "2001:250::1", "8.8.8.8"] {
            assert!(
                !is_sentinel_address(addr.parse().unwrap()),
                "{addr} 公网可达，不能证明本机够得着校园网"
            );
        }
    }

    #[test]
    fn probe_targets_come_from_the_always_direct_portals() {
        let targets = model::campus_probe_targets();
        assert!(
            targets
                .iter()
                .any(|(host, port)| host == "172.30.255.42" && *port == 80),
            "{targets:?}"
        );
        assert!(
            targets
                .iter()
                .any(|(host, port)| host == "net.szu.edu.cn" && *port == 443),
            "{targets:?}"
        );
    }
}
