use super::{service::collect_candidate_urls, AppContentStore};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
};
use tauri::Emitter;

pub(crate) static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static IS_UPDATING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressPayload {
    pub status: String,
    pub progress: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

pub(crate) fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

pub(crate) fn emit_progress(payload: UpdateProgressPayload) {
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("in-app-update-progress", payload);
    }
}

pub(crate) fn start_update(store: &AppContentStore) -> Result<(), String> {
    if IS_UPDATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("更新任务已在进行中".to_string());
    }

    let snapshot = store.snapshot();
    let update = snapshot
        .update
        .data
        .ok_or_else(|| {
            IS_UPDATING.store(false, Ordering::SeqCst);
            "当前未检测到可用更新".to_string()
        })?;

    let direct_url = update.latest_ver_url.clone();
    let version = update.latest_ver.clone();
    let accelerators = store.accelerators().to_vec();

    std::thread::spawn(move || {
        let result = download_and_install(&direct_url, &version, &accelerators);
        IS_UPDATING.store(false, Ordering::SeqCst);
        if let Err(err_msg) = result {
            emit_progress(UpdateProgressPayload {
                status: "error".to_string(),
                progress: 0,
                downloaded_bytes: 0,
                total_bytes: 0,
                error: Some(err_msg),
            });
        }
    });

    Ok(())
}

pub(crate) fn download_and_install(
    direct_url: &str,
    version: &str,
    accelerators: &[reqwest::Url],
) -> Result<(), String> {
    let candidate_urls = collect_candidate_urls(direct_url, accelerators);

    emit_progress(UpdateProgressPayload {
        status: "downloading".to_string(),
        progress: 0,
        downloaded_bytes: 0,
        total_bytes: 0,
        error: None,
    });

    let client = reqwest::blocking::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("构建下载客户端失败: {e}"))?;

    let safe_version: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    let dest_path = std::env::temp_dir().join(format!("your-szu-life-installer-{safe_version}.exe"));

    let mut last_error = String::from("无可用下载源");

    for candidate in &candidate_urls {
        let resp = match client.get(candidate).send() {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                last_error = format!("下载节点返回状态码 {}", r.status());
                continue;
            }
            Err(e) => {
                last_error = format!("连接下载节点失败: {e}");
                continue;
            }
        };

        let total_bytes = resp.content_length().unwrap_or(0);
        let mut file = match std::fs::File::create(&dest_path) {
            Ok(f) => f,
            Err(e) => return Err(format!("创建临时安装文件失败: {e}")),
        };

        let mut reader = resp;
        let mut buffer = [0u8; 64 * 1024];
        let mut downloaded_bytes = 0u64;
        let mut last_emit_bytes = 0u64;
        let mut last_emit_time = std::time::Instant::now();
        let mut failed = false;

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if let Err(e) = file.write_all(&buffer[..n]) {
                        last_error = format!("写入安装文件失败: {e}");
                        failed = true;
                        break;
                    }
                    downloaded_bytes += n as u64;

                    let progress = if total_bytes > 0 {
                        ((downloaded_bytes as f64 / total_bytes as f64) * 100.0).clamp(0.0, 100.0)
                            as u32
                    } else {
                        0
                    };

                    if downloaded_bytes.saturating_sub(last_emit_bytes) >= 256 * 1024
                        || last_emit_time.elapsed().as_millis() >= 150
                    {
                        last_emit_bytes = downloaded_bytes;
                        last_emit_time = std::time::Instant::now();
                        emit_progress(UpdateProgressPayload {
                            status: "downloading".to_string(),
                            progress,
                            downloaded_bytes,
                            total_bytes,
                            error: None,
                        });
                    }
                }
                Err(e) => {
                    last_error = format!("读取下载数据失败: {e}");
                    failed = true;
                    break;
                }
            }
        }

        if failed {
            let _ = std::fs::remove_file(&dest_path);
            continue;
        }

        let _ = file.flush();
        drop(file);

        emit_progress(UpdateProgressPayload {
            status: "installing".to_string(),
            progress: 100,
            downloaded_bytes,
            total_bytes,
            error: None,
        });

        match std::process::Command::new(&dest_path).spawn() {
            Ok(_) => {
                emit_progress(UpdateProgressPayload {
                    status: "ready".to_string(),
                    progress: 100,
                    downloaded_bytes,
                    total_bytes,
                    error: None,
                });
                return Ok(());
            }
            Err(e) => {
                return Err(format!("启动安装程序失败: {e}"));
            }
        }
    }

    Err(format!("所有下载节点均失败，最后错误: {last_error}"))
}
