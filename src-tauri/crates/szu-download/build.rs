fn main() {
    szu_build_support::emit_platform_cfgs();

    // `tauri-build` 只会给它自己那个 crate 打印 `cargo:rustc-env=TAURI_ENV_TARGET_TRIPLE`，
    // 依赖 crate 拿不到；开发模式查找 `src-tauri/binaries/aria2c-<triple>` 需要这两个值。
    let target = std::env::var("TARGET").expect("cargo 必须提供 TARGET");
    println!("cargo:rustc-env=SZU_TARGET_TRIPLE={target}");

    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo 必须提供 CARGO_MANIFEST_DIR");
    println!("cargo:rustc-env=SZU_BINARIES_DIR={manifest}/../../binaries");
}
