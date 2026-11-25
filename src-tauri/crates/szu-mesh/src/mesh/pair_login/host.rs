//! 配对 host：未登录的新设备在向导里开启一个临时 EasyTier 网络，
//! 用 overlay 内的 HTTP 服务把本机设备信息暴露给授权端，并接收一次性凭据。

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tauri::Emitter;

use super::super::api;
use super::super::bridge;
use super::super::node::Node;
use super::{
    now_ms, PairDevice, PairHostStatus, PairHostView, PairInfoResponse, PairSecret, PairSealedPayload,
    PAIR_HOST_EVENT, PAIR_PORT, PAIR_PROTOCOL, PAIR_TTL,
};
use szu_core::ports;

/// host 端真正提供服务前的准备阶段最多等多久（与 Node 自身的 ready 超时一致）。
const START_TIMEOUT: Duration = Duration::from_secs(15);

/// 配对状态机的纯状态部分，便于不依赖 Tauri 的单测。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InnerStatus {
    Waiting,
    Connected,
    Accepted,
    Expired,
}

impl InnerStatus {
    fn view(self) -> PairHostStatus {
        match self {
            Self::Waiting => PairHostStatus::Waiting,
            Self::Connected => PairHostStatus::Connected,
            Self::Accepted => PairHostStatus::Accepted,
            Self::Expired => PairHostStatus::Expired,
        }
    }
}

#[derive(Default)]
struct HostInner {
    status: Option<InnerStatus>,
    credentials: Option<super::PairCredentials>,
}

impl HostInner {
    fn status(&self) -> InnerStatus {
        self.status.unwrap_or(InnerStatus::Waiting)
    }

    /// 第一次被授权端拉取信息：Waiting → Connected。
    fn mark_connected(&mut self) -> bool {
        if self.status() == InnerStatus::Waiting {
            self.status = Some(InnerStatus::Connected);
            true
        } else {
            false
        }
    }

    /// 只接受一次；必须先 Connected。
    fn accept(
        &mut self,
        credentials: super::PairCredentials,
    ) -> Result<bool, (StatusCode, &'static str)> {
        match self.status() {
            InnerStatus::Expired => return Err((StatusCode::GONE, "pair expired")),
            InnerStatus::Accepted => return Err((StatusCode::CONFLICT, "already authorized")),
            InnerStatus::Waiting => return Err((StatusCode::CONFLICT, "not connected yet")),
            InnerStatus::Connected => {}
        }
        self.credentials = Some(credentials);
        self.status = Some(InnerStatus::Accepted);
        Ok(true)
    }

    /// 到期只影响还没完成授权的会话。
    fn mark_expired(&mut self) -> bool {
        match self.status() {
            InnerStatus::Waiting | InnerStatus::Connected => {
                self.status = Some(InnerStatus::Expired);
                true
            }
            _ => false,
        }
    }

    fn take_credentials(&mut self) -> Option<super::PairCredentials> {
        self.credentials.take()
    }
}

pub struct HostSession {
    node: Arc<Node>,
    state: Arc<HostState>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    stopped: Mutex<bool>,
}

struct HostState {
    inner: Mutex<HostInner>,
    app: tauri::AppHandle,
    pair: Arc<PairSecret>,
    device: PairDevice,
    expires_at_ms: i64,
    token: Arc<str>,
}

impl HostState {
    fn status(&self) -> InnerStatus {
        self.inner.lock().map(|inner| inner.status()).unwrap_or(InnerStatus::Expired)
    }

    fn view(&self, status: InnerStatus) -> PairHostView {
        PairHostView {
            status: status.view(),
            code: self.pair.display_code().to_owned(),
            deep_link: self.pair.deep_link(),
            expires_at_ms: self.expires_at_ms,
        }
    }

    fn emit(&self, view: PairHostView) {
        let _ = self.app.emit(PAIR_HOST_EVENT, view);
    }

    fn emit_current(&self) {
        let status = self.status();
        self.emit(self.view(status));
    }

    fn mark_connected(&self) {
        let changed = self
            .inner
            .lock()
            .map(|mut inner| inner.mark_connected())
            .unwrap_or(false);
        if changed {
            self.emit_current();
        }
    }

    fn accept(&self, credentials: super::PairCredentials) -> Result<(), Response> {
        let outcome = self
            .inner
            .lock()
            .map(|mut inner| inner.accept(credentials))
            .unwrap_or_else(|_| Err((StatusCode::INTERNAL_SERVER_ERROR, "pair state unavailable")));
        match outcome {
            Ok(_) => {
                self.emit_current();
                Ok(())
            }
            Err((status, message)) => Err((status, message).into_response()),
        }
    }
}

impl HostSession {
    pub async fn start(
        app: tauri::AppHandle,
        device_id: &str,
        device_name: &str,
    ) -> Result<Self, String> {
        let pair = Arc::new(PairSecret::generate());
        let node = Arc::new(
            Node::start_named(&pair.identity(), device_name, super::HOST_INSTANCE).await?,
        );

        match Self::start_inner(app, device_id, device_name, pair.clone(), node.clone()).await {
            Ok(session) => Ok(session),
            Err(error) => {
                node.stop().await;
                Err(error)
            }
        }
    }

    async fn start_inner(
        app: tauri::AppHandle,
        device_id: &str,
        device_name: &str,
        pair: Arc<PairSecret>,
        node: Arc<Node>,
    ) -> Result<Self, String> {
        let expires_at_ms = now_ms() + PAIR_TTL.as_millis() as i64;
        // 设备信息只取一次；之后信息接口只读这份快照。
        let status = tokio::time::timeout(
            START_TIMEOUT,
            api::local_status(&app, &node, device_id, device_name),
        )
        .await
        .map_err(|_| "读取本机设备信息超时".to_string())??;
        let device = PairDevice {
            name: status.name,
            platform: status.platform,
            app_version: status.app_version,
        };

        let token: Arc<str> = Arc::from(pair.token());
        let state = Arc::new(HostState {
            inner: Mutex::new(HostInner::default()),
            app: app.clone(),
            pair,
            device,
            expires_at_ms,
            token,
        });

        let (listener, _allocation) = ports::bind_loopback(ports::PortService::Mesh, None)
            .map_err(|error| error.to_string())?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| format!("读取配对服务端口失败: {error}"))?;
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|error| format!("配对服务监听失败: {error}"))?;

        let router = Router::new()
            .route("/v1/pair/info", get(info))
            .route("/v1/pair/authorize", post(authorize))
            // 凭据载荷很小；超限直接拒绝，不把大 body 读进内存。
            .layer(DefaultBodyLimit::max(8 * 1024))
            .with_state(state.clone());

        let serve = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                log::warn!("配对服务停止: {error}");
            }
        });
        let inbound = match bridge::inbound(node.clone(), PAIR_PORT, local_addr).await {
            Ok(task) => task,
            Err(error) => {
                serve.abort();
                return Err(error);
            }
        };
        let expiry = spawn_expiry(state.clone(), node.clone());

        Ok(Self {
            node,
            state,
            tasks: Mutex::new(vec![serve, inbound, expiry]),
            stopped: Mutex::new(false),
        })
    }

    pub fn view(&self) -> PairHostView {
        self.state.current_view()
    }

    /// 一次性取走凭据（前端随后立刻启动现有 authserver 登录）。
    pub fn take_credentials(&self) -> Option<super::PairCredentials> {
        self.state
            .inner
            .lock()
            .ok()
            .and_then(|mut inner| inner.take_credentials())
    }

    /// 停止实例并清理全部任务。调用后 Session 不再可用。
    pub async fn stop(self) {
        if let Ok(mut stopped) = self.stopped.lock() {
            *stopped = true;
        }
        let tasks: Vec<_> = self
            .tasks
            .lock()
            .map(|mut tasks| tasks.drain(..).collect())
            .unwrap_or_default();
        for task in tasks {
            task.abort();
        }
        self.node.stop().await;
    }
}

impl Drop for HostSession {
    fn drop(&mut self) {
        let already_stopped = self.stopped.lock().map(|flag| *flag).unwrap_or(true);
        if already_stopped {
            return;
        }
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.drain(..) {
                task.abort();
            }
        }
        // EasyTier 实例的删除是 async；这里放弃等待，进程退出时随实例销毁。
        let node = self.node.clone();
        tauri::async_runtime::spawn(async move { node.stop().await });
    }
}

impl HostState {
    fn current_view(&self) -> PairHostView {
        self.view(self.status())
    }
}

fn spawn_expiry(state: Arc<HostState>, node: Arc<Node>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let delay = (state.expires_at_ms - now_ms()).max(0) as u64;
        tokio::time::sleep(Duration::from_millis(delay)).await;
        let changed = state
            .inner
            .lock()
            .map(|mut inner| inner.mark_expired())
            .unwrap_or(false);
        if changed {
            state.emit_current();
            node.stop().await;
        }
    })
}

async fn info(State(state): State<Arc<HostState>>, headers: HeaderMap) -> Response {
    if !api::authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid pair token").into_response();
    }
    if state.status() == InnerStatus::Expired {
        return (StatusCode::GONE, "pair expired").into_response();
    }
    state.mark_connected();
    Json(PairInfoResponse {
        protocol: PAIR_PROTOCOL,
        code: state.pair.display_code().to_owned(),
        expires_at_ms: state.expires_at_ms,
        device: state.device.clone(),
    })
    .into_response()
}

async fn authorize(
    State(state): State<Arc<HostState>>,
    headers: HeaderMap,
    Json(sealed): Json<PairSealedPayload>,
) -> Response {
    if !api::authorized(&headers, &state.token) {
        return (StatusCode::UNAUTHORIZED, "invalid pair token").into_response();
    }
    let credentials = match state.pair.open(&sealed) {
        Ok(credentials) => credentials.normalized(),
        Err(error) => return (StatusCode::BAD_REQUEST, error).into_response(),
    };
    match state.accept(credentials) {
        Ok(()) => Json(json!({ "accepted": true })).into_response(),
        Err(response) => response,
    }
}

/// host 与 client 共用的请求头名只在协议层定义，这里用测试钉住。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::pair_login::{PairCredentials, TOKEN_HEADER};

    fn credentials() -> PairCredentials {
        PairCredentials {
            campus_card_number: "123456".into(),
            password: "password-1".into(),
        }
    }

    #[test]
    fn state_machine_requires_a_connected_client_and_is_one_shot() {
        let mut inner = HostInner::default();
        assert_eq!(inner.status(), InnerStatus::Waiting);
        assert!(inner.accept(credentials()).is_err(), "未连接时拒绝授权");
        assert!(inner.mark_connected());
        assert!(!inner.mark_connected(), "重复 info 只算一次状态变化");
        inner.accept(credentials()).expect("已连接后接受");
        assert_eq!(inner.status(), InnerStatus::Accepted);
        assert!(inner.accept(credentials()).is_err(), "只接受一次");
        assert!(inner.take_credentials().is_some());
        assert!(inner.take_credentials().is_none(), "取走即空");
    }

    #[test]
    fn expiry_does_not_override_accepted() {
        let mut inner = HostInner::default();
        assert!(inner.mark_connected());
        inner.accept(credentials()).unwrap();
        assert!(!inner.mark_expired());
        assert_eq!(inner.status(), InnerStatus::Accepted);

        let mut inner = HostInner::default();
        assert!(inner.mark_expired());
        assert_eq!(inner.status(), InnerStatus::Expired);
        assert!(!inner.mark_connected(), "过期后不能再回到 Connected");
        assert!(inner.accept(credentials()).is_err());
    }

    #[test]
    fn pair_endpoint_paths_and_port_are_pinned() {
        // overlay 端口不能与 sync / campus proxy / 签到房间撞号。
        assert_ne!(PAIR_PORT, crate::mesh::SYNC_API_PORT);
        assert_ne!(PAIR_PORT, crate::mesh::CAMPUS_PROXY_PORT);
        assert_ne!(PAIR_PORT, crate::mesh::share_room::SHARE_ROOM_PORT);
        assert_eq!(TOKEN_HEADER, "x-pair-token");
    }

    #[test]
    fn pair_network_config_passes_easytier_validation() {
        use crate::mesh::node::{build_config_named, MeshRole};

        let pair = PairSecret::from_hex(&"a".repeat(64)).unwrap();
        let toml = build_config_named(
            crate::mesh::pair_login::HOST_INSTANCE,
            &pair.identity(),
            "测试设备",
            MeshRole::Client {
                relay_peer: "ring://00000000-0000-0000-0000-000000000001",
            },
        );
        assert!(toml.contains("listeners = []"));
        let config =
            easytier_core::config::toml::TomlConfig::new_from_str(&toml).expect("配置应能解析");
        let manager = easytier::instance::factory::native_instance_manager();
        let instance = manager
            .create(config, ())
            .expect("配对网络配置应通过 EasyTier 的能力校验");
        drop(instance);
    }
}
