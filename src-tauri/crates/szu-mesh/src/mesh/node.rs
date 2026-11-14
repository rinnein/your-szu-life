//! 进程内的 EasyTier 节点。
//!
//! 所有设备都跑在 `no_tun` 模式下：不创建 TUN 设备、不碰系统路由，也不需要
//! Android 的 VpnService。对等设备之间需要 TCP 时，走 EasyTier 的用户态数据面
//! （`ffi-dataplane`）：`bind_overlay` 在虚拟 IP 上监听，`connect_overlay` 直接
//! 拨虚拟网目标，二者都只影响本进程。
//!
//! 这里是全应用**唯一**一处组装 EasyTier 配置的地方：网络身份、rendezvous peer
//! 列表、RPC portal 与各项 flag 都在 [`build_config`] 里确定。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Mutex;
use std::time::Duration;

use easytier::instance::factory::{native_instance_manager, NativeInstanceManager};
use easytier_core::config::toml::TomlConfig;
use easytier_core::gateway::{DataPlaneTcpListener, DataPlaneTcpStream};
use easytier_core::instance::manager::ConfigFileControl;

use super::identity::MeshIdentity;
use szu_core::ports;

/// 节点启动后等待虚拟 IP 分配的最长时间。
const READY_TIMEOUT: Duration = Duration::from_secs(10);
const DATA_PLANE_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// 与 EasyTier 的默认监听端口不同，这里固定从自组网端口段里挑一个，并同时用于
/// TCP 与 UDP。监听器必须绑 `0.0.0.0`（P2P 传输必需），这是项目中「本地服务只
/// 监听回环」约定的唯一例外；安全性由网络密钥承担。
const LISTENER_PORT_PREFERRED: u16 = ports::MESH_PORT_RANGE.0;
const LISTENER_PORT_END: u16 = ports::MESH_PORT_RANGE.1;

/// 主网实例名。临时房间（签到码共享等）用第二个实例，见 [`Node::start_named`]。
pub const DEFAULT_INSTANCE_NAME: &str = "your-szu-life-mesh";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayPeer {
    pub peer_id: u32,
    pub ipv4: Option<Ipv4Addr>,
    pub hostname: String,
    pub cost: i32,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySelf {
    pub peer_id: u32,
    pub ipv4: Option<Ipv4Addr>,
    pub hostname: String,
}

/// 一个已经启动的 EasyTier 实例。Drop 前必须调用 [`Node::stop`]。
pub struct Node {
    manager: NativeInstanceManager,
    instance_id: uuid::Uuid,
    /// 启动时写死的虚拟 IP；DHCP 之后一般不变，但以 `self_info()` 的实时值为准。
    boot_ipv4: Mutex<Option<Ipv4Addr>>,
}

impl Node {
    pub async fn start(identity: &MeshIdentity, hostname: &str) -> Result<Self, String> {
        Self::start_named(identity, hostname, DEFAULT_INSTANCE_NAME).await
    }

    /// 以自定义网络身份与实例名启动第二个 EasyTier 实例（临时房间场景）。
    /// 多实例在 manager 里按 uuid 并存，端口各自独立挑选。
    pub async fn start_named(
        identity: &MeshIdentity,
        hostname: &str,
        instance_name: &str,
    ) -> Result<Self, String> {
        let rpc_port = ports::reserve_loopback(ports::PortService::Mesh, None)
            .map_err(|error| error.to_string())?
            .port;
        let listener_port = pick_listener_port();
        let toml = build_config_named(instance_name, identity, hostname, rpc_port, listener_port);
        let config = TomlConfig::new_from_str(&toml)
            .map_err(|error| format!("设备同步网络配置无效: {error:#}"))?;
        let manager = native_instance_manager();
        let instance_id = manager
            .run_network_instance(config, ConfigFileControl::STATIC_CONFIG)
            .map_err(|error| format!("设备同步网络启动失败: {error:#}"))?;

        let node = Self {
            manager,
            instance_id,
            boot_ipv4: Mutex::new(None),
        };
        if let Err(error) = node.wait_ready().await {
            node.stop().await;
            return Err(error);
        }
        Ok(node)
    }

    async fn wait_ready(&self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            let Some(instance) = self.manager.instance(self.instance_id) else {
                return Err("设备同步网络实例已退出".into());
            };
            if let Some(error) = instance.latest_error().filter(|error| !error.trim().is_empty()) {
                return Err(format!("设备同步网络启动失败: {error}"));
            }
            if instance.is_ready() {
                if let Some(ipv4) = instance
                    .node_snapshot()
                    .await
                    .ipv4_addr
                    .as_ref()
                    .and_then(parse_inet_address)
                {
                    if let Ok(mut cached) = self.boot_ipv4.lock() {
                        *cached = Some(ipv4);
                    }
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("设备同步网络启动超时（未分配到虚拟 IP）".into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn self_info(&self) -> Option<OverlaySelf> {
        let instance = self.manager.instance(self.instance_id)?;
        let snapshot = instance.node_snapshot().await;
        Some(OverlaySelf {
            peer_id: snapshot.peer_id,
            ipv4: snapshot
                .ipv4_addr
                .as_ref()
                .and_then(parse_inet_address)
                .or_else(|| self.boot_ipv4.lock().ok().and_then(|ip| *ip)),
            hostname: snapshot.hostname,
        })
    }

    pub async fn peers(&self) -> Vec<OverlayPeer> {
        let Some(instance) = self.manager.instance(self.instance_id) else {
            return Vec::new();
        };
        instance
            .route_snapshots()
            .await
            .into_iter()
            .map(|route| OverlayPeer {
                peer_id: route.peer_id,
                ipv4: route
                    .ipv4_addr
                    .as_ref()
                    .and_then(parse_inet_address),
                hostname: route.hostname,
                cost: route.cost,
                latency_ms: route
                    .path_latency
                    .checked_abs()
                    .map(|value| value as u64)
                    .filter(|value| *value > 0),
            })
            .collect()
    }

    /// 在虚拟网的 `port` 上监听，只接受虚拟网内的连接。
    pub async fn bind_overlay(&self, port: u16) -> Result<DataPlaneTcpListener, String> {
        self.manager
            .data_plane_tcp_bind(&self.instance_id, port, DATA_PLANE_TIMEOUT)
            .await
            .map_err(|error| format!("监听虚拟网端口 {port} 失败: {error}"))
    }

    /// 拨虚拟网内的 `peer:port`。
    pub async fn connect_overlay(
        &self,
        peer: Ipv4Addr,
        port: u16,
    ) -> Result<DataPlaneTcpStream, String> {
        self.manager
            .data_plane_tcp_connect(
                &self.instance_id,
                SocketAddr::new(peer.into(), port),
                CONNECT_TIMEOUT,
            )
            .await
            .map_err(|error| format!("连接虚拟网 {peer}:{port} 失败: {error}"))
    }

    pub async fn stop(&self) {
        let _ = self
            .manager
            .delete_network_instances([self.instance_id])
            .await;
    }
}

/// `cidr::Ipv4Inet` 与 proto 的 `Ipv4Inet` 都实现了 `Display`（形如 `10.126.126.2/24`），
/// 统一按字符串取地址部分，避免依赖其内部类型。
fn parse_inet_address(value: &impl std::fmt::Display) -> Option<Ipv4Addr> {
    value
        .to_string()
        .split('/')
        .next()
        .and_then(|address| address.parse().ok())
}

/// 主网配置入口（node 测试使用）；运行时统一走 [`build_config_named`]。
#[cfg(test)]
fn build_config(
    identity: &MeshIdentity,
    hostname: &str,
    rpc_port: u16,
    listener_port: u16,
) -> String {
    build_config_named(
        DEFAULT_INSTANCE_NAME,
        identity,
        hostname,
        rpc_port,
        listener_port,
    )
}

/// 组装一份 EasyTier TOML 配置；`instance_name` 只用于管理器内的展示区分。
pub(crate) fn build_config_named(
    instance_name: &str,
    identity: &MeshIdentity,
    hostname: &str,
    rpc_port: u16,
    listener_port: u16,
) -> String {
    let peers = preset_peers();
    let mut out = String::new();
    out.push_str(&format!("instance_name = \"{}\"\n", escape_toml(instance_name)));
    out.push_str(&format!("hostname = \"{}\"\n", escape_toml(hostname)));
    out.push_str("dhcp = true\n");
    out.push_str(&format!(
        "listeners = [\"tcp://0.0.0.0:{listener_port}\", \"udp://0.0.0.0:{listener_port}\"]\n"
    ));
    out.push_str(&format!("rpc_portal = \"127.0.0.1:{rpc_port}\"\n\n"));
    out.push_str("[network_identity]\n");
    out.push_str(&format!(
        "network_name = \"{}\"\n",
        escape_toml(&identity.network_name)
    ));
    out.push_str(&format!(
        "network_secret = \"{}\"\n\n",
        escape_toml(&identity.network_secret)
    ));
    for peer in peers {
        out.push_str("[[peer]]\n");
        out.push_str(&format!("uri = \"{}\"\n\n", escape_toml(&peer)));
    }
    out.push_str("[flags]\n");
    out.push_str("no_tun = true\n");
    // Android / 普通用户都没有绑定物理网卡的权限；关掉后仍可通过中继与打洞组网。
    out.push_str("bind_device = false\n");
    out.push_str("disable_upnp = true\n");
    out.push_str("enable_encryption = true\n");
    out.push_str("encryption_algorithm = \"aes-gcm\"\n");
    out.push_str("latency_first = true\n");
    out
}

fn escape_toml(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '"' => "\\\"".to_string(),
            '\n' | '\r' | '\t' => " ".to_string(),
            c if c.is_control() => " ".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// 固定挑一个可用的 0.0.0.0 TCP 端口，UDP 复用同一端口号。
pub(crate) fn pick_listener_port() -> u16 {
    for port in LISTENER_PORT_PREFERRED..=LISTENER_PORT_END {
        if let Ok(listener) = std::net::TcpListener::bind(("0.0.0.0", port)) {
            drop(listener);
            return port;
        }
    }
    LISTENER_PORT_PREFERRED
}

/// 读取根目录预置的 rendezvous 节点。
///
/// 列表为空或全部不可达时节点仍能启动，只是无法发现跨网设备；这里只做格式校验，
/// 不因为单个坏条目放弃整份列表。
fn preset_peers() -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct PeersFile {
        #[serde(default)]
        peers: Vec<String>,
    }

    let parsed: PeersFile = match serde_json::from_str(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../easytier-peers.json"))) {
        Ok(parsed) => parsed,
        Err(error) => {
            log::warn!("easytier-peers.json 无法解析，按空列表处理: {error}");
            return Vec::new();
        }
    };
    parsed
        .peers
        .into_iter()
        .filter_map(|raw| match validate_peer_uri(&raw) {
            Some(uri) => Some(uri),
            None => {
                log::warn!("忽略非法的预置 peer: {raw}");
                None
            }
        })
        .collect()
}

/// 只接受 `tcp://` / `udp://` / `ws://` / `wss://` / `quic://` / `faketcp://`
/// 形式的 `host:port`，拒绝带凭据、路径或查询串的地址。
fn validate_peer_uri(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = reqwest::Url::parse(raw).ok()?;
    let scheme = url.scheme();
    if !matches!(scheme, "tcp" | "udp" | "ws" | "wss" | "quic" | "faketcp") {
        return None;
    }
    if url.host_str().is_none() || url.port().is_none() {
        return None;
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    if url.path() != "/" && !url.path().is_empty() {
        return None;
    }
    Some(raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> MeshIdentity {
        MeshIdentity {
            network_name: "your-szu-life:self:0011223344556677".into(),
            network_secret: "a".repeat(64),
            student_id: "0000".into(),
        }
    }

    #[test]
    fn config_is_no_tun_and_has_no_exit_or_proxy_network() {
        let toml = build_config(&identity(), "设备 A", 54201, 54210);
        assert!(toml.contains("no_tun = true"));
        assert!(toml.contains("bind_device = false"));
        assert!(toml.contains("encryption_algorithm = \"aes-gcm\""));
        assert!(toml.contains("dhcp = true"));
        assert!(!toml.contains("proxy_network"));
        assert!(!toml.contains("exit_node"));
        assert!(toml.contains("rpc_portal = \"127.0.0.1:54201\""));
        assert!(toml.contains("listeners = [\"tcp://0.0.0.0:54210\""));
    }

    #[test]
    fn network_identity_is_serialized_in_the_identity_table() {
        let toml = build_config(&identity(), "device", 54201, 54210);
        assert!(toml.contains("[network_identity]"));
        assert!(toml.contains("network_name = \"your-szu-life:self:0011223344556677\""));
        assert!(toml.contains(&format!("network_secret = \"{}\"", "a".repeat(64))));
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn peering_uris_are_embedded_as_peer_tables() {
        let toml = build_config(&identity(), "device", 54201, 54210);
        for peer in preset_peers() {
            assert!(toml.contains(&format!("uri = \"{peer}\"")));
        }
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn hostname_is_escaped() {
        let toml = build_config(&identity(), "a\"b\\c", 54201, 54210);
        assert!(toml.contains("hostname = \"a\\\"b\\\\c\""));
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn peer_uri_validation_rejects_unexpected_shapes() {
        assert_eq!(
            validate_peer_uri("tcp://example.com:11010").as_deref(),
            Some("tcp://example.com:11010")
        );
        assert!(validate_peer_uri("http://example.com:11010").is_none());
        assert!(validate_peer_uri("tcp://example.com").is_none());
        assert!(validate_peer_uri("tcp://user:pw@example.com:11010").is_none());
        assert!(validate_peer_uri("tcp://example.com:11010/path").is_none());
    }

    #[test]
    fn inet_display_is_parsed_to_an_address() {
        assert_eq!(
            parse_inet_address(&"10.126.126.2/24").map(|ip| ip.to_string()),
            Some("10.126.126.2".to_string())
        );
        assert!(parse_inet_address(&"not-an-address").is_none());
    }

    /// 真实创建一次实例（不启动），确保生成的 TOML 通过 EasyTier 的能力校验。
    /// 缺 `dhcp-ipv4` / `icmp-proxy` 等 feature 时，这里会像运行时一样报错。
    #[test]
    fn generated_config_passes_easytier_capability_validation() {
        let manager = native_instance_manager();
        let toml = build_config(&identity(), "测试设备", 54299, 54298);
        let config = TomlConfig::new_from_str(&toml).expect("配置应能解析");
        let instance = manager
            .create(config, ())
            .expect("配置应通过 EasyTier 的能力校验");
        drop(instance);
    }
}
