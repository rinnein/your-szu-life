//! 对等设备之间唯一的 HTTP 接口。
//!
//! 服务只监听本机回环（由 [`super::bridge`] 从虚拟网转发进来），请求必须带
//! `x-mesh-token`。token 由网络密钥派生，因此只有同网络（= 同一账号）的设备能
//! 读到设备状态与同步数据。
//!
//! 响应使用固定字符串而不是 Rust 枚举，避免两端版本不一致时整份响应解析失败。

use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

use super::node::Node;
use crate::preferences;
use crate::settings_sync::engine::{
    BankApplyRequest, BankRowsRequest, ItemPayload, SyncStore,
};

pub(crate) const TOKEN_HEADER: &str = "x-mesh-token";

/// 任意两台设备之间的自我描述。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerStatus {
    pub(crate) device_id: String,
    pub(crate) name: String,
    pub(crate) platform: String,
    pub(crate) app_version: String,
    pub(crate) overlay_ip: Option<String>,
    /// 对端是否开启了设置同步（未开启时不会响应同步请求）。
    #[serde(default)]
    pub(crate) sync_enabled: bool,
    /// 对端是否开启了「校园网访问」。
    #[serde(default)]
    pub(crate) campus_access: bool,
    #[serde(default)]
    pub(crate) on_campus: bool,
    #[serde(default)]
    pub(crate) region: String,
    #[serde(default)]
    pub(crate) network_state: String,
    #[serde(default)]
    pub(crate) ssid: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ApiState {
    pub(crate) token: Arc<str>,
    pub(crate) app: tauri::AppHandle,
    pub(crate) node: Arc<Node>,
    pub(crate) device_id: Arc<str>,
    pub(crate) device_name: Arc<str>,
}

pub(crate) fn router(state: ApiState) -> Router {
    Router::new()
        .route("/v1/status", get(status))
        .route("/v1/sync/state", get(sync_state))
        .route(
            "/v1/sync/item/{id}",
            get(sync_get_item).post(sync_apply_item),
        )
        .route("/v1/sync/bank/rows", post(sync_bank_rows))
        .route("/v1/sync/bank/apply", post(sync_bank_apply))
        // 题库按 200 行一批交换，题干/选项较多时会超过 axum 默认的 2 MiB。
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .with_state(state)
}

fn sync_store(app: &tauri::AppHandle) -> Option<Arc<SyncStore>> {
    app.try_state::<Arc<SyncStore>>()
        .map(|store| store.inner().clone())
}

/// 同步接口只在「设置同步」开启时可用；关闭时对端会直接得到 403。
fn sync_available() -> Result<(), Response> {
    if preferences::load::<bool>(preferences::SYNC_ENABLED)
        .ok()
        .flatten()
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err((StatusCode::FORBIDDEN, "sync disabled").into_response())
    }
}

async fn sync_state(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    if let Err(response) = sync_available() {
        return response;
    }
    let Some(store) = sync_store(&state.app) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "sync unavailable").into_response();
    };
    match store.peer_state(&state.app).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn sync_get_item(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    if let Err(response) = sync_available() {
        return response;
    }
    let Some(store) = sync_store(&state.app) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "sync unavailable").into_response();
    };
    match store.peer_item(&state.app, &id).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

async fn sync_apply_item(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<ItemPayload>,
) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    if let Err(response) = sync_available() {
        return response;
    }
    let Some(store) = sync_store(&state.app) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "sync unavailable").into_response();
    };
    match store.peer_apply_item(&state.app, &id, payload).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error).into_response(),
    }
}

async fn sync_bank_rows(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<BankRowsRequest>,
) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    if let Err(response) = sync_available() {
        return response;
    }
    let Some(store) = sync_store(&state.app) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "sync unavailable").into_response();
    };
    match store.peer_bank_rows(&state.app, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn sync_bank_apply(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(request): Json<BankApplyRequest>,
) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    if let Err(response) = sync_available() {
        return response;
    }
    let Some(store) = sync_store(&state.app) else {
        return (StatusCode::SERVICE_UNAVAILABLE, "sync unavailable").into_response();
    };
    match store.peer_apply_bank(&state.app, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn status(State(state): State<ApiState>, headers: HeaderMap) -> Response {
    if !authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid mesh token").into_response();
    }
    match build_status(&state).await {
        Ok(status) => Json(status).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

async fn build_status(state: &ApiState) -> Result<PeerStatus, String> {
    local_status(&state.app, &state.node, &state.device_id, &state.device_name).await
}

/// 本机对外的自我描述。设备列表里的「本机」与 `/v1/status` 共用这一份。
pub(crate) async fn local_status(
    app: &tauri::AppHandle,
    node: &Node,
    device_id: &str,
    device_name: &str,
) -> Result<PeerStatus, String> {
    let overlay_ip = node
        .self_info()
        .await
        .and_then(|info| info.ipv4)
        .map(|ip| ip.to_string());
    let (on_campus, region, network_state, ssid) = campus_status(app);
    Ok(PeerStatus {
        device_id: device_id.to_owned(),
        name: device_name.to_owned(),
        platform: std::env::consts::OS.to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        overlay_ip,
        sync_enabled: preferences::load::<bool>(preferences::SYNC_ENABLED)
            .ok()
            .flatten()
            .unwrap_or(false),
        campus_access: preferences::load::<bool>(preferences::SYNC_CAMPUS_ACCESS)
            .ok()
            .flatten()
            .unwrap_or(false),
        on_campus,
        region,
        network_state,
        ssid,
    })
}

fn campus_status(app: &tauri::AppHandle) -> (bool, String, String, Option<String>) {
    use crate::campus_network::model::{NetworkConnectionState, NetworkRegion};

    let Some(store) = app.try_state::<crate::campus_network::CampusNetworkStore>() else {
        return (false, "none".into(), "unknown".into(), None);
    };
    let Ok(overview) = crate::campus_network::service::get_network_overview(&store) else {
        return (false, "none".into(), "unknown".into(), None);
    };
    let region = match overview.region {
        NetworkRegion::None => "none",
        NetworkRegion::Staff => "staff",
        NetworkRegion::Dormitory => "dormitory",
    };
    let state = match overview.state {
        NetworkConnectionState::NoNetwork => "noNetwork",
        NetworkConnectionState::Unknown => "unknown",
        NetworkConnectionState::NotLoggedIn => "notLoggedIn",
        NetworkConnectionState::Connected => "connected",
    };
    (
        overview.region != NetworkRegion::None,
        region.to_owned(),
        state.to_owned(),
        overview.ssid,
    )
}

/// 常量时间比较，避免用长度/前缀差异推断 token。
fn authorized(headers: &HeaderMap, expected: &str) -> bool {
    let Some(actual) = headers.get(TOKEN_HEADER).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let expected = expected.as_bytes();
    let actual = actual.as_bytes();
    if actual.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in actual.iter().zip(expected) {
        diff |= a ^ b;
    }
    diff == 0
}

/// 对等接口的认证 token：由网络密钥派生，只在内存中使用。
pub(crate) fn derive_token(network_secret: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{network_secret}:api:v1").as_bytes());
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn token_is_stable_and_derived_from_the_secret() {
        let a = derive_token("secret-a");
        assert_eq!(a, derive_token("secret-a"));
        assert_ne!(a, derive_token("secret-b"));
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn authorization_requires_the_exact_token() {
        let mut headers = HeaderMap::new();
        assert!(!authorized(&headers, "token"));
        headers.insert(TOKEN_HEADER, HeaderValue::from_static("token"));
        assert!(authorized(&headers, "token"));
        assert!(!authorized(&headers, "other"));
        headers.insert(TOKEN_HEADER, HeaderValue::from_static("token-longer"));
        assert!(!authorized(&headers, "token"));
    }
}
