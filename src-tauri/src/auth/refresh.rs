use super::{
    model::is_student_id,
    probe::{
        auth_probe_succeeded, is_auth_login_url, is_auth_profile_url, is_authserver_url,
        probe_identity,
    },
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
    if session
        .as_ref()
        .is_some_and(|session| !session.can_restore())
    {
        return RefreshOutcome::Invalid;
    }
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
    let response = match client
        .get(endpoint.clone())
        .header("Accept", "application/json")
        .send()
    {
        Ok(response) => response,
        Err(_) => return RefreshOutcome::Unavailable,
    };

    let status = response.status().as_u16();
    let response_url = response.url().clone();
    let location = response
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| response_url.join(value).ok());
    match classify_response(status, &response_url, location.as_ref()) {
        ResponseDisposition::Valid => {}
        ResponseDisposition::Invalid => return RefreshOutcome::Invalid,
        ResponseDisposition::Unavailable => return RefreshOutcome::Unavailable,
    }

    let response_cookies = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|header| header.to_str().ok())
        .filter_map(|header| response_cookie(header, &response_url))
        .collect::<Vec<_>>();
    let mut body = Vec::new();
    if response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut body)
        .is_err()
        || body.len() > MAX_RESPONSE_BYTES as usize
    {
        return RefreshOutcome::Unavailable;
    }
    let text = match String::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return RefreshOutcome::Unavailable,
    };
    let payload = serde_json::json!({
        "ok": true,
        "status": status,
        "url": response_url.as_str(),
        "text": text,
    })
    .to_string();
    if !auth_probe_succeeded(&payload) {
        return RefreshOutcome::Unavailable;
    }
    let Some(identity) = probe_identity(&payload) else {
        return RefreshOutcome::Unavailable;
    };
    if identity.student_id != expected_student_id {
        return RefreshOutcome::Mismatch;
    }

    let mut refreshed = session.unwrap_or_else(|| AuthSessionSecret {
        format_version: 2,
        student_id: expected_student_id.to_owned(),
        cookies: Vec::new(),
    });
    merge_response_cookies(&mut refreshed, response_cookies);
    if refreshed.cookies.is_empty() {
        return RefreshOutcome::Unavailable;
    }
    refreshed.student_id = expected_student_id.to_owned();
    RefreshOutcome::Refreshed(refreshed)
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
