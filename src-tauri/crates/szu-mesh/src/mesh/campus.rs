//! 选择「校园网访问」的出口设备。
//!
//! 只要满足以下条件就建立出口：本机够不着校园网（区域判定与可达性探测都不
//! 成立）、网络里有其它在线、自己也开启了「同账户校园网共享」并且够得着校园网
//! 的设备。选中延迟最低的一台，把它的 `CAMPUS_PROXY_PORT` 桥接到本机回环，
//! 并把它交给 `netproxy` 作为校内目标的兜底出口。任何条件不满足都立即拆除。
//!
//! 借用不经过本机开关：开关只决定本机是否把校园网共享出去。

use std::net::Ipv4Addr;
use std::sync::Arc;

use super::node::Node;
use super::{bridge, MeshStore, CAMPUS_PROXY_PORT};

pub fn refresh(store: &Arc<MeshStore>, node: &Arc<Node>) {
    if szu_core::netproxy::campus_reachable() {
        clear(store);
        return;
    }
    let best = store
        .devices()
        .into_iter()
        .filter(|device| !device.is_self && device.reachable && device.campus_access)
        .filter(|device| device.campus_reachable)
        .filter_map(|device| {
            let ip: Ipv4Addr = device.overlay_ip?.parse().ok()?;
            Some((ip, device.latency_ms.unwrap_or(u64::MAX)))
        })
        .min_by_key(|(ip, latency)| (*latency, *ip));
    let Some((ip, _)) = best else {
        clear(store);
        return;
    };
    if store.campus_peer() == Some(ip) {
        return;
    }
    match bridge::outbound(node.clone(), ip, CAMPUS_PROXY_PORT) {
        Ok(bridge) => {
            let port = bridge.local_addr.port();
            store.set_campus_bridge(Some((ip, bridge)));
            match reqwest::Url::parse(&format!("http://127.0.0.1:{port}")) {
                Ok(url) => {
                    szu_core::netproxy::campus_access::set_egress(Some(url));
                    log::info!("校园网访问出口已切换到 {ip}（本机回环端口 {port}）");
                }
                Err(_) => clear(store),
            }
        }
        Err(error) => {
            log::warn!("校园网出口桥接失败: {error}");
            clear(store);
        }
    }
}

pub fn clear(store: &MeshStore) {
    if store.campus_peer().is_some() {
        store.set_campus_bridge(None);
        szu_core::netproxy::campus_access::set_egress(None);
    }
}
