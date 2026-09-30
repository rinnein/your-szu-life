use super::persistence;
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

/// No URL or authentication data is persisted. A signed URL changing is not a cache miss.
pub(super) struct DurationCache {
    path: PathBuf,
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
    pub fn new(path: PathBuf) -> Self {
        let mut values = persistence::read::<HashMap<String, f64>>(&path)
            .ok()
            .flatten()
            .unwrap_or_default();
        values.retain(|_, duration| valid(*duration));
        Self {
            path,
            values: Mutex::new(values),
            flights: Mutex::new(HashMap::new()),
            dirty: AtomicBool::new(false),
        }
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
        {
            let values = self.values.lock().unwrap();
            if let Some(duration) = values.get(&key) {
                if self.dirty.load(Ordering::Relaxed) {
                    persistence::write(&self.path, &*values)?;
                    self.dirty.store(false, Ordering::Relaxed);
                }
                return Ok(*duration);
            }
        }
        let duration = probe().await?;
        if cancel.is_cancelled() {
            return Err("已取消".into());
        }
        if !valid(duration) {
            return Err("视频时长无效或不足一秒".into());
        }
        let mut values = self.values.lock().unwrap();
        values.insert(key, duration);
        self.dirty.store(true, Ordering::Relaxed);
        // Keep the in-memory value even if storage fails: retrying must not reparse the video.
        persistence::write(&self.path, &*values)?;
        self.dirty.store(false, Ordering::Relaxed);
        Ok(duration)
    }
}
fn valid(duration: f64) -> bool {
    duration.is_finite() && duration > 1.0
}
