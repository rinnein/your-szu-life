use super::*;
use reqwest::cookie::CookieStore;

#[test]
fn cookies_are_operation_scoped_and_unchecked_preserves_unrelated_session_cookies() {
    let first = StaffSessionClient::new().unwrap();
    let second = StaffSessionClient::new().unwrap();
    let origin = crate::campus_network::staff::staff_url("/");
    first
        .jar
        .add_cookie_str("session=test-only; Path=/; Secure", &origin);
    first.remember(Some("dGVzdC1vbmx5")).unwrap();
    assert!(first
        .jar
        .cookies(&origin)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("remember=dGVzdC1vbmx5"));
    assert!(second.jar.cookies(&origin).is_none());
    first.remember(None).unwrap();
    assert_eq!(
        first.jar.cookies(&origin).unwrap().to_str().unwrap(),
        "session=test-only"
    );
    assert!(first
        .jar
        .cookies(&Url::parse("http://172.30.255.42/").unwrap())
        .is_none());
}
#[test]
fn staff_session_rejects_other_origins_before_sending_cookies() {
    let session = StaffSessionClient::new().unwrap();
    session.remember(Some("dGVzdC1vbmx5")).unwrap();
    for url in [
        "http://net.szu.edu.cn/",
        "https://net.szu.edu.cn:8800/",
        "https://evil.example/",
        "http://172.30.255.42/",
    ] {
        assert_eq!(
            session.get(&Url::parse(url).unwrap()).unwrap_err(),
            HttpError::UnsafeUrl
        );
    }
}
