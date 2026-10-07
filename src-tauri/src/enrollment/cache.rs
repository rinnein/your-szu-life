use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use std::path::Path;

pub struct Cache(Connection);
impl Cache {
    pub fn open(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|_| "无法打开选课缓存")?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS snapshots (account TEXT NOT NULL, kind TEXT NOT NULL, scope TEXT NOT NULL, payload TEXT NOT NULL, updated INTEGER NOT NULL, PRIMARY KEY(account,kind,scope)); PRAGMA user_version=1;").map_err(|_| "无法初始化选课缓存")?;
        Ok(Self(db))
    }
    pub fn workspace(
        &self,
        account: &str,
        scope: &str,
        value: &impl Serialize,
    ) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "选课缓存序列化失败")?;
        let transaction = self
            .0
            .unchecked_transaction()
            .map_err(|_| "无法开始缓存事务")?;
        for key in ["latest", scope] {
            transaction.execute("INSERT INTO snapshots VALUES (?1,'workspace',?2,?3,?4) ON CONFLICT(account,kind,scope) DO UPDATE SET payload=excluded.payload,updated=excluded.updated",params![account,key,raw,chrono::Utc::now().timestamp_millis()]).map_err(|_|"无法保存选课事务")?;
        }
        transaction
            .commit()
            .map_err(|_| "无法提交选课缓存事务".into())
    }
    pub fn get<T: DeserializeOwned>(
        &self,
        account: &str,
        kind: &str,
        scope: &str,
    ) -> Result<Option<T>, String> {
        let raw: Option<String> = self
            .0
            .query_row(
                "SELECT payload FROM snapshots WHERE account=?1 AND kind=?2 AND scope=?3",
                params![account, kind, scope],
                |r| r.get(0),
            )
            .optional()
            .map_err(|_| "无法读取选课缓存")?;
        raw.map(|s| serde_json::from_str(&s).map_err(|_| "选课缓存损坏，未覆盖原数据".into()))
            .transpose()
    }
    pub fn catalog(
        &self,
        account: &str,
        context: &super::model::Context,
        query: &super::model::Query,
    ) -> Result<serde_json::Value, String> {
        use super::model::{Context, Course, Query};
        let mut statement=self.0.prepare("SELECT scope,payload FROM snapshots WHERE account=?1 AND kind='catalog' ORDER BY updated DESC").map_err(|_|"无法读取缓存目录")?;
        let rows = statement
            .query_map([account], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|_| "无法读取缓存目录")?;
        let mut courses = Vec::<Course>::new();
        let mut seen = std::collections::HashSet::new();
        let mut updated: Option<i64> = None;
        let mut pages = 0;
        for row in rows {
            let (scope, raw) = row.map_err(|_| "缓存目录读取失败")?;
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
    pub fn put(
        &self,
        account: &str,
        kind: &str,
        scope: &str,
        value: &impl Serialize,
    ) -> Result<(), String> {
        let raw = serde_json::to_string(value).map_err(|_| "选课缓存序列化失败")?;
        self.0.execute("INSERT INTO snapshots VALUES (?1,?2,?3,?4,?5) ON CONFLICT(account,kind,scope) DO UPDATE SET payload=excluded.payload,updated=excluded.updated", params![account,kind,scope,raw,chrono::Utc::now().timestamp_millis()]).map_err(|_| "无法保存选课缓存")?;
        Ok(())
    }
}
