//! 旧版 JSON 设置 → `settings.sqlite` 的一次性迁移。
//!
//! 迁移规则：DB 中已有该键时以 DB 为准；只有“解析/校验成功且写入成功”才删除
//! 旧文件，任何失败都保留原文件，避免用户设置在升级过程中丢失。

use crate::preferences::db::SettingsDatabase;
use crate::preferences::{
    APPEARANCE, DEBUG_ENABLED, LOG_LEVEL, MCP_ENABLED, NOTIFICATIONS,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{
    fs,
    io,
    path::{Path, PathBuf},
};
use toasty::migration::{MigrationFile, MigrationSet};

pub(crate) static SETTINGS_MIGRATIONS: MigrationSet = MigrationSet::new(&[
    MigrationFile::new(
        1,
        "0001_init_preferences.sql",
        r#"
    CREATE TABLE IF NOT EXISTS preference_records (
        key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    "#,
    ),
    MigrationFile::new(
        2,
        "0002_init_onboarding_steps.sql",
        r#"
    CREATE TABLE IF NOT EXISTS onboarding_step_records (
        id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        revision INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    "#,
    ),
]);

#[derive(Clone, Debug)]
pub(crate) struct LegacySettingsPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

#[derive(Default)]
pub(crate) struct MigrationReport {
    pub imported: Vec<&'static str>,
    pub kept: Vec<&'static str>,
}

#[derive(Deserialize)]
struct LegacyLogConfig {
    level: String,
}

#[derive(Deserialize)]
struct LegacyMcpConfig {
    #[serde(default)]
    enabled: bool,
}

pub(crate) async fn migrate_legacy_json(
    paths: &LegacySettingsPaths,
    db: &SettingsDatabase,
) -> Result<MigrationReport, String> {
    let mut report = MigrationReport::default();

    let appearance_path = paths.config_dir.join("appearance.json");
    if let Some(value) = read_legacy::<crate::appearance::Appearance>(
        &appearance_path,
        "appearance.json",
        &mut report,
    ) {
        if value.validate().is_ok() {
            save_if_absent(db, APPEARANCE, &value, &appearance_path, "appearance.json", &mut report)
                .await?;
        } else {
            report.kept.push("appearance.json");
        }
    }

    let notifications_path = paths.config_dir.join("notifications.json");
    if let Some(value) = read_legacy::<crate::notification::settings::NotificationSettings>(
        &notifications_path,
        "notifications.json",
        &mut report,
    ) {
        save_if_absent(
            db,
            NOTIFICATIONS,
            &value,
            &notifications_path,
            "notifications.json",
            &mut report,
        )
        .await?;
    }

    let log_path = paths.config_dir.join("log-settings.json");
    if let Some(value) = read_legacy::<LegacyLogConfig>(&log_path, "log-settings.json", &mut report) {
        match crate::logging::parse_level(&value.level) {
            Some(level) => {
                let normalized = crate::logging::level_to_str(level).to_string();
                save_if_absent(
                    db,
                    LOG_LEVEL,
                    &normalized,
                    &log_path,
                    "log-settings.json",
                    &mut report,
                )
                .await?;
            }
            None => report.kept.push("log-settings.json"),
        }
    }

    let debug_path = paths.config_dir.join("campus-network-debug.json");
    if let Some(value) =
        read_legacy::<bool>(&debug_path, "campus-network-debug.json", &mut report)
    {
        save_if_absent(
            db,
            DEBUG_ENABLED,
            &value,
            &debug_path,
            "campus-network-debug.json",
            &mut report,
        )
        .await?;
    }

    let mcp_path = paths.data_dir.join("mcp-config.json");
    if let Some(value) = read_legacy::<LegacyMcpConfig>(&mcp_path, "mcp-config.json", &mut report) {
        save_if_absent(
            db,
            MCP_ENABLED,
            &value.enabled,
            &mcp_path,
            "mcp-config.json",
            &mut report,
        )
        .await?;
    }

    Ok(report)
}

fn read_legacy<T: DeserializeOwned>(
    path: &Path,
    label: &'static str,
    report: &mut MigrationReport,
) -> Option<T> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            eprintln!("无法读取旧版设置 {}: {error}", path.display());
            report.kept.push(label);
            return None;
        }
    };
    match serde_json::from_str(&raw) {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("旧版设置 {} 格式无效，原文件已保留: {error}", path.display());
            report.kept.push(label);
            None
        }
    }
}

async fn save_if_absent<T: Serialize>(
    db: &SettingsDatabase,
    key: &str,
    value: &T,
    path: &Path,
    label: &'static str,
    report: &mut MigrationReport,
) -> Result<(), String> {
    if db.load(key).await?.is_some() {
        // DB 中的值更新（例如旧版本回退后又被新版本覆盖），旧文件直接视为过期。
        remove_legacy(path, label, report);
        return Ok(());
    }
    let raw = serde_json::to_string(value).map_err(|error| error.to_string())?;
    db.save(key, &raw).await?;
    report.imported.push(label);
    remove_legacy(path, label, report);
    Ok(())
}

fn remove_legacy(path: &Path, label: &'static str, report: &mut MigrationReport) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            eprintln!("旧版设置 {} 已导入，但无法删除原文件: {error}", path.display());
            report.kept.push(label);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open_db(dir: &Path) -> SettingsDatabase {
        SettingsDatabase::open(&dir.join("settings.sqlite"))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn imports_and_removes_all_legacy_json_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        let data = dir.path().join("data");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(
            config.join("appearance.json"),
            r##"{"mode":"dark","seed":"#123456","variant":"tonal-spot","seedSource":"user"}"##,
        )
        .unwrap();
        fs::write(
            config.join("notifications.json"),
            r#"{"enabled":true,"campusNetwork":false,"electricity":true,"enrollment":true,"oj":true,"uooc":false}"#,
        )
        .unwrap();
        fs::write(config.join("log-settings.json"), r#"{"level":"debug"}"#).unwrap();
        fs::write(config.join("campus-network-debug.json"), r#"true"#).unwrap();
        fs::write(data.join("mcp-config.json"), r#"{"enabled":true}"#).unwrap();

        let db = open_db(dir.path()).await;
        let report = migrate_legacy_json(
            &LegacySettingsPaths {
                config_dir: config.clone(),
                data_dir: data.clone(),
            },
            &db,
        )
        .await
        .unwrap();

        assert_eq!(report.imported.len(), 5);
        assert!(report.kept.is_empty());
        for path in [
            config.join("appearance.json"),
            config.join("notifications.json"),
            config.join("log-settings.json"),
            config.join("campus-network-debug.json"),
            data.join("mcp-config.json"),
        ] {
            assert!(!path.exists(), "{} 应该已删除", path.display());
        }

        let appearance: crate::appearance::Appearance =
            serde_json::from_str(&db.load(APPEARANCE).await.unwrap().expect("appearance row"))
                .unwrap();
        assert_eq!(appearance.seed, "#123456");
        assert_eq!(
            serde_json::from_str::<bool>(&db.load(MCP_ENABLED).await.unwrap().unwrap()).unwrap(),
            true
        );
        assert_eq!(
            serde_json::from_str::<String>(&db.load(LOG_LEVEL).await.unwrap().unwrap()).unwrap(),
            "DEBUG"
        );
        assert_eq!(
            serde_json::from_str::<bool>(&db.load(DEBUG_ENABLED).await.unwrap().unwrap()).unwrap(),
            true
        );
    }

    #[tokio::test]
    async fn keeps_corrupt_legacy_file_and_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        fs::create_dir_all(&config).unwrap();
        let path = config.join("appearance.json");
        fs::write(&path, "not json").unwrap();

        let db = open_db(dir.path()).await;
        let report = migrate_legacy_json(
            &LegacySettingsPaths {
                config_dir: config.clone(),
                data_dir: dir.path().join("data"),
            },
            &db,
        )
        .await
        .unwrap();

        assert!(report.kept.contains(&"appearance.json"));
        assert!(path.exists(), "损坏的旧设置必须保留");
        assert_eq!(db.load(APPEARANCE).await.unwrap(), None);
    }

    #[tokio::test]
    async fn existing_database_value_wins_and_stale_file_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config");
        fs::create_dir_all(&config).unwrap();
        let path = config.join("campus-network-debug.json");
        fs::write(&path, "false").unwrap();

        let db = open_db(dir.path()).await;
        db.save(DEBUG_ENABLED, "true").await.unwrap();
        migrate_legacy_json(
            &LegacySettingsPaths {
                config_dir: config.clone(),
                data_dir: dir.path().join("data"),
            },
            &db,
        )
        .await
        .unwrap();

        assert_eq!(db.load(DEBUG_ENABLED).await.unwrap().as_deref(), Some("true"));
        assert!(!path.exists());
    }
}
