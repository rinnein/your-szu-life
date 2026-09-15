use super::{
    config::{parse_repository, ContentConfig},
    model::{is_newer, parse_document, web_url, Announcements, Document, UpdateInfo},
    runtime::{AutomaticSchedule, SingleFlight},
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
fn announcements_require_title_pic_and_exactly_one_action() {
    let valid = json!({
        "announcements": [
            {"title": "详情", "desc": "正文", "pic": "http://intranet.test/a.png"},
            {"title": "外链", "goto": "https://example.com", "pic": "https://example.com/a.jpg"}
        ]
    });
    assert!(parse_document::<Announcements>(&serde_json::to_vec(&valid).unwrap()).is_ok());
    assert!(parse_document::<Announcements>(br#"{"announcements":[]}"#).is_ok());
    for invalid in [
        json!({"announcements":[{"title":"缺图","desc":"x"}]}),
        json!({"announcements":[{"title":"双操作","desc":"x","goto":"https://example.com","pic":"https://example.com/a"}]}),
        json!({"announcements":[{"title":"无操作","pic":"https://example.com/a"}]}),
        json!({"announcements":[{"title":"非法图","desc":"x","pic":"file:///tmp/a"}]}),
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
    };
    assert!(update.validate().is_ok());
    assert!(is_newer("0.1.0", "v0.2.0"));
    assert!(is_newer("1.0.0-alpha", "1.0.0"));
    assert!(!is_newer("1.0.0", "1.0.0+other-build"));
    assert!(!is_newer("1.0.0", "0.9.0"));
    assert!(!is_newer("not-semver", "1.0.0"));
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
