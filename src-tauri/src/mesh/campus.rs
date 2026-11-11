//! 选择「校园网访问」的出口设备。
//!
//! 只在同时满足以下条件时建立出口：本机不在校园网、开关打开、网络里有其它
//! 在线且处于校园网、自己也开启了校园网访问的设备。选中延迟最低的一台，把它的
//! `CAMPUS_PROXY_PORT` 桥接到本机回环，并把它交给 `netproxy` 作为校内目标的
//! 回退出口。任何条件不满足都立即拆除，回到原来的出网逻辑。

use std::net::Ipv4Addr;
use std::sync::Arc;

use tauri::AppHandle;
use tauri::Manager;

use super::node::Node;
use super::{bridge, MeshStore, CAMPUS_PROXY_PORT};

pub(crate) fn refresh(app: &AppHandle, store: &Arc<MeshStore>, node: &Arc<Node>) {
    if !super::campus_access_enabled() || local_on_campus(app) {
        clear(store);
        return;
    }
    let best = store
        .devices()
        .into_iter()
        .filter(|device| {
            !device.is_self && device.reachable && device.campus_access && device.on_campus
        })
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
                    crate::netproxy::campus_access::set_egress(Some(url));
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

pub(crate) fn clear(store: &MeshStore) {
    if store.campus_peer().is_some() {
        store.set_campus_bridge(None);
        crate::netproxy::campus_access::set_egress(None);
    }
}

fn local_on_campus(app: &AppHandle) -> bool {
    let Some(store) = app.try_state::<crate::campus_network::CampusNetworkStore>() else {
        return false;
    };
    crate::campus_network::service::get_network_overview(&store)
        .map(|overview| overview.region != crate::campus_network::model::NetworkRegion::None)
        .unwrap_or(false)
}
