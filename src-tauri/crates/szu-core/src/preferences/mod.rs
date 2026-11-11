//! 非机密偏好与初始化设置进度的全局入口。
//!
//! 所有读写都经 [`crate::migrations::db::run_sync`] 落到 `settings.sqlite`。
//! 该模块必须在日志初始化之前启动：日志级别本身也是这里的偏好。

use serde::{de::DeserializeOwned, Serialize};
use std::sync::OnceLock;
use tauri::Manager;

pub mod db;

pub const APPEARANCE: &str = "appearance";
pub const NOTIFICATIONS: &str = "notifications";
pub const ANALYTICS_ENABLED: &str = "analytics.enabled";
pub const MCP_ENABLED: &str = "mcp.enabled";
pub const MCP_PORT: &str = "mcp.port";
pub const LOG_LEVEL: &str = "logging.level";
pub const DEBUG_ENABLED: &str = "debug.enabled";
pub const DEMO_ENABLED: &str = "demo.enabled";
/// 用户可直接获取/管理的文件统一目录。设备相关，不随设置包导出。
pub const FILE_DIRECTORY: &str = "file.directory";
/// 应用内更新频道：`"stable"`（默认）或 `"canary"`。
pub const UPDATE_CHANNEL: &str = "update.channel";
/// 设备同步总开关。设备相关，不参与同步。
pub const SYNC_ENABLED: &str = "sync.enabled";
/// 「校园网访问」开关：本机不在校园网时借用同账号设备的校园网出口。
pub const SYNC_CAMPUS_ACCESS: &str = "sync.campus_access";
/// 本机在同步网络里的稳定标识（随机 UUID，首次启用时生成）。
pub const SYNC_DEVICE_ID: &str = "sync.device_id";
/// 用户可见的设备名，只影响设备列表展示。
pub const SYNC_DEVICE_NAME: &str = "sync.device_name";

static DATABASE: OnceLock<db::SettingsDatabase> = OnceLock::new();

/// 打开 `settings.sqlite`，执行 schema 迁移并导入旧版 JSON 设置。
pub fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    if DATABASE.get().is_some() {
        return Ok(());
    }
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法确定应用数据目录".to_string())?;
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "无法确定应用配置目录".to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|error| format!("无法创建设置目录: {error}"))?;
    let db_path = data_dir.join("settings.sqlite");
    let database = crate::migrations::db::run_sync(db::SettingsDatabase::open(&db_path))?;
    let paths = crate::migrations::preferences::LegacySettingsPaths {
        config_dir,
        data_dir,
    };
    let report = crate::migrations::db::run_sync(
        crate::migrations::preferences::migrate_legacy_json(&paths, &database),
    )?;
    if !report.imported.is_empty() {
        eprintln!(
            "已迁移旧版设置到 settings.sqlite: {}",
            report.imported.join("、")
        );
    }
    if !report.kept.is_empty() {
        eprintln!(
            "以下旧版设置未能迁移，原文件已保留: {}",
            report.kept.join("、")
        );
    }
    let _ = DATABASE.set(database);
    Ok(())
}

pub fn database() -> Result<&'static db::SettingsDatabase, String> {
    DATABASE
        .get()
        .ok_or_else(|| "设置数据库尚未初始化".to_string())
}

pub fn load<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String> {
    let raw = crate::migrations::db::run_sync(database()?.load(key))?;
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|error| format!("设置 {key} 的存储格式无效: {error}"))
    })
    .transpose()
}

pub fn save<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string(value).map_err(|error| error.to_string())?;
    crate::migrations::db::run_sync(database()?.save(key, &raw))
}

/// 初始化设置步骤状态：`id -> (status, revision)`。
pub fn load_onboarding_rows() -> Result<std::collections::BTreeMap<String, (String, u32)>, String> {
    let rows = crate::migrations::db::run_sync(database()?.onboarding_rows())?;
    Ok(rows
        .into_iter()
        .map(|(id, status, revision)| (id, (status, revision.max(0) as u32)))
        .collect())
}

pub fn save_onboarding_step(id: &str, status: &str, revision: u32) -> Result<(), String> {
    crate::migrations::db::run_sync(database()?.save_onboarding_step(id, status, revision as i64))
}
