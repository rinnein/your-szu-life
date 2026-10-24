use super::{client::Aria2WebSocketClient, process::Aria2Lease};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct DownloadJobOptions {
    pub uris: Vec<String>,
    pub dest_dir: PathBuf,
    pub filename: String,
    pub expected_sha256: Option<String>,
    pub expected_size: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct ProgressUpdate {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
}

pub fn verify_file_hash_and_size(
    path: &Path,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };

    if let Some(expected_len) = expected_size {
        if metadata.len() != expected_len {
            return false;
        }
    }

    if let Some(expected_hash) = expected_sha256 {
        let Ok(mut file) = File::open(path) else {
            return false;
        };
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => hasher.update(&buffer[..n]),
                Err(_) => return false,
            }
        }
        let digest = format!("{:x}", hasher.finalize());
        if !digest.eq_ignore_ascii_case(expected_hash) {
            return false;
        }
    }

    true
}

pub async fn download_with_aria2<F>(
    app: Option<&tauri::AppHandle>,
    options: DownloadJobOptions,
    on_progress: F,
) -> Result<PathBuf, String>
where
    F: Fn(ProgressUpdate) + Send + Sync + 'static,
{
    if options.uris.is_empty() {
        return Err("下载地址列表为空".to_string());
    }

    fs::create_dir_all(&options.dest_dir)
        .map_err(|e| format!("创建下载目标目录失败: {e}"))?;

    let lease = Aria2Lease::acquire(app).await?;

    let target_file = options.dest_dir.join(&options.filename);
    let aria2_meta_path = options.dest_dir.join(format!("{}.aria2", options.filename));
    if aria2_meta_path.is_file() {
        let _ = fs::remove_file(&aria2_meta_path);
    }

    let mut client = Aria2WebSocketClient::connect(&lease.ws_url, &lease.secret).await?;
    let gid = client
        .add_uri(&options.uris, &options.dest_dir, &options.filename)
        .await?;

    let mut last_progress_emit = std::time::Instant::now();

    loop {
        tokio::time::sleep(Duration::from_millis(150)).await;

        if let Err(e) = lease.check_alive().await {
            return Err(e);
        }

        let status = match client.tell_status(&gid).await {
            Ok(s) => s,
            Err(e) => {
                if let Err(proc_err) = lease.check_alive().await {
                    return Err(proc_err);
                }
                let _ = client.remove(&gid).await;
                return Err(format!("查询下载进度失败: {e}"));
            }
        };

        if last_progress_emit.elapsed() >= Duration::from_millis(100) {
            last_progress_emit = std::time::Instant::now();
            let total = if status.total_bytes() > 0 {
                status.total_bytes()
            } else {
                options.expected_size.unwrap_or(0)
            };
            on_progress(ProgressUpdate {
                downloaded_bytes: status.completed_bytes(),
                total_bytes: total,
                speed_bytes_per_sec: status.speed_bytes_per_sec(),
            });
        }

        match status.status.as_str() {
            "complete" => break,
            "error" => {
                let err_msg = status
                    .error_message
                    .unwrap_or_else(|| "aria2c 报告下载失败".to_string());
                return Err(format!("aria2c 下载失败: {err_msg}"));
            }
            "removed" => {
                return Err("aria2c 下载任务已被取消".to_string());
            }
            _ => {}
        }
    }

    // Final progress report
    if let Ok(metadata) = fs::metadata(&target_file) {
        on_progress(ProgressUpdate {
            downloaded_bytes: metadata.len(),
            total_bytes: metadata.len(),
            speed_bytes_per_sec: 0,
        });
    }

    if !verify_file_hash_and_size(
        &target_file,
        options.expected_size,
        options.expected_sha256.as_deref(),
    ) {
        let _ = fs::remove_file(&target_file);
        return Err("文件完整性校验失败（大小或哈希不匹配）".to_string());
    }

    lease.release().await;

    Ok(target_file)
}
