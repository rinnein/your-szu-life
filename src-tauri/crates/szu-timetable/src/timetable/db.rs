use std::path::Path;

use super::model::ReminderState;

#[derive(Debug, toasty::Model)]
pub struct TimetableStateRecord {
    #[key]
    pub id: String,
    /// 提醒/同步状态（ReminderState JSON）
    pub payload_json: String,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub struct TimetableDatabase {
    pub db: toasty::Db,
}

impl TimetableDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = szu_core::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(TimetableStateRecord),
            Some(&crate::timetable::migrations::TIMETABLE_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    /// 读取状态；首次使用返回默认值，其它错误必须上抛，避免默认值覆盖真实记录。
    pub async fn load_state(&self) -> Result<ReminderState, String> {
        let mut db = self.db.clone();
        match TimetableStateRecord::get_by_id(&mut db, "current").await {
            Ok(row) => serde_json::from_str(&row.payload_json)
                .map_err(|_| "课表提醒状态数据损坏".to_string()),
            Err(error) if error.is_record_not_found() => Ok(ReminderState::default()),
            Err(error) => {
                log::warn!("读取课表提醒状态失败: {error}");
                Err(format!("读取课表提醒状态失败: {error}"))
            }
        }
    }

    pub async fn save_state(&self, state: &ReminderState) -> Result<(), String> {
        let payload_json =
            serde_json::to_string(state).map_err(|_| "课表提醒状态序列化失败".to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match TimetableStateRecord::get_by_id(&mut db, "current").await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    payload_json,
                    updated_at_ms: now
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新课表提醒状态失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(TimetableStateRecord {
                    id: "current".to_string(),
                    payload_json,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建课表提醒状态失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取课表提醒状态失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }
}
