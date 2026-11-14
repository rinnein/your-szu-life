//! 签到模块的 Tauri command 集中地。
//!
//! 注册清单（app.rs / build.rs AppManifest / permissions/default.toml /
//! src/tests.rs）由守卫测试交叉校验，新增命令必须四处同步。

use super::*;
use tauri::WebviewWindow;

use model::{Building, LastPhotoView, QwStatusView, ResolvedPoint, TimetableCache};

fn local(window: &WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内页面访问签到数据".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn signin_get_buildings(
    window: WebviewWindow,
    _app: tauri::AppHandle,
) -> Result<Vec<Building>, String> {
    local(&window)?;
    Ok(buildings::builtin().to_vec())
}

#[tauri::command]
pub async fn signin_get_timetable(
    window: WebviewWindow,
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<TimetableCache, String> {
    local(&window)?;
    let service = service(&app)?;
    service.refresh_timetable(force.unwrap_or(false)).await
}

#[tauri::command]
pub async fn signin_get_lms_rollcalls(
    window: WebviewWindow,
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<Vec<szu_lms::lms::rollcall::RollcallItem>, String> {
    local(&window)?;
    service(&app)?.lms_service()?.rollcall_list(force.unwrap_or(false)).await
}

/// 雷达默认点解析：LMS 用 `course_id` 作记忆键，SZU 用 `course_name`；
/// 二者至少传一个。
#[tauri::command]
pub async fn signin_resolve_radar_point(
    window: WebviewWindow,
    app: tauri::AppHandle,
    course_id: Option<i64>,
    course_name: Option<String>,
    course_title: String,
) -> Result<ResolvedPoint, String> {
    local(&window)?;
    let key = match (&course_name, course_id) {
        (Some(name), _) => szu_course_key(name),
        (None, Some(id)) => id.to_string(),
        _ => return Err("需要 course_id 或 course_name 之一".into()),
    };
    service(&app)?
        .resolve_radar_point(&key, &course_title)
        .await
}

#[tauri::command]
pub async fn signin_submit_lms_number(
    window: WebviewWindow,
    app: tauri::AppHandle,
    rollcall_id: i64,
    code: String,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.submit_lms_number(rollcall_id, code.trim()).await
}

#[tauri::command]
pub async fn signin_submit_lms_radar(
    window: WebviewWindow,
    app: tauri::AppHandle,
    rollcall_id: i64,
    course_id: i64,
    course_title: String,
    building_id: Option<i64>,
    custom_latitude: Option<f64>,
    custom_longitude: Option<f64>,
    // 自定义坐标是否为 WGS-84（浏览器定位读数）；false 表示已是 GCJ-02
    //（如「使用上一次成功的定位」恢复的点）。
    custom_is_wgs84: Option<bool>,
) -> Result<ResolvedPoint, String> {
    local(&window)?;
    let custom = match (custom_latitude, custom_longitude) {
        (Some(lat), Some(lng)) => Some((lat, lng, custom_is_wgs84.unwrap_or(true))),
        _ => None,
    };
    service(&app)?
        .submit_lms_radar(rollcall_id, course_id, &course_title, custom, building_id)
        .await
}

/// 扫码直提：解析二维码（rollcallId 以 QR JSON 内的值为准，`rollcall_id`
/// 参数仅作 QR 里缺失时的兜底）→ 校验有效期 → 提交；房间活跃时顺手共享
/// （需求约定：扫描提交成功后立即共享）。
#[tauri::command]
pub async fn signin_submit_lms_qr(
    window: WebviewWindow,
    app: tauri::AppHandle,
    rollcall_id: Option<i64>,
    raw: String,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    let mut fields = room::parse_qr_fields(&raw).ok_or("无效的签到二维码")?;
    if fields.rollcall_id == 0 {
        fields.rollcall_id = rollcall_id.ok_or("二维码缺少点名 ID")?;
    }
    let now = chrono::Utc::now().timestamp_millis();
    if room::qr_valid_until_ms(&fields, now) <= now {
        return Err("签到码已过期，请扫描最新的二维码".into());
    }
    service
        .submit_lms_qr(fields.rollcall_id, &fields.data, &service.device_id()?)
        .await?;
    // 已加入房间时立即共享（自己的提交也要让同伴受益）。
    if service.room_active() {
        let _ = service.qr_room_share(&fields);
    }
    Ok(())
}

#[tauri::command]
pub async fn signin_submit_szu(
    window: WebviewWindow,
    app: tauri::AppHandle,
    course_name: String,
    building_id: Option<i64>,
    custom_latitude: Option<f64>,
    custom_longitude: Option<f64>,
    custom_is_wgs84: Option<bool>,
    photo_b64: Option<String>,
) -> Result<(), String> {
    local(&window)?;
    let custom = match (custom_latitude, custom_longitude) {
        (Some(lat), Some(lng)) => Some((lat, lng, custom_is_wgs84.unwrap_or(true))),
        _ => None,
    };
    service(&app)?
        .submit_szu(&course_name, building_id, custom, photo_b64.as_deref())
        .await
}

#[tauri::command]
pub async fn signin_get_last_photo(
    window: WebviewWindow,
    app: tauri::AppHandle,
    course_name: String,
) -> Result<LastPhotoView, String> {
    local(&window)?;
    service(&app)?.last_photo(&course_name).await
}

#[tauri::command]
pub async fn signin_qr_room_join(
    window: WebviewWindow,
    app: tauri::AppHandle,
    course_id: i64,
    rollcall_id: Option<i64>,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    service.qr_room_join(course_id, rollcall_id).await
}

/// 网关（qw.szu.edu.cn）会话状态。`force` 时重新探活一次；
/// 统一认证完成后向导会调用它完成会话的自动签发。
#[tauri::command]
pub async fn signin_get_qw_status(
    window: WebviewWindow,
    app: tauri::AppHandle,
    force: Option<bool>,
) -> Result<QwStatusView, String> {
    local(&window)?;
    Ok(service(&app)?.qw_status(force.unwrap_or(false)).await)
}

#[tauri::command]
pub async fn signin_qr_room_leave(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    local(&window)?;
    // 用户主动停止：时间窗结束前不再自动为该课程建房。
    service(&app)?.qr_room_leave().await;
    Ok(())
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareOutcome {
    /// true = 新码已广播；false = 去重命中（重复码未重复共享）。
    pub shared: bool,
}

/// 共享一份扫码结果到当前房间（不自动提交；共享者自己通常已签到）。
#[tauri::command]
pub async fn signin_qr_room_share(
    window: WebviewWindow,
    app: tauri::AppHandle,
    raw: String,
) -> Result<ShareOutcome, String> {
    local(&window)?;
    let service = service(&app)?;
    let fields = room::parse_qr_fields(&raw).ok_or("无效的签到二维码")?;
    let shared = service.qr_room_share(&fields)?;
    if shared {
        let _ = service.qr_events.broadcast(&room::QrEvent::Shared {
            course_id: service
                .room
                .lock()
                .unwrap()
                .as_ref()
                .map(|session| session.course_id)
                .unwrap_or_default(),
            ok: true,
            message: "已共享 1 份签到码".into(),
        });
    }
    Ok(ShareOutcome { shared })
}

#[tauri::command]
pub fn subscribe_signin_qr_events(
    window: WebviewWindow,
    app: tauri::AppHandle,
    channel: tauri::ipc::Channel<room::QrEvent>,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.qr_events.subscribe(channel);
    Ok(())
}
