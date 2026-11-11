//! 演示模式：无真实统一认证账户时以 mock 数据展示前端，用于检查界面样式。
//!
//! 只负责状态与门控：演示开启即同时开启调试模式，`needs_setup` 在此期间视为
//! 已完成（见 [`crate::onboarding`]），登录数据由前端的 demo shim 返回 mock。

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

pub mod commands;

static DEMO_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn is_enabled() -> bool {
    DEMO_ENABLED.load(Ordering::Acquire)
}

pub fn initialize(store: &crate::campus_network::CampusNetworkStore) {
    let enabled = crate::preferences::load::<bool>(crate::preferences::DEMO_ENABLED)
        .ok()
        .flatten()
        .unwrap_or(false);
    DEMO_ENABLED.store(enabled, Ordering::Release);
    // 演示模式基于调试模式：修正历史遗留的不一致状态。
    if enabled && !store.debug_enabled() {
        let _ = crate::preferences::save(crate::preferences::DEBUG_ENABLED, &true);
        store.force_debug_enabled(true);
    }
}

/// 切换演示模式。开启时同步开启调试模式并广播模拟的已登录状态；关闭时一并
/// 关闭调试模式、广播未登录并通知前端重新进入初始化向导。
pub fn set_enabled(
    app: &tauri::AppHandle,
    store: &crate::campus_network::CampusNetworkStore,
    enabled: bool,
) -> Result<(), String> {
    crate::preferences::save(crate::preferences::DEMO_ENABLED, &enabled)?;
    DEMO_ENABLED.store(enabled, Ordering::Release);
    // 复用调试模式入口：写偏好、更新原子量并广播 debug-mode-changed。
    crate::campus_network::diagnostics::set_mode(app, store, enabled)?;
    if enabled {
        let _ = app.emit(
            "auth-state-changed",
            crate::auth::model::AuthState::Authenticated {
                name: Some("演示同学".into()),
                student_id: "20250001".into(),
            },
        );
    } else {
        let _ = app.emit(
            "auth-state-changed",
            crate::auth::model::AuthState::Unauthenticated,
        );
        if let Ok(state) = crate::onboarding::state_view() {
            let _ = app.emit("onboarding-restarted", &state);
        }
    }
    Ok(())
}
