use super::*;
use crate::campus_network::{
    http::{HttpError, HttpResponse},
    portal::parse_jsonp_callback,
    testing::*,
};
use std::sync::Mutex;

#[test]
fn extracts_only_static_config_without_executing_javascript() {
    let html = format!(
        r#"<!--<script>var CONFIG={{ip:'bad'}};</script>-->
        <script type="application/json">{{"CONFIG":"not-script"}}</script>
        <script>var text="var CONFIG = {{ip:'bad'}}"; function nope() {{ var CONFIG = {{ip:'bad'}}; }}</script>
        {}"#,
        staff_html(TEST_IP, true)
    );
    let config = parse_staff_config(&html).unwrap();
    assert_eq!(config.ip, TEST_IP);
    assert_eq!(config.acid, "2");
    assert!(config.mac_auth);
    let calculated = html.replace(&format!("ip: '{TEST_IP}'"), "ip: getIPAddress()");
    assert!(parse_staff_config(&calculated).is_none());
    assert!(parse_staff_config(&staff_html("0.0.0.0", true)).is_none());
    assert!(parse_staff_config(&staff_html("127.0.0.1", true)).is_none());
    assert!(parse_staff_config(&format!(
        "{}<script>CONFIG = unknown();</script>",
        staff_html(TEST_IP, true)
    ))
    .is_none());
}

#[test]
fn supports_quoted_keys_false_flags_and_ignores_nonliteral_overrides() {
    let html = r#"<script>const CONFIG = {'ip':'172.28.0.8', 'acid': 3, portal:{ MacAuth:false }, lang:'zh-CN'||'en'};</script>"#;
    let config = parse_staff_config(html).unwrap();
    assert!(!config.mac_auth);
    assert_eq!(config.acid, "3");
    assert!(parse_staff_config(&html.replace("lang:", "ip: (() => 'evil')(), lang:")).is_none());
}

#[test]
fn follows_http_and_entity_escaped_meta_redirects_on_the_same_https_host() {
    let paths = Mutex::new(Vec::new());
    let http = FakeHttp(|url: &Url| {
        paths.lock().unwrap().push(url.path().to_owned());
        Ok(match url.path() {
            "/" => HttpResponse {
                status: 302,
                location: Some("/index_1.html".into()),
                ..Default::default()
            },
            "/index_1.html" => html(
                r#"<meta http-equiv="refresh" content="0;url=/srun_portal_pc?ac_id=2&amp;theme=pro">"#,
            ),
            "/srun_portal_pc" => {
                assert_eq!(
                    url.query_pairs().find(|(key, _)| key == "theme").unwrap().1,
                    "pro"
                );
                html(&staff_html(TEST_IP, true))
            }
            "/cgi-bin/rad_user_info" => {
                assert_eq!(
                    url.query_pairs().find(|(key, _)| key == "ip").unwrap().1,
                    TEST_IP
                );
                jsonp(url, staff_online(TEST_IP))
            }
            _ => panic!("unexpected probe path"),
        })
    });
    let probe = probe_staff_portal(&http, 1234);
    assert_eq!(probe.state, PortalOnlineState::Online);
    assert_eq!(paths.lock().unwrap().len(), 4);
}

#[test]
fn rejects_unsafe_redirects_and_bounds_cycles() {
    for target in [
        "http://net.szu.edu.cn/",
        "https://evil.example/",
        "https://net.szu.edu.cn:8800/",
        "//net.szu.edu.cn.evil.example/",
        "https://user@net.szu.edu.cn/",
        "https://net.szu.edu.cn/#fragment",
    ] {
        assert!(
            staff_redirect(&staff_url("/"), target).is_none(),
            "{target}"
        );
    }
    let count = Mutex::new(0);
    let http = FakeHttp(|_: &Url| {
        *count.lock().unwrap() += 1;
        Ok(HttpResponse {
            status: 302,
            location: Some("/".into()),
            ..Default::default()
        })
    });
    assert_eq!(
        probe_staff_portal(&http, 1).state,
        PortalOnlineState::Unknown
    );
    assert_eq!(*count.lock().unwrap(), 1);
    let counter = Mutex::new(0);
    let http = FakeHttp(|_: &Url| {
        let mut count = counter.lock().unwrap();
        *count += 1;
        Ok(HttpResponse {
            status: 302,
            location: Some(format!("/hop{count}")),
            ..Default::default()
        })
    });
    assert_eq!(
        probe_staff_portal(&http, 1).state,
        PortalOnlineState::Unknown
    );
    assert_eq!(*counter.lock().unwrap(), MAX_REDIRECTS + 1);
}

#[test]
fn callback_is_checked_and_trailing_code_is_rejected() {
    assert!(parse_jsonp_callback("cb({\"error\":\"ok\"});", "cb").is_ok());
    for body in [
        "other({\"error\":\"ok\"});",
        "cb({});evil();",
        "cb({});;",
        "cb({/*comment*/});",
    ] {
        assert!(parse_jsonp_callback(body, "cb").is_err());
    }
    assert!(parse_jsonp_callback("window.cb({});", "window.cb").is_err());
}

#[test]
fn maps_cumulative_usage_and_keeps_leading_zeroes_without_other_devices() {
    let mut probe = StaffPortalProbe {
        config: parse_staff_config(&staff_html(TEST_IP, true)),
        ..Default::default()
    };
    let mut value = staff_online(TEST_IP);
    value["all_bytes"] = Value::from(1);
    value["bytes_in"] = Value::from(2);
    apply_staff_info(&mut probe, &value);
    let session = probe.session.unwrap();
    assert_eq!(session.username, "001234");
    assert_eq!(session.used_bytes, Some(25_473_392_014));
    assert_eq!(session.used_seconds, Some(663_767));
    assert_eq!(session.login_started_at_ms, Some(1_788_827_879_000));
    let wire = serde_json::to_value(session).unwrap();
    assert!(wire.get("online_device_detail").is_none());
    assert!(wire.get("password").is_none());
}

#[test]
fn missing_zero_and_malformed_values_are_distinct() {
    let mut probe = StaffPortalProbe {
        config: parse_staff_config(&staff_html(TEST_IP, false)),
        ..Default::default()
    };
    let mut value = staff_online(TEST_IP);
    value["sum_bytes"] = Value::from("0");
    value.as_object_mut().unwrap().remove("sum_seconds");
    value["real_name"] = Value::from("");
    apply_staff_info(&mut probe, &value);
    let session = probe.session.as_ref().unwrap();
    assert_eq!(session.used_bytes, Some(0));
    assert_eq!(session.used_seconds, None);
    assert_eq!(session.real_name, None);
    value["sum_bytes"] = Value::from(-1);
    value["sum_seconds"] = Value::from(1.5);
    value["add_time"] = Value::from(u64::MAX);
    apply_staff_info(&mut probe, &value);
    let session = probe.session.unwrap();
    assert_eq!(session.used_bytes, None);
    assert_eq!(session.used_seconds, None);
    assert_eq!(session.login_started_at_ms, None);
}

#[test]
fn only_explicit_not_online_is_offline_and_online_requires_current_ip() {
    let mut probe = StaffPortalProbe {
        config: parse_staff_config(&staff_html(TEST_IP, true)),
        ..Default::default()
    };
    apply_staff_info(&mut probe, &staff_online("172.28.0.99"));
    assert_eq!(probe.state, PortalOnlineState::Unknown);
    assert!(probe.session.is_none());
    for value in [
        serde_json::json!({"error":"server_error"}),
        serde_json::json!({"error":"ok","online_ip":TEST_IP}),
    ] {
        apply_staff_info(&mut probe, &value);
        assert_eq!(probe.state, PortalOnlineState::Unknown);
    }
    apply_staff_info(&mut probe, &serde_json::json!({"error":"not_online_error"}));
    assert_eq!(probe.state, PortalOnlineState::Offline);
}

#[test]
fn http_errors_and_malformed_api_responses_never_claim_offline() {
    for mode in [0, 1, 2, 3] {
        let http = FakeHttp(|url: &Url| {
            if url.path() == "/" {
                return Ok(html(&staff_html(TEST_IP, true)));
            }
            match mode {
                0 => Err(HttpError::Timeout),
                1 => Ok(HttpResponse {
                    status: 503,
                    ..Default::default()
                }),
                2 => Ok(html("bad({});")),
                _ => Ok(html("<html>unrelated page</html>")),
            }
        });
        let probe = probe_staff_portal(&http, 12);
        assert_eq!(probe.state, PortalOnlineState::Unknown);
        assert!(probe.config.is_some());
        assert!(probe.session.is_none());
    }
}
