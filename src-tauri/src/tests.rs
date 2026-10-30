use std::collections::BTreeSet;

#[test]
fn command_names_match_handlers_manifest_and_local_permissions() {
    let commands: BTreeSet<_> = [
        include_str!("app_content/commands.rs"),
        include_str!("appearance/commands.rs"),
        include_str!("auth/commands.rs"),
        include_str!("onboarding/commands.rs"),
        include_str!("storage/commands.rs"),
        include_str!("settings/commands.rs"),
        include_str!("campus_network/commands.rs"),
        include_str!("campus_payments/commands.rs"),
        include_str!("uooc/commands.rs"),
        include_str!("oj/commands.rs"),
        include_str!("lms/commands.rs"),
        include_str!("enrollment/commands.rs"),
        include_str!("ocr/commands.rs"),
        include_str!("embedding_model/commands.rs"),
        include_str!("notification/commands.rs"),
        include_str!("netproxy/commands.rs"),
    ]
    .into_iter()
    .flat_map(|source| source.lines())
    .filter_map(|line| {
        line.strip_prefix("pub(crate) fn ")
            .or_else(|| line.strip_prefix("pub(crate) async fn "))
    })
    .filter_map(|line| line.split_once('(').map(|(name, _)| name.to_owned()))
    .chain(
        include_str!("mcp/mod.rs")
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .filter_map(|w| {
                if w[0].trim() == "#[tauri::command]" {
                    w[1].split_once("pub fn ")
                        .and_then(|(_, rest)| rest.split_once('('))
                        .map(|(name, _)| name.to_owned())
                } else {
                    None
                }
            }),
    )
    .collect();
    assert!(commands.len() >= 33);
    let handlers: BTreeSet<_> = include_str!("app.rs")
        .lines()
        .filter(|line| !line.contains("initialize"))
        .filter_map(|line| {
            line.split_once("::commands::")
                .or_else(|| line.split_once("crate::mcp::"))
        })
        .map(|(_, name)| name.trim_end_matches(',').trim().to_owned())
        .collect();
    assert_eq!(commands, handlers);
    let manifest: BTreeSet<_> = include_str!("../build.rs")
        .lines()
        // 只解析 `AppManifest::new().commands(&[...])` 中的条目，
        // 避免把 build.rs 里其它字符串（侧车二进制、链接参数等）也算进来。
        .skip_while(|line| !line.contains("AppManifest::new().commands("))
        .take_while(|line| !line.trim_start().starts_with("])"))
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|line| line.split_once('"').map(|(name, _)| name.to_owned()))
        .collect();
    assert_eq!(commands, manifest);
    let permissions: BTreeSet<_> = include_str!("../permissions/default.toml")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("\"allow-"))
        .filter_map(|line| line.split_once('"').map(|(name, _)| name.replace('-', "_")))
        .collect();
    assert_eq!(commands, permissions);
}
