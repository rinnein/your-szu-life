//! 远程授权登录的 Tauri 命令。
//!
//! 刻意不提供 MCP 工具/资源：这里会读写统一认证明文密码，任何本地 MCP 客户端
//! 若能触发或读取都等于凭据外泄口；流程也必须依赖另一台设备的物理扫码与人工确认。

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};

use super::client::{self, ClientSession};
use super::host::HostSession;
use super::{PairCredentialsView, PairHostView, PairLoginStore, PairPeerView};

fn mesh_store(app: &AppHandle) -> Result<Arc<super::super::MeshStore>, String> {
    app.try_state::<Arc<super::super::MeshStore>>()
        .map(|store| store.inner().clone())
        .ok_or_else(|| "设备状态尚未初始化".to_string())
}

fn clear_invite(store: &PairLoginStore) {
    if let Ok(mut invite) = store.invite.lock() {
        *invite = None;
    }
}

#[tauri::command]
pub async fn pair_login_host_start(
    app: AppHandle,
    store: State<'_, Arc<PairLoginStore>>,
) -> Result<PairHostView, String> {
    let store = store.inner().clone();
    let mesh = mesh_store(&app)?;
    let device_id = mesh.device_id()?;
    let device_name = mesh.device_name();

    let previous = store.host.lock().await.take();
    if let Some(previous) = previous {
        previous.stop().await;
    }

    let session = HostSession::start(app.clone(), &device_id, &device_name).await?;
    let view = session.view();
    *store.host.lock().await = Some(session);
    Ok(view)
}

#[tauri::command]
pub async fn pair_login_host_cancel(store: State<'_, Arc<PairLoginStore>>) -> Result<(), String> {
    let session = store.host.lock().await.take();
    if let Some(session) = session {
        session.stop().await;
    }
    Ok(())
}

/// 一次性取走配对得到的凭据；取走后立即停止 host 会话。
#[tauri::command]
pub async fn pair_login_take_credentials(
    store: State<'_, Arc<PairLoginStore>>,
) -> Result<Option<PairCredentialsView>, String> {
    let session = store.host.lock().await.take();
    let Some(session) = session else {
        return Ok(None);
    };
    let credentials = session.take_credentials();
    session.stop().await;
    Ok(credentials.map(PairCredentialsView::from))
}

/// 接受一条配对链接（深链或设置页粘贴/扫码结果）。secret 只留在 Rust 内存。
#[tauri::command]
pub fn pair_login_client_accept_url(
    store: State<'_, Arc<PairLoginStore>>,
    url: String,
) -> Result<(), String> {
    let invite = client::accept_invite(&url)?;
    let mut guard = store
        .invite
        .lock()
        .map_err(|_| "配对邀请状态不可用".to_string())?;
    *guard = Some(invite);
    Ok(())
}

/// 加入配对网络并拉取 host 的设备信息。重复调用返回同一个已连接的视图。
#[tauri::command]
pub async fn pair_login_client_connect(
    app: AppHandle,
    store: State<'_, Arc<PairLoginStore>>,
) -> Result<PairPeerView, String> {
    let store = store.inner().clone();
    let invite = {
        let guard = store
            .invite
            .lock()
            .map_err(|_| "配对邀请状态不可用".to_string())?;
        guard
            .clone()
            .ok_or_else(|| "配对邀请已失效，请重新扫码".to_string())?
    };
    if invite.is_expired() {
        clear_invite(&store);
        return Err("配对邀请已过期，请重新扫码".into());
    }

    let existing = store.client.lock().await.take();
    if let Some(existing) = existing {
        if existing.is_usable() && existing.network_name() == invite.network_name() {
            let view = existing.view().clone();
            *store.client.lock().await = Some(existing);
            return Ok(view);
        }
        existing.stop().await;
    }

    let device_name = client::device_name(&app);
    let session = ClientSession::join(&device_name, invite).await?;
    let view = session.view().clone();
    *store.client.lock().await = Some(session);
    Ok(view)
}

#[tauri::command]
pub async fn pair_login_client_authorize(
    store: State<'_, Arc<PairLoginStore>>,
) -> Result<(), String> {
    let store = store.inner().clone();
    let credentials = client::load_credentials()?;
    let session = store
        .client
        .lock()
        .await
        .take()
        .ok_or_else(|| "配对连接已失效，请重新连接".to_string())?;
    if !session.is_usable() {
        session.stop().await;
        clear_invite(&store);
        return Err("配对已过期，请在新设备上重新生成".into());
    }
    match session.authorize(&credentials).await {
        Ok(()) => {
            clear_invite(&store);
            session.stop().await;
            Ok(())
        }
        Err(error) => {
            // 发送失败时保留连接，允许用户重试。
            *store.client.lock().await = Some(session);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn pair_login_client_cancel(store: State<'_, Arc<PairLoginStore>>) -> Result<(), String> {
    let session = store.client.lock().await.take();
    if let Some(session) = session {
        session.stop().await;
    }
    clear_invite(&store);
    Ok(())
}
