use super::model::*;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub(super) fn cache_key(value: &str) -> String {
    format!("{:x}", Sha1::digest(value.as_bytes()))
}

pub(super) struct Cache {
    root: Option<PathBuf>,
    io: Mutex<()>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CachedDocument<T> {
    schema: u8,
    key: String,
    pub saved_at: u64,
    pub source: Source,
    pub data: T,
}

impl Cache {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root: root.map(|path| path.join("public-content-v1")),
            io: Mutex::new(()),
        }
    }

    fn path(&self, scope: &str, file: &str) -> Option<PathBuf> {
        Some(self.root.as_ref()?.join(cache_key(scope)).join(file))
    }

    pub fn load<T: Document>(
        &self,
        scope: &str,
        name: &str,
        now: u64,
    ) -> Option<CachedDocument<T>> {
        let _guard = self.io.lock().unwrap();
        let bytes = read_limited(&self.path(scope, name)?, JSON_LIMIT + 16 * 1024)?;
        let cached: CachedDocument<T> = serde_json::from_slice(&bytes).ok()?;
        if cached.schema != 1
            || cached.key != scope
            || cached.saved_at > now
            || cached.data.validate().is_err()
        {
            return None;
        }
        Some(cached)
    }

    pub fn save<T: Document>(
        &self,
        scope: &str,
        name: &str,
        data: &T,
        source: Source,
        saved_at: u64,
    ) {
        let Some(path) = self.path(scope, name) else {
            return;
        };
        let record = CachedDocument {
            schema: 1,
            key: scope.into(),
            saved_at,
            source,
            data,
        };
        // Cache failure must not turn a valid network response into a failed check.
        if let Ok(bytes) = serde_json::to_vec(&record) {
            let _guard = self.io.lock().unwrap();
            let _ = atomic_write(&path, &bytes);
        }
    }

    pub fn load_image(&self, scope: &str, url: &str) -> Option<Vec<u8>> {
        let _guard = self.io.lock().unwrap();
        read_limited(
            &self.path(scope, &format!("images/{}.image", cache_key(url)))?,
            IMAGE_LIMIT,
        )
    }

    pub fn save_image(&self, scope: &str, url: &str, body: &[u8]) {
        let Some(path) = self.path(scope, &format!("images/{}.image", cache_key(url))) else {
            return;
        };
        let _guard = self.io.lock().unwrap();
        if atomic_write(&path, body).is_err() {
            return;
        }
        // At most 32 MiB on disk, even if every announcement uses a different 4 MiB image.
        if let Some(directory) = path.parent() {
            trim_images(directory);
        }
    }

    pub fn prune_images(&self, scope: &str, allowed: &[String]) {
        let Some(directory) = self.path(scope, "images") else {
            return;
        };
        let keep: std::collections::HashSet<_> = allowed
            .iter()
            .map(|url| format!("{}.image", cache_key(url)))
            .collect();
        let _guard = self.io.lock().unwrap();
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                if !keep.contains(&entry.file_name().to_string_lossy().into_owned()) {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }
    }
}

fn read_limited(path: &Path, limit: usize) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > limit as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().expect("cache paths have a parent");
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn trim_images(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((entry.path(), metadata.len(), metadata.modified().ok()))
        })
        .collect();
    files.sort_by_key(|(_, _, modified)| *modified);
    let mut size: u64 = files.iter().map(|(_, length, _)| length).sum();
    for (path, length, _) in files {
        if size <= 32 * 1024 * 1024 {
            break;
        }
        if fs::remove_file(path).is_ok() {
            size = size.saturating_sub(length);
        }
    }
}
