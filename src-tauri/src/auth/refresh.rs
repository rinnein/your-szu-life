use super::{
    model::is_student_id,
    probe::{
        auth_probe_succeeded, is_auth_login_url, is_auth_profile_url, is_authserver_url,
        probe_identity,
    },
    session::SessionRestoreError,
    session::{AuthSessionSecret, StoredAuthCookie},
};
use reqwest::{blocking::Client, cookie::Jar, redirect::Policy, Url};
use std::{io::Read, sync::Arc, time::Duration};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

const USER_CONF_URL: &str = super::AUTH_USER_CONF_URL;
const MAX_RESPONSE_BYTES: u64 = 512 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RefreshOutcome {
    Refreshed(AuthSessionSecret),
    Invalid,
    Mismatch,
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
enum ResponseDisposition {
    Valid,
    Invalid,
    Unavailable,
}

type ResponseCookie = StoredAuthCookie;

pub(super) fn refresh_session(
    session: Option<AuthSessionSecret>,
    expected_student_id: &str,
) -> RefreshOutcome {
    if !is_student_id(expected_student_id) {
        return RefreshOutcome::Unavailable;
    }
    let session = match session
        .as_ref()
        .map(|session| session.restorable_snapshot(expected_student_id, OffsetDateTime::now_utc()))
        .transpose()
    {
        Ok(session) => session.flatten(),
        Err(SessionRestoreError::AccountMismatch) => return RefreshOutcome::Mismatch,
        Err(SessionRestoreError::Unsupported) => return RefreshOutcome::Invalid,
        Err(SessionRestoreError::InvalidCookie) => return RefreshOutcome::Unavailable,
    };
    let endpoint = match USER_CONF_URL.parse::<Url>() {
        Ok(url) => url,
        Err(_) => return RefreshOutcome::Unavailable,
    };
    #[cfg(target_os = "android")]
    if crate::platform_tls::initialize_platform_verifier().is_err() {
        return RefreshOutcome::Unavailable;
    }
    let jar = Arc::new(Jar::default());
    if let Some(existing) = session.as_ref() {
        for cookie in &existing.cookies {
            if cookie.is_definitely_expired(OffsetDateTime::now_utc()) {
                continue;
            }
            let Ok(serialized) = cookie.to_webview_cookie() else {
                continue;
            };
            jar.add_cookie_str(&serialized.to_string(), &endpoint);
        }
    }
    let client = match Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .redirect(Policy::none())
        .retry(reqwest::retry::never())
        .cookie_provider(jar.clone())
        .build()
    {
        Ok(client) => client,
        Err(_) => return RefreshOutcome::Unavailable,
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    refresh_with_transport(session, expected_student_id, |url| {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(());
        }
        let response = client
            .get(url)
            .header("Accept", "application/json")
            .timeout(remaining.min(Duration::from_secs(8)))
            .send()
            .map_err(|_| ())?;
        let status = response.status().as_u16();
        let url = response.url().clone();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| url.join(value).ok());
        let updates = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|header| header.to_str().ok())
            .filter_map(|header| response_cookie(header, &url))
            .collect();
        let mut body = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|_| ())?;
        if body.len() > MAX_RESPONSE_BYTES as usize {
            return Err(());
        }
        Ok(AuthResponse {
            status,
            url,
            location,
            updates,
            text: String::from_utf8(body).map_err(|_| ())?,
        })
    })
}

struct AuthResponse {
    status: u16,
    url: Url,
    location: Option<Url>,
    updates: Vec<ResponseCookie>,
    text: String,
}

fn verify_response(response: &AuthResponse, expected: &str) -> RefreshOutcome {
    match classify_response(response.status, &response.url, response.location.as_ref()) {
        ResponseDisposition::Invalid => return RefreshOutcome::Invalid,
        ResponseDisposition::Unavailable => return RefreshOutcome::Unavailable,
        ResponseDisposition::Valid => {}
    }
    let payload = serde_json::json!({ "ok": true, "status": response.status, "url": response.url.as_str(), "text": response.text }).to_string();
    if !auth_probe_succeeded(&payload) {
        return RefreshOutcome::Unavailable;
    }
    if probe_identity(&payload).is_none_or(|identity| identity.student_id != expected) {
        return RefreshOutcome::Mismatch;
    }
    RefreshOutcome::Refreshed(AuthSessionSecret {
        student_id: expected.into(),
        format_version: 2,
        cookies: Vec::new(),
    })
}

fn refresh_with_transport(
    session: Option<AuthSessionSecret>,
    expected: &str,
    mut request: impl FnMut(Url) -> Result<AuthResponse, ()>,
) -> RefreshOutcome {
    let can_resume_sso = session
        .as_ref()
        .is_some_and(|session| !session.cookies.is_empty());
    let mut stored = session.unwrap_or_else(|| AuthSessionSecret {
        student_id: expected.into(),
        format_version: 2,
        cookies: Vec::new(),
    });
    let endpoint: Url = USER_CONF_URL.parse().expect("official endpoint");
    let Ok(mut first) = request(endpoint.clone()) else {
        return RefreshOutcome::Unavailable;
    };
    let outcome = verify_response(&first, expected);
    merge_response_cookies(&mut stored, std::mem::take(&mut first.updates));
    match outcome {
        RefreshOutcome::Refreshed(_) => {
            return if stored.cookies.is_empty() {
                RefreshOutcome::Unavailable
            } else {
                RefreshOutcome::Refreshed(stored)
            }
        }
        RefreshOutcome::Invalid if can_resume_sso => {}
        outcome => return outcome,
    }
    // An expired personalInfo session is not proof that the CAS login is gone.
    // Let CASTGC/JSESSIONID establish a new service session without credentials.
    let mut url = super::probe::authentication_url(None);
    for _ in 0..8 {
        let Ok(mut response) = request(url) else {
            return RefreshOutcome::Unavailable;
        };
        merge_response_cookies(&mut stored, std::mem::take(&mut response.updates));
        if (300..400).contains(&response.status) {
            let Some(next) = response.location else {
                return RefreshOutcome::Unavailable;
            };
            if !is_trusted_sso_url(&next) {
                return RefreshOutcome::Unavailable;
            }
            url = next;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return RefreshOutcome::Unavailable;
        }
        if is_auth_login_url(response.url.as_str()) {
            // Only the actual official login form warrants dropping the session.
            return if response.text.contains("login-main") && response.text.contains("pwdFromId") {
                RefreshOutcome::Invalid
            } else {
                RefreshOutcome::Unavailable
            };
        }
        if !is_auth_profile_url(response.url.as_str()) {
            return RefreshOutcome::Unavailable;
        }
        let Ok(mut verified) = request(endpoint) else {
            return RefreshOutcome::Unavailable;
        };
        let outcome = verify_response(&verified, expected);
        merge_response_cookies(&mut stored, std::mem::take(&mut verified.updates));
        return match outcome {
            RefreshOutcome::Refreshed(_) => {
                if stored.cookies.is_empty() {
                    RefreshOutcome::Unavailable
                } else {
                    RefreshOutcome::Refreshed(stored)
                }
            }
            // A redirect loop/transient service failure must not erase CASTGC.
            RefreshOutcome::Invalid => RefreshOutcome::Unavailable,
            other => other,
        };
    }
    RefreshOutcome::Unavailable
}

fn is_trusted_sso_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str() == Some(super::AUTH_HOST)
        && url.port_or_known_default() == Some(443)
        && (is_auth_login_url(url.as_str()) || is_auth_profile_url(url.as_str()))
}

fn classify_response(
    status: u16,
    response_url: &Url,
    location: Option<&Url>,
) -> ResponseDisposition {
    if location.is_some_and(|url| is_auth_login_url(url.as_str())) {
        return ResponseDisposition::Invalid;
    }
    if (status == 401 || status == 403) && is_authserver_url(response_url.as_str()) {
        return ResponseDisposition::Invalid;
    }
    if !(200..300).contains(&status)
        || !is_auth_profile_url(response_url.as_str())
        || location.is_some()
    {
        return ResponseDisposition::Unavailable;
    }
    ResponseDisposition::Valid
}

fn merge_response_cookies(session: &mut AuthSessionSecret, response: Vec<ResponseCookie>) {
    for cookie in response {
        if !super::session::is_auth_cookie_domain(&cookie.domain)
            || !super::session::is_valid_cookie_path(&cookie.path)
        {
            continue;
        }
        let existing = session.cookies.iter_mut().find(|stored| {
            stored.name == cookie.name
                && stored.domain == cookie.domain
                && stored.path == cookie.path
                && stored.host_only == cookie.host_only
        });
        let expired = cookie
            .expires
            .as_deref()
            .and_then(|value| OffsetDateTime::parse(value, &Rfc2822).ok())
            .is_some_and(|expires| expires <= OffsetDateTime::now_utc());
        if cookie.value.is_empty() || expired {
            session.cookies.retain(|stored| {
                !(stored.name == cookie.name
                    && stored.domain == cookie.domain
                    && stored.path == cookie.path
                    && stored.host_only == cookie.host_only)
            });
            continue;
        }
        if let Some(existing) = existing {
            existing.value = cookie.value;
            existing.expires = cookie.expires;
            existing.http_only = cookie.http_only;
            existing.secure = cookie.secure;
            existing.same_site = cookie.same_site;
            existing.partitioned = cookie.partitioned;
        } else {
            session.cookies.push(StoredAuthCookie {
                name: cookie.name,
                value: cookie.value,
                domain: cookie.domain,
                host_only: cookie.host_only,
                path: cookie.path,
                expires: cookie.expires,
                http_only: cookie.http_only,
                secure: cookie.secure,
                same_site: cookie.same_site,
                partitioned: cookie.partitioned,
            });
        }
    }
}

fn response_cookie(raw: &str, response_url: &Url) -> Option<ResponseCookie> {
    let cookie = tauri::webview::cookie::Cookie::parse(raw.to_owned()).ok()?;
    let mut stored = StoredAuthCookie::from_webview_cookie(cookie.clone().into_owned())?;
    if cookie.path().is_none_or(|path| !path.starts_with('/')) {
        // RFC 6265 default-path is the request's directory, not always '/'.
        stored.path = response_url
            .path()
            .rsplit_once('/')
            .map(|(directory, _)| if directory.is_empty() { "/" } else { directory })
            .unwrap_or("/")
            .to_owned();
    }
    stored.to_webview_cookie().ok()?;
    Some(stored)
}

#[cfg(test)]
mod tests {
    use super::super::AUTH_HOST;
    use super::*;

    fn fixture_session() -> AuthSessionSecret {
        AuthSessionSecret {
            format_version: 3,
            student_id: "1234567890".into(),
            cookies: vec![StoredAuthCookie {
                name: "CASTGC".into(),
                value: "test-only-cas-session".into(),
                domain: AUTH_HOST.into(),
                host_only: true,
                path: "/authserver".into(),
                expires: None,
                http_only: Some(true),
                secure: Some(true),
                same_site: None,
                partitioned: None,
            }],
        }
    }
    fn reply(url: Url, status: u16, redirect: Option<&str>, text: &str) -> AuthResponse {
        AuthResponse {
            status,
            location: redirect.map(|location| url.join(location).unwrap()),
            url,
            updates: Vec::new(),
            text: text.into(),
        }
    }

    #[test]
    fn valid_saved_session_never_visits_login() {
        let mut requests = 0;
        let result = refresh_with_transport(Some(fixture_session()), "1234567890", |url| {
            requests += 1;
            assert_eq!(url.as_str(), USER_CONF_URL);
            Ok(reply(url, 200, None, r#"{"datas":{"uid":"1234567890"}}"#))
        });
        assert!(matches!(result, RefreshOutcome::Refreshed(_)));
        assert_eq!(requests, 1);
    }

    #[test]
    fn expired_service_session_resumes_cas_without_a_password() {
        let mut paths = Vec::new();
        let result = refresh_with_transport(Some(fixture_session()), "1234567890", |url| {
            paths.push(url.path().to_owned());
            Ok(match paths.len() {
                1 => reply(url, 302, Some(super::super::AUTH_LOGIN_URL), ""),
                2 => {
                    let mut result = reply(url, 302, Some(super::super::AUTH_PROFILE_URL), "");
                    result.updates.push(
                        response_cookie(
                            "MOD_AUTH_CAS=renewed-test-only; Path=/; HttpOnly",
                            &result.url,
                        )
                        .unwrap(),
                    );
                    result
                }
                3 => reply(url, 200, None, "<html>profile</html>"),
                4 => reply(url, 200, None, r#"{"datas":{"uid":"1234567890"}}"#),
                _ => panic!("unbounded SSO requests"),
            })
        });
        let RefreshOutcome::Refreshed(session) = result else {
            panic!("CAS restoration failed");
        };
        assert_eq!(paths.len(), 4);
        assert!(session.cookies.iter().any(|cookie| cookie.name == "CASTGC"));
        assert!(session
            .cookies
            .iter()
            .any(|cookie| cookie.name == "MOD_AUTH_CAS" && cookie.host_only));
    }

    #[test]
    fn only_a_real_login_form_discards_the_old_session() {
        for (status, text, invalid) in [
            (
                200,
                "<div class='login-main'><form id='pwdFromId'></form></div>",
                true,
            ),
            (200, "proxy error", false),
            (503, "unavailable", false),
        ] {
            let mut step = 0;
            let result = refresh_with_transport(Some(fixture_session()), "1234567890", |url| {
                step += 1;
                Ok(if step == 1 {
                    reply(url, 302, Some(super::super::AUTH_LOGIN_URL), "")
                } else {
                    reply(url, status, None, text)
                })
            });
            assert_eq!(matches!(result, RefreshOutcome::Invalid), invalid);
            assert_eq!(step, 2);
        }
    }

    #[test]
    fn sso_does_not_follow_external_redirects_or_loop_forever() {
        for external in [true, false] {
            let mut calls = 0;
            let result = refresh_with_transport(Some(fixture_session()), "1234567890", |url| {
                calls += 1;
                Ok(reply(
                    url,
                    302,
                    Some(if external && calls > 1 {
                        "https://evil.example/"
                    } else {
                        super::super::AUTH_LOGIN_URL
                    }),
                    "",
                ))
            });
            assert_eq!(result, RefreshOutcome::Unavailable);
            assert_eq!(calls, if external { 2 } else { 9 });
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Read-only live session diagnostic; requires explicit user authorization"]
    fn verify_saved_session_without_writing_credentials() {
        assert!(
            std::env::var("SZU_READ_SAVED_SESSION").as_deref() == Ok("1"),
            "set SZU_READ_SAVED_SESSION=1 to explicitly enable this live diagnostic"
        );
        let session =
            crate::storage::read_saved_session_for_diagnostic().expect("saved session unavailable");
        let expected = session.student_id.clone();
        let outcome = refresh_session(Some(session), &expected);
        eprintln!(
            "Saved-session result: {}",
            match outcome {
                RefreshOutcome::Refreshed(_) => "accepted",
                RefreshOutcome::Invalid => "reauthentication requested",
                RefreshOutcome::Mismatch => "identity mismatch",
                RefreshOutcome::Unavailable => "temporarily unavailable",
            }
        );
        assert!(
            matches!(outcome, RefreshOutcome::Refreshed(_)),
            "saved session was not accepted by this transport"
        );
    }

    #[test]
    fn rejects_invalid_student_ids_without_network_access() {
        assert_eq!(refresh_session(None, "bad"), RefreshOutcome::Unavailable);
    }

    #[test]
    fn refresh_preserves_host_scope_and_http_cookie_defaults() {
        let endpoint = Url::parse(USER_CONF_URL).unwrap();
        let host = response_cookie("MOD_AUTH_CAS=current; Path=/; HttpOnly", &endpoint).unwrap();
        assert!(host.host_only);
        assert!(host.to_webview_cookie().unwrap().domain().is_none());
        // In an HTTP response, any explicit Domain is a domain cookie, with or
        // without a dot. AndroidX's canonical-domain representation differs.
        let domain = response_cookie(
            "MOD_AUTH_CAS=domain; Domain=authserver.szu.edu.cn; Path=/",
            &endpoint,
        )
        .unwrap();
        assert!(!domain.host_only);
        let default_path = response_cookie("sid=test; SameSite=None; Secure", &endpoint).unwrap();
        assert_eq!(default_path.path, "/personalInfo/common");
        assert_eq!(default_path.same_site.as_deref(), Some("None"));
        let deletion = response_cookie("sid=test; Max-Age=0", &endpoint).unwrap();
        assert!(deletion.is_definitely_expired(OffsetDateTime::now_utc()));
    }

    #[test]
    fn refreshed_host_cookie_and_deletion_do_not_overwrite_a_domain_cookie() {
        let endpoint = Url::parse(USER_CONF_URL).unwrap();
        let host = response_cookie("MOD_AUTH_CAS=old; Path=/; HttpOnly", &endpoint).unwrap();
        let domain = response_cookie(
            "MOD_AUTH_CAS=domain; Domain=authserver.szu.edu.cn; Path=/",
            &endpoint,
        )
        .unwrap();
        let mut session = AuthSessionSecret {
            format_version: 2,
            student_id: "1234567890".into(),
            cookies: vec![host, domain.clone()],
        };
        merge_response_cookies(
            &mut session,
            vec![response_cookie("MOD_AUTH_CAS=new; Path=/; HttpOnly", &endpoint).unwrap()],
        );
        assert_eq!(session.cookies.len(), 2);
        assert_eq!(session.cookies[0].value, "new");
        assert_eq!(session.cookies[1], domain);
        merge_response_cookies(
            &mut session,
            vec![response_cookie("MOD_AUTH_CAS=; Path=/; Max-Age=0", &endpoint).unwrap()],
        );
        assert_eq!(session.cookies, vec![domain]);
    }

    #[test]
    fn classifies_auth_failure_without_treating_portal_redirects_as_logout() {
        let profile = Url::parse(USER_CONF_URL).unwrap();
        let login = Url::parse("https://authserver.szu.edu.cn/authserver/login").unwrap();
        let portal = Url::parse("http://172.30.255.42/801/eportal").unwrap();
        assert_eq!(
            classify_response(302, &profile, Some(&login)),
            ResponseDisposition::Invalid
        );
        assert_eq!(
            classify_response(302, &profile, Some(&portal)),
            ResponseDisposition::Unavailable
        );
        assert_eq!(
            classify_response(503, &profile, None),
            ResponseDisposition::Unavailable
        );
        assert_eq!(
            classify_response(200, &profile, None),
            ResponseDisposition::Valid
        );
    }

    #[test]
    fn response_cookies_update_and_remove_only_allowed_session_entries() {
        let mut session = AuthSessionSecret {
            format_version: 2,
            student_id: "1234567890".into(),
            cookies: vec![StoredAuthCookie {
                name: "sid".into(),
                value: "old".into(),
                domain: AUTH_HOST.into(),
                host_only: true,
                path: "/".into(),
                expires: None,
                http_only: Some(true),
                secure: Some(true),
                same_site: None,
                partitioned: None,
            }],
        };
        merge_response_cookies(
            &mut session,
            vec![
                ResponseCookie {
                    name: "sid".into(),
                    value: "new".into(),
                    domain: AUTH_HOST.into(),
                    host_only: true,
                    path: "/".into(),
                    http_only: Some(true),
                    secure: Some(true),
                    expires: Some("Fri, 13 Sep 2030 00:00:00 +0000".into()),
                    same_site: Some("Lax".into()),
                    partitioned: None,
                },
                ResponseCookie {
                    name: "evil".into(),
                    value: "ignored".into(),
                    domain: "example.com".into(),
                    host_only: false,
                    path: "/".into(),
                    http_only: Some(false),
                    secure: Some(true),
                    expires: None,
                    same_site: None,
                    partitioned: None,
                },
            ],
        );
        assert_eq!(session.cookies.len(), 1);
        assert_eq!(session.cookies[0].value, "new");
        assert_eq!(
            session.cookies[0].expires.as_deref(),
            Some("Fri, 13 Sep 2030 00:00:00 +0000")
        );
        assert_eq!(session.cookies[0].same_site.as_deref(), Some("Lax"));
    }
}
