//! 评教相关 Tauri 命令（窗口白名单与 enrollment 一致）。

use super::model::{EvaluationConfig, EvaluationOverview, EvaluationRunEvent, RunOutcome};
use tauri::{ipc::Channel, Manager};

#[tauri::command]
pub async fn evaluation_overview(
    window: tauri::WebviewWindow,
    force: Option<bool>,
) -> std::result::Result<EvaluationOverview, String> {
    super::super::commands::local(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        super::overview(&app, force.unwrap_or(false)).map_err(|error| error.message)
    })
    .await
    .map_err(|_| "评教任务异常".to_string())?
}

#[tauri::command]
pub async fn evaluation_run(
    window: tauri::WebviewWindow,
    channel: Channel<EvaluationRunEvent>,
    comment: Option<String>,
    dry_run: Option<bool>,
) -> std::result::Result<RunOutcome, String> {
    super::super::commands::local(&window)?;
    let app = window.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        super::run(
            &app,
            comment.as_deref(),
            dry_run.unwrap_or(false),
            &mut |event| {
                let _ = channel.send(event);
            },
        )
        .map_err(|error| error.message)
    })
    .await
    .map_err(|_| "评教任务异常".to_string())?
}

#[tauri::command]
pub fn evaluation_save_config(
    app: tauri::AppHandle,
    comment: String,
) -> std::result::Result<EvaluationConfig, String> {
    super::save_comment(&app, &comment)
}
