use crate::campus_payments::electricity::db::ElectricityDatabase;
use crate::campus_payments::electricity::history::Room;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use toasty::migration::{MigrationFile, MigrationSet};

pub static ELECTRICITY_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    1,
    "0001_init_electricity.sql",
    r#"
    CREATE TABLE IF NOT EXISTS electricity_room_records (
        id TEXT PRIMARY KEY,
        location_json TEXT NOT NULL,
        readings_json TEXT NOT NULL,
        recharges_json TEXT NOT NULL,
        usage_checks_json TEXT NOT NULL,
        recharge_checks_json TEXT NOT NULL,
        balance REAL,
        balance_date TEXT,
        usage_updated_at INTEGER,
        recharge_updated_at INTEGER,
        retry_after INTEGER NOT NULL,
        error TEXT,
        recharge_error TEXT,
        recharge_retry_after INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    "#,
)]);

#[derive(Default, Deserialize, Serialize)]
struct LegacyDiskCache {
    version: u8,
    rooms: Vec<Room>,
}

/// 检查并迁移遗留的电费 JSON 历史文件 (electricity-history-v1.json / history.json -> electricity.sqlite)
pub async fn migrate_electricity_history_if_needed(
    db_path: &Path,
    db: &ElectricityDatabase,
) -> Result<Vec<Room>, String> {
    let Some(parent) = db_path.parent() else {
        return Ok(vec![]);
    };

    let mut legacy_rooms = Vec::new();
    for old_name in &["electricity-history-v1.json", "history.json"] {
        let old_file = parent.join(old_name);
        if old_file.exists() && old_file != db_path {
            if let Ok(bytes) = fs::read(&old_file) {
                if let Ok(disk) = serde_json::from_slice::<LegacyDiskCache>(&bytes) {
                    if disk.version == 1 {
                        legacy_rooms = disk.rooms;
                    }
                }
            }
            let _ = fs::remove_file(old_file);
        }
    }

    if !legacy_rooms.is_empty() {
        for r in &legacy_rooms {
            let _ = db.save_room(r).await;
        }
    }

    Ok(legacy_rooms)
}
