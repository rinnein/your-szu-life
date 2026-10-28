use super::model::SavedState;
use serde::de::DeserializeOwned;
use std::path::Path;

/// UOOC 运行状态模型 (Toasty Model)
///
/// 状态整体以单行快照保存：任务、测验与讨论数据都随登录态自动重建，
/// 因此只需保证一次原子写入，无需为每个字段建立关系表。
#[derive(Debug, toasty::Model)]
pub struct UoocStateRecord {
    #[key]
    pub id: String,
    pub settings_json: String,
    pub tasks_json: String,
    pub quizzes_json: String,
    pub discussion_json: String,
    pub qa_api_key: Option<String>,
    pub qa_bootstrapped_json: String,
    pub updated_at_ms: i64,
}

/// 同步封装的 Toasty 数据库，供 `Service` 在持有运行时锁时直接读写。
#[derive(Clone)]
pub(super) struct UoocStateDatabase {
    db: toasty::Db,
}

impl UoocStateDatabase {
    pub(super) fn open(db_path: &Path) -> Result<Self, String> {
        let db = crate::migrations::db::run_sync(async {
            crate::migrations::db::open_sqlite_db(
                db_path,
                toasty::models!(UoocStateRecord),
                Some(&crate::migrations::uooc::UOOC_STATE_MIGRATIONS),
            )
            .await
        })?;
        Ok(Self { db })
    }

    pub(super) fn load(&self) -> Result<Option<SavedState>, String> {
        let mut db = self.db.clone();
        crate::migrations::db::run_sync(async {
            let row = match UoocStateRecord::get_by_id(&mut db, "current").await {
                Ok(row) => row,
                Err(error) if error.is_record_not_found() => return Ok(None),
                Err(error) => {
                    log::warn!("读取 UOOC 本地状态失败: {error}");
                    return Err("读取 UOOC 本地状态失败，已保留磁盘上的原有数据".into());
                }
            };
            Ok(Some(SavedState {
                settings: parse(&row.settings_json)?,
                tasks: parse(&row.tasks_json)?,
                quizzes: parse(&row.quizzes_json)?,
                discussion: parse(&row.discussion_json)?,
                qa_api_key: row.qa_api_key,
                initial_qa_bootstrapped_accounts: parse(&row.qa_bootstrapped_json)?,
            }))
        })
    }

    pub(super) fn save(&self, state: &SavedState) -> Result<(), String> {
        let settings_json = serialize(&state.settings)?;
        let tasks_json = serialize(&state.tasks)?;
        let quizzes_json = serialize(&state.quizzes)?;
        let discussion_json = serialize(&state.discussion)?;
        let qa_api_key = state.qa_api_key.clone();
        let qa_bootstrapped_json = serialize(&state.initial_qa_bootstrapped_accounts)?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        crate::migrations::db::run_sync(async {
            match UoocStateRecord::get_by_id(&mut db, "current").await {
                Ok(mut existing) => toasty::update!(existing {
                    settings_json,
                    tasks_json,
                    quizzes_json,
                    discussion_json,
                    qa_api_key,
                    qa_bootstrapped_json,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新 UOOC 状态失败: {e}"))?,
                Err(error) if error.is_record_not_found() => {
                    toasty::create!(UoocStateRecord {
                        id: "current".to_string(),
                        settings_json,
                        tasks_json,
                        quizzes_json,
                        discussion_json,
                        qa_api_key,
                        qa_bootstrapped_json,
                        updated_at_ms: now,
                    })
                    .exec(&mut db)
                    .await
                    .map_err(|e| format!("创建 UOOC 状态失败: {e}"))?;
                }
                Err(error) => {
                    // 读不出旧行时不要盲目 create：否则会撞主键，或在异常情况下
                    // 把一份默认状态写到已有数据之上。
                    return Err(format!("读取 UOOC 状态失败，本次未写入: {error}"));
                }
            }
            Ok(())
        })
    }
}

fn parse<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|_| "UOOC 本地数据损坏".into())
}

fn serialize(value: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|_| "UOOC 数据序列化失败".into())
}
