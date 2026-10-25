use tauri::Manager;

const TRAY_ICON: tauri::image::Image<'static> = tauri::include_image!("icons/32x32.png");

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

pub(crate) fn setup(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    let show = MenuItem::with_id(app, "show", "打开窗口", true, None::<&str>)?;
    let exit = MenuItem::with_id(app, "exit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &exit])?;
    TrayIconBuilder::new()
        .icon(TRAY_ICON)
        .menu(&menu)
        .tooltip("泥深")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                focus_main_window(app);
            }
            "exit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

pub(crate) fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::Focused(false) => {
            if let Some(store) = window
                .app_handle()
                .try_state::<crate::campus_payments::CampusPaymentsStore>()
            {
                crate::campus_payments::electricity::set_background(
                    window.app_handle().clone(),
                    store.electricity.clone(),
                    true,
                );
            }
        }
        tauri::WindowEvent::Focused(true) => {
            if let Some(store) = window
                .app_handle()
                .try_state::<crate::campus_payments::CampusPaymentsStore>()
            {
                crate::campus_payments::electricity::set_background(
                    window.app_handle().clone(),
                    store.electricity.clone(),
                    false,
                );
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if window.label() == "main"
                && window
                    .app_handle()
                    .get_webview_window("main")
                    .and_then(|view| view.url().ok())
                    .is_some_and(|url| url.path() == "/")
                && (crate::campus_network::keep_running_in_background(window.app_handle())
                    || crate::uooc::keep_running(window.app_handle())
                    || crate::oj::keep_running(window.app_handle()))
            {
                api.prevent_close();
                let _ = window.hide();
                if let Some(store) = window
                    .app_handle()
                    .try_state::<crate::campus_payments::CampusPaymentsStore>()
                {
                    crate::campus_payments::electricity::set_background(
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
