//! 巡课乐观缓存：学期/课程/课次/视频信息的 SQLite KV 持久层（仿 lms/db.rs）。
//!
//! 读取永远立即返回缓存（无论多旧），后台按条目各自的 TTL 重验证；
//! 只有缓存完全不存在时才阻塞首拉。

use std::path::Path;

#[derive(Debug, toasty::Model)]
pub struct JxxkCacheRecord {
    #[key]
    pub id: String,
    /// 缓存条目 JSON（Semester 数组 / CoursePage / LessonPage / LessonStreams）
    pub payload_json: String,
    pub updated_at_ms: i64,
}

pub struct JxxkDatabase {
    pub db: toasty::Db,
    /// 序列化写操作：get-then-create 在并发下会触发 UNIQUE 约束冲突。
    save_lock: tokio::sync::Mutex<()>,
}

impl Clone for JxxkDatabase {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            save_lock: tokio::sync::Mutex::new(()),
        }
    }
}

/// 单条缓存条目。
pub struct CacheEntry {
    pub id: String,
    pub payload_json: String,
    pub updated_at_ms: i64,
}

impl JxxkDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = szu_core::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(JxxkCacheRecord),
            Some(&crate::jxxk::migrations::JXXK_MIGRATIONS),
        )
        .await?;
        Ok(Self {
            db,
            save_lock: tokio::sync::Mutex::new(()),
        })
    }

    /// 读取全部缓存条目；库不存在/为空时返回空表，其它错误必须上抛，
    /// 避免空数据随后被写回磁盘覆盖真实缓存。
    pub async fn load_all(&self) -> Result<Vec<CacheEntry>, String> {
        let mut db = self.db.clone();
        let rows = JxxkCacheRecord::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("读取巡课本地缓存失败: {e}"))?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(CacheEntry {
                id: row.id,
                payload_json: row.payload_json,
                updated_at_ms: row.updated_at_ms,
            });
        }
        Ok(entries)
    }

    pub async fn save(&self, entry: &CacheEntry) -> Result<(), String> {
        // 序列化并发写：get-then-create 非原子。
        let _guard = self.save_lock.lock().await;
        let mut db = self.db.clone();
        match JxxkCacheRecord::get_by_id(&mut db, &entry.id).await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    payload_json: entry.payload_json.clone(),
                    updated_at_ms: entry.updated_at_ms,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新巡课缓存失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(JxxkCacheRecord {
                    id: entry.id.clone(),
                    payload_json: entry.payload_json.clone(),
                    updated_at_ms: entry.updated_at_ms,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建巡课缓存失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取巡课缓存失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }

    /// 删除缓存条目（任务记录移除时调用）。
    pub async fn delete(&self, id: &str) -> Result<(), String> {
        let mut db = self.db.clone();
        match JxxkCacheRecord::get_by_id(&mut db, id).await {
            Ok(row) => {
                row.delete().exec(&mut db).await.map_err(|e| {
                    format!("删除巡课缓存条目失败: {e}")
                })?;
            }
            Err(error) if error.is_record_not_found() => {}
            Err(error) => return Err(format!("读取巡课缓存条目失败: {error}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        static COUNTER: Mutex<u32> = Mutex::new(0);
        let mut counter = COUNTER.lock().unwrap();
        *counter += 1;
        std::env::temp_dir().join(format!(
            "jxxk-cache-{}-{}-{tag}.sqlite",
            std::process::id(),
            *counter
        ))
    }

    use std::sync::Mutex;

    #[tokio::test]
    async fn roundtrip_and_reopen() {
        let path = temp_db_path("roundtrip");
        let db = JxxkDatabase::open(&path).await.unwrap();
        assert!(db.load_all().await.unwrap().is_empty());

        db.save(&CacheEntry {
            id: "semesters".into(),
            payload_json: r#"[{"id":"sem-1","name":"2026-2027-1"}]"#.into(),
            updated_at_ms: 111,
        })
        .await
        .unwrap();
        db.save(&CacheEntry {
            id: "lessons:c1".into(),
            payload_json: "{\"recordCount\":3}".into(),
            updated_at_ms: 222,
        })
        .await
        .unwrap();

        let entries = db.load_all().await.unwrap();
        assert_eq!(entries.len(), 2);

        // 更新已有条目
        db.save(&CacheEntry {
            id: "semesters".into(),
            payload_json: "[]".into(),
            updated_at_ms: 333,
        })
        .await
        .unwrap();
        let entries = db.load_all().await.unwrap();
        let semesters = entries.iter().find(|e| e.id == "semesters").unwrap();
        assert_eq!(semesters.payload_json, "[]");
        assert_eq!(semesters.updated_at_ms, 333);

        // 重开数据库后数据仍在
        let reopened = JxxkDatabase::open(&path).await.unwrap();
        let entries = reopened.load_all().await.unwrap();
        assert_eq!(entries.len(), 2);
    }
}
