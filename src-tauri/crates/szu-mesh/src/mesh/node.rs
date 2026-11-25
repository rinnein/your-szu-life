//! 进程内的 EasyTier 节点。
//!
//! 所有设备都跑在 `no_tun` 模式下：不创建 TUN 设备、不碰系统路由，也不需要
//! Android 的 VpnService。对等设备之间需要 TCP 时，走 EasyTier 的用户态数据面
//! （`ffi-dataplane`）：`bind_overlay` 在虚拟 IP 上监听，`connect_overlay` 直接
//! 拨虚拟网目标，二者都只影响本进程。
//!
//! 实例分两种角色（见 [`MeshRole`]）：
//!
//! - **公共中继**（[`super::public_node`]）：常驻、开放三组真实监听端口、连预置
//!   rendezvous，并为 `your-szu-life:*` 网络转发；
//! - **功能实例**（主自组网、配对、签到房间）：零监听端口，唯一初始 peer 是本机
//!   公共中继的 `ring://<instance-id>`，跨设备数据全部经公共网络中转。
//!
//! 这里是全应用**唯一**一处组装 EasyTier 配置的地方：网络身份、peer 列表、
//! 监听端口、中继白名单与各项 flag 都在 [`build_config_named`] 里确定。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
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

/// 全进程唯一的 EasyTier 实例管理器。
///
/// `ring://<instance-id>` 的注册表挂在 manager 内部的 `CoreProcessRuntime` 上，
/// 只有共享同一个 manager 的实例才能用它互连；而 `native_instance_manager()`
/// 每次调用都会新建一个 process runtime。若每个 Node 各建一个 manager，功能实例
/// 就连不上本机的公共中继。上游 `easytier-ffi` 也是同样的全局单例做法。
fn instance_manager() -> Arc<NativeInstanceManager> {
    static MANAGER: OnceLock<Arc<NativeInstanceManager>> = OnceLock::new();
    MANAGER
        .get_or_init(|| Arc::new(native_instance_manager()))
        .clone()
}

/// EasyTier 的各协议监听端口组。同一端口号只承载不同底层传输：tcp+udp 一组、
/// ws（TCP）+quic（UDP）一组，faketcp 是纯 TCP 独占一组；各组由统一端口模块
/// 分配互不相同的端口，只启用这三组（wg 面向 TUN 设备、wss 需 TLS 证书，均未
/// 启用）。只有常驻公共中继会绑定这些端口，功能实例零监听。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ListenerPorts {
    /// tcp + udp 共用的端口。
    pub tcp_udp: u16,
    /// ws 与 quic 共用的端口（分别落在 TCP 与 UDP 上，不冲突）。
    pub ws_quic: u16,
    /// faketcp 监听端口（同为 TCP 底座，不能与 tcp/ws 共号）。
    pub faketcp: u16,
}

/// 实例角色：决定监听端口、初始 peer 与中继白名单。
pub(crate) enum MeshRole<'a> {
    /// 常驻公共中继：开放全部协议监听、连 `peers`（生产是预置 rendezvous），
    /// 并为 `your-szu-life:*` 中继。
    Public {
        listeners: ListenerPorts,
        peers: Vec<String>,
    },
    /// 功能实例：零监听端口，只经进程内 `ring://` 连本机公共中继。
    Client { relay_peer: &'a str },
}

/// 主自组网实例名；另有常驻公共实例（见 [`super::public_node::INSTANCE_NAME`]）。
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
    manager: Arc<NativeInstanceManager>,
    instance_id: uuid::Uuid,
    /// 启动时写死的虚拟 IP；DHCP 之后一般不变，但以 `self_info()` 的实时值为准。
    boot_ipv4: Mutex<Option<Ipv4Addr>>,
}

impl Node {
    /// 启动主自组网实例。所有功能实例都依赖本机公共中继。
    pub async fn start(identity: &MeshIdentity, hostname: &str) -> Result<Self, String> {
        Self::start_named(identity, hostname, DEFAULT_INSTANCE_NAME).await
    }

    /// 以自定义网络身份与实例名启动一个功能实例（配对 host/client、签到房间共用）。
    ///
    /// 功能实例零端口：不监听任何真实端口，唯一初始 peer 是本机常驻公共中继的
    /// `ring://<instance-id>`；跨设备数据由公共中继所在的公共网络转发。
    pub async fn start_named(
        identity: &MeshIdentity,
        hostname: &str,
        instance_name: &str,
    ) -> Result<Self, String> {
        let relay = super::public_node::ensure()
            .await
            .map_err(|error| format!("公共初始化节点未运行: {error}"))?;
        let relay_peer = format!("ring://{}", relay.instance_id());
        let toml = build_config_named(
            instance_name,
            identity,
            hostname,
            MeshRole::Client {
                relay_peer: &relay_peer,
            },
        );
        Self::launch(instance_name, &toml).await
    }

    /// 启动常驻公共中继实例（见 [`super::public_node`]）。三组协议监听端口由端口
    /// 模块互异分配，是本项目唯一一处 `0.0.0.0` 监听。
    pub(crate) async fn start_public(
        identity: &MeshIdentity,
        hostname: &str,
        instance_name: &str,
    ) -> Result<Self, String> {
        Self::start_relay(identity, hostname, instance_name, preset_peers()).await
    }

    /// 公共中继的实际装配；`peers` 由调用方给（生产是预置 rendezvous，测试传空，
    /// 避免测试访问真实网络）。
    async fn start_relay(
        identity: &MeshIdentity,
        hostname: &str,
        instance_name: &str,
        peers: Vec<String>,
    ) -> Result<Self, String> {
        let mut allocated = ports::reserve_loopback_n(ports::PortService::Mesh, 3)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|allocation| allocation.port);
        let listeners = ListenerPorts {
            tcp_udp: allocated.next().expect("已请求 3 个端口"),
            ws_quic: allocated.next().expect("已请求 3 个端口"),
            faketcp: allocated.next().expect("已请求 3 个端口"),
        };
        let toml = build_config_named(
            instance_name,
            identity,
            hostname,
            MeshRole::Public { listeners, peers },
        );
        Self::launch(instance_name, &toml).await
    }

    /// 用已经组装好的 TOML 启动实例并等待虚拟 IP。失败时显式删除实例，避免它
    /// 留在 manager 的实例表里（`create` 会登记，drop 不会移除）。
    async fn launch(instance_name: &str, toml: &str) -> Result<Self, String> {
        let config = TomlConfig::new_from_str(toml)
            .map_err(|error| format!("组网实例「{instance_name}」配置无效: {error:#}"))?;
        let manager = instance_manager();
        let instance_id = manager
            .run_network_instance(config, ConfigFileControl::STATIC_CONFIG)
            .map_err(|error| format!("组网实例「{instance_name}」启动失败: {error:#}"))?;

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

    /// 本实例的 EasyTier id；`ring://` 互连与日志定位都用它。
    pub fn instance_id(&self) -> uuid::Uuid {
        self.instance_id
    }

    async fn wait_ready(&self) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            let Some(instance) = self.manager.instance(self.instance_id) else {
                return Err("组网实例已退出".into());
            };
            if let Some(error) = instance.latest_error().filter(|error| !error.trim().is_empty()) {
                return Err(format!("组网实例启动失败: {error}"));
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
                return Err("组网实例启动超时（未分配到虚拟 IP）".into());
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

/// 公共中继配置入口（node 测试使用）；运行时统一走 [`build_config_named`]。
#[cfg(test)]
fn build_public_config(identity: &MeshIdentity, hostname: &str, listeners: &ListenerPorts) -> String {
    build_config_named(
        super::public_node::INSTANCE_NAME,
        identity,
        hostname,
        MeshRole::Public {
            listeners: *listeners,
            peers: preset_peers(),
        },
    )
}

/// 功能实例配置入口（node 测试使用）。
#[cfg(test)]
fn build_client_config(identity: &MeshIdentity, hostname: &str, relay_peer: &str) -> String {
    build_config_named(
        DEFAULT_INSTANCE_NAME,
        identity,
        hostname,
        MeshRole::Client { relay_peer },
    )
}

/// 组装一份 EasyTier TOML 配置；`instance_name` 只用于管理器内的展示区分。
///
/// 两种角色的差异全部在这里：公共中继用固定的大子网、三组协议监听、预置
/// rendezvous 与 `your-szu-life:*` 中继白名单；功能实例零监听、只连本机公共中继
/// 的 `ring://`，并显式把中继白名单设为空（EasyTier 默认是 `"*"`，会给任意网络
/// 做中继）。
pub(crate) fn build_config_named(
    instance_name: &str,
    identity: &MeshIdentity,
    hostname: &str,
    role: MeshRole<'_>,
) -> String {
    let (dhcp, listeners, peers, relay_whitelist) = match &role {
        MeshRole::Public { listeners, peers } => (
            format!("dhcp = \"{}\"", super::public_node::DHCP_CIDR),
            format!(
                "listeners = [\"tcp://0.0.0.0:{tcp_udp}\", \"udp://0.0.0.0:{tcp_udp}\", \
                 \"ws://0.0.0.0:{ws_quic}\", \"quic://0.0.0.0:{ws_quic}\", \
                 \"faketcp://0.0.0.0:{faketcp}\"]",
                tcp_udp = listeners.tcp_udp,
                ws_quic = listeners.ws_quic,
                faketcp = listeners.faketcp,
            ),
            peers.clone(),
            super::public_node::RELAY_WHITELIST,
        ),
        MeshRole::Client { relay_peer } => (
            "dhcp = true".to_owned(),
            "listeners = []".to_owned(),
            vec![(*relay_peer).to_owned()],
            "",
        ),
    };
    let mut out = String::new();
    out.push_str(&format!("instance_name = \"{}\"\n", escape_toml(instance_name)));
    out.push_str(&format!("hostname = \"{}\"\n", escape_toml(hostname)));
    out.push_str(&dhcp);
    out.push('\n');
    out.push_str(&listeners);
    out.push_str("\n\n");
    out.push_str("[network_identity]\n");
    out.push_str(&format!(
        "network_name = \"{}\"\n",
        escape_toml(&identity.network_name)
    ));
    out.push_str(&format!(
        "network_secret = \"{}\"\n\n",
        escape_toml(&identity.network_secret)
    ));
    // 不写密钥：EasyTier 解析 TOML 时会调用 normalize_secure_mode_config，自己生成
    // 随机的每设备 x25519 密钥对并经 route peer info 广播公钥（官方 --secure-mode 行为）。
    out.push_str("[secure_mode]\n");
    out.push_str("enabled = true\n\n");
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
    out.push_str(&format!(
        "relay_network_whitelist = \"{}\"\n",
        escape_toml(relay_whitelist)
    ));
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
/// 形式的 `host:port` 与无端口的 `txt://` 发现域（端点由 DNS TXT/SRV 解析），
/// 拒绝带凭据、路径或查询串的地址。
fn validate_peer_uri(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = reqwest::Url::parse(raw).ok()?;
    let scheme = url.scheme();
    let is_discovery = scheme == "txt";
    if !matches!(scheme, "tcp" | "udp" | "ws" | "wss" | "quic" | "faketcp") && !is_discovery {
        return None;
    }
    if url.host_str().is_none() || (url.port().is_none() && !is_discovery) {
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

    fn test_listener_ports() -> ListenerPorts {
        ListenerPorts {
            tcp_udp: 54210,
            ws_quic: 54211,
            faketcp: 54212,
        }
    }

    /// 功能实例的测试配置；`ring://` 对端用固定 uuid。
    fn client_config(hostname: &str) -> String {
        build_client_config(
            &identity(),
            hostname,
            "ring://00000000-0000-0000-0000-000000000001",
        )
    }

    #[test]
    fn public_config_opens_every_protocol_and_relays_only_our_networks() {
        let toml = build_public_config(&identity(), "public-init", &test_listener_ports());
        assert!(toml.contains("dhcp = \"10.127.0.0/16\""));
        assert!(toml.contains(
            "listeners = [\"tcp://0.0.0.0:54210\", \"udp://0.0.0.0:54210\", \
             \"ws://0.0.0.0:54211\", \"quic://0.0.0.0:54211\", \"faketcp://0.0.0.0:54212\"]"
        ));
        assert!(toml.contains("relay_network_whitelist = \"your-szu-life:*\""));
        // rpc_portal 不在当前 EasyTier 的 TOML schema 里，写了也会被忽略；不要再输出。
        assert!(!toml.contains("rpc_portal"));
        for peer in preset_peers() {
            assert!(toml.contains(&format!("uri = \"{peer}\"")));
        }
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn client_config_has_no_listener_and_only_the_ring_relay() {
        let toml = client_config("设备 A");
        assert!(toml.contains("dhcp = true"));
        assert!(toml.contains("listeners = []"));
        assert!(toml.contains("relay_network_whitelist = \"\""));
        assert!(toml.contains("[[peer]]\nuri = \"ring://00000000-0000-0000-0000-000000000001\""));
        assert_eq!(toml.matches("[[peer]]").count(), 1);
        assert!(!toml.contains("rpc_portal"));
        // 功能实例不再连预置 rendezvous，跨设备只经本机公共中继。
        for peer in preset_peers() {
            assert!(!toml.contains(&format!("uri = \"{peer}\"")));
        }
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn config_is_no_tun_and_has_no_exit_or_proxy_network() {
        for toml in [
            build_public_config(&identity(), "public-init", &test_listener_ports()),
            client_config("设备 A"),
        ] {
            assert!(toml.contains("no_tun = true"));
            assert!(toml.contains("bind_device = false"));
            assert!(toml.contains("encryption_algorithm = \"aes-gcm\""));
            assert!(!toml.contains("proxy_network"));
            assert!(!toml.contains("exit_node"));
        }
    }

    #[test]
    fn network_identity_is_serialized_in_the_identity_table() {
        let toml = client_config("device");
        assert!(toml.contains("[network_identity]"));
        assert!(toml.contains("network_name = \"your-szu-life:self:0011223344556677\""));
        assert!(toml.contains(&format!("network_secret = \"{}\"", "a".repeat(64))));
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn secure_mode_is_enabled_and_easytier_generates_the_keypair() {
        let toml = client_config("device");
        assert!(toml.contains("[secure_mode]"));
        assert!(toml.contains("enabled = true"));
        let config = TomlConfig::new_from_str(&toml).expect("配置应能解析");
        use easytier_core::config::toml::ConfigLoader as _;
        let secure = config
            .get_secure_mode()
            .expect("EasyTier 应在解析时归一化 [secure_mode] 段");
        assert!(secure.enabled);
        let private = secure.local_private_key.expect("应已随机生成私钥");
        let public = secure.local_public_key.expect("应已派生公钥");
        assert!(!private.is_empty());
        assert!(!public.is_empty());
        assert_ne!(private, public);
    }

    #[test]
    fn hostname_is_escaped() {
        let toml = build_client_config(
            &identity(),
            "a\"b\\c",
            "ring://00000000-0000-0000-0000-000000000001",
        );
        assert!(toml.contains("hostname = \"a\\\"b\\\\c\""));
        assert!(TomlConfig::new_from_str(&toml).is_ok());
    }

    #[test]
    fn txt_discovery_peer_uri_is_accepted() {
        assert_eq!(
            validate_peer_uri("txt://example.com").as_deref(),
            Some("txt://example.com")
        );
        assert!(validate_peer_uri("http://example.com").is_none());
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
    ///
    /// 每个用例都新建一个一次性 manager：`create` 会把实例登记进 manager 的实例表，
    /// 用进程级单例会让这些从未启动的实例一直留在静态表里。
    fn assert_capability_valid(toml: &str) {
        let manager = native_instance_manager();
        let config = TomlConfig::new_from_str(toml).expect("配置应能解析");
        let instance = manager
            .create(config, ())
            .expect("配置应通过 EasyTier 的能力校验");
        drop(instance);
    }

    #[test]
    fn public_config_passes_easytier_capability_validation() {
        assert_capability_valid(&build_public_config(
            &identity(),
            "public-init",
            &ListenerPorts {
                tcp_udp: 54298,
                ws_quic: 54297,
                faketcp: 54296,
            },
        ));
    }

    #[test]
    fn client_config_passes_easytier_capability_validation() {
        assert_capability_valid(&client_config("测试设备"));
    }

    async fn launch_client(
        instance_name: &str,
        identity: &MeshIdentity,
        hostname: &str,
        relay: &Node,
    ) -> Node {
        let relay_peer = format!("ring://{}", relay.instance_id());
        let toml = build_config_named(
            instance_name,
            identity,
            hostname,
            MeshRole::Client {
                relay_peer: &relay_peer,
            },
        );
        Node::launch(instance_name, &toml)
            .await
            .unwrap_or_else(|error| panic!("功能实例 {instance_name} 应能启动: {error}"))
    }

    async fn wait_for_peer(node: &Node, hostname: &str) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            let peers = node.peers().await;
            if peers.iter().any(|peer| peer.hostname == hostname) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{hostname} 没有出现在对端列表里: {peers:?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// 真实启动四个实例，验证完整拓扑：两个公共中继在同一个公共网络（经 ring 互连），
    /// 两个功能实例分属不同中继、同属一个账号网络，应能互相发现；中继本身在功能
    /// 实例的网络里没有虚拟 IP，不会出现在设备列表里。
    ///
    /// 忽略原因：会真实启动 4 个 EasyTier 实例（各自 DHCP 引导，十秒级），两个中继
    /// 还会各绑 3 个 `0.0.0.0` 端口；不访问外部网络（中继的 peer 列表为空，中继之间
    /// 走 ring）。修改 manager 单例、ring 装配、中继白名单或 [`MeshRole`] 时手动运行：
    /// `cargo test -p szu-mesh -- --ignored`
    #[tokio::test]
    #[ignore = "启动真实 EasyTier 实例（十秒级、绑定 6 个端口）；改 manager/ring/中继装配时手动运行"]
    async fn clients_behind_different_relays_find_each_other_over_ring() {
        let public = crate::mesh::identity::public_identity();
        let relay_a = Node::start_relay(
            &public,
            crate::mesh::public_node::HOSTNAME,
            "test-relay-a",
            Vec::new(),
        )
        .await
        .expect("公共中继 A 应能启动");
        let relay_b = Node::start_relay(
            &public,
            crate::mesh::public_node::HOSTNAME,
            "test-relay-b",
            vec![format!("ring://{}", relay_a.instance_id())],
        )
        .await
        .expect("公共中继 B 应能启动");

        let account = MeshIdentity {
            network_name: "your-szu-life:self:0011223344556677".into(),
            network_secret: "b".repeat(64),
            student_id: "0000".into(),
        };
        let client_a = launch_client("test-client-a", &account, "客户端A", &relay_a).await;
        let client_b = launch_client("test-client-b", &account, "客户端B", &relay_b).await;

        wait_for_peer(&client_a, "客户端B").await;
        wait_for_peer(&client_b, "客户端A").await;

        for client in [&client_a, &client_b] {
            for peer in client.peers().await {
                if peer.hostname.starts_with("PublicServer_") {
                    assert!(
                        peer.ipv4.is_none(),
                        "公共中继不应带虚拟 IP，否则会出现在设备列表里: {peer:?}"
                    );
                }
            }
        }

        client_a.stop().await;
        client_b.stop().await;
        relay_b.stop().await;
        relay_a.stop().await;
    }
}
