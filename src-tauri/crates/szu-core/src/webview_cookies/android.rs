//! Calls the app-owned bridge from worker threads, never Android's UI thread.
use jni::{
    objects::{JClass, JObject, JString, JValue},
    JNIEnv, JavaVM,
};

fn with_bridge<T>(
    call: impl FnOnce(&mut JNIEnv<'_>, JClass<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let context = ndk_context::android_context();
    let vm = unsafe { JavaVM::from_raw(context.vm().cast()) }
        .map_err(|_| "无法连接 Android Cookie 服务")?;
    let mut env = vm
        .attach_current_thread()
        .map_err(|_| "无法连接 Android Cookie 服务")?;
    // The Android context is a borrowed global reference, not ours to delete.
    let context =
        std::mem::ManuallyDrop::new(unsafe { JObject::from_raw(context.context().cast()) });
    let result = (|| {
        let loader = env
            .call_method(
                &*context,
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .and_then(|v| v.l())
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        let name = env
            .new_string("in.rinne.your_szu_life.WebviewCookieBridge")
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        let class = env
            .call_method(
                loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .and_then(|v| v.l())
            .map_err(|_| "无法加载 Android Cookie 服务")?;
        call(&mut env, JClass::from(class))
    })();
    // Java exceptions must not leak into the next JNI call or include cookie headers.
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_clear();
    }
    result
}

use super::{cdp, Error, Patch, Scope, Snapshot};
use serde::Serialize;
use std::sync::OnceLock;

pub fn alive(window: &tauri::WebviewWindow) -> bool {
    use tauri::Manager;
    window
        .app_handle()
        .get_webview_window(window.label())
        .is_some()
        && window.url().is_ok()
}
fn serial() -> &'static tokio::sync::Mutex<()> {
    static SERIAL: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    SERIAL.get_or_init(|| tokio::sync::Mutex::new(()))
}
fn socket() -> Result<String, Error> {
    with_bridge(|env, class| {
        let ready = env
            .call_static_method(&class, "prepareDevTools", "()Z", &[])
            .and_then(|v| v.z())
            .map_err(|_| "bridge".to_string())?;
        if !ready {
            return Err("bridge".into());
        }
        let value = env
            .call_static_method(class, "devToolsSocketName", "()Ljava/lang/String;", &[])
            .and_then(|v| v.l())
            .map_err(|_| "bridge".to_string())?;
        env.get_string(&JString::from(value))
            .map(|s| s.into())
            .map_err(|_| "bridge".into())
    })
    .map_err(|_| Error::Unavailable)
}
fn headers(scope: &Scope) -> Result<Vec<String>, Error> {
    with_bridge(|env, class| {
        let urls: Vec<_> = scope.urls.iter().map(|u| u.as_str()).collect();
        let input = env
            .new_string(serde_json::to_string(&urls).map_err(|_| "bridge".to_owned())?)
            .map_err(|_| "bridge".to_owned())?;
        let value = env
            .call_static_method(
                class,
                "readHeaders",
                "(Ljava/lang/String;)Ljava/lang/String;",
                &[JValue::Object(&input)],
            )
            .and_then(|v| v.l())
            .map_err(|_| "bridge".to_owned())?;
        let text: String = env
            .get_string(&JString::from(value))
            .map_err(|_| "bridge".to_owned())?
            .into();
        serde_json::from_str(&text).map_err(|_| "bridge".to_owned())
    })
    .map_err(|_| Error::Unavailable)
}
#[derive(Serialize)]
struct NativeWrite {
    url: String,
    header: String,
}
fn native_write(scope: &Scope, patches: &[Patch]) -> Result<(), Error> {
    let data: Vec<NativeWrite> = patches
        .iter()
        .map(|p| {
            let mut cookie = p
                .cookie
                .to_webview_cookie()
                .map_err(|_| Error::InvalidMetadata)?;
            if p.remove {
                cookie.set_max_age(time::Duration::ZERO);
            }
            Ok(NativeWrite {
                url: p.cookie.url(scope)?.to_string(),
                header: cookie.to_string(),
            })
        })
        .collect::<Result<_, Error>>()?;
    with_bridge(|env, class| {
        let input = env
            .new_string(serde_json::to_string(&data).map_err(|_| "bridge".to_string())?)
            .map_err(|_| "bridge".to_string())?;
        let ok = env
            .call_static_method(
                class,
                "writeCookies",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&input)],
            )
            .and_then(|v| v.z())
            .map_err(|_| "bridge".to_string())?;
        if ok {
            Ok(())
        } else {
            Err("bridge".into())
        }
    })
    .map_err(|_| Error::WriteUncertain)
}
struct AndroidTransport<'a> {
    window: &'a tauri::WebviewWindow,
}
impl super::ops::Transport for AndroidTransport<'_> {
    fn native_available(&mut self) -> bool {
        with_bridge(|env, class| {
            env.call_static_method(class, "supportsCookieInfo", "()Z", &[])
                .and_then(|v| v.z())
                .map_err(|_| "bridge".into())
        })
        .unwrap_or(false)
    }
    fn capture_native(&mut self, scope: &Scope) -> Result<Snapshot, Error> {
        if !alive(self.window) {
            return Err(Error::Unavailable);
        }
        let raw = with_bridge(|env, class| {
            let urls: Vec<_> = scope.urls.iter().map(|u| u.as_str()).collect();
            let input = env
                .new_string(serde_json::to_string(&urls).map_err(|_| "bridge".to_owned())?)
                .map_err(|_| "bridge".to_owned())?;
            let value = env
                .call_static_method(
                    class,
                    "readCookieInfo",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    &[JValue::Object(&input)],
                )
                .and_then(|v| v.l())
                .map_err(|_| "bridge".to_owned())?;
            let raw: String = env
                .get_string(&JString::from(value))
                .map_err(|_| "bridge".to_owned())?
                .into();
            Ok(raw)
        })
        .map_err(|_| Error::Unavailable)?;
        super::native::decode(&raw, scope)
    }
    fn write_native(&mut self, scope: &Scope, patches: &[Patch]) -> Result<(), Error> {
        if !alive(self.window) {
            return Err(Error::Unavailable);
        }
        native_write(scope, patches)
    }
    fn capture_cdp(&mut self, scope: &Scope) -> Result<Snapshot, Error> {
        let socket = socket()?;
        crate::migrations::db::run_sync(async {
            cdp::read_retry(|| async {
                let mut snapshot = cdp::open(&socket, self.window)
                    .await?
                    .capture(scope)
                    .await?;
                cdp::select_http(&mut snapshot, scope, &headers(scope)?)?;
                Ok(snapshot)
            })
            .await
        })
    }
    fn apply_cdp(&mut self, scope: &Scope, patches: &[Patch]) -> Result<(), Error> {
        let socket = socket()?;
        crate::migrations::db::run_sync(async {
            cdp::open(&socket, self.window)
                .await?
                .apply(scope, patches)
                .await
        })
    }
    fn replace_cdp(&mut self, scope: &Scope, snapshot: &Snapshot) -> Result<(), Error> {
        let socket = socket()?;
        crate::migrations::db::run_sync(async {
            cdp::open(&socket, self.window)
                .await?
                .replace(scope, snapshot)
                .await
        })
    }
}
fn execute<T: Send>(
    window: &tauri::WebviewWindow,
    call: impl FnOnce(&mut AndroidTransport<'_>) -> Result<T, Error> + Send,
) -> Result<T, Error> {
    crate::migrations::db::run_sync(async {
        let _lease = serial().lock().await;
        if !alive(window) {
            return Err(Error::Unavailable);
        }
        call(&mut AndroidTransport { window })
    })
}
pub fn capture(window: &tauri::WebviewWindow, scope: &Scope) -> Result<Snapshot, Error> {
    execute(window, |transport| super::ops::capture(transport, scope))
}
pub fn apply(
    window: &tauri::WebviewWindow,
    scope: &Scope,
    patches: &[Patch],
) -> Result<(), Error> {
    execute(window, |transport| {
        super::ops::apply(transport, scope, patches)
    })
}
pub fn replace(
    window: &tauri::WebviewWindow,
    scope: &Scope,
    snapshot: &Snapshot,
) -> Result<(), Error> {
    execute(window, |transport| {
        super::ops::replace(transport, scope, snapshot)
    })
}
