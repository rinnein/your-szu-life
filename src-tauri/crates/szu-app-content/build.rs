fn main() {
    szu_build_support::emit_platform_cfgs();
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo 必须提供 CARGO_MANIFEST_DIR");
    szu_build_support::emit_embedded_env(std::path::Path::new(&format!(
        "{manifest}/../../../.env"
    )));
}
