//! 设备自组网与设置同步的骨架。
//!
//! 生命周期只有两种状态：节点未运行 / 节点运行中。运行中意味着：
//!
//! 1. 进程内有一个 `no_tun` 的 EasyTier 节点；
//! 2. 本机回环上有一个只接受 mesh token 的 HTTP 接口；
//! 3. 虚拟网端口通过 [`bridge`] 接到该接口；
//! 4. [`devices`] 按活动档位刷新设备列表。
//!
//! 网络身份由 [`identity`] 从账户资料派生；开关保存在本地 `settings.sqlite`
//! （`sync.enabled` / `sync.campus_access`），不进入同步数据。

use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

pub(crate) mod api;
pub(crate) mod bridge;
pub(crate) mod campus;
mod campus_proxy;
pub(crate) mod commands;
pub(crate) mod devices;
pub(crate) mod identity;
pub(crate) mod mcp;
pub(crate) mod node;

/// 虚拟网内的同步接口端口（只在 overlay 命名空间里使用，不占本机端口）。
pub(crate) const SYNC_API_PORT: u16 = 54301;
/// 虚拟网内的校园网出口代理端口。
pub(crate) const CAMPUS_PROXY_PORT: u16 = 54302;

pub(crate) const STATUS_EVENT: &str = "mesh-status-changed";
pub(crate) const DEVICES_EVENT: &str = "mesh-devices-changed";

pub(crate) const DEFAULT_DEVICE_NAME: &str = "我的设备";
const MAX_DEVICE_NAME_CHARS: usize = 40;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MeshStatusView {
    pub(crate) enabled: bool,
    pub(crate) campus_access: bool,
    pub(crate) running: bool,
    pub(crate) device_id: String,
    pub(crate) device_name: String,
    pub(crate) overlay_ip: Option<String>,
    /// 当前借用的校园网出口设备名（为空表示没有使用）。
    pub(crate) campus_egress: Option<String>,
    /// 本机对等接口的 loopback 端口；只用于诊断。
    pub(crate) api_port: Option<u16>,
    pub(crate) error: Option<String>,
}

struct Running {
    node: Arc<node::Node>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
    api_port: u16,
    /// 对等接口的认证 token（由网络密钥派生）。
    token: Arc<str>,
}

pub(crate) struct MeshStore {
    lifecycle: tokio::sync::Mutex<Option<Running>>,
    devices: Mutex<Vec<devices::MeshDeviceView>>,
    bridges: Mutex<HashMap<Ipv4Addr, bridge::Bridge>>,
    campus_bridge: Mutex<Option<(Ipv4Addr, bridge::Bridge)>>,
    last_error: Mutex<Option<String>>,
    device_id: Mutex<Option<String>>,
}

impl MeshStore {
    pub(crate) fn new() -> Self {
        Self {
            lifecycle: tokio::sync::Mutex::new(None),
            devices: Mutex::new(Vec::new()),
            bridges: Mutex::new(HashMap::new()),
            campus_bridge: Mutex::new(None),
            last_error: Mutex::new(None),
            device_id: Mutex::new(None),
        }
    }

    pub(crate) fn device_id(&self) -> Result<String, String> {
        let mut cached = self.device_id.lock().map_err(|_| "设备标识锁失败")?;
        if let Some(id) = cached.as_ref() {
            return Ok(id.clone());
        }
        let id = match crate::preferences::load::<String>(crate::preferences::SYNC_DEVICE_ID)
            .map_err(|error| format!("读取设备标识失败: {error}"))?
        {
            Some(id) if !id.trim().is_empty() => id,
            _ => {
                let id = uuid::Uuid::new_v4().to_string();
                crate::preferences::save(crate::preferences::SYNC_DEVICE_ID, &id)
                    .map_err(|error| format!("保存设备标识失败: {error}"))?;
                id
            }
        };
        *cached = Some(id.clone());
        Ok(id)
    }

    pub(crate) fn device_name(&self) -> String {
        crate::preferences::load::<String>(crate::preferences::SYNC_DEVICE_NAME)
            .ok()
            .flatten()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_DEVICE_NAME.to_owned())
    }

    pub(crate) fn devices(&self) -> Vec<devices::MeshDeviceView> {
        self.devices
            .lock()
            .map(|devices| devices.clone())
            .unwrap_or_default()
    }

    pub(crate) fn replace_devices(&self, next: Vec<devices::MeshDeviceView>) -> bool {
        let Ok(mut current) = self.devices.lock() else {
            return false;
        };
        if *current == next {
            return false;
        }
        *current = next;
        true
    }

    fn clear_devices(&self) {
        if let Ok(mut devices) = self.devices.lock() {
            devices.clear();
        }
        if let Ok(mut bridges) = self.bridges.lock() {
            bridges.clear();
        }
    }

    fn set_error(&self, error: Option<String>) {
        if let Ok(mut last) = self.last_error.lock() {
            *last = error;
        }
    }

    fn error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|error| error.clone())
    }

    /// 取得（必要时创建）指向 `peer` 同步接口的本机桥接端口。
    ///
    /// 调用方必须位于 tokio 运行时内：`bridge::outbound` 会 spawn 接受循环。
    pub(crate) fn bridge_port(&self, node: &Arc<node::Node>, peer: Ipv4Addr) -> Option<u16> {
        let mut bridges = self.bridges.lock().ok()?;
        if let Some(bridge) = bridges.get(&peer) {
            return Some(bridge.local_addr.port());
        }
        let bridge = bridge::outbound(node.clone(), peer, SYNC_API_PORT).ok()?;
        let port = bridge.local_addr.port();
        bridges.insert(peer, bridge);
        Some(port)
    }

    pub(crate) fn campus_peer(&self) -> Option<Ipv4Addr> {
        self.campus_bridge
            .lock()
            .ok()
            .and_then(|bridge| bridge.as_ref().map(|(ip, _)| *ip))
    }

    /// 替换当前校园网出口桥接；传 `None` 会丢弃旧桥并关闭监听。
    pub(crate) fn set_campus_bridge(&self, next: Option<(Ipv4Addr, bridge::Bridge)>) {
        if let Ok(mut current) = self.campus_bridge.lock() {
            *current = next;
        }
    }

    fn retain_bridges(&self, keep: &HashSet<Ipv4Addr>) {
        if let Ok(mut bridges) = self.bridges.lock() {
            bridges.retain(|peer, _| keep.contains(peer));
        }
    }

    fn api_port(&self) -> Option<u16> {
        self.lifecycle
            .try_lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|running| running.api_port))
    }

    /// 当前节点（用于同步引擎发起对端请求）。未运行时为 `None`。
    pub(crate) fn node(&self) -> Option<Arc<node::Node>> {
        self.lifecycle
            .try_lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|running| running.node.clone()))
    }

    /// 对等接口的认证 token。只在节点运行时可用。
    pub(crate) fn token(&self) -> Option<Arc<str>> {
        self.lifecycle
            .try_lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|running| running.token.clone()))
    }

    pub(crate) fn running(&self) -> bool {
        self.lifecycle
            .try_lock()
            .map(|guard| guard.is_some())
            .unwrap_or(true)
    }
}

pub(crate) fn enabled() -> bool {
    crate::preferences::load::<bool>(crate::preferences::SYNC_ENABLED)
        .ok()
        .flatten()
        .unwrap_or(false)
}

/// 节点是否正在运行。供同步/校园网模块判断当前能力。
pub(crate) fn is_running(app: &AppHandle) -> bool {
    app.try_state::<Arc<MeshStore>>()
        .is_some_and(|store| store.running())
}

pub(crate) fn campus_access_enabled() -> bool {
    crate::preferences::load::<bool>(crate::preferences::SYNC_CAMPUS_ACCESS)
        .ok()
        .flatten()
        .unwrap_or(false)
}

pub(crate) fn status(store: &MeshStore) -> MeshStatusView {
    let devices = store.devices();
    let overlay_ip = devices
        .iter()
        .find(|device| device.is_self)
        .and_then(|device| device.overlay_ip.clone());
    let campus_egress = store.campus_peer().map(|ip| {
        let ip = ip.to_string();
        devices
            .iter()
            .find(|device| device.overlay_ip.as_deref() == Some(ip.as_str()))
            .map(|device| device.name.clone())
            .unwrap_or(ip)
    });
    MeshStatusView {
        enabled: enabled(),
        campus_access: campus_access_enabled(),
        running: store.running(),
        device_id: store.device_id().unwrap_or_default(),
        device_name: store.device_name(),
        overlay_ip,
        campus_egress,
        api_port: store.api_port(),
        error: store.error(),
    }
}

fn emit_status(app: &AppHandle, store: &MeshStore) {
    let _ = app.emit(STATUS_EVENT, status(store));
}

/// 启动时按本地开关决定是否拉起节点。失败只记录日志并写进状态，不影响应用启动。
pub(crate) fn initialize(app: &AppHandle) {
    let store = app
        .try_state::<Arc<MeshStore>>()
        .map(|state| state.inner().clone());
    let Some(store) = store else {
        return;
    };
    let _ = store.device_id();
    if !enabled() && !campus_access_enabled() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = start(&app, &store).await {
            log::error!("设备同步网络启动失败: {error}");
        }
    });
}

pub(crate) async fn start(app: &AppHandle, store: &Arc<MeshStore>) -> Result<(), String> {
    let mut guard = store.lifecycle.lock().await;
    if guard.is_some() {
        return Ok(());
    }
    store.set_error(None);
    match start_inner(app, store).await {
        Ok(running) => {
            *guard = Some(running);
            emit_status(app, store);
            Ok(())
        }
        Err(error) => {
            store.set_error(Some(error.clone()));
            store.clear_devices();
            emit_status(app, store);
            Err(error)
        }
    }
}

async fn start_inner(app: &AppHandle, store: &Arc<MeshStore>) -> Result<Running, String> {
    let identity = identity::derive_current()?;
    let device_id = store.device_id()?;
    let device_name = store.device_name();
    let node = Arc::new(node::Node::start(&identity, &device_name).await?);

    let token: Arc<str> = api::derive_token(&identity.network_secret).into();
    let state = api::ApiState {
        token: token.clone(),
        app: app.clone(),
        node: node.clone(),
        device_id: device_id.into(),
        device_name: device_name.clone().into(),
    };

    let (listener, allocation) = crate::ports::bind_loopback(crate::ports::PortService::Mesh, None)
        .map_err(|error| error.to_string())?;
    let api_port = allocation.port;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("读取设备同步接口端口失败: {error}"))?;
    let listener = tokio::net::TcpListener::from_std(listener)
        .map_err(|error| format!("设备同步接口监听失败: {error}"))?;
    let router = api::router(state);
    let mut tasks = vec![tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::warn!("设备同步接口停止: {error}");
        }
    })];

    let inbound = bridge::inbound(node.clone(), SYNC_API_PORT, local_addr)
        .await
        .inspect_err(|_| {
            for task in &tasks {
                task.abort();
            }
        })?;
    tasks.push(inbound);
    // 校园网出口代理始终随节点启动；开关关闭时请求会被直接拒绝，对端也不会选它。
    match campus_proxy::serve(node.clone()).await {
        Ok(task) => tasks.push(task),
        Err(error) => log::warn!("校园网出口代理启动失败: {error}"),
    }
    tasks.push(tokio::spawn(devices::refresh_loop(
        app.clone(),
        store.clone(),
        node.clone(),
        token.clone(),
    )));

    let device_id = store.device_id().unwrap_or_default();
    store.replace_devices(vec![devices::offline_self(
        &device_id,
        &device_name,
    )]);

    Ok(Running {
        node,
        tasks,
        api_port,
        token,
    })
}

pub(crate) async fn stop(app: &AppHandle, store: &Arc<MeshStore>) {
    let mut guard = store.lifecycle.lock().await;
    if let Some(running) = guard.take() {
        for task in running.tasks {
            task.abort();
        }
        running.node.stop().await;
    }
    store.clear_devices();
    campus::clear(store);
    store.set_error(None);
    emit_status(app, store);
}

/// 设置开关并立即生效。`enabled` 与 `campus_access` 任一为真时都需要节点在跑。
pub(crate) async fn set_switches(
    app: &AppHandle,
    store: &Arc<MeshStore>,
    sync_enabled: bool,
    campus_access: bool,
) -> Result<(), String> {
    crate::preferences::save(crate::preferences::SYNC_ENABLED, &sync_enabled)
        .map_err(|error| format!("保存同步开关失败: {error}"))?;
    crate::preferences::save(crate::preferences::SYNC_CAMPUS_ACCESS, &campus_access)
        .map_err(|error| format!("保存校园网访问开关失败: {error}"))?;
    if sync_enabled || campus_access {
        start(app, store).await?;
    } else {
        stop(app, store).await;
    }
    if let Some(node) = store.node() {
        campus::refresh(app, store, &node);
    }
    emit_status(app, store);
    Ok(())
}

pub(crate) fn normalize_device_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("设备名称不能为空".into());
    }
    if name.chars().count() > MAX_DEVICE_NAME_CHARS {
        return Err(format!("设备名称不能超过 {MAX_DEVICE_NAME_CHARS} 个字符"));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_name_is_trimmed_and_bounded() {
        assert_eq!(normalize_device_name("  床垫  ").unwrap(), "床垫");
        assert!(normalize_device_name("   ").is_err());
        assert!(normalize_device_name(&"名".repeat(MAX_DEVICE_NAME_CHARS + 1)).is_err());
    }
}
