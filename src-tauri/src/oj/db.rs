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
    /// 班级/测验缓存（迁移 2 新增）。
    pub groups_json: String,
    pub contests_json: String,
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
            // 只有“这一行还不存在”才是首次使用；其它错误必须上抛，
            // 否则默认数据会被随后的 save_data 写回磁盘。
            Err(error) if error.is_record_not_found() => return Ok(Data::default()),
            Err(error) => {
                log::warn!("读取 OJ 本地状态失败: {error}");
                return Err("读取 OJ 本地状态失败，已保留磁盘上的原有数据".into());
            }
        };

        let sites: Vec<Site> = serde_json::from_str(&state_row.sites_json)
            .map_err(|_| "OJ 站点数据损坏，已保留磁盘上的原有数据")?;
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
        let groups: std::collections::BTreeMap<String, GroupCache> =
            serde_json::from_str(&state_row.groups_json).unwrap_or_default();
        let contests: std::collections::BTreeMap<String, ContestCache> =
            serde_json::from_str(&state_row.contests_json).unwrap_or_default();

        let notice_rows = OjNoticeRecord::all()
            .exec(&mut db)
            .await
            .map_err(|error| {
                log::warn!("读取 OJ 通知失败: {error}");
                "读取 OJ 通知失败，已保留磁盘上的原有数据".to_string()
            })?;
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
            groups,
            contests,
        })
    }

    pub(crate) async fn save_data(&self, data: &Data) -> Result<(), String> {
        let sites_json = serde_json::to_string(&data.sites).unwrap_or_default();
        let drafts_json = serde_json::to_string(&data.drafts).unwrap_or_default();
        let known_json = serde_json::to_string(&data.known).unwrap_or_default();
        let uncertain_json = serde_json::to_string(&data.uncertain).unwrap_or_default();
        let scheduled_json = serde_json::to_string(&data.scheduled).unwrap_or_default();
        let preferences_json = serde_json::to_string(&data.preferences).unwrap_or_default();
        let groups_json = serde_json::to_string(&data.groups).unwrap_or_default();
        let contests_json = serde_json::to_string(&data.contests).unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();

        let mut db = self.db.clone();
        match OjStateRecord::get_by_id(&mut db, "current").await {
            Ok(mut existing) => {
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
                    groups_json,
                    contests_json,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新 OJ 状态失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
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
                    groups_json,
                    contests_json,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建 OJ 状态失败: {e}"))?;
            }
            Err(error) => {
                // 读不出旧行时不要盲目 create，避免把默认状态写到已有数据之上。
                return Err(format!("读取 OJ 状态失败，本次未写入: {error}"));
            }
        }

        // 通知按 id 增量同步：已存在的行要更新 read，站点删除后残留的行要清理。
        let site_ids: std::collections::BTreeSet<&str> =
            data.sites.iter().map(|site| site.id.as_str()).collect();
        for n in &data.notices {
            match OjNoticeRecord::get_by_id(&mut db, &n.id).await {
                Ok(mut existing) => {
                    toasty::update!(existing { read: n.read })
                        .exec(&mut db)
                        .await
                        .map_err(|e| format!("更新 OJ 通知状态失败: {e}"))?;
                }
                Err(error) if error.is_record_not_found() => {
                    toasty::create!(OjNoticeRecord {
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
                    .await
                    .map_err(|e| format!("创建 OJ 通知失败: {e}"))?;
                }
                Err(error) => {
                    return Err(format!("读取 OJ 通知失败，本次未写入: {error}"));
                }
            }
        }
        for row in OjNoticeRecord::all()
            .exec(&mut db)
            .await
            .map_err(|e| format!("读取 OJ 通知失败: {e}"))?
        {
            if !site_ids.contains(row.site_id.as_str()) {
                let _ = row.delete().exec(&mut db).await;
            }
        }

        Ok(())
    }
}
