use super::{client::Aria2WebSocketClient, process::Aria2Lease};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

/// 进度轮询间隔（毫秒）。
///
/// aria2 的 WebSocket 通知只覆盖状态迁移事件（`onDownloadStart` /
/// `onDownloadComplete` / `onDownloadError` 等），`onDownloadProgress`
/// 默认不广播，且 1.37 已移除按 gid 订阅的 `aria2.setNotification`
/// （调用会返回 `No such method`）。因此获取进度只能轮询 `aria2.tellStatus`。
///
/// 150ms 约等于 6.7Hz，足以驱动平滑的进度条与速率显示，同时把 RPC 往返
/// 控制在每秒个位数。调低更顺滑但 RPC 更频繁，调高更省电但动画变顿。
pub const PROGRESS_POLL_INTERVAL_MS: u64 = 150;

#[derive(Clone, Debug)]
pub struct DownloadJobOptions {
    pub uris: Vec<String>,
    pub prioritized_uris: Vec<(u32, String)>,
    pub dest_dir: PathBuf,
    pub filename: String,
    pub expected_sha256: Option<String>,
    pub expected_size: Option<u64>,
    /// 附加的 HTTP 请求头（部分站点要求 Referer 等才可下载）。
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
    pub port: u16,
    pub num_pieces: u32,
    pub piece_length: u64,
    pub bitfield: Option<String>,
    pub connections: u32,
    pub is_resumed: bool,
    pub resumed_bytes: u64,
    pub total_active_tasks: u32,
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn create_metalink_xml(
    filename: &str,
    prioritized_uris: &[(u32, String)],
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<metalink xmlns=\"urn:ietf:params:xml:ns:metalink\">\n");
    xml.push_str(&format!("  <file name=\"{}\">\n", xml_escape(filename)));

    if let Some(size) = expected_size {
        xml.push_str(&format!("    <size>{size}</size>\n"));
    }

    if let Some(hash) = expected_sha256 {
        xml.push_str(&format!(
            "    <hash type=\"sha-256\">{}</hash>\n",
            xml_escape(hash)
        ));
    }

    for (priority, uri) in prioritized_uris {
        xml.push_str(&format!(
            "    <url priority=\"{priority}\">{}</url>\n",
            xml_escape(uri)
        ));
    }

    xml.push_str("  </file>\n");
    xml.push_str("</metalink>");
    xml
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

/// 下载任务的运行时控制句柄：暂停/继续/取消由外部在轮询间隙下发。
/// 命令以下一轮 `tellStatus` 轮询（≤150ms）生效。
#[derive(Default)]
pub struct DownloadControl {
    /// 0=无操作 1=暂停 2=继续 3=取消
    command: std::sync::atomic::AtomicU8,
}

impl DownloadControl {
    const PAUSE: u8 = 1;
    const RESUME: u8 = 2;
    const CANCEL: u8 = 3;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) {
        self.command
            .store(Self::PAUSE, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.command
            .store(Self::RESUME, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn cancel(&self) {
        self.command
            .store(Self::CANCEL, std::sync::atomic::Ordering::SeqCst);
    }

    fn take(&self) -> u8 {
        self.command.swap(0, std::sync::atomic::Ordering::SeqCst)
    }
}

pub async fn download_with_aria2<F>(
    app: Option<&tauri::AppHandle>,
    options: DownloadJobOptions,
    on_progress: F,
) -> Result<PathBuf, String>
where
    F: Fn(ProgressUpdate) + Send + Sync + 'static,
{
    download_with_aria2_controlled(app, options, &DownloadControl::new(), on_progress).await
}

/// 带 [`DownloadControl`] 的下载：jxxk 录像等多任务管理场景使用。
pub async fn download_with_aria2_controlled<F>(
    app: Option<&tauri::AppHandle>,
    options: DownloadJobOptions,
    control: &DownloadControl,
    on_progress: F,
) -> Result<PathBuf, String>
where
    F: Fn(ProgressUpdate) + Send + Sync + 'static,
{
    if options.uris.is_empty() && options.prioritized_uris.is_empty() {
        return Err("下载地址列表为空".to_string());
    }

    fs::create_dir_all(&options.dest_dir).map_err(|e| format!("创建下载目标目录失败: {e}"))?;

    let target_file = options.dest_dir.join(&options.filename);
    let aria2_meta_path = options.dest_dir.join(format!("{}.aria2", options.filename));
    let had_partial_cache = target_file.is_file() || aria2_meta_path.is_file();

    let lease = Aria2Lease::acquire(app).await?;

    let mut client = Aria2WebSocketClient::connect(&lease.ws_url, &lease.secret).await?;
    let gid = if !options.prioritized_uris.is_empty() {
        use base64::Engine;
        let metalink_xml = create_metalink_xml(
            &options.filename,
            &options.prioritized_uris,
            options.expected_size,
            options.expected_sha256.as_deref(),
        );
        let b64 = base64::engine::general_purpose::STANDARD.encode(metalink_xml.as_bytes());
        client.add_metalink(&b64, &options.dest_dir).await?
    } else {
        client
            .add_uri(&options.uris, &options.dest_dir, &options.filename, &options.headers)
            .await?
    };

    let mut last_num_pieces;
    let mut last_piece_length;
    let mut last_bitfield: Option<String> = None;
    let mut initial_resumed_bytes: Option<u64> = None;
    let mut is_resumed = false;
    let mut poll_count: u64 = 0;

    loop {
        tokio::time::sleep(Duration::from_millis(PROGRESS_POLL_INTERVAL_MS)).await;

        if let Err(e) = lease.check_alive().await {
            return Err(e);
        }

        match control.take() {
            DownloadControl::PAUSE => {
                // forcePause 立即生效；失败仅记录，不中断轮询。
                if let Err(e) = client.pause(&gid).await {
                    log::warn!("[aria2] forcePause {gid} 失败: {e}");
                }
            }
            DownloadControl::RESUME => {
                if let Err(e) = client.unpause(&gid).await {
                    log::warn!("[aria2] unpause {gid} 失败: {e}");
                }
            }
            DownloadControl::CANCEL => {
                // forceRemove 立即移除（普通 remove 对活跃下载同样可能延迟生效）。
                if let Err(e) = client.force_remove(&gid).await {
                    log::warn!("[aria2] forceRemove {gid} 失败: {e}");
                }
                return Err("aria2c 下载任务已被取消".to_string());
            }
            _ => {}
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

        if initial_resumed_bytes.is_none() {
            let start_bytes = status.completed_bytes();
            initial_resumed_bytes = Some(start_bytes);
            is_resumed = had_partial_cache && start_bytes > 0;
        }

        last_num_pieces = status.num_pieces();
        last_piece_length = status.piece_length();
        if status.bitfield.is_some() {
            last_bitfield = status.bitfield.clone();
        }

        // 心跳日志（约 3s 一次）：诊断「进度冻结」「RPC 无响应」类问题的现场。
        if poll_count % 20 == 0 {
            log::info!(
                "[aria2] poll {poll_count}: gid={gid} status={} downloaded={} speed={} conn={}",
                status.status,
                status.completed_bytes(),
                status.speed_bytes_per_sec(),
                status.connections()
            );
        }
        poll_count += 1;

        // 循环周期本身即节流粒度（PROGRESS_POLL_INTERVAL_MS），无需二次判断。
        let total = if status.total_bytes() > 0 {
            status.total_bytes()
        } else {
            options.expected_size.unwrap_or(0)
        };
        let total_active_tasks = Aria2Lease::active_leases_count().await;
        on_progress(ProgressUpdate {
            downloaded_bytes: status.completed_bytes(),
            total_bytes: total,
            speed_bytes_per_sec: status.speed_bytes_per_sec(),
            port: lease.port,
            num_pieces: last_num_pieces,
            piece_length: last_piece_length,
            bitfield: status.bitfield.clone(),
            connections: status.connections(),
            is_resumed,
            resumed_bytes: initial_resumed_bytes.unwrap_or(0),
            total_active_tasks,
        });

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
        let total_active_tasks = Aria2Lease::active_leases_count().await;
        on_progress(ProgressUpdate {
            downloaded_bytes: metadata.len(),
            total_bytes: metadata.len(),
            speed_bytes_per_sec: 0,
            port: lease.port,
            num_pieces: last_num_pieces,
            piece_length: last_piece_length,
            bitfield: last_bitfield,
            connections: 0,
            is_resumed,
            resumed_bytes: initial_resumed_bytes.unwrap_or(0),
            total_active_tasks,
        });
    }

    if !verify_file_hash_and_size(
        &target_file,
        options.expected_size,
        options.expected_sha256.as_deref(),
    ) {
        let _ = fs::remove_file(&target_file);
        let aria2_meta_path = options.dest_dir.join(format!("{}.aria2", options.filename));
        let _ = fs::remove_file(&aria2_meta_path);
        return Err("文件完整性校验失败（大小或哈希不匹配）".to_string());
    }

    lease.release().await;

    Ok(target_file)
}
