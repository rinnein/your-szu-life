use super::AppContentStore;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tauri::Emitter;

pub use super::model::UpdateProgressPayload;

pub(crate) static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();
static IS_UPDATING: AtomicBool = AtomicBool::new(false);
static UPDATE_CHANNEL: Mutex<Option<tauri::ipc::Channel<UpdateProgressPayload>>> = Mutex::new(None);

pub(crate) fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

pub(crate) fn emit_progress(payload: UpdateProgressPayload) {
    if let Some(ref ch) = *UPDATE_CHANNEL.lock().unwrap() {
        let _ = ch.send(payload.clone());
    }
    if let Some(app) = APP_HANDLE.get() {
        let _ = app.emit("in-app-update-progress", payload);
    }
}

pub(crate) fn start_update(
    store: &AppContentStore,
    channel: tauri::ipc::Channel<UpdateProgressPayload>,
) -> Result<(), String> {
    if IS_UPDATING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("更新任务已在进行中".to_string());
    }
    *UPDATE_CHANNEL.lock().unwrap() = Some(channel);

    let snapshot = store.snapshot();
    let update = snapshot.update.data.ok_or_else(|| {
        IS_UPDATING.store(false, Ordering::SeqCst);
        "当前未检测到可用更新".to_string()
    })?;

    let direct_url = update.latest_ver_url.clone();
    if !direct_url.to_ascii_lowercase().contains(".msi") {
        IS_UPDATING.store(false, Ordering::SeqCst);
        return Err("未检测到可用的 MSI 安装包更新".to_string());
    }
    let version = update.latest_ver.clone();
    let candidates = store.build_download_candidates(&direct_url);

    std::thread::spawn(move || {
        let result = download_and_install(&version, candidates);
        IS_UPDATING.store(false, Ordering::SeqCst);
        if let Err(err_msg) = result {
            emit_progress(UpdateProgressPayload {
                status: "error".to_string(),
                progress: 0,
                downloaded_bytes: 0,
                total_bytes: 0,
                speed_bytes_per_sec: None,
                port: None,
                num_pieces: None,
                piece_length: None,
                bitfield: None,
                connections: None,
                is_resumed: None,
                resumed_bytes: None,
                total_active_tasks: None,
                error: Some(err_msg),
            });
        }
        let _ = UPDATE_CHANNEL.lock().unwrap().take();
    });

    Ok(())
}

pub(crate) fn download_and_install(
    version: &str,
    candidates: Vec<(u32, String)>,
) -> Result<(), String> {
    emit_progress(UpdateProgressPayload {
        status: "downloading".to_string(),
        progress: 0,
        downloaded_bytes: 0,
        total_bytes: 0,
        speed_bytes_per_sec: None,
        port: None,
        num_pieces: None,
        piece_length: None,
        bitfield: None,
        connections: None,
        is_resumed: None,
        resumed_bytes: None,
        total_active_tasks: None,
        error: None,
    });

    let safe_version: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
        .collect();
    let filename = format!("your-szu-life-installer-{safe_version}.msi");
    let dest_dir = std::env::temp_dir();

    let aria_options = crate::aria2::DownloadJobOptions {
        uris: Vec::new(),
        prioritized_uris: candidates,
        dest_dir: dest_dir.clone(),
        filename: filename.clone(),
        expected_sha256: None,
        expected_size: None,
    };

    let aria_res = tauri::async_runtime::block_on(crate::aria2::download_with_aria2(
        APP_HANDLE.get(),
        aria_options,
        |prog| {
            let progress = if prog.total_bytes > 0 {
                ((prog.downloaded_bytes as f64 / prog.total_bytes as f64) * 100.0).clamp(0.0, 100.0)
                    as u32
            } else {
                0
            };
            emit_progress(UpdateProgressPayload {
                status: "downloading".to_string(),
                progress,
                downloaded_bytes: prog.downloaded_bytes,
                total_bytes: prog.total_bytes,
                speed_bytes_per_sec: Some(prog.speed_bytes_per_sec),
                port: Some(prog.port),
                num_pieces: Some(prog.num_pieces),
                piece_length: Some(prog.piece_length),
                bitfield: prog.bitfield,
                connections: Some(prog.connections),
                is_resumed: Some(prog.is_resumed),
                resumed_bytes: Some(prog.resumed_bytes),
                total_active_tasks: Some(prog.total_active_tasks),
                error: None,
            });
        },
    ));

    match aria_res {
        Ok(installer_path) => {
            emit_progress(UpdateProgressPayload {
                status: "installing".to_string(),
                progress: 100,
                downloaded_bytes: std::fs::metadata(&installer_path)
                    .map(|m| m.len())
                    .unwrap_or(0),
                total_bytes: std::fs::metadata(&installer_path)
                    .map(|m| m.len())
                    .unwrap_or(0),
                speed_bytes_per_sec: Some(0),
                port: None,
                num_pieces: None,
                piece_length: None,
                bitfield: None,
                connections: None,
                is_resumed: None,
                resumed_bytes: None,
                total_active_tasks: None,
                error: None,
            });

            spawn_msi_installer(&installer_path)
        }
        Err(e) => Err(format!("aria2c 下载失败: {e}")),
    }
}

fn spawn_msi_installer(dest_path: &std::path::Path) -> Result<(), String> {
    match std::process::Command::new("msiexec")
        .arg("/i")
        .arg(dest_path)
        .spawn()
    {
        Ok(_) => {
            emit_progress(UpdateProgressPayload {
                status: "ready".to_string(),
                progress: 100,
                downloaded_bytes: std::fs::metadata(dest_path).map(|m| m.len()).unwrap_or(0),
                total_bytes: std::fs::metadata(dest_path).map(|m| m.len()).unwrap_or(0),
                speed_bytes_per_sec: Some(0),
                port: None,
                num_pieces: None,
                piece_length: None,
                bitfield: None,
                connections: None,
                is_resumed: None,
                resumed_bytes: None,
                total_active_tasks: None,
                error: None,
            });
            Ok(())
        }
        Err(e) => Err(format!("启动安装程序失败: {e}")),
    }
}
