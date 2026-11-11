//! 虚拟网与本机回环之间的字节桥。
//!
//! EasyTier 的数据面只提供用户态 TCP 流，不是 OS socket，因此不能直接交给
//! axum / reqwest。这里用两段桥把「虚拟网端口」和「127.0.0.1 端口」接起来：
//!
//! - [`inbound`]：虚拟网 `port` 上的入站连接 → 本机已经在跑的服务；
//! - [`outbound`]：本机临时回环端口 → 虚拟网某个 peer 的 `port`。
//!
//! 两段桥都只把字节原样转发，协议层（HTTP、代理）仍然由两端各自实现。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener as TokioTcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::node::Node;
use crate::ports;

pub(crate) struct Bridge {
    pub(crate) local_addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// 把虚拟网 `overlay_port` 上的入站连接转发到本机 `local_addr`。
pub(crate) async fn inbound(
    node: Arc<Node>,
    overlay_port: u16,
    local_addr: SocketAddr,
) -> Result<JoinHandle<()>, String> {
    let mut listener = node.bind_overlay(overlay_port).await?;
    Ok(tokio::spawn(async move {
        loop {
            let (remote, peer) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    log::warn!("设备同步入站连接监听结束: {error}");
                    break;
                }
            };
            tokio::spawn(async move {
                let mut remote = remote;
                match TcpStream::connect(local_addr).await {
                    Ok(mut local) => {
                        let _ = copy_bidirectional(&mut local, &mut remote).await;
                    }
                    Err(error) => {
                        log::warn!("设备同步入站桥接失败（{peer}）: {error}");
                    }
                }
            });
        }
    }))
}

/// 在本机回环上开一个端口，每条连接都拨到虚拟网里的 `peer:overlay_port`。
pub(crate) fn outbound(node: Arc<Node>, peer: Ipv4Addr, overlay_port: u16) -> Result<Bridge, String> {
    let (listener, _) = ports::bind_loopback(ports::PortService::Mesh, None)
        .map_err(|error| error.to_string())?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("读取桥接端口失败: {error}"))?;
    let listener = TokioTcpListener::from_std(listener)
        .map_err(|error| format!("桥接端口不可用: {error}"))?;
    let task = tokio::spawn(async move {
        loop {
            let (mut local, _) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    log::warn!("设备同步出站桥接监听结束: {error}");
                    break;
                }
            };
            let node = node.clone();
            tokio::spawn(async move {
                match node.connect_overlay(peer, overlay_port).await {
                    Ok(mut remote) => {
                        let _ = copy_bidirectional(&mut local, &mut remote).await;
                    }
                    Err(error) => log::warn!("{error}"),
                }
            });
        }
    });
    Ok(Bridge { local_addr, task })
}
