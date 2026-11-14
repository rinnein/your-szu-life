//! 签到模块本地存储：课表缓存 + 按课程记忆（上次成功定位 / 上次照片）。

use std::path::Path;
use super::model::{LastPoint, TimetableCache};

#[derive(Debug, toasty::Model)]
pub struct TimetableCacheRecord {
    #[key]
    pub id: String,
    /// TimetableCache 整体 JSON。
    pub payload_json: String,
    pub updated_at_ms: i64,
}

#[derive(Debug, toasty::Model)]
pub struct RollcallMemoryRecord {
    /// 课程键：LMS 用 `{course_id}`，SZU 用 `{course_name}`。
    #[key]
    pub course_key: String,
    /// LastPoint JSON；空串表示没有成功记录。
    pub last_point_json: String,
    /// 上次签到的照片（base64 JPEG）；空串表示没有。
    pub last_photo_b64: String,
    pub last_photo_mime: String,
    pub updated_at_ms: i64,
}

#[derive(Clone)]
pub struct SigninDatabase {
    pub db: toasty::Db,
}

impl SigninDatabase {
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let db = szu_core::migrations::db::open_sqlite_db(
            db_path,
            toasty::models!(TimetableCacheRecord, RollcallMemoryRecord),
            Some(&crate::signin::migrations::SIGNIN_MIGRATIONS),
        )
        .await?;
        Ok(Self { db })
    }

    /// 读取课表缓存；首次使用（行不存在）返回默认值，其它错误必须上抛，
    /// 避免默认数据随后被写回磁盘覆盖真实状态。
    pub async fn load_timetable(&self) -> Result<TimetableCache, String> {
        let mut db = self.db.clone();
        match TimetableCacheRecord::get_by_id(&mut db, "current").await {
            Ok(row) => serde_json::from_str(&row.payload_json)
                .map_err(|_| "签到课表缓存数据损坏".to_string()),
            Err(error) if error.is_record_not_found() => Ok(TimetableCache::default()),
            Err(error) => {
                log::warn!("读取签到课表缓存失败: {error}");
                Err("读取签到课表缓存失败".into())
            }
        }
    }

    pub async fn save_timetable(&self, cache: &TimetableCache) -> Result<(), String> {
        let payload_json =
            serde_json::to_string(cache).map_err(|_| "签到课表序列化失败".to_string())?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match TimetableCacheRecord::get_by_id(&mut db, "current").await {
            Ok(mut existing) => {
                toasty::update!(existing { payload_json, updated_at_ms: now })
                    .exec(&mut db)
                    .await
                    .map_err(|e| format!("更新签到课表缓存失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(TimetableCacheRecord {
                    id: "current".to_string(),
                    payload_json,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建签到课表缓存失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取签到课表缓存失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }

    /// 读取课程记忆；无记录返回默认值。
    pub async fn load_memory(&self, course_key: &str) -> Result<CourseMemory, String> {
        let mut db = self.db.clone();
        match RollcallMemoryRecord::get_by_course_key(&mut db, course_key).await {
            Ok(row) => Ok(CourseMemory {
                last_point: parse_point(&row.last_point_json),
                last_photo_b64: row.last_photo_b64,
                last_photo_mime: row.last_photo_mime,
            }),
            Err(error) if error.is_record_not_found() => Ok(CourseMemory::default()),
            Err(error) => {
                log::warn!("读取签到课程记忆失败: {error}");
                Err("读取签到课程记忆失败".into())
            }
        }
    }

    pub async fn save_last_point(
        &self,
        course_key: &str,
        point: &LastPoint,
    ) -> Result<(), String> {
        self.upsert_memory(course_key, |memory| {
            memory.last_point = Some(point.clone());
        })
        .await
    }

    pub async fn save_last_photo(
        &self,
        course_key: &str,
        photo_b64: &str,
        mime: &str,
    ) -> Result<(), String> {
        self.upsert_memory(course_key, |memory| {
            memory.last_photo_b64 = photo_b64.to_string();
            memory.last_photo_mime = mime.to_string();
        })
        .await
    }

    async fn upsert_memory(
        &self,
        course_key: &str,
        mutate: impl FnOnce(&mut CourseMemory),
    ) -> Result<(), String> {
        let mut memory = self.load_memory(course_key).await?;
        mutate(&mut memory);
        let last_point_json = memory
            .last_point
            .as_ref()
            .map(|point| serde_json::to_string(point).unwrap_or_default())
            .unwrap_or_default();
        let now = chrono::Utc::now().timestamp_millis();
        let mut db = self.db.clone();
        match RollcallMemoryRecord::get_by_course_key(&mut db, course_key).await {
            Ok(mut existing) => {
                toasty::update!(existing {
                    last_point_json,
                    last_photo_b64: memory.last_photo_b64,
                    last_photo_mime: memory.last_photo_mime,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("更新签到课程记忆失败: {e}"))?;
            }
            Err(error) if error.is_record_not_found() => {
                toasty::create!(RollcallMemoryRecord {
                    course_key: course_key.to_string(),
                    last_point_json,
                    last_photo_b64: memory.last_photo_b64,
                    last_photo_mime: memory.last_photo_mime,
                    updated_at_ms: now,
                })
                .exec(&mut db)
                .await
                .map_err(|e| format!("创建签到课程记忆失败: {e}"))?;
            }
            Err(error) => {
                return Err(format!("读取签到课程记忆失败，本次未写入: {error}"));
            }
        }
        Ok(())
    }
}

fn parse_point(raw: &str) -> Option<LastPoint> {
    if raw.is_empty() {
        return None;
    }
    serde_json::from_str(raw).ok()
}

/// 单门课程的可复用签到记忆。
#[derive(Clone, Debug, Default)]
pub struct CourseMemory {
    pub last_point: Option<LastPoint>,
    pub last_photo_b64: String,
    pub last_photo_mime: String,
}

impl CourseMemory {
    pub fn has_photo(&self) -> bool {
        !self.last_photo_b64.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TempDir 必须与数据库同生命周期，否则目录被清理后写入报 readonly。
    async fn open_temp_db() -> (tempfile::TempDir, SigninDatabase) {
        let dir = tempfile::tempdir().expect("临时目录");
        let path = dir.path().join("state.sqlite");
        let db = SigninDatabase::open(&path).await.expect("打开临时数据库");
        (dir, db)
    }

    // run_sync 的多语句迁移已在迁移层验证；这里覆盖两条业务链路。

    #[tokio::test]
    async fn timetable_roundtrip() {
        let (_dir, db) = open_temp_db().await;
        assert!(db.load_timetable().await.unwrap().is_empty());
        let cache = TimetableCache {
            fetched_at_ms: 1_000,
            semester: Some("2026-2027-1".into()),
            sessions: vec![super::super::model::TimetableSession {
                date: "2026-11-13".into(),
                course_name: "概率论与数理统计".into(),
                building_name: Some("致理楼".into()),
                building_name_en: Some("Zhili Building".into()),
                classroom: Some("致理楼L1-203".into()),
                start_ms: 1_000,
                end_ms: 2_000,
                semester: None,
            }],
        };
        db.save_timetable(&cache).await.unwrap();
        let loaded = db.load_timetable().await.unwrap();
        assert_eq!(loaded.sessions.len(), 1);
        assert_eq!(loaded.sessions[0].course_name, "概率论与数理统计");
    }

    #[tokio::test]
    async fn memory_point_and_photo_roundtrip() {
        let (_dir, db) = open_temp_db().await;
        assert!(db.load_memory("42").await.unwrap().last_point.is_none());
        let point = LastPoint {
            latitude: 22.528,
            longitude: 113.940,
            label: "致理楼".into(),
            source: "building".into(),
        };
        db.save_last_point("42", &point).await.unwrap();
        db.save_last_photo("42", "aGk=", "image/jpeg").await.unwrap();
        let memory = db.load_memory("42").await.unwrap();
        assert!(memory.has_photo());
        assert_eq!(memory.last_photo_mime, "image/jpeg");
        assert_eq!(memory.last_point.unwrap().label, "致理楼");
        // 另一门课程互不影响。
        assert!(!db.load_memory("43").await.unwrap().has_photo());
    }
}
