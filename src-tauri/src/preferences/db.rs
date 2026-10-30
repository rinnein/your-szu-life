//! 非机密用户偏好与初始化设置进度的 SQLite 存储。
//!
//! 与 [`crate::settings`] 区分：`settings.rs` 负责加密凭据包的导出/导入，
//! 这里的 `settings.sqlite` 只保存外观、通知、MCP 开关、日志级别、调试开关
//! 与初始化设置步骤状态等非机密偏好。

use std::path::Path;

/// 单条偏好。`value_json` 保存该偏好序列化后的 JSON 文本。
#[derive(Debug, toasty::Model)]
pub(crate) struct PreferenceRecord {
    #[key]
    pub key: String,
    pub value_json: String,
    pub updated_at_ms: i64,
}

/// 初始化设置步骤状态。`id` 为步骤 id，`status` 为 `configured`/`skipped`。
#[derive(Debug, toasty::Model)]
pub(crate) struct OnboardingStepRecord {
    #[key]
    pub id: String,
    pub status: String,
    pub revision: i64,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub(crate) struct SettingsDatabase {
    pub(crate) db: toasty::Db,
}

impl SettingsDatabase {
    pub(crate) async fn open(db_path: &Path) -> Result<Self, String> {
        let db = crate::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(PreferenceRecord, OnboardingStepRecord),
            Some(&crate::migrations::preferences::SETTINGS_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    /// 读取一条偏好。只有“行不存在”返回 `None`，其它错误必须上抛，
    /// 避免把读取失败当成默认值再写回磁盘。
    pub(crate) async fn load(&self, key: &str) -> Result<Option<String>, String> {
        let mut db = self.db.clone();
        match PreferenceRecord::get_by_key(&mut db, key).await {
            Ok(row) => Ok(Some(row.value_json)),
            Err(error) if error.is_record_not_found() => Ok(None),
            Err(error) => Err(format!("读取设置失败 ({key}): {error}")),
        }
    }

    pub(crate) async fn save(&self, key: &str, value_json: &str) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match PreferenceRecord::get_by_key(&mut db, key).await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    value_json: value_json.to_owned(),
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("更新设置失败 ({key}): {error}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(PreferenceRecord {
                    key: key.to_owned(),
                    value_json: value_json.to_owned(),
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("创建设置失败 ({key}): {error}"))?;
            }
            Err(error) => {
                return Err(format!("读取设置失败，本次未写入 ({key}): {error}"));
            }
        }
        Ok(())
    }

    pub(crate) async fn onboarding_rows(
        &self,
    ) -> Result<Vec<(String, String, i64)>, String> {
        let mut db = self.db.clone();
        let rows = OnboardingStepRecord::all()
            .exec(&mut db)
            .await
            .map_err(|error| format!("读取初始化设置进度失败: {error}"))?;
        Ok(rows
            .into_iter()
            .map(|row| (row.id, row.status, row.revision))
            .collect())
    }

    #[cfg(test)]
    pub(crate) async fn onboarding_row(
        &self,
        id: &str,
    ) -> Result<Option<(String, i64)>, String> {
        let mut db = self.db.clone();
        match OnboardingStepRecord::get_by_id(&mut db, id).await {
            Ok(row) => Ok(Some((row.status, row.revision))),
            Err(error) if error.is_record_not_found() => Ok(None),
            Err(error) => Err(format!("读取初始化设置进度失败 ({id}): {error}")),
        }
    }

    pub(crate) async fn save_onboarding_step(
        &self,
        id: &str,
        status: &str,
        revision: i64,
    ) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match OnboardingStepRecord::get_by_id(&mut db, id).await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    status: status.to_owned(),
                    revision,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("更新初始化设置进度失败 ({id}): {error}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(OnboardingStepRecord {
                    id: id.to_owned(),
                    status: status.to_owned(),
                    revision,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|error| format!("创建初始化设置进度失败 ({id}): {error}"))?;
            }
            Err(error) => {
                return Err(format!("读取初始化设置进度失败，本次未写入 ({id}): {error}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preference_and_onboarding_rows_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = SettingsDatabase::open(&dir.path().join("settings.sqlite"))
            .await
            .unwrap();
        assert_eq!(db.load("appearance").await.unwrap(), None);
        db.save("appearance", r##"{"seed":"#112233"}"##)
            .await
            .unwrap();
        assert_eq!(
            db.load("appearance").await.unwrap().as_deref(),
            Some(r##"{"seed":"#112233"}"##)
        );

        db.save_onboarding_step("auth", "configured", 1)
            .await
            .unwrap();
        assert_eq!(
            db.onboarding_row("auth").await.unwrap(),
            Some(("configured".into(), 1))
        );
        db.save_onboarding_step("auth", "skipped", 2)
            .await
            .unwrap();
        assert_eq!(
            db.onboarding_row("auth").await.unwrap(),
            Some(("skipped".into(), 2))
        );
        assert_eq!(db.onboarding_rows().await.unwrap().len(), 1);
        assert_eq!(db.onboarding_row("missing").await.unwrap(), None);
    }
}
