use std::collections::BTreeSet;

#[test]
fn command_names_match_handlers_manifest_and_local_permissions() {
    let commands: BTreeSet<_> = [
        include_str!("app_content/commands.rs"),
        include_str!("auth/commands.rs"),
        include_str!("storage/commands.rs"),
        include_str!("campus_network/commands.rs"),
        include_str!("campus_payments/commands.rs"),
        include_str!("uooc/commands.rs"),
        include_str!("oj/commands.rs"),
    ]
    .into_iter()
    .flat_map(|source| source.lines())
    .filter_map(|line| {
        line.strip_prefix("pub(crate) fn ")
            .or_else(|| line.strip_prefix("pub(crate) async fn "))
    })
    .filter_map(|line| line.split_once('(').map(|(name, _)| name.to_owned()))
    .collect();
    assert!(commands.len() >= 33);
    let handlers: BTreeSet<_> = include_str!("app.rs")
        .lines()
        .filter_map(|line| line.split_once("::commands::"))
        .map(|(_, name)| name.trim_end_matches(',').trim().to_owned())
        .collect();
    assert_eq!(commands, handlers);
    let manifest: BTreeSet<_> = include_str!("../build.rs")
        .lines()
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
