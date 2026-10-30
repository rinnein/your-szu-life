use super::{
    config::{ContentConfig, parse_repository},
    model::{Announcements, Document, UpdateInfo, is_newer, parse_document, web_url},
    runtime::{AutomaticSchedule, SingleFlight},
    service::{
        extract_release_version, extract_version_from_asset_name, find_platform_asset_index,
        is_platform_asset,
    },
};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
        assert!(!is_platform_asset(
            "your-szu-life_android_arm64_0.9.0-release.apk"
        ));
        let names = [
            "your-szu-life_android_arm64_0.9.0-release.apk",
            "your-szu-life_android_arm64_0.9.0-release.apk.gz",
        ];
        // Only matches .apk.gz
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

#[test]
fn github_content_service_loads_cache_and_preserves_on_request_failure() {
    use super::{
        cache::Cache,
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{Announcement, Announcements, ContentError, ErrorKind, Source, UpdateInfo},
        runtime::Clock,
        service::ContentService,
    };

    struct RateLimitedHttp;
    impl PublicHttp for RateLimitedHttp {
        fn get(
            &self,
            _url: &reqwest::Url,
            _kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            Err(ContentError::new(
                ErrorKind::RateLimited,
                "GitHub 请求受限，请稍后再试。",
            ))
        }
    }

    struct TestClock(u64);
    impl Clock for TestClock {
        fn now_ms(&self) -> u64 {
            self.0
        }
    }

    let temp_dir = std::env::temp_dir().join(format!("test-cache-{}", uuid::Uuid::new_v4()));
    let config = ContentConfig::new(
        "0.1.0".into(),
        None,
        None,
        Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
    );
    assert!(config.uses_github_releases());

    let scope = format!("github:{}", config.repository.as_ref().unwrap().full_name);
    let cache = Cache::new(Some(temp_dir.clone()));
    let announcements = Announcements {
        announcements: vec![Announcement {
            title: "测试公告".into(),
            desc: Some("公告内容".into()),
            goto: None,
            pic: None,
        }],
    };
    let update = UpdateInfo {
        latest_ver: "0.2.0".into(),
        latest_ver_url: "https://example.com/app.msi".into(),
        changelog: "修复问题".into(),
        release_url: None,
    };
    cache.save(
        &scope,
        "announcements.json",
        &announcements,
        Source::Github,
        1000,
    );
    cache.save(&scope, "latest-release.json", &update, Source::Github, 1000);

    let service = ContentService::new(
        config,
        Arc::new(RateLimitedHttp),
        Arc::new(TestClock(2000)),
        Some(temp_dir.clone()),
        |_| {},
    );

    let snapshot = service.snapshot();
    assert!(
        snapshot.announcements.data.is_some(),
        "announcements must be loaded from cache in new()"
    );
    assert!(snapshot.announcements.from_cache);
    assert_eq!(
        snapshot.announcements.data.as_ref().unwrap().announcements[0].title,
        "测试公告"
    );
    assert!(snapshot.update.data.is_some());
    assert!(snapshot.update.from_cache);
    assert!(snapshot.update_available);

    // Now trigger refresh when request fails (rate limited). The cache must not expire!
    let refreshed = service.refresh();
    assert!(
        refreshed.announcements.data.is_some(),
        "announcements cache must not expire on failure"
    );
    assert!(refreshed.announcements.from_cache);
    assert!(refreshed.announcements.error.is_some());
    assert_eq!(
        refreshed.announcements.data.as_ref().unwrap().announcements[0].title,
        "测试公告"
    );

    assert!(
        refreshed.update.data.is_some(),
        "update cache must not expire on failure"
    );
    assert!(refreshed.update.from_cache);
    assert!(refreshed.update.error.is_some());
    assert!(
        refreshed.update_available,
        "update_available must be preserved from cache"
    );

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_extract_release_tag_and_filename() {
    use super::service::extract_release_tag_and_filename;

    let gh = "https://github.com/rinnein/your-szu-life/releases/download/v0.9.1/app.msi";
    assert_eq!(
        extract_release_tag_and_filename(gh),
        Some(("v0.9.1", "app.msi"))
    );

    let gh_acc = "https://ghfast.top/https://github.com/rinnein/your-szu-life/releases/download/canary/app.apk.gz";
    assert_eq!(
        extract_release_tag_and_filename(gh_acc),
        Some(("canary", "app.apk.gz"))
    );

    let cnb = "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.1/app.msi";
    assert_eq!(
        extract_release_tag_and_filename(cnb),
        Some(("v0.9.1", "app.msi"))
    );

    let cnb_query =
        "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.1/app.msi?share=true";
    assert_eq!(
        extract_release_tag_and_filename(cnb_query),
        Some(("v0.9.1", "app.msi"))
    );

    assert_eq!(
        extract_release_tag_and_filename("https://example.com/other"),
        None
    );
}

#[test]
fn test_cnb_fallback_when_github_fails() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind, Source},
        runtime::Clock,
        service::ContentService,
    };

    struct CnbFallbackHttp;
    impl PublicHttp for CnbFallbackHttp {
        fn get(
            &self,
            url: &reqwest::Url,
            kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            if url.host_str() == Some("api.github.com") {
                return Err(ContentError::new(
                    ErrorKind::RateLimited,
                    "GitHub 请求受限，请稍后再试。",
                ));
            }
            if url.host_str() == Some("cnb.cool") {
                assert_eq!(kind, RequestKind::CnbWeb);
                if url.path().ends_with("/releases/tags/announcement") {
                    let body = json!({
                        "release": {
                            "body": "{\"announcements\":[{\"title\":\"CNB公告\",\"desc\":\"正文\"}]}"
                        }
                    });
                    return Ok(HttpResponse {
                        status: 200,
                        body: serde_json::to_vec(&body).unwrap(),
                        content_type: Some("application/vnd.cnb.web+json".into()),
                        has_next_page: false,
                        retry_at: None,
                    });
                }
                if url.path().ends_with("/releases") {
                    let body = json!({
                        "releases": [
                            {
                                "tag_ref": "refs/tags/v0.9.5",
                                "title": "v0.9.5",
                                "body": "CNB更新日志",
                                "assets": [
                                    {
                                        "name": "your-szu-life_windows_x64_0.9.5-release.msi",
                                        "browser_download_url": "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/your-szu-life_windows_x64_0.9.5-release.msi"
                                    },
                                    {
                                        "name": "your-szu-life_android_arm64_0.9.5-release.apk.gz",
                                        "browser_download_url": "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/your-szu-life_android_arm64_0.9.5-release.apk.gz"
                                    }
                                ]
                            }
                        ]
                    });
                    return Ok(HttpResponse {
                        status: 200,
                        body: serde_json::to_vec(&body).unwrap(),
                        content_type: Some("application/vnd.cnb.web+json".into()),
                        has_next_page: false,
                        retry_at: None,
                    });
                }
            }
            Err(ContentError::new(ErrorKind::Http, "404 Not Found"))
        }
    }

    struct TestClock;
    impl Clock for TestClock {
        fn now_ms(&self) -> u64 {
            10000
        }
    }

    let config = ContentConfig::new(
        "0.1.0".into(),
        None,
        None,
        Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
    );

    let service = ContentService::new(
        config,
        Arc::new(CnbFallbackHttp),
        Arc::new(TestClock),
        None,
        |_| {},
    );

    let snapshot = service.refresh();
    assert_eq!(snapshot.announcements.source, Some(Source::Cnb));
    assert!(snapshot.announcements.data.is_some());
    assert_eq!(
        snapshot.announcements.data.unwrap().announcements[0].title,
        "CNB公告"
    );

    #[cfg(any(target_os = "windows", target_os = "android"))]
    {
        assert_eq!(snapshot.update.source, Some(Source::Cnb));
        assert!(snapshot.update.data.is_some());
        assert_eq!(snapshot.update.data.unwrap().latest_ver, "0.9.5");
        assert!(snapshot.update_available);
    }
}

#[test]
fn test_build_download_candidates_with_cnb() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind},
        runtime::Clock,
        service::ContentService,
    };

    struct SyncCheckHttp;
    impl PublicHttp for SyncCheckHttp {
        fn get(
            &self,
            url: &reqwest::Url,
            _kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            if url.path().contains("/releases/tags/v0.9.5") {
                let body = json!({
                    "release": {
                        "assets": [
                            {
                                "name": "app.msi",
                                "browser_download_url": "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/app.msi"
                            }
                        ]
                    }
                });
                return Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                    content_type: Some("application/vnd.cnb.web+json".into()),
                    has_next_page: false,
                    retry_at: None,
                });
            }
            Err(ContentError::new(ErrorKind::Http, "404 Not Found"))
        }
    }

    struct TestClock;
    impl Clock for TestClock {
        fn now_ms(&self) -> u64 {
            1000
        }
    }

    let mut config = ContentConfig::new(
        "0.1.0".into(),
        None,
        None,
        Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
    );
    config.github_accelerators = vec![reqwest::Url::parse("https://gh-proxy.com/").unwrap()];

    let service = ContentService::new(
        config,
        Arc::new(SyncCheckHttp),
        Arc::new(TestClock),
        None,
        |_| {},
    );

    // Case 1: GitHub URL where CNB is synced
    let gh_url = "https://github.com/rinnein/your-szu-life/releases/download/v0.9.5/app.msi";
    let candidates = service.build_download_candidates(gh_url);
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].0, 1);
    assert_eq!(
        candidates[0].1,
        "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/app.msi"
    );
    assert_eq!(candidates[1].0, 2);
    assert_eq!(
        candidates[1].1,
        "https://gh-proxy.com/https://github.com/rinnein/your-szu-life/releases/download/v0.9.5/app.msi"
    );
    assert_eq!(candidates[2].0, 3);
    assert_eq!(
        candidates[2].1,
        "https://github.com/rinnein/your-szu-life/releases/download/v0.9.5/app.msi"
    );

    // Case 2: Direct CNB URL (checked via CNB, back-derived GitHub mirrors)
    let cnb_url = "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/app.msi";
    let candidates = service.build_download_candidates(cnb_url);
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].0, 1);
    assert_eq!(
        candidates[0].1,
        "https://cnb.cool/rinnein/szu/your-szu-life/-/releases/download/v0.9.5/app.msi"
    );
    assert_eq!(candidates[1].0, 2);
    assert_eq!(
        candidates[1].1,
        "https://gh-proxy.com/https://github.com/rinnein/your-szu-life/releases/download/v0.9.5/app.msi"
    );
    assert_eq!(candidates[2].0, 3);
    assert_eq!(
        candidates[2].1,
        "https://github.com/rinnein/your-szu-life/releases/download/v0.9.5/app.msi"
    );

    // Case 3: GitHub URL where CNB is NOT synced (e.g. tag v0.9.6)
    let not_synced_url =
        "https://github.com/rinnein/your-szu-life/releases/download/v0.9.6/app.msi";
    let candidates = service.build_download_candidates(not_synced_url);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].0, 1);
    assert_eq!(
        candidates[0].1,
        "https://gh-proxy.com/https://github.com/rinnein/your-szu-life/releases/download/v0.9.6/app.msi"
    );
    assert_eq!(candidates[1].0, 2);
    assert_eq!(
        candidates[1].1,
        "https://github.com/rinnein/your-szu-life/releases/download/v0.9.6/app.msi"
    );
}
