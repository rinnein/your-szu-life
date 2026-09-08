use super::*;
use crate::campus_network::{
    http::read_response,
    model::*,
    network::{
        classify_network_overview, portal_probe_from_html, DormPortalProbe, NetworkObservation,
        NetworkSnapshot, RegionEvidence,
    },
};

const ONLINE: &str = include_str!("../fixtures/dorm-online.html");

#[test]
fn decodes_actual_meta_shape_from_gbk_bytes_without_losing_name() {
    let (body, _, errors) = encoding_rs::GBK.encode(ONLINE);
    assert!(!errors);
    for content_type in [
        None,
        Some("text/html; charset=gbk"),
        Some("text/html; charset=\"GB2312\""),
    ] {
        let decoded = decode_portal_html(&body, content_type);
        let session = extract_portal_session_info(&decoded);
        assert_eq!(session.campus_card_number.as_deref(), Some("654321"));
        assert_eq!(session.online_username.as_deref(), Some("张三"));
        assert_eq!(session.charset.as_deref(), Some("gb2312"));
        assert_eq!(session.online_duration, Some(4_294_967_295));
        assert_eq!(session.online_flow, Some(4_294_967_295));
        assert!(!decoded.contains('\u{fffd}'));
    }
}

#[test]
fn encoding_priority_is_http_then_meta_then_utf8_then_gb18030() {
    let unicode = "<meta charset='GB2312'><script>uid='001234';NID='测试🙂';</script>";
    assert_eq!(
        decode_portal_html(unicode.as_bytes(), Some("text/html; CHARSET = UTF-8")),
        unicode
    );
    let meta = "<META CHARSET = \"GbK\"><script>NID='测试';</script>";
    let bytes = encoding_rs::GBK.encode(meta).0;
    assert_eq!(
        decode_portal_html(&bytes, Some("text/html; charset=unknown")),
        meta
    );
    let plain = "<script>NID='测试𠮷';</script>";
    assert_eq!(decode_portal_html(plain.as_bytes(), None), plain);
    let legacy = encoding_rs::GB18030.encode(plain).0;
    assert_eq!(decode_portal_html(&legacy, None), plain);
    assert_eq!(
        decode_portal_html(&legacy, Some("text/html; charset=GB18030")),
        plain
    );
}

#[test]
fn keeps_http_content_type_and_body_bytes_intact() {
    let encoded = encoding_rs::GBK.encode(ONLINE).0;
    let mut response =
        b"HTTP/1.1 200 OK\r\ncOnTeNt-TyPe: text/html; charset=gbk\r\nConnection: close\r\n\r\n"
            .to_vec();
    response.extend_from_slice(&encoded);
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 2048];
        let _ = stream.read(&mut request);
        stream.write_all(&response).unwrap();
    });
    let raw = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap()
        .get(format!("http://{address}/"))
        .send()
        .unwrap();
    let response = read_response(raw).unwrap();
    server.join().unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(
        response.content_type.as_deref(),
        Some("text/html; charset=gbk")
    );
    assert_eq!(response.body, encoded.as_ref());
    assert_eq!(
        extract_portal_session_info(&decode_portal_html(
            &response.body,
            response.content_type.as_deref()
        ))
        .online_username
        .as_deref(),
        Some("张三")
    );
}

#[test]
fn reads_only_exact_static_assignments_in_executable_script_blocks() {
    let html = r#"
    <!-- <meta charset='utf-8'><script>uid='comment';NID='comment';</script> -->
    <div>uid='text';NID='text';</div>
    <script type="application/json">{"uid":"json", "NID":"json"}</script>
    <script src="remote.js">uid='external';NID='external';</script>
    <script>
        // uid='comment';NID='comment';
        /* uid='comment';NID='comment'; */
        var uuid='bad', myuid='bad', NIDLabel='bad', name=";uid='string';NID='string';";
        var expression=/;uid='regex';NID='regex';/;
        function ignored() { var uid='nested';NID='nested'; }
        window.uid='member'; account.NID='member';
        var uid = '001234', NID = "网关姓名";
    </script>"#;
    let session = extract_portal_session_info(html);
    assert_eq!(session.campus_card_number.as_deref(), Some("001234"));
    assert_eq!(session.online_username.as_deref(), Some("网关姓名"));
    assert!(session.charset.is_none());
}

#[test]
fn decodes_js_string_escapes_without_evaluation() {
    let session = extract_portal_session_info(
        r#"<script>uid='\x36\x35\x34\x33\x32\x31';NID="\u5f20\u4e09\"\\\uD83D\uDE00\u{20bb7}";</script>"#,
    );
    assert_eq!(session.campus_card_number.as_deref(), Some("654321"));
    assert_eq!(session.online_username.as_deref(), Some("张三\"\\😀𠮷"));
    assert_eq!(
        extract_portal_value("<script>NID='王\\'同学';</script>", "NID").as_deref(),
        Some("王'同学")
    );
    assert!(extract_portal_value(r#"<script>NID='\uD800';</script>"#, "NID").is_none());
    assert!(extract_portal_value("<script>NID='张' + lookup();</script>", "NID").is_none());
}

#[test]
fn missing_fields_are_independent_and_uid_is_not_an_online_marker() {
    let html = "<script>uid='001234';</script>";
    let session = extract_portal_session_info(html);
    assert_eq!(session.campus_card_number.as_deref(), Some("001234"));
    assert!(session.online_username.is_none());
    assert!(!portal_probe_from_html(html, false).authenticated);
    assert!(portal_probe_from_html(html, false).session.is_none());
    let confirmed = portal_probe_from_html(html, true);
    assert!(confirmed.authenticated);
    assert!(confirmed.session.unwrap().campus_card_number.is_some());
    let logged_out = format!("{html}<!--Dr.COMWebLoginID_0.htm-->");
    assert!(!portal_probe_from_html(&logged_out, true).authenticated);
    assert!(extract_portal_value("<script>NID='  ';</script>", "NID").is_none());
}

#[test]
fn publishes_both_identity_fields_only_as_online_session_data() {
    let session = extract_portal_session_info(ONLINE);
    let payload = serde_json::to_value(&session).unwrap();
    assert_eq!(payload["campusCardNumber"], "654321");
    assert_eq!(payload["onlineUsername"], "张三");
    assert!(payload.get("password").is_none());
    let snapshot = NetworkSnapshot::default();
    assert!(classify_network_overview(
        &NetworkObservation {
            snapshot,
            dorm: DormPortalProbe {
                recognized: true,
                reachable: true,
                state: PortalOnlineState::Offline,
                session: Some(session.clone()),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default()
    )
    .portal_session
    .is_none());
    let staff = NetworkSnapshot {
        ssid: Some("SZU_WLAN".into()),
        ..Default::default()
    };
    let overview = classify_network_overview(
        &NetworkObservation {
            snapshot: staff,
            dorm: DormPortalProbe {
                recognized: true,
                reachable: true,
                state: PortalOnlineState::Online,
                session: Some(session),
                ..Default::default()
            },
            ..Default::default()
        },
        &mut RegionEvidence::default(),
    );
    assert_eq!(overview.region, NetworkRegion::Staff);
    assert!(overview.portal_session.is_none());
}

#[test]
#[ignore = "Read-only check requiring an already authenticated dormitory network"]
fn live_portal_read_only_identity_check() {
    use crate::campus_network::{http::read_http_response_bytes, network::DORM_PORTAL_HOST};
    // Only GET /; never read credentials, start a monitor, or submit a login.
    let response =
        read_http_response_bytes(DORM_PORTAL_HOST, 80, "/").expect("portal must be reachable");
    assert_eq!(response.status, 200);
    let html = decode_portal_html(&response.body, response.content_type.as_deref());
    assert!(
        has_online_portal_marker(&html),
        "portal must already be authenticated"
    );
    let session = extract_portal_session_info(&html);
    assert!(
        session
            .campus_card_number
            .as_deref()
            .is_some_and(|card| card.len() == 6 && card.bytes().all(|byte| byte.is_ascii_digit())),
        "portal must provide uid"
    );
    assert!(
        session
            .online_username
            .as_deref()
            .is_some_and(|name| !name.is_empty() && !name.contains('\u{fffd}')),
        "NID must decode without replacement characters"
    );
    // Compare with an independent explicit decoder for the known live GBK gateway.
    let expected = extract_portal_session_info(&encoding_rs::GBK.decode(&response.body).0);
    assert!(
        session.online_username == expected.online_username,
        "NID must match the gateway's GBK text"
    );
    assert!(
        session.campus_card_number == expected.campus_card_number,
        "uid must match the gateway text"
    );
}
