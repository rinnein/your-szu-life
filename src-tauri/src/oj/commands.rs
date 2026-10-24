use super::*;
fn local(window: &tauri::WebviewWindow) -> Result<(), String> {
    if !(window.label() == "main" || window.label().starts_with("feature-")) {
        return Err("仅允许应用内 OJ 页面访问".into());
    }
    Ok(())
}
#[tauri::command]
pub(crate) fn get_oj_snapshot(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    local(&window)?;
    Ok(service(&app)?.snapshot())
}
#[tauri::command]
pub(crate) fn subscribe_oj_state(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    channel: tauri::ipc::Channel<super::OjDeltaEvent>,
) -> Result<(), String> {
    local(&window)?;
    let service = service(&app)?;
    let snapshot = service.snapshot();
    let _ = channel.send(super::OjDeltaEvent::Initial(Box::new(snapshot)));
    service.state_subscribers.subscribe(channel);
    Ok(())
}
#[tauri::command]
pub(crate) fn save_oj_site(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    input: SiteInput,
) -> Result<String, String> {
    local(&window)?;
    service(&app)?.save_site(input)
}
#[tauri::command]
pub(crate) fn remove_oj_site(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.remove(&site_id)
}
#[tauri::command]
pub(crate) async fn login_oj(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    token: Option<String>,
    value: Option<String>,
) -> Result<(), String> {
    local(&window)?;
    let s = service(&app)?;
    let e = s.entry(&site_id)?;
    let _guard = e.gate.lock().await;
    let result = s.login(&site_id, &e, token.zip(value)).await;
    if result.is_ok() {
        e.state.lock().unwrap().next_sync = 0;
    }
    result
}
#[tauri::command]
pub(crate) async fn get_oj_captcha(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    refresh: bool,
) -> Result<Captcha, String> {
    local(&window)?;
    let s = service(&app)?;
    let e = s.entry(&site_id)?;
    let _guard = e.gate.lock().await;
    if !refresh {
        if let Some((token, _, image)) = &e.state.lock().unwrap().captcha {
            return Ok(Captcha {
                token: token.clone(),
                image: image.clone(),
            });
        }
    }
    s.challenge(&site_id, &e).await
}
#[tauri::command]
pub(crate) async fn auto_recognize_oj_captcha(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
) -> Result<String, String> {
    local(&window)?;
    let s = service(&app)?;
    let e = s.entry(&site_id)?;
    let _guard = e.gate.lock().await;
    let image = {
        let state = e.state.lock().unwrap();
        state
            .captcha
            .as_ref()
            .map(|c| c.2.clone())
            .ok_or_else(|| "当前没有可识别的验证码".to_string())?
    };
    use base64::Engine;
    let raw = image.split_once(',').ok_or("验证码格式不支持")?.1;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| "验证码编码无效")?;
    let task = tauri::async_runtime::spawn_blocking(move || crate::ocr::recognize(&bytes));
    let answer_result = tokio::time::timeout(std::time::Duration::from_secs(15), task)
        .await
        .map_err(|_| "识别超时".to_string())?
        .map_err(|e| e.to_string())?;
    let answer = answer_result.map_err(|e| {
        if e == "ocr_model_missing" || e == "ocr_model_invalid" {
            e
        } else {
            "captcha_recognition_failed".to_string()
        }
    })?;
    if answer.is_empty() {
        return Err("captcha_recognition_failed".into());
    }
    Ok(answer)
}
#[tauri::command]
pub(crate) async fn get_oj_groups(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
) -> Result<Vec<Group>, String> {
    local(&window)?;
    service(&app)?.groups(&site_id).await
}
#[tauri::command]
pub(crate) async fn get_oj_contests(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    group_id: String,
) -> Result<Vec<Contest>, String> {
    local(&window)?;
    service(&app)?.contests(&site_id, &group_id).await
}
#[tauri::command]
pub(crate) async fn get_oj_progress(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
    force: Option<bool>,
    account: Option<String>,
    session_generation: Option<u64>,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?
        .progress(
            &site_id,
            &contest_id,
            force.unwrap_or(false),
            account.as_deref(),
            session_generation,
        )
        .await
}
#[tauri::command]
pub(crate) async fn get_oj_ranking(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
    offset: u32,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?.ranking(&site_id, &contest_id, offset).await
}
#[tauri::command]
pub(crate) async fn get_oj_paper(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
) -> Result<Paper, String> {
    local(&window)?;
    service(&app)?.paper(&site_id, &contest_id).await
}
#[tauri::command]
pub(crate) fn save_oj_draft(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    input: DraftInput,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.save_draft(input)
}
#[tauri::command]
pub(crate) fn get_oj_draft(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
    problem_key: String,
    language: String,
) -> Result<Option<Draft>, String> {
    local(&window)?;
    service(&app)?.draft_for_language(&site_id, &contest_id, &problem_key, &language)
}
#[tauri::command]
pub(crate) async fn submit_oj(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    input: SubmitInput,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?.submit(input).await
}
#[tauri::command]
pub(crate) async fn get_oj_records(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
    kind: u8,
    problem_id: Option<String>,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?
        .records(&site_id, &contest_id, kind, problem_id.as_deref())
        .await
}
#[tauri::command]
pub(crate) async fn get_oj_result(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    submission_id: String,
    kind: u8,
    problem_id: Option<String>,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?
        .result(&site_id, &submission_id, kind, problem_id.as_deref())
        .await
}
#[tauri::command]
pub(crate) fn acknowledge_oj_submission(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    key: String,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.acknowledge(&site_id, &key)
}
#[tauri::command]
pub(crate) async fn send_oj_companion(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    contest_id: String,
    keys: Vec<String>,
    port: u16,
) -> Result<Value, String> {
    local(&window)?;
    service(&app)?
        .send_companion(&site_id, &contest_id, keys, port)
        .await
}
#[tauri::command]
pub(crate) async fn refresh_oj(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.scan(true).await;
    Ok(())
}
#[tauri::command]
pub(crate) fn read_oj_notice(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    local(&window)?;
    let s = service(&app)?;
    let mut r = s.runtime.lock().unwrap();
    if let Some(n) = r.data.notices.iter_mut().find(|n| n.id == id) {
        n.read = true
    }
    s.persist(&r)?;
    drop(r);
    s.publish();
    Ok(())
}
#[tauri::command]
pub(crate) async fn load_oj_image(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    site_id: String,
    url: String,
) -> Result<String, String> {
    local(&window)?;
    service(&app)?.image(&site_id, &url).await
}
#[tauri::command]
pub(crate) fn save_oj_preferences(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    input: PreferencesInput,
) -> Result<Preferences, String> {
    local(&window)?;
    service(&app)?.save_preferences(input)
}
#[tauri::command]
pub(crate) fn configure_oj_companion(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    enabled: bool,
    port: u16,
) -> Result<(), String> {
    local(&window)?;
    service(&app)?.configure_companion(enabled, port)
}
#[tauri::command]
pub(crate) fn set_oj_companion_status(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    port: u16,
    status: CompanionStatus,
) -> Result<(), String> {
    local(&window)?;
    if window.label() != "main" {
        return Err("仅主窗口可维护提交连接".into());
    }
    service(&app)?.companion_status(port, status);
    Ok(())
}
#[tauri::command]
pub(crate) fn activate_oj_companion(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
) -> Result<(), String> {
    local(&window)?;
    app.emit_to("main", "oj-companion-activate", ())
        .map_err(|_| "无法通知主窗口".into())
}
#[tauri::command]
pub(crate) async fn submit_oj_companion(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    input: CompanionSubmission,
) -> Result<CompanionResult, String> {
    local(&window)?;
    if window.label() != "main" {
        return Err("仅主窗口可接收 cph-ng 提交".into());
    }
    service(&app)?.companion_submit(input).await
}
