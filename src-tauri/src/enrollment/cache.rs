use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

#[derive(Debug, toasty::Model)]
pub struct EnrollmentSnapshot {
    #[key]
    pub id: String, // format!("{account}:{kind}:{scope}")
    #[index]
    pub account: String,
    pub kind: String,
    pub scope: String,
    pub payload: String,
    pub updated: i64,
}

pub struct Cache {
    db: toasty::Db,
}

impl Cache {
    pub fn open(path: &Path) -> Result<Self, String> {
        crate::migrations::db::run_sync(async {
            let db = crate::migrations::db::open_sqlite_db(
                path,
                toasty::models!(EnrollmentSnapshot),
                Some(&crate::migrations::enrollment::ENROLLMENT_MIGRATIONS),
            )
            .await?;
            Ok(Self { db })
        })
    }

    pub fn workspace(
        &self,
        account: &str,
        scope: &str,
        value: &impl Serialize,
    ) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "选课缓存序列化失败")?;
        for key in ["latest", scope] {
            self.put_raw(account, "workspace", key, &raw)?;
        }
        Ok(())
    }

    pub fn get<T: DeserializeOwned>(
        &self,
        account: &str,
        kind: &str,
        scope: &str,
    ) -> Result<Option<T>, String> {
        let key = format!("{account}:{kind}:{scope}");
        let mut db = self.db.clone();
        let raw = crate::migrations::db::run_sync(async {
            match EnrollmentSnapshot::get_by_id(&mut db, &key).await {
                Ok(row) => Ok(Some(row.payload)),
                Err(error) if error.is_record_not_found() => Ok(None),
                Err(error) => {
                    log::warn!("读取选课缓存失败: {error}");
                    Err("读取选课缓存失败，已保留磁盘上的原数据".to_string())
                }
            }
        })?;
        raw.map(|s| serde_json::from_str(&s).map_err(|_| "选课缓存损坏，未覆盖原数据".to_string()))
            .transpose()
    }

    pub fn catalog(
        &self,
        account: &str,
        context: &super::model::Context,
        query: &super::model::Query,
    ) -> Result<serde_json::Value, String> {
        use super::model::{Context, Course, Query};
        let mut db = self.db.clone();
        let rows = crate::migrations::db::run_sync(async {
            match EnrollmentSnapshot::filter_by_account(account)
                .exec(&mut db)
                .await
            {
                Ok(list) => {
                    let mut filtered: Vec<EnrollmentSnapshot> = list
                        .into_iter()
                        .filter(|r| r.kind == "catalog")
                        .collect();
                    filtered.sort_by(|a, b| b.updated.cmp(&a.updated));
                    Ok(filtered)
                }
                Err(e) => Err(format!("无法读取缓存目录: {e}")),
            }
        })?;

        let mut courses = Vec::<Course>::new();
        let mut seen = std::collections::HashSet::new();
        let mut updated: Option<i64> = None;
        let mut pages = 0;
        for row in rows {
            let scope = row.scope;
            let raw = row.payload;
            let Ok((stored, stored_query)) = serde_json::from_str::<(Context, Query)>(&scope)
            else {
                continue;
            };
            if &stored != context || stored_query.category != query.category {
                continue;
            }
            let (page, _, at) = serde_json::from_str::<(Vec<Course>, bool, i64)>(&raw)
                .map_err(|_| "选课缓存损坏")?;
            pages += 1;
            updated = Some(updated.unwrap_or(0).max(at));
            for course in page {
                if seen.insert(course.id.clone()) {
                    courses.push(course);
                }
            }
        }
        let keyword = query.keyword.trim().to_lowercase();
        courses.retain(|c| {
            keyword.is_empty()
                || c.code.to_lowercase().contains(&keyword)
                || c.name.to_lowercase().contains(&keyword)
        });
        courses.sort_by(|a, b| a.code.cmp(&b.code).then(a.id.cmp(&b.id)));
        let total = courses.len();
        let start = (query.page as usize).saturating_mul(10);
        let page: Vec<_> = courses.into_iter().skip(start).take(10).collect();
        Ok(
            serde_json::json!({"courses":page,"total":total,"complete":start+10>=total,"updatedAt":updated,"cachedPages":pages,"query":query,"coverage":"partial","context":context}),
        )
    }

    fn put_raw(
        &self,
        account: &str,
        kind: &str,
        scope: &str,
        raw: &str,
    ) -> Result<(), String> {
        let key = format!("{account}:{kind}:{scope}");
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        crate::migrations::db::run_sync(async {
            match EnrollmentSnapshot::get_by_id(&mut db, &key).await {
                Ok(mut existing) => {
                    toasty::update!(existing {
                        payload: raw.to_string(),
                        updated: now,
                    })
                    .exec(&mut db)
                    .await
                    .map_err(|e| format!("无法保存选课缓存: {e}"))?;
                }
                Err(error) if error.is_record_not_found() => {
                    toasty::create!(EnrollmentSnapshot {
                        id: key,
                        account: account.to_string(),
                        kind: kind.to_string(),
                        scope: scope.to_string(),
                        payload: raw.to_string(),
                        updated: now,
                    })
                    .exec(&mut db)
                    .await
                    .map_err(|e| format!("无法保存选课缓存: {e}"))?;
                }
                Err(error) => {
                    return Err(format!("读取选课缓存失败，本次未写入: {error}"));
                }
            }
            Ok(())
        })
    }

    pub fn put(
        &self,
        account: &str,
        kind: &str,
        scope: &str,
        value: &impl Serialize,
    ) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "选课缓存序列化失败")?;
        self.put_raw(account, kind, scope, &raw)
    }
}
