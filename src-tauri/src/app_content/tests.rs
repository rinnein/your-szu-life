use super::{
    config::{parse_repository, ContentConfig},
    model::{
        applicable_announcements, is_newer, parse_document, web_url, AnnouncementMode,
        Announcements, Document, UpdateInfo,
    },
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
        model::{
            Announcement, AnnouncementConditions, AnnouncementMode, Announcements, ContentError,
            ErrorKind, Source, UpdateChannel, UpdateInfo,
        },
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

    let repository = config.repository.as_ref().unwrap();
    let announcement_scope = format!("github:{}", repository.full_name);
    // 更新缓存按频道隔离：稳定版默认频道对应 `:stable` 目录。
    let update_scope = format!("github:{}:stable", repository.full_name);
    let cache = Cache::new(Some(temp_dir.clone()));
    let announcements = Announcements {
        announcements: vec![
            Announcement {
                title: "测试公告".into(),
                id: None,
                mode: AnnouncementMode::Silent,
                conditions: None,
                desc: Some("公告内容".into()),
                goto: None,
                pic: None,
            },
            // 定向公告只对 >=9.0.0 生效，当前版本 0.1.0 的快照里不应出现
            Announcement {
                title: "未来版本公告".into(),
                id: Some("future-only".into()),
                mode: AnnouncementMode::Popup,
                conditions: Some(AnnouncementConditions {
                    ver: Some(">=9.0.0".into()),
                }),
                desc: Some("公告内容".into()),
                goto: None,
                pic: None,
            },
        ],
    };
    let update = UpdateInfo {
        latest_ver: "0.2.0".into(),
        latest_ver_url: "https://example.com/app.msi".into(),
        changelog: "修复问题".into(),
        release_url: None,
    };
    cache.save(
        &announcement_scope,
        "announcements.json",
        &announcements,
        Source::Github,
        1000,
    );
    cache.save(
        &update_scope,
        "latest-release.json",
        &update,
        Source::Github,
        1000,
    );

    let service = ContentService::new(
        config,
        Arc::new(RateLimitedHttp),
        Arc::new(TestClock(2000)),
        Some(temp_dir.clone()),
        UpdateChannel::Stable,
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
    // 快照按当前版本过滤，磁盘缓存保留未过滤的原始数据
    assert_eq!(
        snapshot
            .announcements
            .data
            .as_ref()
            .unwrap()
            .announcements
            .len(),
        1,
        "version-targeted announcements must stay out of the snapshot"
    );
    assert_eq!(
        cache
            .load::<Announcements>(&announcement_scope, "announcements.json", 2000)
            .expect("raw cache entry")
            .data
            .announcements
            .len(),
        2,
        "cache must keep the raw publisher data"
    );

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
        model::{ContentError, ErrorKind, Source, UpdateChannel},
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
        UpdateChannel::Stable,
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
fn image_redirects_stop_at_the_hop_limit_and_reject_unsafe_targets() {
    use super::http::check_image_redirect;

    let image = reqwest::Url::parse("https://tc.lolicc.cc/pic/a.png").unwrap();
    assert!(check_image_redirect(0, &image).is_ok());
    assert!(check_image_redirect(4, &image).is_ok());
    assert!(check_image_redirect(5, &image).is_err());

    for target in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "http://user:pass@example.com/a.png",
    ] {
        assert!(
            check_image_redirect(0, &reqwest::Url::parse(target).unwrap()).is_err(),
            "expected redirect target to be rejected: {target}"
        );
    }
}

#[test]
fn announcement_images_are_downscaled_before_reaching_the_webview() {
    use super::{images::normalize_image, model::IMAGE_LIMIT};
    use image::{DynamicImage, ImageFormat, ImageReader, RgbImage, RgbaImage};
    use std::io::Cursor;

    fn encode(image: DynamicImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    let opaque = encode(DynamicImage::ImageRgb8(RgbImage::from_fn(
        2000,
        1000,
        |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 64]),
    )));
    let (bytes, mime) = normalize_image(&opaque, Some("image/png")).unwrap();
    assert_eq!(mime, "image/jpeg");
    assert!(bytes.len() <= IMAGE_LIMIT);
    let (width, height) = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .unwrap()
        .into_dimensions()
        .unwrap();
    assert!(width <= 1600 && height <= 1600);

    // A small picture keeps its original bytes, including transparency and animation.
    let small = encode(DynamicImage::ImageRgba8(RgbaImage::from_fn(
        32,
        32,
        |x, y| image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 200]),
    )));
    let (bytes, mime) = normalize_image(&small, Some("image/png")).unwrap();
    assert_eq!(mime, "image/png");
    assert_eq!(bytes, small);

    assert!(normalize_image(&small, Some("image/jpeg")).is_err());
}

#[test]
fn test_build_download_candidates_with_cnb() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind, UpdateChannel},
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
        UpdateChannel::Stable,
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

#[test]
fn legacy_announcements_without_new_fields_stay_silent_and_unconditional() {
    let bytes = serde_json::to_vec(&json!({
        "announcements": [{"title": "旧格式", "desc": "正文"}]
    }))
    .unwrap();
    let parsed: Announcements = parse_document(&bytes).unwrap();
    assert_eq!(parsed.announcements[0].mode, AnnouncementMode::Silent);
    assert_eq!(parsed.announcements[0].id, None);
    assert_eq!(parsed.announcements[0].conditions, None);
    // 缺省的新字段不参与序列化，旧格式往返逐字节等价
    let serialized = serde_json::to_string(&parsed).unwrap();
    assert_eq!(
        serialized,
        r#"{"announcements":[{"title":"旧格式","desc":"正文"}]}"#
    );
}

#[test]
fn announcements_tolerate_fields_from_newer_publishers() {
    // 旧版二进制依靠 serde 忽略未知字段读取新版发布的公告，这里固化该向前兼容保证
    let bytes = serde_json::to_vec(&json!({
        "announcements": [{"title": "未来字段", "desc": "正文", "futureField": 123}]
    }))
    .unwrap();
    let parsed: Announcements = parse_document(&bytes).unwrap();
    assert_eq!(parsed.announcements[0].title, "未来字段");
}

#[test]
fn version_conditions_follow_comparator_semantics() {
    use super::model::VersionCondition;
    use semver::Version;

    let current = Version::parse("0.11.2").unwrap();
    for (ver, expected) in [
        (">=0.11.0", true),
        ("<0.12.0", true),
        (">0.11.2", false),
        ("<=0.11.2", true),
        ("0.11.2", true),
        ("0.11.1", false),
        (">=0.10.0, <0.12.0", true),
        (">=0.11.3, <0.12.0", false),
        (">=v0.11.0", true),
        (">= 0.11.0", true),
        (">=0.11.2+build.7", true),
    ] {
        let condition = VersionCondition::parse(ver).unwrap_or_else(|| panic!("解析失败: {ver}"));
        assert_eq!(condition.matches(&current), expected, "ver: {ver}");
    }
    assert_eq!(VersionCondition::parse("not-semver"), None);
    assert_eq!(VersionCondition::parse(""), None);
}

#[test]
fn applicable_announcements_filters_by_version_and_degrades_bad_popup_ids() {
    let doc: Announcements = parse_document(
        &serde_json::to_vec(&json!({
            "announcements": [
                {"title": "全局", "desc": "x"},
                {"title": "命中", "id": "hit", "mode": "popup", "conditions": {"ver": ">=0.11.0, <0.12.0"}, "desc": "x"},
                {"title": "未命中", "id": "miss", "mode": "popup", "conditions": {"ver": ">=1.0.0"}, "desc": "x"},
                {"title": "缺id", "mode": "popup", "desc": "x"},
                {"title": "坏id", "id": "  ", "mode": "popup", "desc": "x"},
                {"title": "提醒", "mode": "alert", "desc": "x"},
                {"title": "坏ver", "conditions": {"ver": "not-semver"}, "desc": "x"},
                {"title": "空conditions", "mode": "alert", "conditions": {}, "desc": "x"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let filtered = applicable_announcements(doc, "0.11.2");
    let titles: Vec<&str> = filtered
        .announcements
        .iter()
        .map(|unit| unit.title.as_str())
        .collect();
    assert_eq!(
        titles,
        vec!["全局", "命中", "缺id", "坏id", "提醒", "空conditions"]
    );

    let by_title = |title: &str| {
        filtered
            .announcements
            .iter()
            .find(|unit| unit.title == title)
            .unwrap()
    };
    assert_eq!(by_title("命中").mode, AnnouncementMode::Popup);
    assert_eq!(by_title("缺id").mode, AnnouncementMode::Silent);
    assert_eq!(by_title("坏id").mode, AnnouncementMode::Silent);
    assert_eq!(by_title("提醒").mode, AnnouncementMode::Alert);
    assert_eq!(by_title("空conditions").mode, AnnouncementMode::Alert);
}

#[test]
fn applicable_announcements_keeps_everything_when_current_version_is_unparseable() {
    let doc: Announcements = parse_document(
        &serde_json::to_vec(&json!({
            "announcements": [
                {"title": "命中", "id": "a", "mode": "popup", "conditions": {"ver": ">=0.1.0"}, "desc": "x"},
                {"title": "未命中", "id": "b", "mode": "popup", "conditions": {"ver": ">=9.0.0"}, "desc": "x"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    // 宁可多显示：版本号本身解析失败时不过滤
    let filtered = applicable_announcements(doc, "not-semver");
    assert_eq!(filtered.announcements.len(), 2);
}

#[test]
fn invalid_mode_values_degrade_to_silent_without_rejecting_the_document() {
    let bytes = serde_json::to_vec(&json!({
        "announcements": [
            {"title": "拼错mode", "mode": "modal", "desc": "x"},
            {"title": "null mode", "mode": null, "desc": "x"},
            {"title": "正常alert", "mode": "alert", "desc": "x"}
        ]
    }))
    .unwrap();
    let parsed: Announcements = parse_document(&bytes).expect("新字段语义错误不得整体拒绝文档");
    assert_eq!(parsed.announcements[0].mode, AnnouncementMode::Silent);
    assert_eq!(parsed.announcements[1].mode, AnnouncementMode::Silent);
    assert_eq!(parsed.announcements[2].mode, AnnouncementMode::Alert);
}

/// 发布资产名必须匹配当前测试主机的平台后缀，否则 `find_platform_asset_index` 会跳过它。
fn channel_test_asset_name(version: &str) -> String {
    #[cfg(target_os = "android")]
    let (platform, arch, extension) = ("android", "arm64", "apk.gz");
    #[cfg(target_os = "windows")]
    let (platform, arch, extension) = ("windows", "x64", "msi");
    #[cfg(target_os = "macos")]
    let (platform, arch, extension) = ("macos", "x64", "dmg");
    #[cfg(not(any(target_os = "android", target_os = "windows", target_os = "macos")))]
    let (platform, arch, extension) = ("linux", "x64", "deb");
    format!("your-szu-life_{platform}_{arch}_{version}-release.{extension}")
}

#[test]
fn update_channel_filters_github_releases_and_falls_back_to_canary() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind, Source, UpdateChannel},
        runtime::Clock,
        service::ContentService,
    };

    struct ChannelHttp {
        /// 是否存在正式版；false 时稳定版频道应回退到 canary。
        stable: bool,
    }

    fn asset(version: &str) -> serde_json::Value {
        let name = channel_test_asset_name(version);
        json!({
            "name": name,
            "browser_download_url": format!(
                "https://github.com/rinnein/your-szu-life/releases/download/v{version}/{name}"
            )
        })
    }

    fn stable_release() -> serde_json::Value {
        json!({
            "tag_name": "v0.1.5",
            "name": "v0.1.5",
            "draft": false,
            "prerelease": false,
            "assets": [asset("0.1.5")]
        })
    }

    fn canary_release() -> serde_json::Value {
        json!({
            "tag_name": "canary",
            "name": "canary v0.2.0",
            "body": "测试版说明",
            "draft": false,
            "prerelease": true,
            "assets": [asset("0.2.0")]
        })
    }

    impl PublicHttp for ChannelHttp {
        fn get(
            &self,
            url: &reqwest::Url,
            _kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            let ok = |body: serde_json::Value| {
                Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                    content_type: Some("application/json".into()),
                    has_next_page: false,
                    retry_at: None,
                })
            };
            if url.host_str() != Some("api.github.com") {
                return Err(ContentError::new(ErrorKind::Http, "404 Not Found"));
            }
            let path = url.path();
            if path.ends_with("/releases/tags/announcement") {
                return ok(json!({
                    "tag_name": "announcement",
                    "body": "{\"announcements\":[]}"
                }));
            }
            if path.ends_with("/releases/tags/canary") {
                return ok(canary_release());
            }
            if path.ends_with("/releases/latest") {
                return if self.stable {
                    ok(stable_release())
                } else {
                    Err(ContentError::new(ErrorKind::Http, "404 Not Found"))
                };
            }
            if path.ends_with("/releases") {
                let mut releases = Vec::new();
                if self.stable {
                    releases.push(stable_release());
                }
                releases.push(canary_release());
                return ok(serde_json::Value::Array(releases));
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

    fn service(stable: bool, channel: UpdateChannel) -> ContentService {
        let config = ContentConfig::new(
            "0.1.0".into(),
            None,
            None,
            Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
        );
        ContentService::new(
            config,
            Arc::new(ChannelHttp { stable }),
            Arc::new(TestClock),
            None,
            channel,
            |_| {},
        )
    }

    // 稳定版只认非 prerelease 的正式版
    let snapshot = service(true, UpdateChannel::Stable).refresh();
    assert_eq!(snapshot.update_channel, UpdateChannel::Stable);
    assert!(!snapshot.update_channel_fallback);
    assert_eq!(snapshot.update.source, Some(Source::Github));
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.1.5");

    // 测试版只认滚动 canary
    let snapshot = service(true, UpdateChannel::Canary).refresh();
    assert_eq!(snapshot.update_channel, UpdateChannel::Canary);
    assert!(!snapshot.update_channel_fallback);
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.2.0");

    // 没有正式版时稳定版回退 canary，并显式标记
    let snapshot = service(false, UpdateChannel::Stable).refresh();
    assert!(snapshot.update_channel_fallback);
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.2.0");
}

#[test]
fn switching_channel_uses_its_own_cached_update_feed() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind, UpdateChannel},
        runtime::Clock,
        service::ContentService,
    };

    struct ChannelHttp;
    impl PublicHttp for ChannelHttp {
        fn get(
            &self,
            url: &reqwest::Url,
            _kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            let ok = |body: serde_json::Value| {
                Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                    content_type: Some("application/json".into()),
                    has_next_page: false,
                    retry_at: None,
                })
            };
            if url.host_str() != Some("api.github.com") {
                return Err(ContentError::new(ErrorKind::Http, "404 Not Found"));
            }
            let path = url.path();
            if path.ends_with("/releases/tags/announcement") {
                return ok(json!({
                    "tag_name": "announcement",
                    "body": "{\"announcements\":[]}"
                }));
            }
            if path.ends_with("/releases/tags/canary") {
                let name = channel_test_asset_name("0.2.0");
                return ok(json!({
                    "tag_name": "canary",
                    "name": "canary v0.2.0",
                    "prerelease": true,
                    "assets": [{"name": name, "browser_download_url": format!("https://example.com/{name}")}]
                }));
            }
            if path.ends_with("/releases/latest") {
                let name = channel_test_asset_name("0.1.5");
                return ok(json!({
                    "tag_name": "v0.1.5",
                    "name": "v0.1.5",
                    "prerelease": false,
                    "assets": [{"name": name, "browser_download_url": format!("https://example.com/{name}")}]
                }));
            }
            if path.ends_with("/releases") {
                let stable_name = channel_test_asset_name("0.1.5");
                let canary_name = channel_test_asset_name("0.2.0");
                return ok(json!([
                    {
                        "tag_name": "v0.1.5",
                        "name": "v0.1.5",
                        "prerelease": false,
                        "assets": [{"name": stable_name, "browser_download_url": format!("https://example.com/{stable_name}")}]
                    },
                    {
                        "tag_name": "canary",
                        "name": "canary v0.2.0",
                        "prerelease": true,
                        "assets": [{"name": canary_name, "browser_download_url": format!("https://example.com/{canary_name}")}]
                    }
                ]));
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

    let temp_dir = std::env::temp_dir().join(format!("test-channel-{}", uuid::Uuid::new_v4()));
    fn config() -> ContentConfig {
        ContentConfig::new(
            "0.1.0".into(),
            None,
            None,
            Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
        )
    }
    let service = ContentService::new(
        config(),
        Arc::new(ChannelHttp),
        Arc::new(TestClock),
        Some(temp_dir.clone()),
        UpdateChannel::Stable,
        |_| {},
    );
    let stable = service.refresh();
    assert_eq!(stable.update.data.as_ref().unwrap().latest_ver, "0.1.5");

    // 切换后先只看到新频道（canary）自己的缓存：此时还没有缓存。
    let switched = service.set_channel(UpdateChannel::Canary);
    assert_eq!(switched.update_channel, UpdateChannel::Canary);
    assert!(switched.update.data.is_none());
    let canary = service.refresh();
    assert_eq!(canary.update.data.as_ref().unwrap().latest_ver, "0.2.0");

    // 换回稳定版：立刻从独立缓存读回正式版，不串到 canary 的数据。
    let switched_back = service.set_channel(UpdateChannel::Stable);
    assert_eq!(
        switched_back.update.data.as_ref().unwrap().latest_ver,
        "0.1.5"
    );

    // 新进程按缓存恢复到各自频道。
    let restored_stable = ContentService::new(
        config(),
        Arc::new(ChannelHttp),
        Arc::new(TestClock),
        Some(temp_dir.clone()),
        UpdateChannel::Stable,
        |_| {},
    );
    assert_eq!(
        restored_stable
            .snapshot()
            .update
            .data
            .as_ref()
            .unwrap()
            .latest_ver,
        "0.1.5"
    );
    let restored_canary = ContentService::new(
        config(),
        Arc::new(ChannelHttp),
        Arc::new(TestClock),
        Some(temp_dir.clone()),
        UpdateChannel::Canary,
        |_| {},
    );
    assert_eq!(
        restored_canary
            .snapshot()
            .update
            .data
            .as_ref()
            .unwrap()
            .latest_ver,
        "0.2.0"
    );

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn cnb_mirror_respects_update_channel() {
    use super::{
        http::{HttpResponse, PublicHttp, RequestKind},
        model::{ContentError, ErrorKind, Source, UpdateChannel},
        runtime::Clock,
        service::ContentService,
    };

    struct CnbChannelHttp {
        stable: bool,
    }

    impl PublicHttp for CnbChannelHttp {
        fn get(
            &self,
            url: &reqwest::Url,
            _kind: RequestKind,
            _limit: usize,
        ) -> Result<HttpResponse, ContentError> {
            // GitHub 全部失败，强制走 CNB 镜像。
            if url.host_str() != Some("cnb.cool") {
                return Err(ContentError::new(ErrorKind::Http, "404 Not Found"));
            }
            let ok = |body: serde_json::Value| {
                Ok(HttpResponse {
                    status: 200,
                    body: serde_json::to_vec(&body).unwrap(),
                    content_type: Some("application/vnd.cnb.web+json".into()),
                    has_next_page: false,
                    retry_at: None,
                })
            };
            if url.path().ends_with("/releases/tags/announcement") {
                return ok(json!({"release": {"body": "{\"announcements\":[]}"}}));
            }
            if url.path().ends_with("/releases") {
                let stable_name = channel_test_asset_name("0.9.5");
                let canary_name = channel_test_asset_name("0.9.9");
                let mut releases = Vec::new();
                if self.stable {
                    releases.push(json!({
                        "tag_ref": "refs/tags/v0.9.5",
                        "title": "v0.9.5",
                        "body": "正式版",
                        "prerelease": false,
                        "assets": [{"name": stable_name, "browser_download_url": format!("https://cnb.cool/x/{stable_name}")}]
                    }));
                }
                releases.push(json!({
                    "tag_ref": "refs/tags/canary",
                    "title": "canary v0.9.9",
                    "body": "测试版",
                    "prerelease": true,
                    "assets": [{"name": canary_name, "browser_download_url": format!("https://cnb.cool/x/{canary_name}")}]
                }));
                return ok(json!({ "releases": releases }));
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

    fn service(stable: bool, channel: UpdateChannel) -> ContentService {
        let config = ContentConfig::new(
            "0.1.0".into(),
            None,
            None,
            Some(&json!({"type": "git", "url": "https://github.com/rinnein/your-szu-life.git"})),
        );
        ContentService::new(
            config,
            Arc::new(CnbChannelHttp { stable }),
            Arc::new(TestClock),
            None,
            channel,
            |_| {},
        )
    }

    let snapshot = service(true, UpdateChannel::Stable).refresh();
    assert_eq!(snapshot.update.source, Some(Source::Cnb));
    assert!(!snapshot.update_channel_fallback);
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.9.5");

    let snapshot = service(true, UpdateChannel::Canary).refresh();
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.9.9");

    let snapshot = service(false, UpdateChannel::Stable).refresh();
    assert!(snapshot.update_channel_fallback);
    assert_eq!(snapshot.update.data.as_ref().unwrap().latest_ver, "0.9.9");
}
