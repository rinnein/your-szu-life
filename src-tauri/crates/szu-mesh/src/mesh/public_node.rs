//! 随进程常驻的公共初始化实例。
//!
//! 所有安装都加入同一个 EasyTier 网络 `your-szu-life:public:init`：彼此既是
//! rendezvous 也是中继。本机的功能实例（主自组网、配对 host/client、签到房间）
//! 零监听端口，只用 `ring://<本实例 id>` 连到这里；跨设备的数据由公共网络转发到
//! 对端所属的公共实例（EasyTier 的 foreign-network owner map）。
//!
//! 生命周期与进程一致：应用启动时拉起，之后不再停止，也不随同步/校园网开关
//! 变化。它不承载任何本机应用数据面（没有 overlay 端口、没有 HTTP 接口），只
//! 交换节点信息并转发端到端加密后的数据。

use std::sync::{Arc, OnceLock};

use super::identity;
use super::node::Node;

/// 公共实例在管理器实例列表里的名字。
pub const INSTANCE_NAME: &str = "your-szu-life-public-init";

/// 公共实例的 peer 列表里显示的名字。固定值，不暴露用户的设备名。
pub const HOSTNAME: &str = "public-init";

/// 公共网络的引导子网：/16 给全部用户留足地址，并与 EasyTier 默认的
/// `10.126.126.0/24` 引导子网错开（功能实例仍用默认子网）。
pub const DHCP_CIDR: &str = "10.127.0.0/16";

/// 只为本项目自己的网络名做中继。EasyTier 的默认值是 `"*"`（任何网络）。
pub const RELAY_WHITELIST: &str = "your-szu-life:*";

static NODE: OnceLock<tokio::sync::Mutex<Option<Arc<Node>>>> = OnceLock::new();

/// 取得（必要时启动）公共实例。失败不缓存，下一次调用会重试。
///
/// 调用方是 [`Node::start_named`]（所有功能实例），因此它必须先于任何功能实例
/// 就绪；启动失败时功能实例直接报错，不回落预置 rendezvous。
pub async fn ensure() -> Result<Arc<Node>, String> {
    let slot = NODE.get_or_init(|| tokio::sync::Mutex::new(None));
    let mut guard = slot.lock().await;
    if let Some(node) = guard.as_ref() {
        return Ok(node.clone());
    }
    let node = Arc::new(
        Node::start_public(&identity::public_identity(), HOSTNAME, INSTANCE_NAME).await?,
    );
    log::info!(
        "公共组网初始化节点已就绪（ring://{}，监听 3 个协议端口）",
        node.instance_id()
    );
    *guard = Some(node.clone());
    Ok(node)
}
