fn main() {
    szu_build_support::emit_platform_cfgs();

    // `analytics` 要用 `APTABASE_APP_KEY`；依赖 crate 不会继承 app 的
    // `cargo:rustc-env`，因此这里自己注入一次。
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("cargo 必须提供 CARGO_MANIFEST_DIR");
    szu_build_support::emit_embedded_env(std::path::Path::new(&format!("{manifest}/../../../.env")));
}
