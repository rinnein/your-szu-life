use tauri::Manager;

const TRAY_ICON: tauri::image::Image<'static> = tauri::include_image!("icons/32x32.png");

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
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
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
            if crate::campus_network::keep_running_in_background(window.app_handle()) {
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
