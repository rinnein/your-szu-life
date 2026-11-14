//! 临时消息房间：短生命周期的多设备消息交换（当前用于签到码共享）。
//!
//! 与主网（每账号一个固定网络）不同，房间按需创建**第二个** EasyTier
//! 实例：`network_name` / `network_secret` 由调用方给定（如按课程隔离的
//! 签到码房间），安全边界就是网络密钥本身，因此房间内协议不做额外鉴权。
//!
//! 数据面与主网一致：overlay 端口经 [`bridge::inbound`] 桥到本机回环上的
//! axum 服务；发给对端时用 [`bridge::outbound`] 建立每 peer 的出站桥。
//! 消息是带去重键的不透明 JSON，协议语义由调用方（szu-signin）定义。

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use super::bridge::{self, Bridge};
use super::identity::MeshIdentity;
use super::node::Node;
use szu_core::ports;

/// 房间虚拟网内的服务端口（overlay 命名空间，与 SYNC_API_PORT / CAMPUS_PROXY_PORT
/// 同一惯例；回环端口由 ports 模块动态分配）。
pub const QR_ROOM_PORT: u16 = 54303;

/// 发送方设备标记，服务端用它跳过回环与识别消息来源。
const ORIGIN_HEADER: &str = "x-room-origin";

/// peer 刷新与 catch-up 的节奏。
const PEER_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// 一条房间消息。`id` 由发送方保证唯一（如 payload hash），用于全房去重。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoomMessage {
    pub id: String,
    /// 不透明业务负载。
    pub payload: serde_json::Value,
    pub sent_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct StoredMessage {
    pub seq: u64,
    #[serde(flatten)]
    pub message: RoomMessage,
}

#[derive(Deserialize)]
struct SinceParams {
    #[serde(default)]
    since_seq: u64,
}

#[derive(Serialize)]
struct MessagesResponse {
    messages: Vec<StoredMessage>,
}

/// 最近消息环形缓冲：按 seq 递增、按 id 去重、超龄清理。
#[derive(Default)]
struct MessageStore {
    next_seq: u64,
    entries: Vec<StoredMessage>,
}

impl MessageStore {
    const MAX_ENTRIES: usize = 32;
    const MAX_AGE_MS: i64 = 120_000;

    fn insert(&mut self, message: RoomMessage) -> Option<StoredMessage> {
        if self.entries.iter().any(|stored| stored.message.id == message.id) {
            return None;
        }
        let seq = self.next_seq;
        self.next_seq += 1;
        let stored = StoredMessage { seq, message };
        self.entries.push(stored.clone());
        if self.entries.len() > Self::MAX_ENTRIES {
            self.entries.remove(0);
        }
        Some(stored)
    }

    fn since(&self, seq: u64) -> Vec<StoredMessage> {
        self.entries
            .iter()
            .filter(|stored| stored.seq > seq)
            .cloned()
            .collect()
    }

    fn latest(&self) -> Option<StoredMessage> {
        self.entries.last().cloned()
    }

    fn prune(&mut self, now_ms: i64) {
        self.entries
            .retain(|stored| now_ms - stored.message.sent_at_ms <= Self::MAX_AGE_MS);
    }
}

#[derive(Clone)]
struct RoomState {
    device_id: Arc<str>,
    store: Arc<Mutex<MessageStore>>,
    /// 收到的新消息（含来自远端的消息；本机发送走 [`RoomHandle::broadcast`]）。
    inbox: Arc<tokio::sync::broadcast::Sender<RoomMessage>>,
    outbound: Arc<Mutex<HashMap<Ipv4Addr, Bridge>>>,
    node: Arc<Node>,
}

impl RoomState {
    fn post_to_peer(&self, peer: Ipv4Addr, message: &RoomMessage) {
        let bridge = {
            let mut outbound = self.outbound.lock().unwrap();
            if let Some(bridge) = outbound.get(&peer) {
                bridge.local_addr.to_string()
            } else {
                let bridge = match bridge::outbound(self.node.clone(), peer, QR_ROOM_PORT) {
                    Ok(bridge) => bridge,
                    Err(error) => {
                        log::warn!("签到房间出站桥建立失败（{peer}）: {error}");
                        return;
                    }
                };
                let addr = bridge.local_addr.to_string();
                outbound.insert(peer, bridge);
                addr
            }
        };
        let client = match szu_core::netproxy::overlay_client_builder()
            .timeout(Duration::from_secs(4))
            .build()
        {
            Ok(client) => client,
            Err(_) => return,
        };
        let payload = message.clone();
        let device_id = self.device_id.clone();
        tokio::spawn(async move {
            let url = format!("http://{bridge}/v1/messages");
            let _ = client
                .post(&url)
                .header(ORIGIN_HEADER, device_id.as_ref())
                .json(&payload)
                .send()
                .await;
        });
    }

    fn known_peers(&self, self_ip: Option<Ipv4Addr>) -> Vec<Ipv4Addr> {
        let mut peers: Vec<Ipv4Addr> = self
            .outbound
            .lock()
            .unwrap()
            .keys()
            .copied()
            .collect();
        peers.sort();
        peers.dedup();
        if let Some(ip) = self_ip {
            peers.retain(|peer| *peer != ip);
        }
        peers
    }
}

/// 一个已加入的房间。Drop 时任务被中止；务必调用 [`RoomHandle::leave`]
/// 清理 EasyTier 实例。
pub struct RoomHandle {
    node: Arc<Node>,
    state: RoomState,
    self_ip: Option<Ipv4Addr>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl RoomHandle {
    /// 加入房间：创建第二个 EasyTier 实例、桥接 overlay 端口、启动回环服务
    /// 与 peer 刷新循环。
    pub async fn join(
        network_name: &str,
        network_secret: &str,
        device_id: &str,
        hostname: &str,
    ) -> Result<Self, String> {
        let identity = MeshIdentity {
            network_name: network_name.to_string(),
            network_secret: network_secret.to_string(),
            student_id: String::new(),
        };
        let node = Arc::new(
            Node::start_named(&identity, hostname, "your-szu-life-signin-room").await?,
        );
        let self_ip = node.self_info().await.and_then(|info| info.ipv4);

        let (listener, _allocation) = ports::bind_loopback(ports::PortService::Mesh, None)
            .map_err(|error| error.to_string())?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| format!("读取签到房间端口失败: {error}"))?;
        let listener = tokio::net::TcpListener::from_std(listener)
            .map_err(|error| format!("签到房间监听失败: {error}"))?;

        let (inbox, _) = tokio::sync::broadcast::channel::<RoomMessage>(64);
        let store = Arc::new(Mutex::new(MessageStore::default()));
        let state = RoomState {
            device_id: Arc::from(device_id.to_string().as_str()),
            store: store.clone(),
            inbox: Arc::new(inbox),
            outbound: Arc::new(Mutex::new(HashMap::new())),
            node: node.clone(),
        };

        let router = Router::new()
            .route("/v1/messages", post(receive).get(poll))
            .layer(axum::extract::DefaultBodyLimit::max(1024 * 1024))
            .with_state(state.clone());
        let serve = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                log::warn!("签到房间服务停止: {error}");
            }
        });

        let inbound = bridge::inbound(node.clone(), QR_ROOM_PORT, local_addr).await?;

        // peer 刷新：维持出站桥、清理过期消息；发现新 peer 时补发最新消息
        // （catch-up，让后加入的设备立刻拿到当前有效的签到码）。
        let refresh_state = state.clone();
        let refresh_self = self_ip;
        let refresh = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(PEER_REFRESH_INTERVAL);
            loop {
                ticker.tick().await;
                refresh_state
                    .store
                    .lock()
                    .unwrap()
                    .prune(now_ms());
                let peers = refresh_state.known_peers(refresh_self);
                for peer in &peers {
                    if !refresh_state.outbound.lock().unwrap().contains_key(peer) {
                        let latest = refresh_state.store.lock().unwrap().latest();
                        if let Some(stored) = latest {
                            refresh_state.post_to_peer(*peer, &stored.message);
                        }
                    }
                }
            }
        });

        Ok(Self {
            node,
            state,
            self_ip,
            tasks: vec![serve, inbound, refresh],
        })
    }

    /// 订阅收到的远端消息。
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<RoomMessage> {
        self.state.inbox.subscribe()
    }

    /// 当前房间内的对端虚拟 IP 列表。
    pub async fn peers(&self) -> Vec<Ipv4Addr> {
        self.state.known_peers(self.self_ip)
    }

    /// 广播一条消息：先入自己的存储（供 GET 拉取与 catch-up），再扇出到
    /// 全部已知 peer。
    pub fn broadcast(&self, message: RoomMessage) {
        {
            let mut store = self.state.store.lock().unwrap();
            store.prune(now_ms());
            store.insert(message.clone());
        }
        for peer in self.state.known_peers(self.self_ip) {
            self.state.post_to_peer(peer, &message);
        }
    }

    /// 点对点发送（不经本地存储）。
    pub fn send_to(&self, peer: Ipv4Addr, message: RoomMessage) {
        self.state.post_to_peer(peer, &message);
    }

    /// 离开并销毁实例。
    pub async fn leave(mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        self.node.stop().await;
    }
}

impl Drop for RoomHandle {
    fn drop(&mut self) {
        for task in self.tasks.drain(..) {
            task.abort();
        }
        // EasyTier 实例的删除是 async；这里放弃等待，进程退出时随实例销毁。
        let node = self.node.clone();
        tauri::async_runtime::spawn(async move { node.stop().await });
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn receive(
    State(state): State<RoomState>,
    headers: HeaderMap,
    Json(message): Json<RoomMessage>,
) -> impl IntoResponse {
    let origin = headers
        .get(ORIGIN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if origin == state.device_id.as_ref() {
        // 自己经网关回环回来的消息，忽略。
        return (StatusCode::OK, Json(serde_json::json!({ "stored": false })));
    }
    let stored = {
        let mut store = state.store.lock().unwrap();
        store.prune(now_ms());
        store.insert(message.clone())
    };
    if let Some(_) = stored {
        // 广播给本进程订阅者；失败只说明没有订阅者或订阅已满，不影响存储。
        let _ = state.inbox.send(message);
    }
    (StatusCode::OK, Json(serde_json::json!({ "stored": stored.is_some() })))
}

async fn poll(
    State(state): State<RoomState>,
    Query(params): Query<SinceParams>,
) -> impl IntoResponse {
    let messages = {
        let mut store = state.store.lock().unwrap();
        store.prune(now_ms());
        store.since(params.since_seq)
    };
    (StatusCode::OK, Json(MessagesResponse { messages }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str) -> RoomMessage {
        RoomMessage {
            id: id.to_string(),
            payload: serde_json::json!({ "data": "abc" }),
            sent_at_ms: 1_000,
        }
    }

    #[test]
    fn store_dedupes_by_id_and_returns_none_for_duplicates() {
        let mut store = MessageStore::default();
        let first = store.insert(message("a")).expect("首条应入库");
        assert_eq!(first.seq, 0);
        assert!(store.insert(message("a")).is_none(), "重复 id 不应入库");
        let second = store.insert(message("b")).expect("第二条应入库");
        assert_eq!(second.seq, 1);
        assert_eq!(store.since(0).len(), 1);
        assert_eq!(store.latest().unwrap().message.id, "b");
    }

    #[test]
    fn store_caps_entries_and_prunes_expired() {
        let mut store = MessageStore::default();
        for index in 0..40 {
            store.insert(message(&format!("m{index}")));
        }
        assert!(store.entries.len() <= MessageStore::MAX_ENTRIES);
        // 把全部消息改成过期时间，prune 后应为空。
        let stale = MessageStore::MAX_AGE_MS + 1_000;
        for stored in &mut store.entries {
            stored.message.sent_at_ms = 1_000;
        }
        store.prune(1_000 + stale);
        assert!(store.entries.is_empty());
    }

    #[test]
    fn room_port_is_in_overlay_namespace() {
        // overlay 端口与回环端口命名空间无关；这里只钉住协议常量，
        // 防止无意间与 sync/campus proxy 撞号。
        assert_ne!(QR_ROOM_PORT, super::super::SYNC_API_PORT);
        assert_ne!(QR_ROOM_PORT, super::super::CAMPUS_PROXY_PORT);
    }

    #[test]
    fn named_config_passes_easytier_validation() {
        // 房间用第二个实例时也必须通过 EasyTier 的能力校验（feature 齐全）。
        let identity = MeshIdentity {
            network_name: "your-szu-life:study/signin/lms/qr:42".into(),
            network_secret: "a".repeat(64),
            student_id: String::new(),
        };
        let manager = easytier::instance::factory::native_instance_manager();
        let toml = super::super::node::build_config_named(
            "your-szu-life-signin-room",
            &identity,
            "测试设备",
            54297,
            54296,
        );
        let config =
            easytier_core::config::toml::TomlConfig::new_from_str(&toml).expect("配置应能解析");
        let instance = manager
            .create(config, ())
            .expect("房间配置应通过 EasyTier 的能力校验");
        drop(instance);
    }
}
