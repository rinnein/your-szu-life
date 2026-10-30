//! 非机密偏好与初始化设置进度的全局入口。
//!
//! 所有读写都经 [`crate::migrations::db::run_sync`] 落到 `settings.sqlite`。
//! 该模块必须在日志初始化之前启动：日志级别本身也是这里的偏好。

use serde::{de::DeserializeOwned, Serialize};
use std::sync::OnceLock;
use tauri::Manager;

pub(crate) mod db;

pub(crate) const APPEARANCE: &str = "appearance";
pub(crate) const NOTIFICATIONS: &str = "notifications";
pub(crate) const MCP_ENABLED: &str = "mcp.enabled";
pub(crate) const LOG_LEVEL: &str = "logging.level";
pub(crate) const DEBUG_ENABLED: &str = "debug.enabled";

static DATABASE: OnceLock<db::SettingsDatabase> = OnceLock::new();

/// 打开 `settings.sqlite`，执行 schema 迁移并导入旧版 JSON 设置。
pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
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

pub(crate) fn database() -> Result<&'static db::SettingsDatabase, String> {
    DATABASE
        .get()
        .ok_or_else(|| "设置数据库尚未初始化".to_string())
}

pub(crate) fn load<T: DeserializeOwned>(key: &str) -> Result<Option<T>, String> {
    let raw = crate::migrations::db::run_sync(database()?.load(key))?;
    raw.map(|raw| {
        serde_json::from_str(&raw).map_err(|error| format!("设置 {key} 的存储格式无效: {error}"))
    })
    .transpose()
}

pub(crate) fn save<T: Serialize>(key: &str, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string(value).map_err(|error| error.to_string())?;
    crate::migrations::db::run_sync(database()?.save(key, &raw))
}

/// 初始化设置步骤状态：`id -> (status, revision)`。
pub(crate) fn load_onboarding_rows() -> Result<std::collections::BTreeMap<String, (String, u32)>, String> {
    let rows = crate::migrations::db::run_sync(database()?.onboarding_rows())?;
    Ok(rows
        .into_iter()
        .map(|(id, status, revision)| (id, (status, revision.max(0) as u32)))
        .collect())
}

pub(crate) fn save_onboarding_step(id: &str, status: &str, revision: u32) -> Result<(), String> {
    crate::migrations::db::run_sync(database()?.save_onboarding_step(id, status, revision as i64))
}
