use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{
    ipc::Invoke,
    plugin::{Plugin, TauriPlugin},
    webview::PageLoadPayload,
    AppHandle, RunEvent, Runtime, Url, Webview, Window,
};

pub(crate) mod commands;
pub(crate) mod mcp;
pub(crate) mod settings;

pub(crate) use settings::NotificationCategory;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationPayload {
    pub category: NotificationCategory,
    pub title: String,
    pub body: String,
    pub route: Option<String>,
    pub action_label: Option<String>,
    pub extra: Option<serde_json::Value>,
    pub id: Option<i32>,
    pub action_type_id: Option<String>,
    pub auto_cancel: Option<bool>,
}

impl NotificationPayload {
    pub(crate) fn new(
        category: NotificationCategory,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            category,
            title: title.into(),
            body: body.into(),
            route: None,
            action_label: None,
            extra: None,
            id: None,
            action_type_id: None,
            auto_cancel: None,
        }
    }

    pub(crate) fn route(mut self, route: impl Into<String>) -> Self {
        self.route = Some(route.into());
        self
    }

    pub(crate) fn action_label(mut self, label: impl Into<String>) -> Self {
        self.action_label = Some(label.into());
        self
    }

    pub(crate) fn extra(mut self, extra: serde_json::Value) -> Self {
        self.extra = Some(extra);
        self
    }

    pub(crate) fn id(mut self, id: i32) -> Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn action_type_id(mut self, action_type_id: impl Into<String>) -> Self {
        self.action_type_id = Some(action_type_id.into());
        self
    }

    pub(crate) fn auto_cancel(mut self, auto_cancel: bool) -> Self {
        self.auto_cancel = Some(auto_cancel);
        self
    }
}

static FRONTEND_FOCUSED: AtomicBool = AtomicBool::new(true);

pub(crate) fn set_foreground_focused(focused: bool) {
    FRONTEND_FOCUSED.store(focused, Ordering::SeqCst);
}

pub(crate) fn is_foreground_focused<R: Runtime>(app: &AppHandle<R>) -> bool {
    if !FRONTEND_FOCUSED.load(Ordering::SeqCst) {
        return false;
    }
    #[cfg(desktop)]
    {
        use tauri::Manager;
        if let Some(window) = app.get_webview_window("main") {
            let focused = window.is_focused().unwrap_or(false);
            let visible = window.is_visible().unwrap_or(false);
            let minimized = window.is_minimized().unwrap_or(false);
            if !focused || !visible || minimized {
                return false;
            }
        }
    }
    true
}

pub(crate) fn notify<R: Runtime>(app: &AppHandle<R>, payload: NotificationPayload) {
    use tauri::Emitter;
    let current_settings = settings::load(app);
    if !current_settings.is_category_enabled(payload.category) {
        return;
    }

    if is_foreground_focused(app) {
        let _ = app.emit("app://in-app-notification", &payload);
        return;
    }

    use tauri_plugin_notification::NotificationExt;
    let mut builder = app
        .notification()
        .builder()
        .title(&payload.title)
        .body(&payload.body);

    if let Some(id) = payload.id {
        builder = builder.id(id);
    }
    if let Some(action_type) = &payload.action_type_id {
        builder = builder.action_type_id(action_type);
    }
    if payload.auto_cancel.unwrap_or(false) {
        builder = builder.auto_cancel();
    }
    if let Some(extra) = &payload.extra {
        if let Some(map) = extra.as_object() {
            for (k, v) in map {
                builder = builder.extra(k, v.clone());
            }
        }
    }
    if let Some(route) = &payload.route {
        builder = builder.extra("route", route.clone());
    }

    let _ = builder.show();
}

pub(crate) fn init<R: Runtime>() -> impl Plugin<R> {
    LocalNotificationPlugin {
        inner: tauri_plugin_notification::init(),
        dev_url: None,
    }
}

// The upstream Notification Web API shim eagerly queries native permissions on
// every document, including external login pages. Keep its local API behavior
// and all native hooks, but never bootstrap it on remote origins.
struct LocalNotificationPlugin<R: Runtime> {
    inner: TauriPlugin<R>,
    dev_url: Option<String>,
}

impl<R: Runtime> Plugin<R> for LocalNotificationPlugin<R> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn initialize(
        &mut self,
        app: &AppHandle<R>,
        config: serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(dev)]
        {
            self.dev_url = app.config().build.dev_url.as_ref().map(ToString::to_string);
        }
        self.inner.initialize(app, config)
    }

    fn initialization_script(&self) -> Option<String> {
        self.inner.initialization_script().map(|script| {
            let guard = include_str!("local-page.js").replacen("export default ", "", 1);
            let dev_url = serde_json::to_string(&self.dev_url).expect("serialize development URL");
            format!("({guard})({dev_url}, function () {{\n{script}\n}});")
        })
    }

    fn window_created(&mut self, window: Window<R>) {
        self.inner.window_created(window);
    }

    fn webview_created(&mut self, webview: Webview<R>) {
        self.inner.webview_created(webview);
    }

    fn on_navigation(&mut self, webview: &Webview<R>, url: &Url) -> bool {
        self.inner.on_navigation(webview, url)
    }

    fn on_page_load(&mut self, webview: &Webview<R>, payload: &PageLoadPayload<'_>) {
        self.inner.on_page_load(webview, payload);
    }

    fn on_event(&mut self, app: &AppHandle<R>, event: &RunEvent) {
        self.inner.on_event(app, event);
    }

    fn extend_api(&mut self, invoke: Invoke<R>) -> bool {
        self.inner.extend_api(invoke)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_bootstrap_is_scoped_through_both_tauri_script_hooks() {
        let plugin = init::<tauri::Wry>();
        assert_eq!(plugin.name(), "notification");
        let script = plugin.initialization_script().unwrap();
        assert!(script.contains("function localNotificationPage"));
        assert!(script.contains("plugin:notification|is_permission_granted"));
        let initialization = plugin.initialization_script_2().unwrap();
        assert_eq!(initialization.script, script);
        assert!(initialization.for_main_frame_only);
    }

    #[test]
    fn remote_capabilities_do_not_grant_notification_access() {
        for entry in
            std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/capabilities")).unwrap()
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let capability: serde_json::Value =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            if capability.get("remote").is_some() {
                for permission in capability["permissions"].as_array().unwrap() {
                    let name = permission
                        .as_str()
                        .or_else(|| permission["identifier"].as_str())
                        .unwrap();
                    assert!(!name.starts_with("notification:"));
                }
            }
        }
    }

    #[test]
    fn foreground_focus_state_transitions() {
        set_foreground_focused(true);
        assert!(FRONTEND_FOCUSED.load(Ordering::SeqCst));
        set_foreground_focused(false);
        assert!(!FRONTEND_FOCUSED.load(Ordering::SeqCst));
        set_foreground_focused(true);
    }
}
