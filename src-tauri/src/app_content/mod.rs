mod cache;
pub(crate) mod commands;
mod config;
mod http;
mod images;
mod model;
mod runtime;
mod service;

use std::{sync::Arc, time::Duration};
use tauri::{Emitter, Manager};

pub(crate) struct AppContentStore(Arc<service::ContentService>);

pub(crate) fn initialize(app: &tauri::AppHandle) -> std::io::Result<()> {
    let config = config::ContentConfig::compiled(app.package_info().version.to_string());
    let http = Arc::new(http::PublicClient::new(&config.version));
    let handle = app.clone();
    let service = Arc::new(service::ContentService::new(
        config,
        http,
        Arc::new(runtime::SystemClock),
        app.path().app_cache_dir().ok(),
        move |snapshot| {
            let _ = handle.emit("app-content-changed", snapshot);
        },
    ));
    app.manage(AppContentStore(service.clone()));
    if service.config.sources.is_some() {
        // Exactly one native worker per process. It is independent of every WebView lifecycle.
        std::thread::Builder::new()
            .name("app-content".into())
            .spawn(move || loop {
                service.automatic_tick();
                // Wall-clock deadlines include suspend time. Check at most 15s after resume;
                // missed days coalesce into one refresh instead of a burst of catch-up requests.
                let now = runtime::Clock::now_ms(&runtime::SystemClock);
                let delay = service
                    .snapshot()
                    .next_automatic_check_at
                    .unwrap_or(now + 15_000)
                    .saturating_sub(now)
                    .clamp(1, 15_000);
                std::thread::sleep(Duration::from_millis(delay));
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
