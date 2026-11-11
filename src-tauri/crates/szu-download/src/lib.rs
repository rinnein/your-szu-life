//! 下载基础设施。
//!
//! `aria2` 负责进程生命周期与 RPC，`file_manager` 决定用户可见的下载目录
//!（Android 走 SAF 授权的文档树，桌面走系统目录选择器）。两者互相调用，
//! 因此必须放在同一个 crate。

pub mod aria2;
pub mod file_manager;
