use std::{
    collections::HashMap,
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tokio_util::sync::CancellationToken;

/// 视频时长缓存，底层持久化于 Toasty ORM SQLite 数据库 (VideoDuration 模型)。
pub(super) struct DurationCache {
    db_path: PathBuf,
    db: tokio::sync::OnceCell<super::quiz::db::QaDatabase>,
    values: Mutex<HashMap<String, f64>>,
    flights: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    dirty: AtomicBool,
}

impl DurationCache {
    pub fn cached(&self, cid: &str, resource: &str) -> Option<f64> {
        self.values
            .lock()
            .unwrap()
            .get(&format!("{cid}:{resource}"))
            .copied()
    }

    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            db: tokio::sync::OnceCell::new(),
            values: Mutex::new(HashMap::new()),
            flights: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
        }
    }

    async fn get_db(&self) -> Result<&super::quiz::db::QaDatabase, String> {
        self.db
            .get_or_try_init(|| async {
                let qa_db = super::quiz::db::QaDatabase::open(&self.db_path).await?;
                let _ = crate::migrations::uooc::migrate_video_durations_if_needed(&self.db_path, &qa_db).await;
                if let Ok(loaded) = qa_db.load_all_video_durations().await {
                    let mut v = self.values.lock().unwrap();
                    for (k, val) in loaded {
                        v.entry(k).or_insert(val);
                    }
                }
                Ok(qa_db)
            })
            .await
    }

    pub async fn get_or_probe<F, Fut>(
        &self,
        cid: &str,
        resource: &str,
        cancel: &CancellationToken,
        probe: F,
    ) -> Result<f64, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<f64, String>>,
    {
        let key = format!("{cid}:{resource}");
        let lock = self
            .flights
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_default()
            .clone();
        let _guard = tokio::select! { biased; _ = cancel.cancelled() => return Err("已取消".into()), guard = lock.lock() => guard };

        // 尝试加载数据库预热缓存
        let _ = self.get_db().await;

        let cached_val = self.values.lock().unwrap().get(&key).copied();
        if let Some(duration) = cached_val {
            if self.dirty.load(Ordering::Relaxed) {
                if let Ok(db) = self.get_db().await {
                    if db.save_video_duration(cid, resource, duration).await.is_ok() {
                        self.dirty.store(false, Ordering::Relaxed);
                    }
                }
            }
            return Ok(duration);
        }

        let duration = probe().await?;
        if cancel.is_cancelled() {
            return Err("已取消".into());
        }
        if !valid(duration) {
            return Err("视频时长无效或不足一秒".into());
        }

        {
            let mut values = self.values.lock().unwrap();
            values.insert(key, duration);
        }
        self.dirty.store(true, Ordering::Relaxed);

        // 单条写入 SQLite。若持久化失败，保留内存值但向外返回 Err，以便重试持久化。
        match self.get_db().await {
            Ok(db) => {
                db.save_video_duration(cid, resource, duration).await?;
                self.dirty.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                return Err(format!("视频时长持久化失败: {e}"));
            }
        }

        Ok(duration)
    }
}

fn valid(duration: f64) -> bool {
    duration.is_finite() && duration > 1.0
}
