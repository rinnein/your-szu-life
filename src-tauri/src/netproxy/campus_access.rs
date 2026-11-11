//! 校外时的「校园网出口」策略。
//!
//! 优先级由需求决定：
//!
//! 1. 本机在校园网内：完全维持原来的绕过/直连逻辑；
//! 2. 校外且没有可用的 overlay 出口：也维持原逻辑（否则会连最后一条直连也丢掉）；
//! 3. 校外且有在校设备：`campus-network.json` 的直连白名单不再强制生效，先按普通
//!    策略走外网；只有探测到「外网连不上」的校内目标才改走 overlay 代理。
//!
//! 探测按 `host:port` 缓存 60 秒，避免每个请求都吃一次连接超时。命中「不可达」后
//! 后续请求直接走在校设备；外网恢复后会在一分钟内回到直连。

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
pub(crate) fn set_egress(egress: Option<Url>) {
    if let Ok(mut state) = state().lock() {
        state.egress = egress;
    }
}

pub(crate) fn egress() -> Option<Url> {
    state().lock().ok().and_then(|state| state.egress.clone())
}

/// 校外 + 有出口：此时校内白名单不再强制直连。
pub(crate) fn overlay_active(egress: Option<&Url>) -> bool {
    egress.is_some() && !super::dns::is_in_campus_network()
}

/// 校外 + 有出口时，校内域名不要再先问校内 DNS（那只会多等 2 秒）。
pub(crate) fn prefer_external_dns() -> bool {
    overlay_active(egress().as_ref())
}

/// 命中「外网不可达」的校内目标返回 overlay 出口；否则返回 `None` 走普通策略。
pub(crate) fn overlay_proxy_for(egress: Option<&Url>, target: &Url) -> Option<Url> {
    let host = target.host_str()?;
    if !model::is_campus_target(host) || super::dns::is_in_campus_network() {
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
                schedule_probe(key, host.to_owned(), target.port_or_known_default());
            }
            None
        }
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
pub(crate) fn mark_probe(key: &str, reachable: bool) {
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

fn schedule_probe(key: String, host: String, port: Option<u16>) {
    let port = port.unwrap_or(443);
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
            match super::dns::resolve_host(&host).await {
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
        let egress = Url::parse("http://127.0.0.1:1234").unwrap();
        crate::netproxy::dns::set_in_campus_network(true);
        assert!(!overlay_active(Some(&egress)));
        crate::netproxy::dns::set_in_campus_network(false);
        assert!(overlay_active(Some(&egress)));
        assert!(!overlay_active(None));
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
        let target = Url::parse("http://net.szu.edu.cn/login").unwrap();
        let egress = Url::parse("http://127.0.0.1:1234").unwrap();
        crate::netproxy::dns::set_in_campus_network(false);
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
    }
}
