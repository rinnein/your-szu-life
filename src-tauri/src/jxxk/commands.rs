use super::*;

fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问巡课数据".into());
    }
    Ok(())
}

/// 获取巡课快照（会话状态与学年学期列表）。
#[tauri::command]
pub(crate) async fn get_jxxk_snapshot(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    local(&window)?;
    let service = service(&app)?;
    service.ensure(false).await;
    Ok(service.snapshot())
}

/// 强制刷新巡课快照（重新建立会话并拉取学期列表）。
#[tauri::command]
pub(crate) async fn refresh_jxxk(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    local(&window)?;
    let service = service(&app)?;
    service.ensure(true).await;
    Ok(service.snapshot())
}

/// 拉取课程列表。`year_semester_id` 传空表示全部学期。
#[tauri::command]
pub(crate) async fn jxxk_list_courses(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    year_semester_id: Option<String>,
    search_key: Option<String>,
    page_index: Option<u32>,
    page_size: Option<u32>,
) -> Result<CoursePage, String> {
    local(&window)?;
    let service = service(&app)?;
    service
        .list_courses(
            year_semester_id.as_deref(),
            search_key.as_deref(),
            page_index.unwrap_or(1),
            page_size.unwrap_or(100),
        )
        .await
}

/// 拉取课次列表（一门课程的直播与录播）。
#[tauri::command]
pub(crate) async fn jxxk_list_lessons(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    course_id: String,
    page_index: Option<u32>,
    page_size: Option<u32>,
) -> Result<LessonPage, String> {
    local(&window)?;
    let service = service(&app)?;
    service
        .list_lessons(&course_id, page_index.unwrap_or(1), page_size.unwrap_or(100))
        .await
}

/// 拉取一节课的多角度视频信息（教师/学生/电脑/主画面）。
#[tauri::command]
pub(crate) async fn jxxk_lesson_streams(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    live_id: String,
) -> Result<LessonStreams, String> {
    local(&window)?;
    let service = service(&app)?;
    service.lesson_streams(&live_id).await
}

/// 获取一节课各画面的 HLS 播放地址（带签名时效，即取即用）。
#[tauri::command]
pub(crate) async fn jxxk_play_urls(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    live_id: String,
) -> Result<Vec<PlayUrlScreen>, String> {
    local(&window)?;
    let service = service(&app)?;
    service.play_urls(&live_id).await
}

/// 获取各画面的明文 mp4 下载直链。
#[tauri::command]
pub(crate) async fn jxxk_download_urls(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    media_video_id: String,
) -> Result<Vec<DownloadUrl>, String> {
    local(&window)?;
    let service = service(&app)?;
    service.download_urls(&media_video_id).await
}

/// 获取直播流地址。直播未开始或已结束时返回业务错误。
#[tauri::command]
pub(crate) async fn jxxk_live_stream(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    live_id: String,
) -> Result<Vec<LiveStreamSource>, String> {
    local(&window)?;
    let service = service(&app)?;
    service.live_stream(&live_id).await
}

/// 启动一个录像下载任务（后台执行），返回任务 id。
#[tauri::command]
pub(crate) async fn jxxk_download(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    file_url: String,
    file_name: String,
    semester_name: String,
    course_name: String,
    teacher_name: String,
    lesson_name: String,
    stream_name: String,
    stream_no: Option<String>,
    media_video_id: String,
    expected_size: Option<u64>,
    subtitle_vtt: Option<String>,
) -> Result<String, String> {
    local(&window)?;
    download::start_download(
        &app,
        download::StartDownloadParams {
            semester_name,
            course_name,
            teacher_name,
            lesson_name,
            stream_name,
            stream_no,
            media_video_id,
            file_name,
            file_url,
            expected_size,
            subtitle_vtt,
        },
    )
    .await
}

/// 订阅下载任务列表：立即推送当前快照，此后每次变化自动推送。
#[tauri::command]
pub(crate) fn subscribe_jxxk_downloads(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    channel: tauri::ipc::Channel<Vec<download::TaskView>>,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    service.downloads.subscribe(channel);
    Ok(())
}

/// 列出全部下载任务快照。
#[tauri::command]
pub(crate) async fn jxxk_download_list(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Vec<download::TaskView>, String> {
    local(&window)?;
    let service = service(&app)?;
    Ok(service.downloads.list())
}

/// 暂停下载任务。
#[tauri::command]
pub(crate) async fn jxxk_download_pause(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    task_id: String,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    download::pause_task(&service.downloads, &task_id)
}

/// 继续下载任务。
#[tauri::command]
pub(crate) async fn jxxk_download_resume(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    task_id: String,
) -> Result<(), String> {
    local(&window)?;
    download::resume_task(&app, &task_id).await
}

/// 媒体 Channel 分片的 base64 长度上限：JSON 直发阈值 8192 留出余量，
/// 使每个分片都走 postMessage 模式下也可靠的直接 eval 通道。
const MEDIA_CHUNK_B64_LEN: usize = 6000;

/// 拉取一节课的平台生成字幕（WebVTT 文本）。
#[tauri::command]
pub(crate) async fn jxxk_subtitle(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    media_video_id: String,
) -> Result<String, String> {
    local(&window)?;
    let client = super::client_for_stream(&app).await?;
    let Some(token) = client.token() else {
        return Err("巡课会话未建立，无法获取字幕".into());
    };
    let path = format!(
        "university-live/protect/aiServer/video/{media_video_id}.vtt?token={token}"
    );
    let (bytes, content_type) = client.fetch_path(&path).await?;
    if !content_type.contains("text") && !content_type.contains("vtt") {
        return Err("该课次没有可用的字幕".into());
    }
    String::from_utf8(bytes).map_err(|_| "字幕内容编码无效".to_string())
}

/// 播放器媒体拉流：按 jxxk-media: 地址回源取 m3u8/分片/密钥，
/// base64 后分片经 Channel 推送（避免大载荷走可能被拦的 ipc:// fetch）。
#[tauri::command]
pub(crate) async fn jxxk_fetch_media(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    url: String,
    on_data: tauri::ipc::Channel<String>,
) -> Result<(), String> {
    use base64::Engine as _;
    local(&window)?;
    let target = super::stream::decode_media_url(&url)?;
    let client = super::client_for_stream(&app).await?;
    match client.fetch_stream(&target).await {
        Ok((bytes, content_type)) => {
            log::info!(
                "[jxxk] fetch_media: {} -> {} bytes ({})",
                target.as_str(),
                bytes.len(),
                content_type
            );
            // m3u8 需要把分片/密钥 URI 重写回 jxxk-media: 地址再交给 hls.js。
            let is_playlist = content_type.contains("mpegurl")
                || content_type.contains("m3u8")
                || target.path().ends_with(".m3u8");
            let bytes = if is_playlist {
                let text = String::from_utf8_lossy(&bytes).into_owned();
                super::stream::rewrite_m3u8(&target, &text).into_bytes()
            } else {
                bytes
            };
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            for chunk in encoded.as_bytes().chunks(MEDIA_CHUNK_B64_LEN) {
                on_data
                    .send(String::from_utf8_lossy(chunk).into_owned())
                    .map_err(|e| format!("推送媒体分片失败: {e}"))?;
            }
            Ok(())
        }
        Err(error) => {
            log::warn!("[jxxk] fetch_media 失败: {} -> {error}", target.as_str());
            Err(error)
        }
    }
}

/// 删除下载任务（可选一并清理落盘文件）。
#[tauri::command]
pub(crate) async fn jxxk_download_remove(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    task_id: String,
    delete_file: Option<bool>,
) -> Result<(), String> {
    local(&window)?;
    download::remove_task(&app, &task_id, delete_file.unwrap_or(true)).await
}
