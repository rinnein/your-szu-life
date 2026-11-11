use std::sync::Mutex;

use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

use szu_todos::todos::model::TodoItem;

const TRAY_ICON: tauri::image::Image<'static> = tauri::include_image!("icons/32x32.png");

/// 托盘待办菜单最多列出的条数，更多条目按截止时间靠前的优先。
const TODO_MENU_LIMIT: usize = 10;
/// 待办菜单项标题的最大字符数，超出部分以省略号截断。
const TODO_TITLE_MAX_CHARS: usize = 20;
/// 托盘「打开待办」事件：前端主窗口监听后以独立窗口打开对应逻辑页面。
const TRAY_OPEN_ROUTE_EVENT: &str = "tray-open-route";

/// 托盘「打开待办」事件的载荷。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrayOpenRoute {
    /// 应用内逻辑页面路径，如 `/study/uooc?tab=exams&subtab=exam`。
    pub route: String,
    /// 外部链接（如有）；由 Rust 侧直接打开，前端不再处理。
    pub external_url: Option<String>,
}

pub(crate) struct TrayState {
    pub(crate) tray: tauri::tray::TrayIcon,
    /// 当前托盘菜单列出的待办，顺序与菜单项 id `todo:{index}` 对应。
    pub(crate) todos: Mutex<Vec<TodoItem>>,
}

pub(crate) fn focus_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("主窗口不存在，无法激活");
        return;
    };
    // 窗口可能被最小化到托盘，show 之后还要解除最小化才能前置。
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

/// 托盘菜单的纯描述（id、文案、是否可用），与具体菜单 API 解耦便于单测。
/// 菜单结构：打开窗口 / 待办块 / 退出，分隔线在实体化时插入。
pub(crate) fn todo_menu_specs(items: &[TodoItem]) -> Vec<(String, String, bool)> {
    let mut specs = vec![("show".to_string(), "打开窗口".to_string(), true)];
    if items.is_empty() {
        specs.push(("todos-empty".to_string(), "暂无待办".to_string(), false));
    } else {
        for (index, item) in items.iter().take(TODO_MENU_LIMIT).enumerate() {
            specs.push((format!("todo:{index}"), todo_menu_label(item), true));
        }
    }
    specs.push(("exit".to_string(), "退出".to_string(), true));
    specs
}

/// 待办菜单文案：`标题（MM-dd 截止）`；截止时间不可解析时只保留标题。
fn todo_menu_label(item: &TodoItem) -> String {
    let mut title: String = item.title.chars().take(TODO_TITLE_MAX_CHARS).collect();
    if item.title.chars().count() > TODO_TITLE_MAX_CHARS {
        title.push('…');
    }
    match due_date_label(item.due_at_ms) {
        Some(date) => format!("{title}（{date} 截止）"),
        None => title,
    }
}

fn due_date_label(due_at_ms: i64) -> Option<String> {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(due_at_ms)
        .single()
        .map(|datetime| datetime.format("%m-%d").to_string())
}

fn build_menu(app: &tauri::AppHandle, items: &[TodoItem]) -> Result<Menu<tauri::Wry>, tauri::Error> {
    let mut kinds: Vec<tauri::menu::MenuItemKind<tauri::Wry>> = Vec::new();
    // 相邻条目之间插入分隔线：得到「打开窗口 / 待办块 / 退出」的分组结构。
    for (index, (id, label, enabled)) in todo_menu_specs(items).into_iter().enumerate() {
        if index > 0 {
            let separator = PredefinedMenuItem::separator(app)?;
            kinds.push(tauri::menu::MenuItemKind::Predefined(separator));
        }
        let item = MenuItem::with_id(app, id, label, enabled, None::<&str>)?;
        kinds.push(tauri::menu::MenuItemKind::MenuItem(item));
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        kinds.iter().map(|kind| kind as _).collect();
    Menu::with_items(app, &refs)
}

/// 待办快照变化后重建托盘菜单并刷新 id ↔ 待办映射。
pub(crate) fn update_todo_menu(app: &tauri::AppHandle, items: &[TodoItem]) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    match build_menu(app, items) {
        Ok(menu) => {
            if let Err(error) = state.tray.set_menu(Some(menu)) {
                log::warn!("托盘菜单更新失败: {error}");
            }
        }
        Err(error) => log::warn!("托盘菜单构建失败: {error}"),
    }
    *state.todos.lock().unwrap() = items.to_vec();
}

fn open_todo(app: &tauri::AppHandle, index: usize) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    let Some(item) = state.todos.lock().unwrap().get(index).cloned() else {
        return;
    };
    if let Some(url) = item.external_url {
        use tauri_plugin_opener::OpenerExt;
        if let Err(error) = app.opener().open_url(url, None::<&str>) {
            log::warn!("托盘待办外链打开失败: {error}");
        }
        return;
    }
    // 主窗口可能正收纳在托盘：先唤醒（保证其 webview 可见、可承载报错提示），
    // 再由前端 DeepLinkNavigation 以独立窗口打开目标逻辑页面。
    focus_main_window(app);
    let _ = app.emit(
        TRAY_OPEN_ROUTE_EVENT,
        TrayOpenRoute {
            route: item.route,
            external_url: None,
        },
    );
}

pub(crate) fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_menu(app.app_handle(), &[])?;
    let tray = TrayIconBuilder::new()
        .icon(TRAY_ICON)
        .menu(&menu)
        .tooltip("泥深")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => focus_main_window(app),
            "exit" => app.exit(0),
            id => {
                if let Some(index) = id.strip_prefix("todo:") {
                    if let Ok(index) = index.parse::<usize>() {
                        open_todo(app, index);
                    }
                }
            }
        })
        .build(app)?;
    app.manage(TrayState {
        tray,
        todos: Mutex::new(Vec::new()),
    });

    Ok(())
}

pub(crate) fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Focused(false) => {
            if let Some(store) = window
                .app_handle()
                .try_state::<szu_core::campus_payments::CampusPaymentsStore>()
            {
                szu_core::campus_payments::electricity::set_background(
                    window.app_handle().clone(),
                    store.electricity.clone(),
                    true,
                );
            }
        }
        tauri::WindowEvent::Focused(true) => {
            if let Some(store) = window
                .app_handle()
                .try_state::<szu_core::campus_payments::CampusPaymentsStore>()
            {
                szu_core::campus_payments::electricity::set_background(
                    window.app_handle().clone(),
                    store.electricity.clone(),
                    false,
                );
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // 关闭一律收纳到托盘：应用驻留后台（待办提醒、校园网等继续运行），
            // 只有托盘菜单「退出」才真正退出。
            if window.label() == "main" {
                api.prevent_close();
                let _ = window.hide();
                if let Some(store) = window
                    .app_handle()
                    .try_state::<szu_core::campus_payments::CampusPaymentsStore>()
                {
                    szu_core::campus_payments::electricity::set_background(
                        window.app_handle().clone(),
                        store.electricity.clone(),
                        true,
                    );
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
