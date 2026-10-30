use super::model::Cache;
use std::path::Path;

#[derive(Debug, toasty::Model)]
pub struct LmsStateRecord {
    #[key]
    pub id: String,
    /// 缓存快照 JSON（Cache 结构）
    pub payload_json: String,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub struct LmsDatabase {
    pub(crate) db: toasty::Db,
}

impl LmsDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = crate::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(LmsStateRecord),
            Some(&crate::migrations::lms::LMS_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    /// 读取缓存快照；首次使用（行不存在）返回默认值，其它错误必须上抛，
    /// 避免默认数据随后被写回磁盘覆盖真实状态。
    pub(crate) async fn load_cache(&self) -> Result<Cache, String> {
        let mut db = self.db.clone();
        match LmsStateRecord::get_by_id(&mut db, "current").await {
            Ok(row) => serde_json::from_str(&row.payload_json)
                .map_err(|_| "畅课本地缓存数据损坏".to_string()),
            Err(error) if error.is_record_not_found() => Ok(Cache::default()),
            Err(error) => {
                log::warn!("读取畅课本地缓存失败: {error}");
                Err("读取畅课本地缓存失败".into())
            }
        }
    }

    pub(crate) async fn save_cache(&self, cache: &Cache) -> Result<(), String> {
        let payload_json =
            serde_json::to_string(cache).map_err(|_| "畅课缓存序列化失败".to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match LmsStateRecord::get_by_id(&mut db, "current").await {
            Ok(mut existing) => {
                toasty::update!(existing { payload_json, updated_at_ms: now })
                    .exec(&mut db)
                    .await
                    .map_err(|e| format!("更新畅课缓存失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(LmsStateRecord {
                    id: "current".to_string(),
                    payload_json,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建畅课缓存失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取畅课缓存失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }
}
