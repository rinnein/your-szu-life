//! 配对 client：已登录设备按二维码里的 secret 加入临时网络，发现 host、
//! 拉取其设备信息，并在用户确认后把本机 vault 里的凭据一次性发给 host。

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::super::bridge::{self, Bridge};
use super::super::node::Node;
use super::{
    now_ms, PairCredentials, PairInfoResponse, PairPeerView, PairSecret, PairSealedPayload,
    CONNECT_TIMEOUT, PAIR_PORT, PAIR_PROTOCOL, PAIR_TTL, TOKEN_HEADER,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const INFO_TIMEOUT: Duration = Duration::from_secs(4);
const AUTHORIZE_TIMEOUT: Duration = Duration::from_secs(8);

/// 一份待用的配对邀请。只存在内存里，过期或取消即丢弃。
#[derive(Clone)]
pub struct StoredInvite {
    pair: PairSecret,
    expires_at_ms: i64,
}

impl StoredInvite {
    pub fn new(pair: PairSecret) -> Self {
        Self {
            pair,
            expires_at_ms: now_ms() + PAIR_TTL.as_millis() as i64,
        }
    }

    pub fn pair(&self) -> &PairSecret {
        &self.pair
    }

    pub fn is_expired(&self) -> bool {
        now_ms() >= self.expires_at_ms
    }

    pub fn network_name(&self) -> &str {
        self.pair.network_name()
    }

    pub fn expires_at_ms(&self) -> i64 {
        self.expires_at_ms
    }
}

/// 解析粘贴/深链里的配对链接。要求本机已有完整账户资料，且不在演示模式。
pub fn accept_invite(input: &str) -> Result<StoredInvite, String> {
    if szu_core::demo::is_enabled() {
        return Err("演示模式下不能授权其它设备登录".into());
    }
    if !szu_core::onboarding::has_complete_profile() {
        return Err("本机尚未完成统一认证，无法授权其它设备登录".into());
    }
    let pair = PairSecret::parse_invite(input)?;
    Ok(StoredInvite::new(pair))
}

pub struct ClientSession {
    pair: PairSecret,
    node: Arc<Node>,
    bridges: HashMap<Ipv4Addr, Bridge>,
    peer: Ipv4Addr,
    view: PairPeerView,
    client: reqwest::Client,
    expires_at_ms: i64,
    expired: Arc<AtomicBool>,
    expiry: Mutex<Option<tokio::task::JoinHandle<()>>>,
    stopped: bool,
}

impl ClientSession {
    /// 加入配对网络并等待 host 出现在 EasyTier 路由表里。
    pub async fn join(device_name: &str, invite: StoredInvite) -> Result<Self, String> {
        let expires_at_ms = invite.expires_at_ms();
        let pair = invite.pair().clone();
        let node = Arc::new(
            Node::start_named(&pair.identity(), device_name, super::CLIENT_INSTANCE).await?,
        );
        match Self::join_inner(node.clone(), pair, expires_at_ms).await {
            Ok(session) => Ok(session),
            Err(error) => {
                node.stop().await;
                Err(error)
            }
        }
    }

    async fn join_inner(
        node: Arc<Node>,
        pair: PairSecret,
        expires_at_ms: i64,
    ) -> Result<Self, String> {
        let client = szu_core::netproxy::overlay_client_builder()
            .timeout(INFO_TIMEOUT)
            .build()
            .map_err(|error| format!("配对 HTTP 客户端创建失败: {error}"))?;
        let self_ip = node.self_info().await.and_then(|info| info.ipv4);
        let mut bridges: HashMap<Ipv4Addr, Bridge> = HashMap::new();
        let (peer, view) =
            Self::find_host(node.clone(), &pair, &client, &mut bridges, self_ip).await?;
        let expired = Arc::new(AtomicBool::new(false));
        let expiry = spawn_expiry(node.clone(), expired.clone(), expires_at_ms);
        Ok(Self {
            pair,
            node,
            bridges,
            peer,
            view,
            client,
            expires_at_ms,
            expired,
            expiry: Mutex::new(Some(expiry)),
            stopped: false,
        })
    }

    async fn find_host(
        node: Arc<Node>,
        pair: &PairSecret,
        client: &reqwest::Client,
        bridges: &mut HashMap<Ipv4Addr, Bridge>,
        self_ip: Option<Ipv4Addr>,
    ) -> Result<(Ipv4Addr, PairPeerView), String> {
        let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
        loop {
            for peer in node.peers().await {
                let Some(ip) = peer.ipv4 else {
                    continue;
                };
                if Some(ip) == self_ip {
                    continue;
                }
                if !bridges.contains_key(&ip) {
                    match bridge::outbound(node.clone(), ip, PAIR_PORT) {
                        Ok(bridge) => {
                            bridges.insert(ip, bridge);
                        }
                        Err(error) => {
                            log::warn!("配对出站桥建立失败（{ip}）: {error}");
                            continue;
                        }
                    }
                }
                let Some(bridge) = bridges.get(&ip) else {
                    continue;
                };
                let url = format!("http://{}/v1/pair/info", bridge.local_addr);
                let info = match fetch_info(client, &url, pair.token()).await {
                    Some(info) => info,
                    None => continue,
                };
                if info.protocol != PAIR_PROTOCOL
                    || !info.code.eq_ignore_ascii_case(pair.display_code())
                {
                    // 同一网络里不是我们要找的 host（或协议版本不兼容），继续等。
                    continue;
                }
                return Ok((
                    ip,
                    PairPeerView {
                        code: info.code,
                        device: info.device,
                    },
                ));
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("无法连接到新设备，请确认两台设备都能访问互联网后重试".to_string());
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    pub fn view(&self) -> &PairPeerView {
        &self.view
    }

    pub fn network_name(&self) -> &str {
        self.pair.network_name()
    }

    /// 会话是否还能用于再次授权（连接成功且未到邀请有效期）。
    pub fn is_usable(&self) -> bool {
        !self.expired.load(Ordering::Acquire) && now_ms() < self.expires_at_ms
    }

    /// 一次性发送凭据。host 端只接受一次，重复调用会得到 409。
    pub async fn authorize(&self, credentials: &PairCredentials) -> Result<(), String> {
        let sealed: PairSealedPayload = self.pair.seal(credentials)?;
        let bridge = self
            .bridges
            .get(&self.peer)
            .ok_or_else(|| "配对连接已失效，请重新扫码".to_string())?;
        let url = format!("http://{}/v1/pair/authorize", bridge.local_addr);
        let response = self
            .client
            .post(&url)
            .timeout(AUTHORIZE_TIMEOUT)
            .header(TOKEN_HEADER, self.pair.token())
            .json(&sealed)
            .send()
            .await
            .map_err(|error| format!("发送授权失败: {error}"))?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            return Err(match status {
                409 => "新设备已经完成授权，或尚未建立连接".to_string(),
                410 => "配对已过期，请在新设备上重新生成".to_string(),
                401 => "配对密钥不匹配，请重新扫码".to_string(),
                _ => format!("新设备拒绝了授权（HTTP {status}）"),
            });
        }
        Ok(())
    }

    pub async fn stop(mut self) {
        self.stopped = true;
        self.abort_expiry();
        self.bridges.clear();
        self.node.stop().await;
    }

    fn abort_expiry(&self) {
        if let Ok(mut guard) = self.expiry.lock() {
            if let Some(handle) = guard.take() {
                handle.abort();
            }
        }
    }
}

impl Drop for ClientSession {
    fn drop(&mut self) {
        self.abort_expiry();
        self.bridges.clear();
        if self.stopped {
            return;
        }
        // EasyTier 实例的删除是 async；这里放弃等待，进程退出时随实例销毁。
        let node = self.node.clone();
        tauri::async_runtime::spawn(async move { node.stop().await });
    }
}

fn spawn_expiry(
    node: Arc<Node>,
    expired: Arc<AtomicBool>,
    expires_at_ms: i64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let delay = (expires_at_ms - now_ms()).max(0) as u64;
        tokio::time::sleep(Duration::from_millis(delay)).await;
        expired.store(true, Ordering::Release);
        node.stop().await;
    })
}

async fn fetch_info(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Option<PairInfoResponse> {
    let response = client
        .get(url)
        .header(TOKEN_HEADER, token)
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<PairInfoResponse>().await.ok()
}

/// 从 vault 读取授权端要发送的凭据。只在 Rust 侧使用，不经过事件或日志。
pub fn load_credentials() -> Result<PairCredentials, String> {
    if szu_core::demo::is_enabled() {
        return Err("演示模式下不能授权其它设备登录".into());
    }
    use szu_core::storage::CredentialStore;
    let profile = szu_core::storage::PlatformCredentialStore
        .load_profile()
        .map_err(|error| format!("读取本机账户资料失败: {error}"))?
        .ok_or_else(|| "本机没有可用的账户资料，请先完成统一认证".to_string())?;
    if !szu_core::auth::model::is_campus_card_number(&profile.campus_card_number) {
        return Err("本机校园卡号不完整，请重新登录后再试".into());
    }
    let credentials = PairCredentials {
        campus_card_number: profile.campus_card_number,
        password: profile.password,
    };
    credentials.validate()?;
    Ok(credentials)
}

/// `MeshStore` 里的设备名。host 与 client 都用它作为 EasyTier hostname。
pub fn device_name(app: &AppHandle) -> String {
    app.try_state::<std::sync::Arc<super::super::MeshStore>>()
        .map(|store| store.device_name())
        .unwrap_or_else(|| "your-szu-life".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_ttl_and_network_name_are_derived_from_the_secret() {
        let pair = PairSecret::from_hex(&"b".repeat(64)).unwrap();
        let invite = StoredInvite::new(pair);
        assert!(!invite.is_expired());
        assert!(invite.network_name().starts_with("your-szu-life:login/pair:"));
    }
}
