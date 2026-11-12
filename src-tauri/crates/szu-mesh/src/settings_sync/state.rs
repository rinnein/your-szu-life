//! 同步状态的本地数据库。
//!
//! 只保存「怎么合并」所需的元数据：整项配置的版本向量与内容哈希、题库每一行的
//! 基线哈希、以及冲突冻结标记。真正的配置值仍然留在各自的存储里（vault、
//! `uooc/state.sqlite`、题库数据库），这里绝不复制一份明文。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, toasty::Model)]
pub struct SyncItemRecord {
    #[key]
    pub item_id: String,
    pub vector_json: String,
    pub content_hash: String,
    pub conflict: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, toasty::Model)]
pub struct SyncRowRecord {
    #[key]
    pub row_key: String,
    pub content_hash: String,
    pub conflict: bool,
    pub updated_at_ms: i64,
}

/// 每个设备一个计数器；比较两个向量即可判断「谁改过、是否并发」。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct VersionVector(pub BTreeMap<String, u64>);

impl VersionVector {
    pub fn bump(&mut self, device: &str) -> u64 {
        let next = self.0.get(device).copied().unwrap_or(0).saturating_add(1);
        self.0.insert(device.to_owned(), next);
        next
    }

    /// 自身是否覆盖对方（逐项 `>=`）。相等时两边互相覆盖。
    pub fn covers(&self, other: &Self) -> bool {
        other
            .0
            .iter()
            .all(|(device, value)| self.0.get(device).copied().unwrap_or(0) >= *value)
    }

    pub fn merge(&mut self, other: &Self) {
        for (device, value) in &other.0 {
            let entry = self.0.entry(device.clone()).or_insert(0);
            if *value > *entry {
                *entry = *value;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemState {
    pub vector: VersionVector,
    pub hash: String,
    pub conflict: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowState {
    pub hash: String,
    pub conflict: bool,
}

#[derive(Clone)]
pub struct SyncDatabase {
    db: toasty::Db,
}

impl SyncDatabase {
    pub async fn open(root: &Path) -> Result<Self, String> {
        let db = szu_core::migrations::db::open_sqlite_db(
            &root.join("sync.sqlite"),
            toasty::models!(SyncItemRecord, SyncRowRecord),
            Some(&crate::mesh::migrations::MESH_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    pub async fn load_items(&self) -> Result<BTreeMap<String, ItemState>, String> {
        let mut db = self.db.clone();
        let rows = SyncItemRecord::all()
            .exec(&mut db)
            .await
            .map_err(|error| format!("读取同步项状态失败: {error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let vector = serde_json::from_str(&row.vector_json).unwrap_or_default();
                (
                    row.item_id,
                    ItemState {
                        vector,
                        hash: row.content_hash,
                        conflict: row.conflict,
                    },
                )
            })
            .collect())
    }

    pub async fn save_item(&self, item_id: &str, state: &ItemState) -> Result<(), String> {
        let vector_json = serde_json::to_string(&state.vector)
            .map_err(|error| format!("同步向量序列化失败: {error}"))?;
        let now = now_ms();
        let mut db = self.db.clone();
        match SyncItemRecord::get_by_item_id(&mut db, item_id).await {
            Ok(mut existing) => toasty::update!(existing {
                vector_json,
                content_hash: state.hash.clone(),
                conflict: state.conflict,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|error| format!("更新同步项状态失败: {error}"))?,
            Err(error) if error.is_record_not_found() => {
                toasty::create!(SyncItemRecord {
                    item_id: item_id.to_owned(),
                    vector_json,
                    content_hash: state.hash.clone(),
                    conflict: state.conflict,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("创建同步项状态失败: {error}"))?;
            }
            Err(error) => return Err(format!("读取同步项状态失败: {error}")),
        }
        Ok(())
    }

    pub async fn load_rows(&self) -> Result<BTreeMap<String, RowState>, String> {
        let mut db = self.db.clone();
        let rows = SyncRowRecord::all()
            .exec(&mut db)
            .await
            .map_err(|error| format!("读取题库同步状态失败: {error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    row.row_key,
                    RowState {
                        hash: row.content_hash,
                        conflict: row.conflict,
                    },
                )
            })
            .collect())
    }

    pub async fn save_row(&self, row_key: &str, state: &RowState) -> Result<(), String> {
        let now = now_ms();
        let mut db = self.db.clone();
        match SyncRowRecord::get_by_row_key(&mut db, row_key).await {
            Ok(mut existing) => toasty::update!(existing {
                content_hash: state.hash.clone(),
                conflict: state.conflict,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|error| format!("更新题库同步状态失败: {error}"))?,
            Err(error) if error.is_record_not_found() => {
                toasty::create!(SyncRowRecord {
                    row_key: row_key.to_owned(),
                    content_hash: state.hash.clone(),
                    conflict: state.conflict,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("创建题库同步状态失败: {error}"))?;
            }
            Err(error) => return Err(format!("读取题库同步状态失败: {error}")),
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 迁移建出的表名必须与 toasty 模型派生的表名一致（sync_item_records /
    // sync_row_records），否则所有读写都会以 "no such table" 失败。
    #[tokio::test]
    async fn sync_database_round_trips_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = SyncDatabase::open(dir.path()).await.unwrap();

        let mut vector = VersionVector::default();
        vector.bump("device-a");
        db.save_item(
            "uooc.session",
            &ItemState {
                vector: vector.clone(),
                hash: "hash-a".into(),
                conflict: false,
            },
        )
        .await
        .unwrap();
        db.save_row(
            "31:1",
            &RowState {
                hash: "row-a".into(),
                conflict: false,
            },
        )
        .await
        .unwrap();

        // 更新既有行，覆盖 upsert 的 update 分支。
        vector.bump("device-a");
        db.save_item(
            "uooc.session",
            &ItemState {
                vector: vector.clone(),
                hash: "hash-b".into(),
                conflict: true,
            },
        )
        .await
        .unwrap();

        let items = db.load_items().await.unwrap();
        let rows = db.load_rows().await.unwrap();
        assert_eq!(
            items.get("uooc.session"),
            Some(&ItemState {
                vector,
                hash: "hash-b".into(),
                conflict: true,
            })
        );
        assert_eq!(
            rows.get("31:1"),
            Some(&RowState {
                hash: "row-a".into(),
                conflict: false,
            })
        );
    }

    #[test]
    fn vector_bumps_are_per_device() {
        let mut vector = VersionVector::default();
        assert_eq!(vector.bump("a"), 1);
        assert_eq!(vector.bump("a"), 2);
        assert_eq!(vector.bump("b"), 1);
        assert_eq!(vector.0.get("a"), Some(&2));
        assert_eq!(vector.0.get("b"), Some(&1));
    }

    #[test]
    fn covers_requires_every_counter() {
        let mut a = VersionVector::default();
        a.bump("a");
        let mut b = a.clone();
        assert!(b.covers(&a));
        assert!(a.covers(&b));
        b.bump("b");
        assert!(b.covers(&a));
        assert!(!a.covers(&b));
    }

    #[test]
    fn merge_keeps_the_maximum() {
        let mut a = VersionVector::default();
        a.bump("a");
        a.bump("a");
        let mut b = VersionVector::default();
        b.bump("b");
        a.merge(&b);
        assert_eq!(a.0.get("a"), Some(&2));
        assert_eq!(a.0.get("b"), Some(&1));
    }
}
