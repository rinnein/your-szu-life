//! 设备列表：把 EasyTier 的 route 快照与对等接口的自我描述合并成一份视图。
//!
//! 刷新频率跟随 `activity` 的三档（同步页在前台时最快），每次只做一次轻量
//! 元数据交换，不做任何设置内容的读写。

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use super::api::{self, PeerStatus, TOKEN_HEADER};
use super::node::Node;
use super::{MeshStore, DEVICES_EVENT};
use szu_core::activity;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MeshDeviceView {
    pub device_id: String,
    pub name: String,
    pub platform: String,
    pub app_version: String,
    pub overlay_ip: Option<String>,
    pub is_self: bool,
    pub reachable: bool,
    pub latency_ms: Option<u64>,
    pub sync_enabled: bool,
    pub campus_access: bool,
    pub on_campus: bool,
    pub region: String,
    pub network_state: String,
    pub ssid: Option<String>,
}

impl MeshDeviceView {
    fn from_status(
        status: PeerStatus,
        is_self: bool,
        reachable: bool,
        latency_ms: Option<u64>,
    ) -> Self {
        Self {
            device_id: status.device_id,
            name: status.name,
            platform: status.platform,
            app_version: status.app_version,
            overlay_ip: status.overlay_ip,
            is_self,
            reachable,
            latency_ms,
            sync_enabled: status.sync_enabled,
            campus_access: status.campus_access,
            on_campus: status.on_campus,
            region: status.region,
            network_state: status.network_state,
            ssid: status.ssid,
        }
    }

    fn unknown_peer(hostname: String, overlay_ip: Ipv4Addr, latency_ms: Option<u64>) -> Self {
        Self {
            device_id: String::new(),
            name: if hostname.is_empty() {
                overlay_ip.to_string()
            } else {
                hostname
            },
            platform: String::new(),
            app_version: String::new(),
            overlay_ip: Some(overlay_ip.to_string()),
            is_self: false,
            reachable: false,
            latency_ms,
            sync_enabled: false,
            campus_access: false,
            on_campus: false,
            region: "unknown".into(),
            network_state: "unknown".into(),
            ssid: None,
        }
    }
}

/// 后台刷新循环。由 [`super::start_with`] 启动，随节点一起 abort。
pub async fn refresh_loop(
    app: tauri::AppHandle,
    store: Arc<MeshStore>,
    node: Arc<Node>,
    token: Arc<str>,
) {
    let client = match szu_core::netproxy::overlay_client_builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            log::error!("设备列表 HTTP 客户端创建失败: {error}");
            return;
        }
    };
    loop {
        let interval = activity::interval(
            Some("mesh"),
            Duration::from_secs(15),
            Duration::from_secs(60),
            Duration::from_secs(180),
        );
        match refresh_once(&app, &store, &node, &client, &token).await {
            Ok(devices) => {
                if store.replace_devices(devices.clone()) {
                    let _ = app.emit(DEVICES_EVENT, &devices);
                }
                // 设备/本机校园网状态都可能变化：每轮都重新决定是否借用出口。
                super::campus::refresh(&app, &store, &node);
            }
            Err(error) => log::warn!("刷新设备列表失败: {error}"),
        }
        tokio::time::sleep(interval).await;
    }
}

async fn refresh_once(
    app: &tauri::AppHandle,
    store: &MeshStore,
    node: &Arc<Node>,
    client: &reqwest::Client,
    token: &str,
) -> Result<Vec<MeshDeviceView>, String> {
    let device_id = store.device_id()?;
    let device_name = store.device_name();
    let self_status = api::local_status(app, node, &device_id, &device_name).await?;
    let mut devices = vec![MeshDeviceView::from_status(self_status, true, true, None)];

    let self_peer_id = node.self_info().await.map(|info| info.peer_id);
    let mut seen = HashSet::new();
    for peer in node.peers().await {
        if Some(peer.peer_id) == self_peer_id {
            continue;
        }
        let Some(ip) = peer.ipv4 else {
            continue;
        };
        seen.insert(ip);
        let Some(port) = store.bridge_port(node, ip) else {
            devices.push(MeshDeviceView::unknown_peer(
                peer.hostname,
                ip,
                peer.latency_ms,
            ));
            continue;
        };
        match fetch_status(client, port, token).await {
            Some(status) => devices.push(MeshDeviceView::from_status(
                status,
                false,
                true,
                peer.latency_ms,
            )),
            None => devices.push(MeshDeviceView::unknown_peer(
                peer.hostname,
                ip,
                peer.latency_ms,
            )),
        }
    }
    store.retain_bridges(&seen);

    devices.sort_by(|a, b| {
        b.is_self
            .cmp(&a.is_self)
            .then_with(|| a.overlay_ip.cmp(&b.overlay_ip))
            .then_with(|| a.device_id.cmp(&b.device_id))
    });
    Ok(devices)
}

async fn fetch_status(client: &reqwest::Client, port: u16, token: &str) -> Option<PeerStatus> {
    let response = client
        .get(format!("http://127.0.0.1:{port}/v1/status"))
        .header(TOKEN_HEADER, token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<PeerStatus>().await.ok()
}

/// 设备列表里的本机视图（启动早期、刷新循环还没跑过时给前端用）。
pub fn offline_self(device_id: &str, device_name: &str, app_version: &str) -> MeshDeviceView {
    MeshDeviceView {
        device_id: device_id.to_owned(),
        name: device_name.to_owned(),
        platform: std::env::consts::OS.to_string(),
        app_version: app_version.to_owned(),
        overlay_ip: None,
        is_self: true,
        reachable: false,
        latency_ms: None,
        sync_enabled: false,
        campus_access: false,
        on_campus: false,
        region: "unknown".into(),
        network_state: "unknown".into(),
        ssid: None,
    }
}
