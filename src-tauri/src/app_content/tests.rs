use super::{
    config::{parse_repository, ContentConfig},
    model::{is_newer, parse_document, web_url, Announcements, Document, UpdateInfo},
    runtime::{AutomaticSchedule, SingleFlight},
    service::{
        extract_release_version, extract_version_from_asset_name, find_platform_asset_index,
        is_platform_asset,
    },
};
use serde_json::json;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

#[test]
fn public_urls_allow_http_https_private_hosts_and_reject_unsafe_schemes() {
    for value in [
        "http://172.30.255.42/content/",
        "https://intranet.example.test/path",
        "http://127.0.0.1:8080/feed",
    ] {
        assert!(
            web_url(value).is_ok(),
            "expected URL to be accepted: {value}"
        );
    }
    for value in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://example.com/feed",
        "http://user:pass@example.com/feed",
        "http://user@example.com/feed",
    ] {
        assert!(
            web_url(value).is_err(),
            "expected URL to be rejected: {value}"
        );
    }
}

#[test]
fn content_source_preserves_directory_path_prefix() {
    let config = ContentConfig::new(
        "0.1.0".into(),
        Some("http://intranet.test/prefix/content"),
        Some("https://backup.test/feeds/"),
        None,
    );
    let sources = config.sources.expect("valid sources");
    assert_eq!(
        sources.primary.as_str(),
        "http://intranet.test/prefix/content/"
    );
    assert_eq!(
        sources.fallback.unwrap().as_str(),
        "https://backup.test/feeds/"
    );
}

#[test]
fn announcements_allow_missing_pictures_but_require_title_and_exactly_one_action() {
    let valid = json!({
        "announcements": [
            {"title": "详情", "desc": "正文", "pic": "http://intranet.test/a.png"},
            {"title": "外链", "goto": "https://example.com", "pic": "https://example.com/a.jpg"},
            {"title": "缺图详情", "desc": "正文"},
            {"title": "缺图外链", "goto": "https://example.com"},
            {"title": "空图", "desc": "正文", "pic": ""},
            {"title": "空白图", "desc": "正文", "pic": "  "},
            {"title": "null 图", "desc": "正文", "pic": null}
        ]
    });
    assert!(parse_document::<Announcements>(&serde_json::to_vec(&valid).unwrap()).is_ok());
    assert!(parse_document::<Announcements>(br#"{"announcements":[]}"#).is_ok());
    for invalid in [
        json!({"announcements":[{"desc":"x"}]}),
        json!({"announcements":[{"title":" ","desc":"x"}]}),
        json!({"announcements":[{"title":"双操作","desc":"x","goto":"https://example.com","pic":"https://example.com/a"}]}),
        json!({"announcements":[{"title":"无操作","pic":"https://example.com/a"}]}),
        json!({"announcements":[{"title":"非法图","desc":"x","pic":"file:///tmp/a"}]}),
        json!({"announcements":[{"title":"错误图类型","desc":"x","pic":123}]}),
    ] {
        assert!(parse_document::<Announcements>(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}

#[test]
fn update_schema_and_semver_comparison_follow_precedence() {
    let update = UpdateInfo {
        latest_ver: "v0.2.0-beta.1+build.9".into(),
        latest_ver_url: "http://intranet.test/releases".into(),
        changelog: "修复问题".into(),
        release_url: Some("https://github.com/rinnein/your-szu-life/releases/tag/v0.2.0".into()),
    };
    assert!(update.validate().is_ok());
    assert!(is_newer("0.1.0", "v0.2.0"));
    assert!(is_newer("1.0.0-alpha", "1.0.0"));
    assert!(!is_newer("1.0.0", "1.0.0+other-build"));
    assert!(!is_newer("1.0.0", "0.9.0"));
    assert!(!is_newer("not-semver", "1.0.0"));
}

#[test]
fn extract_version_from_asset_name_handles_all_package_formats() {
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_android_arm64_0.9.0-release.apk.gz")
            .as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_android_arm64_0.9.0-release.apk").as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_linux_x64_0.9.0-release.deb").as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_linux_x64_0.9.0-release.AppImage")
            .as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_windows_x64_0.9.0-release.exe").as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_windows_x64_0.9.0-release.msi").as_deref(),
        Some("0.9.0")
    );
    assert_eq!(
        extract_version_from_asset_name("your-szu-life_android_arm64_1.2.3-beta.1-release.apk.gz")
            .as_deref(),
        Some("1.2.3-beta.1")
    );
    assert_eq!(extract_version_from_asset_name("unrelated-file.txt"), None);
}

#[test]
fn extract_release_version_resolves_canary_tag_from_asset_and_name() {
    assert_eq!(
        extract_release_version(
            "canary",
            Some("canary"),
            "your-szu-life_android_arm64_0.9.0-release.apk.gz"
        )
        .as_deref(),
        Some("0.9.0")
    );

    assert_eq!(
        extract_release_version("canary", Some("canary v0.9.1"), "custom_build.apk.gz").as_deref(),
        Some("0.9.1")
    );

    assert_eq!(
        extract_release_version("v1.0.0", Some("Release 1.0.0"), "app.exe").as_deref(),
        Some("1.0.0")
    );
}

#[test]
fn platform_asset_matcher_handles_expected_extensions() {
    #[cfg(target_os = "linux")]
    {
        assert!(is_platform_asset(
            "your-szu-life_linux_x64_0.9.0-release.deb"
        ));
        assert!(is_platform_asset(
            "your-szu-life_linux_x64_0.9.0-release.AppImage"
        ));
        assert!(!is_platform_asset(
            "your-szu-life_windows_x64_0.9.0-release.exe"
        ));
        assert!(!is_platform_asset(
            "your-szu-life_windows_x64_0.9.0-release.msi"
        ));
        let names = ["foo.txt", "your-szu-life_linux_x64_0.9.0-release.deb"];
        assert_eq!(find_platform_asset_index(&names), Some(1));
    }
    #[cfg(target_os = "windows")]
    {
        assert!(is_platform_asset(
            "your-szu-life_windows_x64_0.9.0-release.msi"
        ));
        assert!(!is_platform_asset(
            "your-szu-life_windows_x64_0.9.0-release.exe"
        ));
        let names = [
            "your-szu-life_windows_x64_0.9.0-release.exe",
            "your-szu-life_windows_x64_0.9.0-release.msi",
        ];
        assert_eq!(find_platform_asset_index(&names), Some(1));
    }
    #[cfg(target_os = "android")]
    {
        assert!(is_platform_asset(
            "your-szu-life_android_arm64_0.9.0-release.apk.gz"
        ));
        assert!(is_platform_asset(
            "your-szu-life_android_arm64_0.9.0-release.apk"
        ));
        let names = [
            "your-szu-life_android_arm64_0.9.0-release.apk",
            "your-szu-life_android_arm64_0.9.0-release.apk.gz",
        ];
        // Prefers compressed .apk.gz
        assert_eq!(find_platform_asset_index(&names), Some(1));
    }
}

#[test]
fn repository_parser_supports_package_json_forms_without_guessing() {
    let string = serde_json::Value::String("https://github.com/example/project.git".into());
    let object = json!({"type":"git", "url":"git+https://github.com/example/project.git"});
    assert_eq!(
        parse_repository(&string).unwrap().full_name,
        "example/project"
    );
    assert_eq!(
        parse_repository(&object).unwrap().full_name,
        "example/project"
    );
    assert!(parse_repository(&json!("https://gitlab.com/example/project")).is_none());
    assert!(parse_repository(&json!("https://user:pass@github.com/example/project")).is_none());
}

#[test]
fn automatic_schedule_runs_at_start_and_once_after_long_sleep() {
    const DAY: u64 = 24 * 60 * 60 * 1000;
    let mut schedule = AutomaticSchedule::new(100);
    assert!(schedule.take_due(100));
    assert!(!schedule.take_due(100 + DAY - 1));
    assert!(schedule.take_due(100 + DAY));
    assert!(!schedule.take_due(100 + DAY + 1));
    assert!(schedule.take_due(100 + DAY * 4 + 100));
    assert_eq!(schedule.next_at, 100 + DAY * 5);
}

#[test]
fn single_flight_merges_overlapping_callers() {
    let flight = Arc::new(SingleFlight::<usize>::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let first = {
        let flight = flight.clone();
        let calls = calls.clone();
        thread::spawn(move || {
            flight.run(|| {
                calls.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(30));
                42
            })
        })
    };
    thread::sleep(Duration::from_millis(5));
    let second = {
        let flight = flight.clone();
        thread::spawn(move || {
            flight.run(|| {
                panic!("overlapping caller became a second leader");
            })
        })
    };
    assert_eq!(first.join().unwrap(), 42);
    assert_eq!(second.join().unwrap(), 42);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_collect_candidate_urls() {
    use super::service::collect_candidate_urls;
    let direct = "https://github.com/rinnein/your-szu-life/releases/download/v1.0.0/setup.msi";
    let accelerators = vec![
        reqwest::Url::parse("https://gh-proxy.com/").unwrap(),
        reqwest::Url::parse("https://ghfast.top/").unwrap(),
    ];
    let urls = collect_candidate_urls(direct, &accelerators);
    assert_eq!(urls.len(), 3);
    assert_eq!(
        urls[0],
        "https://gh-proxy.com/https://github.com/rinnein/your-szu-life/releases/download/v1.0.0/setup.msi"
    );
    assert_eq!(
        urls[1],
        "https://ghfast.top/https://github.com/rinnein/your-szu-life/releases/download/v1.0.0/setup.msi"
    );
    assert_eq!(urls[2], direct);
}
