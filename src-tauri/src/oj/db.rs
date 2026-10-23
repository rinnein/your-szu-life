use super::model::*;
use std::path::Path;

#[derive(Debug, toasty::Model)]
pub struct OjNoticeRecord {
    #[key]
    pub id: String,
    #[index]
    pub site_id: String,
    pub account: String,
    pub contest_id: String,
    pub title: String,
    pub message: String,
    pub timestamp: i64,
    pub read: bool,
}

#[derive(Debug, toasty::Model)]
pub struct OjStateRecord {
    #[key]
    pub id: String,
    pub version: i64,
    pub sites_json: String,
    pub drafts_json: String,
    pub known_json: String,
    pub uncertain_json: String,
    pub scheduled_json: String,
    pub companion_port: i64,
    pub preferences_json: String,
    pub companion_enabled: bool,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub struct OjDatabase {
    pub(crate) db: toasty::Db,
}

impl OjDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = crate::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(OjNoticeRecord, OjStateRecord),
            Some(&crate::migrations::oj::OJ_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    pub(crate) async fn load_data(&self) -> Result<Data, String> {
        let mut db = self.db.clone();
        let state_row = match OjStateRecord::get_by_id(&mut db, "current").await {
            Ok(row) => row,
            Err(_) => return Ok(Data::default()),
        };

        let sites: Vec<Site> = serde_json::from_str(&state_row.sites_json).unwrap_or_else(|_| defaults());
        let drafts: std::collections::BTreeMap<String, Draft> =
            serde_json::from_str(&state_row.drafts_json).unwrap_or_default();
        let known: std::collections::BTreeMap<String, Known> =
            serde_json::from_str(&state_row.known_json).unwrap_or_default();
        let uncertain: std::collections::BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&state_row.uncertain_json).unwrap_or_default();
        let scheduled: std::collections::BTreeMap<String, i32> =
            serde_json::from_str(&state_row.scheduled_json).unwrap_or_default();
        let preferences: std::collections::BTreeMap<String, Preferences> =
            serde_json::from_str(&state_row.preferences_json).unwrap_or_default();

        let notice_rows = match OjNoticeRecord::all().exec(&mut db).await {
            Ok(rows) => rows,
            Err(_) => vec![],
        };
        let mut notices: Vec<Notice> = notice_rows
            .into_iter()
            .map(|r| Notice {
                id: r.id,
                site_id: r.site_id,
                account: r.account,
                contest_id: r.contest_id,
                title: r.title,
                message: r.message,
                timestamp: r.timestamp,
                read: r.read,
            })
            .collect();
        notices.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        if notices.len() > 500 {
            notices.truncate(500);
        }

        Ok(Data {
            version: state_row.version as u8,
            sites,
            drafts,
            known,
            notices,
            uncertain,
            scheduled,
            companion_port: state_row.companion_port.max(1).min(65535) as u16,
            preferences,
            companion_enabled: state_row.companion_enabled,
        })
    }

    pub(crate) async fn save_data(&self, data: &Data) -> Result<(), String> {
        let sites_json = serde_json::to_string(&data.sites).unwrap_or_default();
        let drafts_json = serde_json::to_string(&data.drafts).unwrap_or_default();
        let known_json = serde_json::to_string(&data.known).unwrap_or_default();
        let uncertain_json = serde_json::to_string(&data.uncertain).unwrap_or_default();
        let scheduled_json = serde_json::to_string(&data.scheduled).unwrap_or_default();
        let preferences_json = serde_json::to_string(&data.preferences).unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();

        let mut db = self.db.clone();
        if let Ok(mut existing) = OjStateRecord::get_by_id(&mut db, "current").await {
            toasty::update!(existing {
                version: data.version as i64,
                sites_json,
                drafts_json,
                known_json,
                uncertain_json,
                scheduled_json,
                companion_port: data.companion_port as i64,
                preferences_json,
                companion_enabled: data.companion_enabled,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("更新 OJ 状态失败: {e}"))?;
        } else {
            toasty::create!(OjStateRecord {
                id: "current".to_string(),
                version: data.version as i64,
                sites_json,
                drafts_json,
                known_json,
                uncertain_json,
                scheduled_json,
                companion_port: data.companion_port as i64,
                preferences_json,
                companion_enabled: data.companion_enabled,
                updated_at_ms: now,
            })
            .exec(&mut db)
            .await
            .map_err(|e| format!("创建 OJ 状态失败: {e}"))?;
        }

        // 保存通知记录（避免每次全量替换全表，只插入新增的通知）
        for n in &data.notices {
            if OjNoticeRecord::get_by_id(&mut db, &n.id).await.is_err() {
                let _ = toasty::create!(OjNoticeRecord {
                    id: n.id.clone(),
                    site_id: n.site_id.clone(),
                    account: n.account.clone(),
                    contest_id: n.contest_id.clone(),
                    title: n.title.clone(),
                    message: n.message.clone(),
                    timestamp: n.timestamp,
                    read: n.read,
                })
                .exec(&mut db)
                .await;
            }
        }

        Ok(())
    }
}
