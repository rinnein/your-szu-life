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

/// 下载落点。
///
/// 调用方只描述「下到哪个位置」；是否需要先写应用私有暂存目录再复制，由下载层
/// 根据 [`DownloadTarget::publish`] 自行处理，对调用方完全透明。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTarget {
    /// aria2 实际写入目录。桌面端就是最终目录；需要 SAF 复制时是暂存目录。
    pub work_dir: PathBuf,
    /// 最终目录的展示路径。
    pub display_dir: String,
    /// 需要在下载完成后复制到系统文档树时给出（Android SAF）。
    #[serde(default)]
    pub publish: Option<PublishTarget>,
}

/// 下载完成后把文件复制到 Android SAF 文档树的目标。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishTarget {
    /// SAF 文档树 URI。
    pub tree_uri: String,
    /// 树内相对目录，始终用 `/` 分隔。
    pub relative_dir: String,
    /// 该任务的暂存根目录；复制成功并校验后整体清理。
    pub staging_root: PathBuf,
}

impl DownloadTarget {
    /// 直接写入最终目录，不需要复制。
    pub fn local(dir: PathBuf, display: impl Into<String>) -> Self {
        Self {
            work_dir: dir,
            display_dir: display.into(),
            publish: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DownloadJobOptions {
    pub uris: Vec<String>,
    pub prioritized_uris: Vec<(u32, String)>,
    pub target: DownloadTarget,
    pub filename: String,
    pub expected_sha256: Option<String>,
    pub expected_size: Option<u64>,
    /// 附加的 HTTP 请求头（部分站点要求 Referer 等才可下载）。
    pub headers: Vec<(String, String)>,
}

/// 下载阶段。复制到最终目录属于一次下载的一部分，前端据此显示「保存中」。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadPhase {
    #[default]
    Downloading,
    Saving,
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
    /// 当前阶段；`Saving` 表示 aria2 已结束、正在复制到最终目录。
    #[serde(default)]
    pub phase: DownloadPhase,
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

    let dest_dir = options.target.work_dir.clone();
    fs::create_dir_all(&dest_dir).map_err(|e| format!("创建下载目标目录失败: {e}"))?;

    // 需要 SAF 复制时，先把最终目录里已有的同名文件取回暂存目录，
    // aria2 才能从它断点续传，而不是从零重下。
    if let Some(publish) = &options.target.publish {
        let publish = publish.clone();
        let staging_file = dest_dir.join(&options.filename);
        tokio::task::spawn_blocking(move || {
            crate::file_manager::seed_staging(&publish, &staging_file)
        })
        .await
        .map_err(|error| format!("准备续传文件失败: {error}"))??;
    }

    let target_file = dest_dir.join(&options.filename);
    let aria2_meta_path = dest_dir.join(format!("{}.aria2", options.filename));
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
        client.add_metalink(&b64, &dest_dir).await?
    } else {
        client
            .add_uri(&options.uris, &dest_dir, &options.filename, &options.headers)
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
            phase: DownloadPhase::Downloading,
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
    let final_bytes = fs::metadata(&target_file)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let total_active_tasks = Aria2Lease::active_leases_count().await;
    on_progress(ProgressUpdate {
        downloaded_bytes: final_bytes,
        total_bytes: final_bytes,
        speed_bytes_per_sec: 0,
        port: lease.port,
        num_pieces: last_num_pieces,
        piece_length: last_piece_length,
        bitfield: last_bitfield.clone(),
        connections: 0,
        is_resumed,
        resumed_bytes: initial_resumed_bytes.unwrap_or(0),
        total_active_tasks,
        phase: DownloadPhase::Downloading,
    });

    if !verify_file_hash_and_size(
        &target_file,
        options.expected_size,
        options.expected_sha256.as_deref(),
    ) {
        let _ = fs::remove_file(&target_file);
        let aria2_meta_path = dest_dir.join(format!("{}.aria2", options.filename));
        let _ = fs::remove_file(&aria2_meta_path);
        return Err("文件完整性校验失败（大小或哈希不匹配）".to_string());
    }

    let port = lease.port;
    lease.release().await;

    if let Some(publish) = &options.target.publish {
        // aria2 已结束，接下来由下载层负责复制到最终目录；调用方只看 phase。
        on_progress(ProgressUpdate {
            downloaded_bytes: final_bytes,
            total_bytes: final_bytes,
            speed_bytes_per_sec: 0,
            port,
            num_pieces: last_num_pieces,
            piece_length: last_piece_length,
            bitfield: last_bitfield,
            connections: 0,
            is_resumed,
            resumed_bytes: initial_resumed_bytes.unwrap_or(0),
            total_active_tasks: Aria2Lease::active_leases_count().await,
            phase: DownloadPhase::Saving,
        });
        let publish_task = publish.clone();
        let publish_dir = dest_dir.clone();
        let publish_name = options.filename.clone();
        let expected_size = options.expected_size;
        tokio::task::spawn_blocking(move || {
            crate::file_manager::publish_staging(
                &publish_task,
                &publish_dir,
                &publish_name,
                expected_size,
            )
        })
        .await
        .map_err(|error| format!("保存文件任务失败: {error}"))??;
        crate::file_manager::clear_staging(publish);
    }

    Ok(target_file)
}
