//! 泥深应用核心域。
//!
//! 这里放「被所有上层功能共享、且不依赖任何功能模块」的部分：平台基础
//! （路径、日志、端口、活动档位、IPC 通道、数据库句柄）、凭据与统一认证、
//! 网络代理策略、校园网、电费、通知、向导与偏好设置。
//!
//! 本 crate 的 `pub` 只服务于同仓库内的兄弟 crate 编译，不是对外公共 API：
//! 不要把它当作稳定接口，也尽量不要在 crate 之外新增对内部类型的直接依赖。

#![recursion_limit = "512"]

pub mod activity;
pub mod analytics;
pub mod app_paths;
pub mod appearance;
pub mod auth;
pub mod campus_network;
pub mod campus_payments;
pub mod demo;
pub mod ipc_channel;
pub mod logging;
pub mod migrations;
pub mod netproxy;
pub mod notification;
pub mod onboarding;
#[cfg(target_os = "android")]
pub mod platform_tls;
pub mod ports;
pub mod preferences;
pub mod settings;
pub mod storage;
pub mod webview_cookies;
