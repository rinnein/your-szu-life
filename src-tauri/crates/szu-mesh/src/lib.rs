//! 设备自组网与设置同步。
//!
//! `mesh` 用进程内 EasyTier（`no_tun`）把同账户设备连成虚拟网，`settings_sync`
//! 在这张网上做白名单设置的版本向量合并。两者共享 token、端口与 HTTP 路由，
//! 因此放在同一个 crate。

pub mod mesh;
pub mod settings_sync;
