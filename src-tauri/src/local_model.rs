use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::Emitter;
use reqwest::Url;

pub(crate) const GITHUB_ACCELERATORS: &str = include_str!("../../github-accelerator.json");
pub(crate) const HUGGINGFACE_ACCELERATORS: &str = include_str!("../../huggingface-accelerator.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    pub revision: String,
    pub sha256: String,
    pub bytes: u64,
    pub url: String,
}

impl Manifest {
    pub(crate) fn load(json_str: &str, model_label: &str) -> Result<Self, String> {
        let manifest: Self = serde_json::from_str(json_str)
            .map_err(|e| format!("{model_label} 模型配置格式错误：{e}"))?;
        if manifest.revision.trim().is_empty()
            || manifest.sha256.len() != 64
            || manifest.bytes == 0
        {
            return Err(format!("{model_label} 模型配置未通过校验"));
        }
        let url = Url::parse(&manifest.url)
            .map_err(|_| format!("{model_label} 模型下载地址无效"))?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(format!("{model_label} 模型下载地址必须是无凭据 HTTPS 地址"));
        }
        Ok(manifest)
    }

    pub(crate) fn verify(&self, path: &Path) -> bool {
        let Ok(metadata) = fs::metadata(path) else {
            return false;
        };
        if metadata.len() != self.bytes {
            return false;
        }
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => hasher.update(&buffer[..n]),
                Err(_) => return false,
            }
        }
        let digest = format!("{:x}", hasher.finalize());
        digest == self.sha256
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ModelDownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelStatus {
    pub status: String,
    pub revision: String,
    pub sha256: String,
    pub bytes: u64,
    pub source: Option<String>,
    pub error: Option<String>,
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: Option<u64>,
}

pub(crate) fn mirror_candidates(primary_url: &str) -> Vec<(String, Url)> {
    let mut mirrors = Vec::new();
    let Ok(primary) = Url::parse(primary_url) else {
        return mirrors;
    };
    let host = primary.host_str().unwrap_or_default();

    if host.contains("huggingface") {
        if let Ok(values) = serde_json::from_str::<Vec<String>>(HUGGINGFACE_ACCELERATORS) {
            for prefix in values {
                let Ok(base) = Url::parse(&prefix) else {
                    continue;
                };
                if base.scheme() != "https"
                    || !base.username().is_empty()
                    || base.password().is_some()
                {
                    continue;
                }
                let path = primary.path().trim_start_matches('/');
                if let Ok(url) = base.join(path) {
                    mirrors.push(("huggingface-accelerator".into(), url));
                }
            }
        }
    } else if host.contains("github") {
        if let Ok(values) = serde_json::from_str::<Vec<String>>(GITHUB_ACCELERATORS) {
            for prefix in values {
                let candidate = format!("{prefix}{primary}");
                if let Ok(url) = Url::parse(&candidate) {
                    if url.scheme() == "https"
                        && url.username().is_empty()
                        && url.password().is_none()
                    {
                        mirrors.push(("github-accelerator".into(), url));
                    }
                }
            }
        }
    }
    mirrors
}

pub(crate) struct LocalModelStore {
    pub(crate) manifest: Manifest,
    pub(crate) cache: PathBuf,
    pub(crate) bundled: Option<PathBuf>,
    pub(crate) state: Mutex<LocalModelStatus>,
    pub(crate) operation: Mutex<()>,
    pub(crate) model_name: &'static str,
    pub(crate) progress_event: &'static str,
    pub(crate) status_event: &'static str,
}

impl LocalModelStore {
    pub(crate) fn new(
        manifest: Manifest,
        cache: PathBuf,
        bundled: Option<PathBuf>,
        model_name: &'static str,
        progress_event: &'static str,
        status_event: &'static str,
    ) -> Self {
        let initial_status = LocalModelStatus {
            status: "missing".into(),
            revision: manifest.revision.clone(),
            sha256: manifest.sha256.clone(),
            bytes: manifest.bytes,
            source: None,
            error: None,
            downloaded_bytes: None,
            total_bytes: None,
            speed_bytes_per_sec: None,
        };
        Self {
            manifest,
            cache,
            bundled,
            state: Mutex::new(initial_status),
            operation: Mutex::new(()),
            model_name,
            progress_event,
            status_event,
        }
    }

    pub(crate) fn target(&self) -> PathBuf {
        self.cache.join(format!("{}.onnx", self.manifest.sha256))
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>, String> {
        let target = self.target();
        if !self.manifest.verify(&target) {
            return Err(format!("{}_missing", self.model_name.to_lowercase()));
        }
        fs::read(target).map_err(|_| format!("{}_invalid", self.model_name.to_lowercase()))
    }

    pub(crate) fn status(&self) -> LocalModelStatus {
        self.state.lock().unwrap().clone()
    }

    pub(crate) fn set_status(&self, status: &str, source: Option<String>, error: Option<String>) {
        let mut value = self.state.lock().unwrap();
        value.status = status.into();
        value.source = source;
        value.error = error;
        if status != "downloading" {
            value.downloaded_bytes = None;
            value.total_bytes = None;
            value.speed_bytes_per_sec = None;
        }
    }

    pub(crate) fn refresh_from_disk(&self) {
        let target = self.target();
        if self.manifest.verify(&target) {
            self.set_status("ready", Some("cache".into()), None);
            return;
        }
        if let Some(bundled) = &self.bundled {
            if self.manifest.verify(bundled) {
                if let Err(error) = self.install_file(bundled) {
                    self.set_status("error", Some("bundled".into()), Some(error));
                } else {
                    self.set_status("ready", Some("bundled".into()), None);
                }
                return;
            }
        }
        self.set_status("missing", None, None);
    }

    fn install_file(&self, source: &Path) -> Result<(), String> {
        fs::create_dir_all(&self.cache)
            .map_err(|e| format!("无法创建 {} 模型目录：{e}", self.model_name))?;
        let temporary = self.cache.join(format!("{}.install", self.manifest.sha256));
        let mut input = File::open(source)
            .map_err(|e| format!("无法读取内置 {} 模型：{e}", self.model_name))?;
        let mut output = File::create(&temporary)
            .map_err(|e| format!("无法写入 {} 模型：{e}", self.model_name))?;
        std::io::copy(&mut input, &mut output)
            .map_err(|e| format!("复制 {} 模型失败：{e}", self.model_name))?;
        output
            .flush()
            .map_err(|e| format!("保存 {} 模型失败：{e}", self.model_name))?;
        output
            .sync_all()
            .map_err(|e| format!("同步 {} 模型失败：{e}", self.model_name))?;
        drop(output);
        if !self.manifest.verify(&temporary) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("内置 {} 模型校验失败", self.model_name));
        }
        if self.target().exists() {
            let _ = fs::remove_file(self.target());
        }
        fs::rename(temporary, self.target())
            .map_err(|e| format!("安装 {} 模型失败：{e}", self.model_name))
    }

    pub(crate) fn download(
        &self,
        app: Option<&tauri::AppHandle>,
        force: bool,
    ) -> Result<(), String> {
        let _guard = self.operation.lock().unwrap();
        if !force {
            self.refresh_from_disk();
        }
        if !force && self.state.lock().unwrap().status == "ready" {
            return Ok(());
        }
        self.set_status("downloading", None, None);

        let temporary = self
            .cache
            .join(format!("{}.download", self.manifest.sha256));

        // 1. 尝试使用 aria2c 多源并发下载
        let mut uris = vec![self.manifest.url.clone()];
        for (_source_name, mirror_url) in mirror_candidates(&self.manifest.url) {
            uris.push(mirror_url.to_string());
        }

        let aria_options = crate::aria2::DownloadJobOptions {
            uris,
            dest_dir: self.cache.clone(),
            filename: format!("{}.download", self.manifest.sha256),
            expected_sha256: Some(self.manifest.sha256.clone()),
            expected_size: Some(self.manifest.bytes),
        };

        let progress_event = self.progress_event;
        let app_handle_opt = app.cloned();

        let aria_res = tauri::async_runtime::block_on(crate::aria2::download_with_aria2(
            app,
            aria_options,
            move |prog| {
                let payload = ModelDownloadProgress {
                    downloaded_bytes: prog.downloaded_bytes,
                    total_bytes: prog.total_bytes,
                    speed_bytes_per_sec: prog.speed_bytes_per_sec,
                };
                if let Some(ref handle) = app_handle_opt {
                    let _ = handle.emit(progress_event, payload);
                }
            },
        ));

        match aria_res {
            Ok(downloaded_path) => {
                if self.target().exists() {
                    let _ = fs::remove_file(self.target());
                }
                if let Err(e) = fs::rename(&downloaded_path, self.target()) {
                    let err = format!("安装 {} 模型失败：{e}", self.model_name);
                    self.set_status("error", None, Some(err.clone()));
                    return Err(err);
                }
                self.set_status("ready", Some("aria2".to_string()), None);
                Ok(())
            }
            Err(e) => {
                let _ = fs::remove_file(&temporary);
                let err = format!("aria2c 下载 {} 模型失败: {e}", self.model_name);
                self.set_status("error", None, Some(err.clone()));
                Err(err)
            }
        }
    }

    pub(crate) async fn download_command(
        self: Arc<Self>,
        app: tauri::AppHandle,
        force: bool,
    ) -> Result<LocalModelStatus, String> {
        self.set_status("downloading", None, None);
        let mut downloading_status = self.state.lock().unwrap().clone();
        downloading_status.downloaded_bytes = Some(0);
        downloading_status.total_bytes = Some(downloading_status.bytes);
        downloading_status.speed_bytes_per_sec = Some(0);
        let _ = app.emit(self.status_event, downloading_status);

        let background = self.clone();
        let app_handle = app.clone();
        let status_event = self.status_event;
        tokio::task::spawn_blocking(move || {
            let result = background.download(Some(&app_handle), force);
            let status = background.state.lock().unwrap().clone();
            let _ = app_handle.emit(status_event, status);
            result
        })
        .await
        .map_err(|_| format!("{} 模型下载任务崩溃", self.model_name))??;

        Ok(self.status())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn test_mirror_candidates_for_huggingface() {
        let candidates = mirror_candidates("https://huggingface.co/Xenova/bge-small-zh-v1.5/resolve/main/onnx/model_quantized.onnx");
        assert!(!candidates.is_empty());
        for (name, url) in &candidates {
            assert_eq!(name, "huggingface-accelerator");
            assert!(url.path().ends_with("onnx/model_quantized.onnx"));
        }
    }

    #[test]
    fn test_mirror_candidates_for_github() {
        let candidates = mirror_candidates("https://raw.githubusercontent.com/sml2h3/ddddocr/master/ddddocr/common.onnx");
        assert!(!candidates.is_empty());
        for (name, url) in &candidates {
            assert_eq!(name, "github-accelerator");
            assert!(url.to_string().contains("raw.githubusercontent.com"));
        }
    }

    #[test]
    fn test_mirror_random_selection_exhaustion() {
        let mut available = mirror_candidates("https://huggingface.co/Xenova/bge-small-zh-v1.5/resolve/main/onnx/model_quantized.onnx");
        let initial_count = available.len();
        let mut selected = Vec::new();

        while !available.is_empty() {
            let index = rand::rng().random_range(0..available.len());
            let (name, url) = available.swap_remove(index);
            assert!(!selected.contains(&url));
            selected.push(url);
            let _ = name;
        }

        assert_eq!(selected.len(), initial_count);
    }
}
