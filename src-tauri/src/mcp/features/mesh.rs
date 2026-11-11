use std::sync::Arc;

use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::tool;
use rmcp::tool_router;
use tauri::Manager;

#[tool_router(router = mesh_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "mesh_get_status",
        description = "获取设备同步网络状态（开关、是否运行、本机虚拟 IP、设备名与最近错误）。对应资源 your-szu-life://data/mesh/status"
    )]
    pub async fn mesh_get_status(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Arc<szu_mesh::mesh::MeshStore>>()
            .ok_or_else(|| "设备同步模块未初始化".to_string())?;
        serde_json::to_string_pretty(&szu_mesh::mesh::status(&store))
            .map_err(|error| format!("序列化失败: {error}"))
    }

    #[tool(
        name = "mesh_list_devices",
        description = "列出同一账号自组网内的全部设备（名称、虚拟 IP、是否在线、是否处于校园网）。对应资源 your-szu-life://data/mesh/devices"
    )]
    pub async fn mesh_list_devices(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Arc<szu_mesh::mesh::MeshStore>>()
            .ok_or_else(|| "设备同步模块未初始化".to_string())?;
        serde_json::to_string_pretty(&store.devices())
            .map_err(|error| format!("序列化失败: {error}"))
    }

    #[tool(
        name = "mesh_get_campus_access",
        description = "获取「校园网访问」状态：开关、本机是否在校内、当前借用的出口设备，以及网络内各设备的校内检测结果。对应资源 your-szu-life://data/mesh/campus-access"
    )]
    pub async fn mesh_get_campus_access(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<Arc<szu_mesh::mesh::MeshStore>>()
            .ok_or_else(|| "设备同步模块未初始化".to_string())?;
        let devices = store.devices();
        let self_device = devices.iter().find(|device| device.is_self);
        let campus_devices: Vec<_> = devices
            .iter()
            .filter(|device| !device.is_self && device.reachable && device.on_campus)
            .map(|device| {
                serde_json::json!({
                    "deviceId": device.device_id,
                    "name": device.name,
                    "overlayIp": device.overlay_ip,
                    "campusAccess": device.campus_access,
                })
            })
            .collect();
        serde_json::to_string_pretty(&serde_json::json!({
            "enabled": szu_mesh::mesh::campus_access_enabled(),
            "running": szu_mesh::mesh::is_running(&self.app),
            "localOnCampus": self_device.is_some_and(|device| device.on_campus),
            "egress": store.campus_peer().map(|ip| ip.to_string()),
            "campusDevices": campus_devices,
        }))
        .map_err(|error| format!("序列化失败: {error}"))
    }
}
