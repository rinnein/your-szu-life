//! 供各 crate 的 `build.rs` 复用的公共构建逻辑。
//!
//! 这些逻辑必须放在 build-dependency 里：`tauri-build` 只会给它自己那个 crate
//! 打印 `cargo:rustc-cfg=desktop/mobile/dev`，依赖 crate 拿不到，因此每个带
//! `#[cfg(desktop)]` 之类代码的 crate 都要自己重新声明一次。

/// 复刻 `tauri-build` 的平台与 profile cfg：
///
/// - `desktop` / `mobile`：按目标平台是否 Android/iOS 决定；
/// - `dev`：`tauri` 的 `dev` 等价于「没有开启 `tauri/custom-protocol`」，
///   也就是非 release 构建；这里用 `PROFILE == "debug"` 对应。
///
/// 同时打印 `rustc-check-cfg`，否则较新的 rustc 会对未知 cfg 报
/// `unexpected cfg condition name` 警告。
pub fn emit_platform_cfgs() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let mobile = target_os == "ios" || target_os == "android";

    for name in ["desktop", "mobile", "dev"] {
        println!("cargo:rustc-check-cfg=cfg({name})");
    }
    println!(
        "cargo:rustc-cfg={}",
        if mobile { "mobile" } else { "desktop" }
    );

    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile == "debug" {
        println!("cargo:rustc-cfg=dev");
    }
}
