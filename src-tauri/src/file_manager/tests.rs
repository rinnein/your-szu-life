use super::android::parse_picked_json;
use super::model::{DirectoryConfig, DirectorySelection};
use super::paths;

#[test]
fn parses_android_pick_payload() {
    let raw = r#"{"displayPath":"/storage/emulated/0/Download/泥深","treeUri":"content://com.android.externalstorage.documents/tree/primary%3ADownload%2F泥深","hasEntries":true}"#;
    let parsed = parse_picked_json(raw).unwrap();
    assert_eq!(parsed.display_path, "/storage/emulated/0/Download/泥深");
    assert!(parsed.tree_uri.starts_with("content://"));
    assert!(parsed.has_entries);
}

#[test]
fn android_payload_defaults_missing_flags_to_false() {
    let raw = r#"{"displayPath":"/sdcard/Download","treeUri":"content://x/tree/y"}"#;
    let parsed = parse_picked_json(raw).unwrap();
    assert!(!parsed.has_entries);
}

#[test]
fn rejects_android_payload_without_authority() {
    assert!(parse_picked_json(r#"{"displayPath":"/sdcard/Download"}"#).is_err());
    assert!(parse_picked_json(r#"{"treeUri":"content://x/tree/y"}"#).is_err());
    assert!(parse_picked_json("not json").is_err());
}

#[test]
fn directory_config_round_trips_including_android_tree() {
    let config = DirectoryConfig {
        selection: DirectorySelection::AndroidTree {
            uri: "content://com.android.externalstorage.documents/tree/primary%3ADownload".into(),
        },
        relative_base: paths::BRAND_DIR.to_string(),
        display_root: "/storage/emulated/0/Download/泥深".into(),
        selected: "/storage/emulated/0/Download".into(),
        via_default: false,
        chosen_at: 1_760_000_000_000,
    };
    let raw = serde_json::to_string(&config).unwrap();
    let back: DirectoryConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(config, back);
    assert!(raw.contains("\"kind\":\"androidTree\""));
}

#[test]
fn filesystem_config_round_trips() {
    let config = DirectoryConfig {
        selection: DirectorySelection::Filesystem {
            path: "/home/me/Documents".into(),
        },
        relative_base: String::new(),
        display_root: "/home/me/Documents/泥深".into(),
        selected: "/home/me/Documents".into(),
        via_default: false,
        chosen_at: 1,
    };
    let raw = serde_json::to_string(&config).unwrap();
    let back: DirectoryConfig = serde_json::from_str(&raw).unwrap();
    assert_eq!(config, back);
    assert!(raw.contains("\"kind\":\"filesystem\""));
}

#[test]
fn default_selection_is_tagged_without_payload() {
    let raw = serde_json::to_string(&DirectorySelection::Default).unwrap();
    assert_eq!(raw, r#"{"kind":"default"}"#);
    let back: DirectorySelection = serde_json::from_str(&raw).unwrap();
    assert_eq!(back, DirectorySelection::Default);
}
