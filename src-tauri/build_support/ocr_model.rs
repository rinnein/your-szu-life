#![allow(dead_code)]

use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

struct Model {
    revision: String,
    sha256: String,
    bytes: u64,
}
impl Model {
    fn read(path: &Path) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_reader(
            File::open(path).map_err(|error| format!("cannot read OCR manifest: {error}"))?,
        )
        .map_err(|error| format!("invalid OCR manifest: {error}"))?;
        let model = Self {
            revision: value["revision"].as_str().unwrap_or_default().into(),
            sha256: value["sha256"].as_str().unwrap_or_default().into(),
            bytes: value["bytes"].as_u64().unwrap_or_default(),
        };
        if model.revision.len() != 40
            || !model.revision.bytes().all(|b| b.is_ascii_hexdigit())
            || model.sha256.len() != 64
            || !model.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || model.bytes == 0
            || model.bytes > 256 * 1024 * 1024
            || value["repository"] != "https://github.com/sml2h3/ddddocr"
            || value["file"] != "ddddocr/common.onnx"
        {
            return Err(
                "OCR manifest must pin the approved model, commit, size and SHA-256".into(),
            );
        }
        Ok(model)
    }
    fn verify(&self, path: &Path) -> bool {
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        if file.metadata().map(|m| m.len()).ok() != Some(self.bytes) {
            return false;
        }
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => hash.update(&buffer[..count]),
                Err(_) => return false,
            }
        }
        format!("{:x}", hash.finalize()) == self.sha256.to_ascii_lowercase()
    }
}

// The OS releases the lock if a build crashes. Downloads from parallel desktop
// and Android builds share a cache but never expose a partially written model.
fn ensure_model(
    model: &Model,
    cache: &Path,
    download: impl FnOnce(&mut File) -> Result<(), String>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(cache).map_err(|e| format!("cannot create OCR cache: {e}"))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join(format!("{}.lock", model.sha256)))
        .map_err(|e| format!("cannot open OCR cache lock: {e}"))?;
    lock.lock()
        .map_err(|e| format!("cannot lock OCR cache: {e}"))?;
    let target = cache.join(format!("{}.onnx", model.sha256));
    if model.verify(&target) {
        return target.canonicalize().map_err(|e| e.to_string());
    }
    let temporary = cache.join(format!("{}.download", model.sha256));
    let result = (|| {
        let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
        download(&mut file)?;
        file.flush().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if !model.verify(&temporary) {
            return Err(
                "downloaded OCR model failed size/SHA-256 validation; refusing to embed it".into(),
            );
        }
        // Windows rename cannot replace an existing file. The cache lock keeps
        // all cooperating builders out of this replacement interval.
        if target.exists() {
            fs::remove_file(&target).map_err(|e| e.to_string())?;
        }
        fs::rename(&temporary, &target).map_err(|e| e.to_string())?;
        target.canonicalize().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn prepare() -> Result<(), String> {
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing Cargo manifest directory")?);
    let manifest = root.join("resources/ocr/ddddocr/model.json");
    let cache = env::var_os("SZU_LIFE_MODEL_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/model-cache"));
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed=build_support/ocr_model.rs");
    println!("cargo:rerun-if-env-changed=SZU_LIFE_MODEL_CACHE");
    let model = Model::read(&manifest)?;
    let path = ensure_model(&model, &cache, |file| {
        let url = format!(
            "https://raw.githubusercontent.com/sml2h3/ddddocr/{}/ddddocr/common.onnx",
            model.revision
        );
        println!(
            "cargo:warning=Downloading pinned ddddocr model into {}",
            cache.display()
        );
        let client = reqwest::blocking::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::limited(5))
            .build()
            .map_err(|e| format!("cannot initialize model downloader: {e}"))?;
        let response = client.get(url).send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|e| format!("cannot download OCR model: {e}. For offline builds, place {}.onnx in SZU_LIFE_MODEL_CACHE", model.sha256))?;
        if response
            .content_length()
            .is_some_and(|size| size > model.bytes)
        {
            return Err("OCR download is larger than the pinned model size".into());
        }
        std::io::copy(&mut response.take(model.bytes + 1), file)
            .map_err(|e| format!("cannot save OCR download: {e}"))?;
        Ok(())
    })?;
    println!("cargo:rerun-if-changed={}", path.display());
    println!(
        "cargo:rustc-env=SZU_LIFE_DDDDOCR_MODEL_PATH={}",
        path.display()
    );
    Ok(())
}

pub fn stage_bundled() -> Result<(), String> {
    let root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing Cargo manifest directory")?);
    let manifest = Model::read(&root.join("resources/ocr/ddddocr/model.json"))?;
    let cache = env::var_os("SZU_LIFE_MODEL_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/model-cache"));
    let source = cache.join(format!("{}.onnx", manifest.sha256));
    if !manifest.verify(&source) {
        return Err(format!(
            "bundled OCR build requires a verified model at {}",
            source.display()
        ));
    }
    let stage = root.join("target/bundled-ocr/common.onnx");
    fs::create_dir_all(stage.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(&source, &stage).map_err(|e| format!("cannot stage bundled OCR model: {e}"))?;
    println!("cargo:rerun-if-env-changed=SZU_LIFE_BUNDLE_OCR_MODEL");
    println!("cargo:rerun-if-env-changed=SZU_LIFE_MODEL_CACHE");
    println!("cargo:rerun-if-changed={}", source.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(bytes: &[u8]) -> Model {
        Model {
            revision: "a".repeat(40),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len() as u64,
        }
    }
    #[test]
    fn missing_cache_is_downloaded_and_verified_cache_is_reused_offline() {
        let cache = tempfile::tempdir().unwrap();
        let data = b"model fixture";
        let spec = model(data);
        let path = ensure_model(&spec, cache.path(), |f| {
            f.write_all(data).map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert_eq!(
            ensure_model(&spec, cache.path(), |_| panic!(
                "valid cache must not download"
            ))
            .unwrap(),
            path
        );
    }
    #[test]
    fn corrupted_cache_is_replaced_only_with_verified_bytes() {
        let cache = tempfile::tempdir().unwrap();
        let spec = model(b"good");
        let path = cache.path().join(format!("{}.onnx", spec.sha256));
        fs::write(&path, b"evil").unwrap();
        assert!(ensure_model(&spec, cache.path(), |f| f
            .write_all(b"nope")
            .map_err(|e| e.to_string()))
        .is_err());
        assert_eq!(fs::read(&path).unwrap(), b"evil");
        ensure_model(&spec, cache.path(), |f| {
            f.write_all(b"good").map_err(|e| e.to_string())
        })
        .unwrap();
        assert_eq!(fs::read(path).unwrap(), b"good");
    }
    #[test]
    fn incomplete_download_does_not_leave_a_model_or_temporary_file() {
        let cache = tempfile::tempdir().unwrap();
        let spec = model(b"complete");
        assert!(ensure_model(&spec, cache.path(), |file| {
            file.write_all(b"part").unwrap();
            Err("connection lost".into())
        })
        .is_err());
        assert_eq!(fs::read_dir(cache.path()).unwrap().count(), 1); // lock only
    }
    #[test]
    fn parallel_builds_download_once() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let cache = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let jobs: Vec<_> = (0..4)
            .map(|_| {
                let path = cache.path().to_owned();
                let calls = calls.clone();
                std::thread::spawn(move || {
                    ensure_model(&model(b"shared"), &path, |file| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(Duration::from_millis(20));
                        file.write_all(b"shared").map_err(|e| e.to_string())
                    })
                    .unwrap()
                })
            })
            .collect();
        for job in jobs {
            assert!(job.join().unwrap().exists());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn manifest_cannot_switch_to_an_unpinned_model() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.json");
        fs::write(&path, br#"{"revision":"main","sha256":"bad","bytes":20}"#).unwrap();
        assert!(Model::read(&path).is_err());
    }
}
