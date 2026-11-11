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

/// 需要在编译期打进二进制的配置项。
///
/// 优先取构建进程的环境变量，其次读仓库根的 `.env`；两者都没有时 `option_env!`
/// 会得到 `None`，由各模块按「未配置」降级。
pub const EMBEDDED_ENV_VARS: [&str; 3] = [
    "SZU_LIFE_BASE_URL",
    "SZU_LIFE_FALLBACK_BASE_URL",
    "APTABASE_APP_KEY",
];

/// 把 [`EMBEDDED_ENV_VARS`] 注入当前 crate，并声明重跑条件。
pub fn emit_embedded_env(dotenv_path: &std::path::Path) {
    let dotenv = load_dotenv(dotenv_path);
    println!("cargo:rerun-if-changed={}", dotenv_path.display());
    for name in EMBEDDED_ENV_VARS {
        if let Some(value) = configured_value(name, &dotenv) {
            println!("cargo:rustc-env={name}={value}");
        }
        println!("cargo:rerun-if-env-changed={name}");
    }
}

fn load_dotenv(path: &std::path::Path) -> std::collections::HashMap<String, String> {
    if !path.is_file() {
        return std::collections::HashMap::new();
    }
    dotenvy::from_path_iter(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
        .collect::<Result<std::collections::HashMap<_, _>, _>>()
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", path.display()))
}

fn configured_value(
    name: &str,
    dotenv: &std::collections::HashMap<String, String>,
) -> Option<String> {
    match std::env::var_os(name) {
        Some(value) => value.into_string().ok(),
        None => dotenv.get(name).cloned(),
    }
}

/// 校验 Cargo 侧的应用版本与 `package.json` 一致。
///
/// 应用对外暴露的版本**只有一个来源**：仓库根的 `package.json`
/// （`tauri.conf.json` 的 `version` 指向它，因此运行时 `package_info().version`
/// 也来自它）。Cargo 的 `[workspace.package].version` 只是构建元数据，必须跟着
/// `package.json` 走；这里在构建期直接报错，避免发版时只改一处而导致两个版本并存。
pub fn ensure_app_version_matches_package_json(package_json: &std::path::Path, cargo_version: &str) {
    println!("cargo:rerun-if-changed={}", package_json.display());
    let raw = std::fs::read_to_string(package_json)
        .unwrap_or_else(|error| panic!("无法读取 {}: {error}", package_json.display()));
    let parsed: serde_json::Value = serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("{} 不是合法 JSON: {error}", package_json.display()));
    let package_version = parsed
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_else(|| panic!("{} 缺少 version", package_json.display()));
    assert!(
        package_version == cargo_version,
        "package.json 的 version ({package_version}) 与 Cargo 的版本 ({cargo_version}) 不一致：\
         应用版本以 package.json 为准，请把 src-tauri/Cargo.toml 的 \
         [workspace.package].version 改成同一个值"
    );
}
