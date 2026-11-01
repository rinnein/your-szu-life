//! 录像下载任务注册表：多任务的进度上报、暂停/继续/删除。
//!
//! 每个任务在后台经 `aria2::download_with_aria2_controlled` 执行，进度写入
//! [`TaskCore`] 并节流广播 `jxxk-download-changed` 事件；管理页轮询
//! `jxxk_download_list` 或监听事件刷新。

use super::client::JXXK_ORIGIN;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use std::time::{Duration, Instant};
use axum::response::IntoResponse as _;
use tauri::{AppHandle, Manager};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum TaskStatus {
    /// 已登记，aria2 任务尚未建立。
    Preparing,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

pub(crate) struct TaskState {
    pub status: TaskStatus,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub error: Option<String>,
    pub created_at: i64,
    /// aria2 下载明细：分片位图等，供前端复用 DownloadSession 卡片。
    pub port: u16,
    pub num_pieces: u32,
    pub piece_length: u64,
    pub bitfield: Option<String>,
    pub connections: u32,
    pub is_resumed: bool,
    pub resumed_bytes: u64,
    pub total_active_tasks: u32,
}

/// 单个下载任务的共享核心。后台任务与命令通过它交互。
pub(crate) struct TaskCore {
    pub app: Option<AppHandle>,
    pub registry: std::sync::Weak<DownloadRegistry>,
    pub id: String,
    pub semester_name: String,
    pub course_name: String,
    pub teacher_name: String,
    /// 字幕/重定向解析所需的媒体视频 id。
    pub media_video_id: String,
    /// 码流序号（重定向时匹配对应角度的下载直链）。
    pub stream_no: Option<String>,
    pub lesson_name: String,
    pub stream_name: String,
    pub file_name: String,
    pub dest_dir: PathBuf,
    pub file_url: String,
    pub expected_size: Option<u64>,
    pub control: crate::aria2::DownloadControl,
    /// 后台任务结束信号：remove 需等 aria2 真正停止后再删文件。
    /// run_download 持有发送端，结束时 drop 关闭通道即视为完成。
    pub done: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    /// 应用重启后任务未接回 aria2（状态停在 Paused），resume 时重新拉起。
    pub needs_respawn: std::sync::atomic::AtomicBool,
    /// 缓存的最新直链与获取时间，用于 302 重定向时复用，防 16 并发连接惊群请求。
    pub latest_url: Mutex<(String, Instant)>,
    pub state: Mutex<TaskState>,
    /// 事件节流。
    last_emit: Mutex<Instant>,
}

impl TaskCore {
    fn view(&self) -> TaskView {
        let state = self.state.lock().unwrap();
        // 仅活跃下载/暂停状态带 bitfield，已完成或失败条目置空以防列表 JSON 膨胀。
        let bitfield = if matches!(state.status, TaskStatus::Downloading | TaskStatus::Paused) {
            state.bitfield.clone()
        } else {
            None
        };
        TaskView {
            id: self.id.clone(),
            course_name: self.course_name.clone(),
            lesson_name: self.lesson_name.clone(),
            stream_name: self.stream_name.clone(),
            file_name: self.file_name.clone(),
            status: state.status,
            downloaded: state.downloaded,
            total: state.total,
            speed: state.speed,
            error: state.error.clone(),
            created_at: state.created_at,
            port: state.port,
            num_pieces: state.num_pieces,
            piece_length: state.piece_length,
            bitfield,
            connections: state.connections,
            is_resumed: state.is_resumed,
            resumed_bytes: state.resumed_bytes,
            total_active_tasks: state.total_active_tasks,
        }
    }

    pub(crate) fn set_status(&self, status: TaskStatus, error: Option<String>) {
        {
            let mut state = self.state.lock().unwrap();
            state.status = status;
            state.error = error;
            if matches!(
                status,
                TaskStatus::Paused
                    | TaskStatus::Completed
                    | TaskStatus::Failed
                    | TaskStatus::Cancelled
            ) {
                state.speed = 0;
            }
        }
        // 状态变化（暂停/继续/完成/失败）必须即时可见，绕过进度节流。
        self.broadcast_change_force();
        persist_task(self);
    }

    /// 推送任务列表到订阅者，进度路径 500ms 节流。
    /// 锁仅在判定/刷新时间戳时持有，调用广播前立即释放，严防重入死锁。
    pub(crate) fn broadcast_change(&self) {
        {
            let mut last = self.last_emit.lock().unwrap();
            if last.elapsed() < Duration::from_millis(500) {
                return;
            }
            *last = Instant::now();
        }
        self.notify_subscribers();
    }

    pub(crate) fn broadcast_change_force(&self) {
        {
            let mut last = self.last_emit.lock().unwrap();
            *last = Instant::now();
        }
        self.notify_subscribers();
    }

    fn notify_subscribers(&self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.broadcast();
        } else if let Some(app) = &self.app {
            if let Ok(service) = super::service(app) {
                service.downloads.broadcast();
            }
        }
    }
}

/// 返回给前端的任务快照。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskView {
    pub id: String,
    pub course_name: String,
    pub lesson_name: String,
    pub stream_name: String,
    pub file_name: String,
    pub status: TaskStatus,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub error: Option<String>,
    pub created_at: i64,
    pub port: u16,
    pub num_pieces: u32,
    pub piece_length: u64,
    pub bitfield: Option<String>,
    pub connections: u32,
    pub is_resumed: bool,
    pub resumed_bytes: u64,
    pub total_active_tasks: u32,
}

#[derive(Default)]
pub(crate) struct DownloadRegistry {
    tasks: Mutex<Vec<Arc<TaskCore>>>,
    /// 任务列表订阅者：前端经 Channel 接收全量快照推送。
    subscribers: crate::ipc_channel::SubscriberPool<Vec<TaskView>>,
}

impl DownloadRegistry {
    pub(crate) fn register(&self, core: Arc<TaskCore>) {
        self.tasks.lock().unwrap().insert(0, core);
        self.broadcast();
    }

    pub(crate) fn get(&self, id: &str) -> Option<Arc<TaskCore>> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .find(|task| task.id == id)
            .cloned()
    }

    /// 摘除任务条目；后台任务随后自行退出。
    pub(crate) fn detach(&self, id: &str) -> Option<Arc<TaskCore>> {
        let mut tasks = self.tasks.lock().unwrap();
        let pos = tasks.iter().position(|task| task.id == id)?;
        let removed = tasks.remove(pos);
        drop(tasks);
        self.broadcast();
        Some(removed)
    }

    pub(crate) fn list(&self) -> Vec<TaskView> {
        self.tasks
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.view())
            .collect()
    }

    /// 订阅任务列表：立即推送当前快照，此后每次变化自动推送。
    pub(crate) fn subscribe(&self, channel: tauri::ipc::Channel<Vec<TaskView>>) {
        channel.send(self.list()).ok();
        self.subscribers.subscribe(channel);
    }

    /// 广播任务列表快照到全部订阅者。
    pub(crate) fn broadcast(&self) {
        self.subscribers.broadcast(&self.list());
    }
}

pub(crate) fn sanitize_filename(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string()
}

/// 跨重启持久化的任务记录（存于 jxxk/state.sqlite，键 `task:<id>`）。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersistedTask {
    pub id: String,
    pub semester_name: String,
    pub course_name: String,
    pub teacher_name: String,
    pub lesson_name: String,
    pub stream_name: String,
    pub stream_no: Option<String>,
    pub media_video_id: String,
    pub file_name: String,
    pub dest_dir: String,
    pub file_url: String,
    pub expected_size: Option<u64>,
    pub status: TaskStatus,
    pub created_at: i64,
}

fn persisted_from_core(core: &TaskCore) -> PersistedTask {
    let state = core.state.lock().unwrap();
    PersistedTask {
        id: core.id.clone(),
        semester_name: core.semester_name.clone(),
        course_name: core.course_name.clone(),
        teacher_name: core.teacher_name.clone(),
        lesson_name: core.lesson_name.clone(),
        stream_name: core.stream_name.clone(),
        stream_no: core.stream_no.clone(),
        media_video_id: core.media_video_id.clone(),
        file_name: core.file_name.clone(),
        dest_dir: core.dest_dir.to_string_lossy().into_owned(),
        file_url: core.file_url.clone(),
        expected_size: core.expected_size,
        status: state.status,
        created_at: state.created_at,
    }
}

/// 把任务记录写进 SQLite（状态变化时调用，非进度节流路径）。
fn persist_task(core: &TaskCore) {
    let Some(app) = &core.app else {
        return;
    };
    let record = persisted_from_core(core);
    let db = match super::service(app) {
        Ok(service) => service.db.clone(),
        Err(error) => {
            log::warn!("[jxxk] 任务记录落盘失败（服务不可用）: {error}");
            return;
        }
    };
    tauri::async_runtime::spawn(async move {
        if let Err(error) = db
            .save(&super::db::CacheEntry {
                id: format!("task:{}", record.id),
                payload_json: serde_json::to_string(&record)
                    .unwrap_or_else(|_| "null".to_string()),
                updated_at_ms: chrono::Utc::now().timestamp_millis(),
            })
            .await
        {
            log::warn!("[jxxk] 任务记录落盘失败: {error}");
        }
    });
}

/// 从持久化记录重建任务核心（应用重启后恢复管理列表）。
#[allow(clippy::too_many_arguments)]
pub(crate) fn task_core_from_record(
    app: Option<AppHandle>,
    record: PersistedTask,
    status: TaskStatus,
    needs_respawn: bool,
    registry: std::sync::Weak<DownloadRegistry>,
) -> Arc<TaskCore> {
    Arc::new(TaskCore {
        app,
        registry,
        id: record.id,
        semester_name: record.semester_name,
        course_name: record.course_name,
        teacher_name: record.teacher_name,
        media_video_id: record.media_video_id,
        stream_no: record.stream_no,
        lesson_name: record.lesson_name,
        stream_name: record.stream_name,
        file_name: record.file_name,
        dest_dir: PathBuf::from(&record.dest_dir),
        file_url: record.file_url.clone(),
        expected_size: record.expected_size,
        control: crate::aria2::DownloadControl::new(),
        done: Mutex::new(None),
        needs_respawn: std::sync::atomic::AtomicBool::new(needs_respawn),
        latest_url: Mutex::new((record.file_url, Instant::now() - Duration::from_secs(3600))),
        state: Mutex::new(TaskState {
            status,
            downloaded: 0,
            total: record.expected_size.unwrap_or(0),
            speed: 0,
            error: None,
            created_at: record.created_at,
            port: 0,
            num_pieces: 0,
            piece_length: 0,
            bitfield: None,
            connections: 0,
            is_resumed: false,
            resumed_bytes: 0,
            total_active_tasks: 0,
        }),
        last_emit: Mutex::new(Instant::now() - Duration::from_millis(600)),
    })
}

static DOWNLOAD_SERVER_PORT: OnceLock<u16> = OnceLock::new();
static DOWNLOAD_SERVER_GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 解析本地下载请求：返回该任务当前有效的下载直链（每次实时获取，永不过期）。
async fn resolve_download_url(app: AppHandle, task_id: String) -> Result<String, String> {
    let service = super::service(&app)?;
    let core = service.downloads.get(&task_id).ok_or("任务不存在")?;

    // 1. 优先复用 5 分钟内获取的有效直链，避免 aria2 16 个并发分片连接同时打向学校平台
    {
        let cached = core.latest_url.lock().unwrap();
        if !cached.0.is_empty() && cached.1.elapsed() < Duration::from_secs(300) {
            return Ok(cached.0.clone());
        }
    }

    // 2. 直链过期或首次拉取，向平台请求最新直链
    let urls = service.download_urls(&core.media_video_id).await?;
    let target = urls
        .iter()
        .find(|u| {
            u.stream_no.as_deref() == core.stream_no.as_deref()
                || u.stream_name.as_deref() == Some(core.stream_name.as_str())
        })
        .ok_or_else(|| "未找到该画面的下载地址".to_string())?;
    let fresh = target.file_url.clone();
    drop(urls);
    // 校验目标仍在深大域内
    let parsed = reqwest::Url::parse(&fresh).map_err(|_| "下载地址无效".to_string())?;
    let Some(host) = parsed.host_str().map(|h| h.trim_end_matches('.')) else {
        return Err("下载地址缺少主机名".into());
    };
    if parsed.scheme() != "https" || !(host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")) {
        return Err(format!("下载地址不受信任: {host}"));
    }

    // 更新缓存
    *core.latest_url.lock().unwrap() = (fresh.clone(), Instant::now());
    Ok(fresh)
}

/// 启动本地下载入口服务器（127.0.0.1 随机端口，进程级单例）。
/// aria2 下载的 URI 指向此服务器；服务器 302 重定向到实时解析的新直链，
/// 使 URL 过期/更换不再影响已下载的分片（aria2 重试时重新经过本入口）。
async fn ensure_download_server(app: &AppHandle) -> Result<u16, String> {
    let _guard = DOWNLOAD_SERVER_GUARD.lock().await;
    if let Some(port) = DOWNLOAD_SERVER_PORT.get() {
        return Ok(*port);
    }
    let app_handle = app.clone();
    let router = axum::Router::new().route(
        "/jxxk-download/{task_id}",
        axum::routing::get(move |axum::extract::Path(task_id): axum::extract::Path<String>| async move {
            let response = match resolve_download_url(app_handle.clone(), task_id).await {
                Ok(fresh) => axum::http::Response::builder()
                    .status(axum::http::StatusCode::FOUND)
                    .header("Location", fresh)
                    .body(axum::body::Body::empty()),
                Err(message) => axum::http::Response::builder()
                    .status(axum::http::StatusCode::NOT_FOUND)
                    .header("Content-Type", "text/plain; charset=utf-8")
                    .body(axum::body::Body::from(message)),
            };
            match response {
                Ok(response) => response,
                Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("绑定下载入口端口失败: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("读取下载入口端口失败: {e}"))?
        .port();
    let _ = DOWNLOAD_SERVER_PORT.set(port);
    log::info!("[jxxk] 下载入口服务器已启动: 127.0.0.1:{port}");
    tauri::async_runtime::spawn(async move {
        if let Err(error) = axum::serve(listener, router).await {
            log::error!("[jxxk] 下载入口服务器退出: {error}");
        }
    });
    Ok(port)
}

/// 启动下载的入参。
pub(crate) struct StartDownloadParams {
    pub semester_name: String,
    pub course_name: String,
    pub teacher_name: String,
    pub lesson_name: String,
    pub stream_name: String,
    pub stream_no: Option<String>,
    pub media_video_id: String,
    pub file_name: String,
    /// aria2 实际抓取的入口为本地链接（重定向到实时直链），此字段仅存档。
    pub file_url: String,
    pub expected_size: Option<u64>,
    /// 页面打开时已拉取的平台字幕（WebVTT 文本），随任务一并落盘。
    pub subtitle_vtt: Option<String>,
}

/// 启动一个下载任务：登记、派发后台执行，返回任务 id。
pub(crate) async fn start_download(
    app: &AppHandle,
    params: StartDownloadParams,
) -> Result<String, String> {
    let url = reqwest::Url::parse(&params.file_url).map_err(|_| "下载地址无效".to_string())?;
    let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
        return Err("下载地址缺少主机名".into());
    };
    if url.scheme() != "https" || !(host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")) {
        return Err(format!("下载地址不受信任: {host}"));
    }
    let safe_name = sanitize_filename(&params.file_name);
    if safe_name.is_empty() {
        return Err("文件名无效".into());
    }
    let lesson = sanitize_filename(&params.lesson_name);

    // 目录规整：下载/jxxk/<学期>-<课程>(<老师>)/<选集>/
    let mut dest_dir = app
        .path()
        .download_dir()
        .map_err(|_| "无法确定下载目录".to_string())?;
    dest_dir.push("jxxk");
    let mut group = params.semester_name.clone();
    if !group.is_empty() {
        group.push('-');
    }
    group.push_str(&params.course_name);
    if !params.teacher_name.is_empty() {
        group.push_str(&format!("({})", params.teacher_name));
    }
    let group = sanitize_filename(&group);
    if !group.is_empty() {
        dest_dir.push(group);
    }
    if !lesson.is_empty() {
        dest_dir.push(lesson.clone());
    }
    // 字幕随选集目录一并落盘（直接写文件，不过 aria2、不进任务列表）。
    if let Some(vtt) = &params.subtitle_vtt {
        if !lesson.is_empty() && vtt.starts_with("WEBVTT") {
            std::fs::create_dir_all(&dest_dir)
                .map_err(|e| format!("创建下载目标目录失败: {e}"))?;
            let vtt_path = dest_dir.join(format!("{lesson}.vtt"));
            if let Err(error) = std::fs::write(&vtt_path, vtt) {
                log::warn!("[jxxk] 字幕落盘失败 {}: {error}", vtt_path.display());
            }
        }
    }

    let service = super::service(app)?;
    let id = uuid::Uuid::new_v4().to_string();
    let core = Arc::new(TaskCore {
        app: Some(app.clone()),
        registry: Arc::downgrade(&service.downloads),
        id: id.clone(),
        semester_name: params.semester_name.clone(),
        course_name: params.course_name,
        teacher_name: params.teacher_name.clone(),
        media_video_id: params.media_video_id,
        stream_no: params.stream_no.clone(),
        lesson_name: params.lesson_name,
        stream_name: params.stream_name,
        file_name: safe_name.clone(),
        dest_dir: dest_dir.clone(),
        file_url: params.file_url.clone(),
        expected_size: params.expected_size,
        control: crate::aria2::DownloadControl::new(),
        done: Mutex::new(None),
        needs_respawn: std::sync::atomic::AtomicBool::new(false),
        latest_url: Mutex::new((params.file_url, Instant::now())),
        state: Mutex::new(TaskState {
            status: TaskStatus::Preparing,
            downloaded: 0,
            total: params.expected_size.unwrap_or(0),
            speed: 0,
            error: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            port: 0,
            num_pieces: 0,
            piece_length: 0,
            bitfield: None,
            connections: 0,
            is_resumed: false,
            resumed_bytes: 0,
            total_active_tasks: 0,
        }),
        last_emit: Mutex::new(Instant::now() - Duration::from_millis(600)),
    });
    service.downloads.register(core.clone());
    persist_task(&core);

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        run_download(app_handle, core).await;
    });
    Ok(id)
}

/// 后台执行体：aria2 下载 + 进度写回 + 事件广播。
async fn run_download(app: AppHandle, core: Arc<TaskCore>) {
    let port = match ensure_download_server(&app).await {
        Ok(port) => port,
        Err(error) => {
            core.set_status(TaskStatus::Failed, Some(error.clone()));
            log::warn!("[jxxk] 下载入口不可用: {error}");
            return;
        }
    };
    // aria2 抓取本地入口链接：每次（重）请求都 302 到实时解析的新直链，
    // URL 过期只需重新请求入口，已下载分片全部保留。
    let entry_url = format!("http://127.0.0.1:{port}/jxxk-download/{}", core.id);
    let options = crate::aria2::DownloadJobOptions {
        uris: vec![entry_url],
        prioritized_uris: Vec::new(),
        dest_dir: core.dest_dir.clone(),
        filename: core.file_name.clone(),
        expected_sha256: None,
        expected_size: core.expected_size,
        headers: vec![("Referer".to_string(), JXXK_ORIGIN.to_string())],
    };
    // 完成信号：remove 任务等待该信号（或通道关闭）后再清理文件。
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    *core.done.lock().unwrap() = Some(done_rx);
    let progress_core = core.clone();
    let result = crate::aria2::download_with_aria2_controlled(
        Some(&app),
        options,
        &core.control,
        move |update| {
            {
                let mut state = progress_core.state.lock().unwrap();
                if state.status == TaskStatus::Paused {
                    // 暂停期间仍刷新字节数，但保持 Paused 状态。
                    state.downloaded = update.downloaded_bytes;
                    state.total = update.total_bytes;
                    state.speed = 0;
                    drop(state);
                    progress_core.broadcast_change();
                    return;
                }
                state.status = TaskStatus::Downloading;
                state.downloaded = update.downloaded_bytes;
                state.total = update.total_bytes;
                state.speed = update.speed_bytes_per_sec;
                state.port = update.port;
                state.num_pieces = update.num_pieces;
                state.piece_length = update.piece_length;
                state.bitfield = update.bitfield.clone();
                state.connections = update.connections;
                state.is_resumed = update.is_resumed;
                state.resumed_bytes = update.resumed_bytes;
                state.total_active_tasks = update.total_active_tasks;
            }
            progress_core.broadcast_change();
        },
    )
    .await;

    match result {
        Ok(path) => {
            core.set_status(TaskStatus::Completed, None);
            log::info!("[jxxk] 下载完成: {} ({})", core.file_name, path.display());
        }
        Err(error) => {
            let cancelled = error.contains("已被取消");
            core.set_status(
                if cancelled {
                    TaskStatus::Cancelled
                } else {
                    TaskStatus::Failed
                },
                Some(error.clone()),
            );
            log::warn!("[jxxk] 下载结束（{}）: {}", core.file_name, error);
        }
    }
    // 通知等待方：任务已真正退出，可以安全清理文件。
    let _ = done_tx.send(());
}

/// 暂停任务（aria2.pause 在下一轮询生效）。
pub(crate) fn pause_task(registry: &DownloadRegistry, id: &str) -> Result<(), String> {
    let core = registry.get(id).ok_or("任务不存在")?;
    let status = core.state.lock().unwrap().status;
    if !matches!(status, TaskStatus::Downloading | TaskStatus::Preparing) {
        return Err("任务当前不可暂停".into());
    }
    core.control.pause();
    core.set_status(TaskStatus::Paused, None);
    Ok(())
}

/// 继续任务。应用重启后的中断任务（needs_respawn）重新拉起 aria2 下载
/// （.aria2 控制文件仍在，aria2 --continue 从断点续传）。
pub(crate) async fn resume_task(app: &AppHandle, id: &str) -> Result<(), String> {
    let service = super::service(app)?;
    let core = service.downloads.get(id).ok_or("任务不存在")?;
    let status = core.state.lock().unwrap().status;
    if !matches!(
        status,
        TaskStatus::Paused | TaskStatus::Failed | TaskStatus::Cancelled
    ) {
        return Err("任务当前不可继续或重试".into());
    }
    if core
        .needs_respawn
        .load(std::sync::atomic::Ordering::SeqCst)
        || matches!(status, TaskStatus::Failed | TaskStatus::Cancelled)
    {
        core.needs_respawn
            .store(false, std::sync::atomic::Ordering::SeqCst);
        let app_handle = app.clone();
        let core_clone = core.clone();
        tauri::async_runtime::spawn(async move {
            run_download(app_handle, core_clone).await;
        });
        core.set_status(TaskStatus::Downloading, None);
        return Ok(());
    }
    core.control.resume();
    core.set_status(TaskStatus::Downloading, None);
    Ok(())
}

/// 删除任务：活动任务先取消，等 aria2 真正停止后再清理条目与文件
/// （否则会删掉仍被写入的文件、留下重建的 .aria2 控制文件）。
pub(crate) async fn remove_task(
    app: &AppHandle,
    id: &str,
    delete_file: bool,
) -> Result<(), String> {
    let service = super::service(app)?;
    let core = service.downloads.get(id).ok_or("任务不存在")?;
    let status = core.state.lock().unwrap().status;
    let was_active = matches!(
        status,
        TaskStatus::Downloading | TaskStatus::Preparing | TaskStatus::Paused
    );
    if was_active {
        core.control.cancel();
        // 等后台任务处理完 CANCEL 并退出（最多 15s）。
        let rx = core.done.lock().unwrap().take();
        if let Some(rx) = rx {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(15), rx).await;
        }
    }
    service.downloads.detach(id);
    let db = service.db.clone();
    let task_row_id = format!("task:{id}");
    tauri::async_runtime::spawn(async move {
        if let Err(error) = db.delete(&task_row_id).await {
            log::warn!("[jxxk] 删除任务记录失败: {error}");
        }
    });
    if delete_file {
        let target = core.dest_dir.join(&core.file_name);
        let meta = core.dest_dir.join(format!("{}.aria2", core.file_name));
        let _ = std::fs::remove_file(&target);
        let _ = std::fs::remove_file(&meta);
        log::info!("[jxxk] 已删除任务与未完成文件: {}", target.display());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn test_task_core(
    registry: std::sync::Weak<DownloadRegistry>,
    status: TaskStatus,
) -> Arc<TaskCore> {
    Arc::new(TaskCore {
        app: None,
        registry,
        id: "test-task-1".to_string(),
        semester_name: "2026-2027-1".to_string(),
        course_name: "测试课程".to_string(),
        teacher_name: "教师".to_string(),
        media_video_id: "media-1".to_string(),
        stream_no: Some("1".to_string()),
        lesson_name: "第1节".to_string(),
        stream_name: "教师画面".to_string(),
        file_name: "教师画面.mp4".to_string(),
        dest_dir: std::env::temp_dir(),
        file_url: "https://jxxk.szu.edu.cn/test.mp4".to_string(),
        expected_size: Some(1024),
        control: crate::aria2::DownloadControl::new(),
        done: Mutex::new(None),
        needs_respawn: std::sync::atomic::AtomicBool::new(false),
        latest_url: Mutex::new((
            "https://jxxk.szu.edu.cn/test.mp4".to_string(),
            Instant::now(),
        )),
        state: Mutex::new(TaskState {
            status,
            downloaded: 0,
            total: 1024,
            speed: 0,
            error: None,
            created_at: chrono::Utc::now().timestamp_millis(),
            port: 0,
            num_pieces: 0,
            piece_length: 0,
            bitfield: None,
            connections: 0,
            is_resumed: false,
            resumed_bytes: 0,
            total_active_tasks: 0,
        }),
        last_emit: Mutex::new(Instant::now() - Duration::from_millis(600)),
    })
}
