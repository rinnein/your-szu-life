use rand::Rng;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Emitter;
use reqwest::Url;

pub(crate) const GITHUB_ACCELERATORS: &str = include_str!("../../github-accelerator.json");
pub(crate) const HUGGINGFACE_ACCELERATORS: &str = include_str!("../../huggingface-accelerator.json");
pub(crate) const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Manifest {
    #[allow(dead_code)]
    pub repository: String,
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

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize)]
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

    fn build_client(connect_timeout: Duration) -> Result<reqwest::blocking::Client, String> {
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()
            .map_err(|e| format!("无法初始化系统 TLS 验证器：{e}"))?;

        reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .https_only(true)
            .connect_timeout(connect_timeout)
            .timeout(Duration::from_secs(180))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .map_err(|e| format!("无法初始化模型下载器：{e}"))
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

        let primary_url = match Url::parse(&self.manifest.url) {
            Ok(u) => u,
            Err(e) => {
                let err = format!("模型下载地址解析失败: {e}");
                self.set_status("error", None, Some(err.clone()));
                return Err(err);
            }
        };

        let mut errors = Vec::new();

        // 1. 尝试直接下载 (5s 超时)
        let direct_client = match Self::build_client(Duration::from_secs(5)) {
            Ok(c) => c,
            Err(e) => {
                self.set_status("error", None, Some(e.clone()));
                return Err(e);
            }
        };

        match self.try_download_from(
            &direct_client,
            &primary_url,
            "direct",
            &temporary,
            app,
        ) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                errors.push(format!("direct: {error}"));
            }
        }

        // 2. 从 json 中随机选取镜像下载 (10s 超时)；失败则从未尝试过的镜像中随机选取，最多 3 次失败后报错
        let mut available_mirrors = mirror_candidates(&self.manifest.url);
        let mut mirror_failures = 0;

        while !available_mirrors.is_empty() && mirror_failures < 3 {
            let index = rand::rng().random_range(0..available_mirrors.len());
            let (source_name, mirror_url) = available_mirrors.swap_remove(index);

            let mirror_client = match Self::build_client(Duration::from_secs(10)) {
                Ok(c) => c,
                Err(e) => {
                    self.set_status("error", None, Some(e.clone()));
                    return Err(e);
                }
            };

            match self.try_download_from(
                &mirror_client,
                &mirror_url,
                &source_name,
                &temporary,
                app,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    mirror_failures += 1;
                    errors.push(format!("{source_name}: {error}"));
                }
            }
        }

        let last_error = if errors.is_empty() {
            String::from("没有可用的模型下载源")
        } else {
            errors.join(" | ")
        };
        self.set_status("error", None, Some(last_error.clone()));
        Err(last_error)
    }

    fn try_download_from(
        &self,
        client: &reqwest::blocking::Client,
        url: &Url,
        source: &str,
        temporary: &Path,
        app: Option<&tauri::AppHandle>,
    ) -> Result<(), String> {
        let mut response = client
            .get(url.clone())
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|e| e.to_string())?;

        let content_len = response.content_length();
        if content_len.is_some_and(|size| size > self.manifest.bytes) {
            return Err("模型响应超过固定大小".into());
        }
        let total_bytes = content_len.unwrap_or(self.manifest.bytes);
        let mut file = File::create(temporary).map_err(|e| e.to_string())?;

        let mut buffer = [0_u8; 64 * 1024];
        let mut downloaded_bytes: u64 = 0;
        let mut last_emit = Instant::now();
        let mut window_start = Instant::now();
        let mut window_bytes: u64 = 0;
        let mut current_speed: u64 = 0;

        loop {
            let n = match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return Err(e.to_string()),
            };
            file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            downloaded_bytes += n as u64;
            window_bytes += n as u64;

            if downloaded_bytes > self.manifest.bytes {
                return Err("模型响应超过固定大小".into());
            }

            let now = Instant::now();
            let window_elapsed = now.duration_since(window_start);
            if window_elapsed >= Duration::from_millis(400) {
                current_speed = (window_bytes as f64 / window_elapsed.as_secs_f64()) as u64;
                window_bytes = 0;
                window_start = now;
            }

            if now.duration_since(last_emit) >= Duration::from_millis(100) {
                last_emit = now;
                let payload = ModelDownloadProgress {
                    downloaded_bytes,
                    total_bytes,
                    speed_bytes_per_sec: current_speed,
                };
                if let Some(app) = app {
                    let _ = app.emit(self.progress_event, payload);
                }
                let mut val = self.state.lock().unwrap();
                val.downloaded_bytes = Some(downloaded_bytes);
                val.total_bytes = Some(total_bytes);
                val.speed_bytes_per_sec = Some(current_speed);
            }
        }

        file.flush().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);

        let final_payload = ModelDownloadProgress {
            downloaded_bytes,
            total_bytes,
            speed_bytes_per_sec: current_speed,
        };
        if let Some(app) = app {
            let _ = app.emit(self.progress_event, final_payload);
        }

        if !self.manifest.verify(temporary) {
            let _ = fs::remove_file(temporary);
            return Err("模型哈希或大小校验失败".into());
        }
        if self.target().exists() {
            let _ = fs::remove_file(self.target());
        }
        fs::rename(temporary, self.target()).map_err(|e| e.to_string())?;
        self.set_status("ready", Some(source.to_string()), None);
        Ok(())
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
