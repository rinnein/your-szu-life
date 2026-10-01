use tauri::{
    ipc::Invoke,
    plugin::{Plugin, TauriPlugin},
    webview::PageLoadPayload,
    AppHandle, RunEvent, Runtime, Url, Webview, Window,
};

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

    // Leave initialization_script_2 at the trait default so it wraps the scoped
    // script above rather than returning the inner plugin's unguarded script.
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
}
