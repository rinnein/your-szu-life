//! 本地服务端口的统一分配与耗尽上报。
//!
//! 需要监听本机回环端口的服务（MCP、aria2 RPC、JXXK 录像下载入口）各自拥有
//! 互不重叠的端口段，统一由本模块完成可用性探测、分配与耗尽上报。段与段不
//! 重叠，因此不需要跨服务的进程内预留表。
//!
//! MCP 需要稳定端口：调用方把上次保存的端口作为 `preferred` 传入，本模块优先
//! 尝试它；被占用时再在段内寻找其它端口，由调用方负责持久化新端口并提示用户。

use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub(crate) mod commands;

#[cfg(test)]
mod tests;

pub(crate) const MCP_PORT_RANGE: (u16, u16) = (53901, 54000);
pub(crate) const ARIA2_PORT_RANGE: (u16, u16) = (54001, 54100);
pub(crate) const JXXK_DOWNLOAD_PORT_RANGE: (u16, u16) = (54101, 54200);

const NOTICE_EVENT: &str = "app://port-notice";

static APP: OnceLock<AppHandle> = OnceLock::new();
static NOTICE_QUEUE: Mutex<Vec<PortNotice>> = Mutex::new(Vec::new());
static NEXT_NOTICE_ID: AtomicU64 = AtomicU64::new(1);
static FRONTEND_READY: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PortService {
    Mcp,
    Aria2,
    JxxkDownload,
}

impl PortService {
    fn label(self) -> &'static str {
        match self {
            Self::Mcp => "MCP 本地服务",
            Self::Aria2 => "aria2 下载服务",
            Self::JxxkDownload => "JXXK 录像下载服务",
        }
    }

    fn range(self) -> (u16, u16) {
        match self {
            Self::Mcp => MCP_PORT_RANGE,
            Self::Aria2 => ARIA2_PORT_RANGE,
            Self::JxxkDownload => JXXK_DOWNLOAD_PORT_RANGE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortAllocation {
    pub(crate) port: u16,
    /// 首选端口被占用时，记录被跳过的那个端口；首次分配为 `None`。
    pub(crate) switched_from: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PortError {
    Exhausted {
        service: PortService,
        start: u16,
        end: u16,
    },
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exhausted { service, start, end } => write!(
                f,
                "端口分配失败：{}在 {start}–{end} 范围内找不到可用端口，请关闭占用这些端口的程序后重试。",
                service.label()
            ),
        }
    }
}

impl std::error::Error for PortError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PortNoticeKind {
    McpPortChanged,
    PortsExhausted,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PortNotice {
    pub(crate) id: u64,
    pub(crate) kind: PortNoticeKind,
    pub(crate) service: PortService,
    pub(crate) previous_port: Option<u16>,
    pub(crate) current_port: Option<u16>,
    pub(crate) message: String,
}

/// 保存应用句柄，供端口通知在任意调用点直接推送到前端。
pub(crate) fn initialize(app: &AppHandle) {
    let _ = APP.set(app.clone());
}

/// 在服务端口段内绑定一个回环监听器，并保留它（MCP、JXXK 使用）。
pub(crate) fn bind_loopback(
    service: PortService,
    preferred: Option<u16>,
) -> Result<(TcpListener, PortAllocation), PortError> {
    let (start, end) = service.range();
    match allocate_in_range(service, preferred, start, end) {
        Ok(result) => Ok(result),
        Err(error) => {
            report_exhausted(service, start, end);
            Err(error)
        }
    }
}

/// 在服务端口段内探测一个可用端口后立即释放，交给外部进程自己绑定（aria2 使用）。
pub(crate) fn reserve_loopback(
    service: PortService,
    preferred: Option<u16>,
) -> Result<PortAllocation, PortError> {
    let (start, end) = service.range();
    match allocate_in_range(service, preferred, start, end) {
        Ok((listener, allocation)) => {
            drop(listener);
            Ok(allocation)
        }
        Err(error) => {
            report_exhausted(service, start, end);
            Err(error)
        }
    }
}

/// MCP 端口发生切换时的用户提示。
pub(crate) fn notify_mcp_port_changed(from: u16, to: u16) {
    let message = format!(
        "MCP 本地服务原端口 {from} 已被占用，已自动切换到 {to} 并保存该设置。\
         若外部客户端已配置旧端口，请同步更新为 http://127.0.0.1:{to}/mcp。"
    );
    log::warn!("{message}");
    push_notice(PortNotice {
        id: next_notice_id(),
        kind: PortNoticeKind::McpPortChanged,
        service: PortService::Mcp,
        previous_port: Some(from),
        current_port: Some(to),
        message,
    });
}

/// 取走启动阶段积压的端口通知，并把前端标记为已就绪。
pub(crate) fn take_pending_notices() -> Vec<PortNotice> {
    // 持锁完成「置位 + 清空」，与 push_notice 的「持锁判断 + 入队」互斥，
    // 避免通知在置位和清空之间入队后永远没人取走。
    let mut queue = NOTICE_QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    FRONTEND_READY.store(true, Ordering::SeqCst);
    std::mem::take(&mut *queue)
}

fn allocate_in_range(
    service: PortService,
    preferred: Option<u16>,
    start: u16,
    end: u16,
) -> Result<(TcpListener, PortAllocation), PortError> {
    let preferred = preferred.filter(|port| (start..=end).contains(port));
    let capacity = end.saturating_sub(start).saturating_add(1) as usize;
    let mut candidates = Vec::with_capacity(capacity);
    if let Some(port) = preferred {
        candidates.push(port);
    }
    candidates.extend((start..=end).filter(|port| Some(*port) != preferred));

    for port in candidates {
        if let Some(listener) = try_bind(port) {
            log::debug!("本地端口已分配: {port}");
            return Ok((
                listener,
                PortAllocation {
                    port,
                    switched_from: preferred.filter(|candidate| *candidate != port),
                },
            ));
        }
    }

    Err(PortError::Exhausted {
        service,
        start,
        end,
    })
}

fn try_bind(port: u16) -> Option<TcpListener> {
    let listener = TcpListener::bind(("127.0.0.1", port)).ok()?;
    listener.set_nonblocking(true).ok()?;
    Some(listener)
}

fn report_exhausted(service: PortService, start: u16, end: u16) {
    let message = PortError::Exhausted {
        service,
        start,
        end,
    }
    .to_string();
    log::error!("{message}");
    push_notice(PortNotice {
        id: next_notice_id(),
        kind: PortNoticeKind::PortsExhausted,
        service,
        previous_port: None,
        current_port: None,
        message,
    });
}

fn next_notice_id() -> u64 {
    NEXT_NOTICE_ID.fetch_add(1, Ordering::Relaxed)
}

fn push_notice(notice: PortNotice) {
    let mut queue = NOTICE_QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if FRONTEND_READY.load(Ordering::SeqCst) {
        drop(queue);
        if let Some(app) = APP.get() {
            if let Err(error) = app.emit(NOTICE_EVENT, notice) {
                log::warn!("发送端口通知失败: {error}");
            }
        }
    } else {
        queue.push(notice);
    }
}
