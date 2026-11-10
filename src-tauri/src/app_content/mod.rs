#[cfg(target_os = "android")]
pub(crate) mod android_update;
mod cache;
mod channel;
pub(crate) mod commands;
mod config;
mod http;
mod images;
pub(crate) mod mcp;
mod model;
mod runtime;
mod service;
#[cfg(target_os = "windows")]
pub(crate) mod windows_update;

use std::{sync::Arc, time::Duration};
use tauri::{Emitter, Manager};

#[derive(Clone)]
pub(crate) struct AppContentStore(Arc<service::ContentService>);

impl AppContentStore {
    pub(crate) fn snapshot(&self) -> model::AppContentSnapshot {
        self.0.snapshot()
    }

    #[cfg(any(target_os = "android", target_os = "windows"))]
    pub(crate) fn build_download_candidates(&self, direct_url: &str) -> Vec<(u32, String)> {
        self.0.build_download_candidates(direct_url)
    }
}

pub(crate) fn initialize(app: &tauri::AppHandle) -> std::io::Result<()> {
    #[cfg(target_os = "android")]
    android_update::set_app_handle(app.clone());
    #[cfg(target_os = "windows")]
    windows_update::set_app_handle(app.clone());
    let config = config::ContentConfig::compiled(app.package_info().version.to_string());
    let http = Arc::new(http::PublicClient::new(&config.version));
    let channel = channel::load();
    let handle = app.clone();
    let service = Arc::new(service::ContentService::new(
        config,
        http,
        Arc::new(runtime::SystemClock),
        app.path().app_cache_dir().ok(),
        channel,
        move |snapshot| {
            let _ = handle.emit("app-content-changed", snapshot);
        },
    ));
    app.manage(AppContentStore(service.clone()));
    if service.config.sources.is_some() || service.config.uses_github_releases() {
        // Exactly one native worker per process. It is independent of every WebView lifecycle.
        std::thread::Builder::new()
            .name("app-content".into())
            .spawn(move || {
                loop {
                    service.automatic_tick();
                    // Wall-clock deadlines include suspend time. Check at most the current
                    // activity-tier delay after resume; missed days coalesce into one refresh
                    // instead of a burst of catch-up requests.
                    let now = runtime::Clock::now_ms(&runtime::SystemClock);
                    let max_delay = crate::activity::interval(
                        Some("app-content"),
                        Duration::from_secs(15),
                        Duration::from_secs(60),
                        Duration::from_secs(600),
                    )
                    .as_millis() as u64;
                    let delay = service
                        .snapshot()
                        .next_automatic_check_at
                        .unwrap_or(now + max_delay)
                        .saturating_sub(now)
                        .clamp(1, max_delay);
                    std::thread::sleep(Duration::from_millis(delay));
                }
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
