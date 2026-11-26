//! 校外时的「校园网出口」策略。
//!
//! 优先级由需求决定：
//!
//! 1. 本机够得着校园网（区域判定在校内，或外部 VPN 把校园网路由过来）：完全
//!    维持原逻辑，校内优化照常生效；
//! 2. 校外且没有可用的 overlay 出口：也维持原逻辑（否则会连最后一条直连也丢掉）；
//! 3. 校外且够不着校园网：`campus-network.json` 的直连白名单不再强制生效，先按
//!    公网出口（override 的 fallback，如 jxxk → c46.szu.edu.cn）访问；只有探测到
//!    「公网出口也连不上」的校内目标才改走 overlay 代理。
//!
//! 探测按 `host:port` 缓存 60 秒，避免每个请求都吃一次连接超时。命中「不可达」后
//! 后续请求直接走在校设备；外网恢复后会在一分钟内回到公网出口。

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqwest::Url;

use super::model;

const PROBE_TTL: Duration = Duration::from_secs(60);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
struct Probe {
    reachable: bool,
    at: Instant,
}

#[derive(Default)]
struct State {
    /// overlay 出口：`http://127.0.0.1:<bridge>`，指向在校设备的代理。
    egress: Option<Url>,
    probes: HashMap<String, Probe>,
    inflight: HashSet<String>,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// 设置/清除当前 overlay 出口。由 mesh 在选中在校设备与掉线时调用。
pub fn set_egress(egress: Option<Url>) {
    if let Ok(mut state) = state().lock() {
        state.egress = egress;
    }
}

pub fn egress() -> Option<Url> {
    state().lock().ok().and_then(|state| state.egress.clone())
}

/// 校外 + 有出口：此时校内白名单不再强制直连。
pub fn overlay_active(egress: Option<&Url>) -> bool {
    egress.is_some() && !super::campus_reachable()
}

/// 够不着校园网时，校内域名不要再先问校内 DNS（那只会多等 2 秒）。
pub fn prefer_external_dns() -> bool {
    !super::campus_reachable()
}

/// 校外且够不着校园网、公网出口也探测不到时，返回 overlay 出口；否则返回 `None`
/// 走普通策略。
pub fn overlay_proxy_for(egress: Option<&Url>, target: &Url) -> Option<Url> {
    let host = target.host_str()?;
    if !model::is_campus_target(host) || super::campus_reachable() {
        return None;
    }
    let egress = egress.cloned()?;
    let key = probe_key(target);
    let now = Instant::now();
    let (cached, stale) = match state().lock() {
        Ok(state) => match state.probes.get(&key) {
            Some(probe) if now.duration_since(probe.at) < PROBE_TTL => {
                (Some(probe.reachable), false)
            }
            Some(_) => (None, true),
            None => (None, true),
        },
        Err(_) => return None,
    };
    match cached {
        Some(false) => Some(egress),
        Some(true) => None,
        None => {
            if stale {
                let (probe_host, probe_port) = probe_target(target);
                schedule_probe(key, probe_host, probe_port);
            }
            None
        }
    }
}

/// 校外实际要连的目标：命中 override 时用它校外生效的 `fallback`（学校公开公网
/// 出口），否则就是原目标本身。
///
/// 探测必须打这个目标。`jxxk.szu.edu.cn` 这类域名只有校内 DNS 才解析得出公网
/// IP，直接探测原域名会永远判不可达，把本来能走公网出口的请求误送去 overlay。
fn probe_target(target: &Url) -> (String, u16) {
    let host = target.host_str().unwrap_or_default().to_owned();
    let port = target.port_or_known_default().unwrap_or(443);
    let Some(rule) = model::find_override(&host) else {
        return (host, port);
    };
    match model::parse_override_target(&rule.fallback) {
        Some(model::OverrideTarget::Origin {
            host,
            port: fallback_port,
            ..
        }) => (host, fallback_port),
        Some(model::OverrideTarget::Host(host)) => (host, port),
        None => (host, port),
    }
}

fn probe_key(target: &Url) -> String {
    format!(
        "{}:{}",
        target.host_str().unwrap_or_default(),
        target.port_or_known_default().unwrap_or(443)
    )
}

/// 测试用：直接写入一条探测结果，避免真的发起连接。
#[cfg(test)]
pub fn mark_probe(key: &str, reachable: bool) {
    if let Ok(mut state) = state().lock() {
        state.probes.insert(
            key.to_owned(),
            Probe {
                reachable,
                at: Instant::now(),
            },
        );
    }
}

fn schedule_probe(key: String, host: String, port: u16) {
    if let Ok(mut state) = state().lock() {
        if !state.inflight.insert(key.clone()) {
            return;
        }
    }
    tauri::async_runtime::spawn(async move {
        let reachable = if let Ok(ip) = host.parse::<std::net::IpAddr>() {
            tokio::time::timeout(
                PROBE_TIMEOUT,
                tokio::net::TcpStream::connect(std::net::SocketAddr::new(ip, port)),
            )
            .await
            .is_ok_and(|result| result.is_ok())
        } else {
            // 只用系统解析器：这里探测的是公网出口，不能先问校内 DNS。
            match super::dns::resolve_system(&host).await {
                Ok(addrs) => {
                    let mut connected = false;
                    for addr in addrs {
                        if tokio::time::timeout(
                            PROBE_TIMEOUT,
                            tokio::net::TcpStream::connect(std::net::SocketAddr::new(addr, port)),
                        )
                        .await
                        .is_ok_and(|result| result.is_ok())
                        {
                            connected = true;
                            break;
                        }
                    }
                    connected
                }
                Err(_) => false,
            }
        };
        if let Ok(mut state) = state().lock() {
            state.probes.insert(
                key.clone(),
                Probe {
                    reachable,
                    at: Instant::now(),
                },
            );
            state.inflight.remove(&key);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_keys_include_the_effective_port() {
        assert_eq!(probe_key(&Url::parse("https://a.example/x").unwrap()), "a.example:443");
        assert_eq!(probe_key(&Url::parse("http://a.example/x").unwrap()), "a.example:80");
        assert_eq!(
            probe_key(&Url::parse("http://a.example:8080/x").unwrap()),
            "a.example:8080"
        );
    }

    #[test]
    fn campus_targets_are_recognised_from_the_preset_config() {
        assert!(model::is_campus_target("net.szu.edu.cn"));
        assert!(model::is_campus_target("172.30.255.42"));
        assert!(model::is_campus_target("jxxk.szu.edu.cn"));
        assert!(!model::is_campus_target("example.com"));
    }

    #[test]
    fn egress_is_only_active_off_campus() {
        let _guard = crate::netproxy::TEST_ENV_LOCK.lock().unwrap();
        let egress = Url::parse("http://127.0.0.1:1234").unwrap();
        // 区域判定在校内：可达，出口不介入。
        crate::netproxy::set_in_campus_network(true);
        crate::netproxy::reachability::set_reachable(None);
        assert!(!overlay_active(Some(&egress)));
        // 校外且够不着校园网：出口生效。
        crate::netproxy::set_in_campus_network(false);
        crate::netproxy::reachability::set_reachable(Some(false));
        assert!(!crate::netproxy::reachability::campus_reachable());
        assert!(overlay_active(Some(&egress)));
        assert!(!overlay_active(None));
        // 校外但校园网可达（如 SecureLink）：出口不再介入。
        crate::netproxy::reachability::set_reachable(Some(true));
        assert!(!overlay_active(Some(&egress)));
        crate::netproxy::reachability::set_reachable(None);
    }

    #[test]
    fn campus_allowlist_stops_forcing_direct_when_an_egress_exists() {
        let target = Url::parse("http://net.szu.edu.cn/").unwrap();
        let settings = crate::netproxy::model::ProxySettings::default();
        assert!(model::bypasses_proxy(&target, &settings, false));
        assert!(!model::bypasses_proxy(&target, &settings, true));
    }

    #[test]
    fn unreachable_campus_targets_use_the_egress() {
        let _guard = crate::netproxy::TEST_ENV_LOCK.lock().unwrap();
        let target = Url::parse("http://net.szu.edu.cn/login").unwrap();
        let egress = Url::parse("http://127.0.0.1:1234").unwrap();
        crate::netproxy::dns::set_in_campus_network(false);
        crate::netproxy::reachability::set_reachable(Some(false));
        mark_probe("net.szu.edu.cn:80", false);
        assert_eq!(
            overlay_proxy_for(Some(&egress), &target),
            Some(egress.clone())
        );

        mark_probe("net.szu.edu.cn:80", true);
        assert_eq!(overlay_proxy_for(Some(&egress), &target), None);

        // 非校内目标永远不走 overlay。
        assert_eq!(
            overlay_proxy_for(Some(&egress), &Url::parse("https://example.com/").unwrap()),
            None
        );

        // 校园网可达时完全不考虑 overlay，即使探测缓存说不可达。
        mark_probe("net.szu.edu.cn:80", false);
        crate::netproxy::reachability::set_reachable(Some(true));
        assert_eq!(overlay_proxy_for(Some(&egress), &target), None);
        crate::netproxy::reachability::set_reachable(None);
    }

    #[test]
    fn probes_target_the_public_egress_of_override_hosts() {
        // jxxk 的校外入口是 override 的 fallback（裸主机 c46.szu.edu.cn），
        // 探测必须打它而不是只有校内 DNS 才解析的原域名。
        assert_eq!(
            probe_target(&Url::parse("https://jxxk.szu.edu.cn/a?b=1").unwrap()),
            ("c46.szu.edu.cn".to_owned(), 443)
        );
        // 没有 override 的目标保持原样。
        assert_eq!(
            probe_target(&Url::parse("https://authserver.szu.edu.cn/").unwrap()),
            ("authserver.szu.edu.cn".to_owned(), 443)
        );
        assert_eq!(
            probe_target(&Url::parse("http://172.21.5.5:8080/x").unwrap()),
            ("172.21.5.5".to_owned(), 8080)
        );
    }
}
