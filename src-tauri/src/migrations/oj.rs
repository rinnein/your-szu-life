use crate::oj::db::OjDatabase;
use crate::oj::model::Data;
use std::path::Path;
use toasty::migration::{MigrationFile, MigrationSet};

pub static OJ_MIGRATIONS: MigrationSet = MigrationSet::new(&[
    MigrationFile::new(
        1,
        "0001_init_oj.sql",
        "CREATE TABLE IF NOT EXISTS oj_notice_records (
        id TEXT PRIMARY KEY,
        site_id TEXT NOT NULL,
        account TEXT NOT NULL,
        contest_id TEXT NOT NULL,
        title TEXT NOT NULL,
        message TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        read INTEGER NOT NULL
    );
-- #[toasty::breakpoint]
    CREATE INDEX IF NOT EXISTS idx_oj_notice_site_id ON oj_notice_records(site_id);
-- #[toasty::breakpoint]
    CREATE TABLE IF NOT EXISTS oj_state_records (
        id TEXT PRIMARY KEY,
        version INTEGER NOT NULL,
        sites_json TEXT NOT NULL,
        drafts_json TEXT NOT NULL,
        known_json TEXT NOT NULL,
        uncertain_json TEXT NOT NULL,
        scheduled_json TEXT NOT NULL,
        companion_port INTEGER NOT NULL,
        preferences_json TEXT NOT NULL,
        companion_enabled INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );",
    ),
    // 班级/测验列表缓存：避免每次打开 OJ 页面都重新请求网络。
    MigrationFile::new(
        2,
        "0002_oj_groups_cache.sql",
        "ALTER TABLE oj_state_records ADD COLUMN groups_json TEXT NOT NULL DEFAULT '{}';
-- #[toasty::breakpoint]
    ALTER TABLE oj_state_records ADD COLUMN contests_json TEXT NOT NULL DEFAULT '{}';",
    ),
]);

/// 检查并迁移遗留的 OJ JSON 状态文件 (state.json -> oj.sqlite)
pub async fn migrate_oj_state_if_needed(
    db_path: &Path,
    db: &OjDatabase,
) -> Result<Option<Data>, String> {
    let legacy_json = if db_path.extension().is_some_and(|ext| ext == "json") {
        db_path.to_path_buf()
    } else {
        db_path.with_extension("json")
    };

    if !legacy_json.exists() {
        return Ok(None);
    }

    let bytes = match std::fs::read(&legacy_json) {
        Ok(b) => b,
        Err(_) => return Ok(None),
    };

    if bytes.len() > 32 * 1024 * 1024 {
        return Err("OJ 本地数据过大".into());
    }

    let data = match serde_json::from_slice::<Data>(&bytes) {
        Ok(d) if d.version == 1 => d,
        Ok(_) => return Err("OJ 本地数据版本不兼容".into()),
        Err(_) => return Ok(None),
    };

    let _ = db.save_data(&data).await;
    let _ = std::fs::remove_file(&legacy_json);

    Ok(Some(data))
}

/// 清理遗留的 OJ JSON 状态文件
pub fn cleanup_legacy_oj_files(db_path: &Path) {
    let legacy_json = db_path.with_extension("json");
    if legacy_json.exists() {
        let _ = std::fs::remove_file(legacy_json);
    }
}
