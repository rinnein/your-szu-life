use super::{
    model::is_student_id,
    probe::{
        auth_probe_succeeded, is_auth_login_url, is_auth_profile_url, is_authserver_url,
        probe_identity,
    },
    session::{AuthSessionSecret, StoredAuthCookie},
    AUTH_HOST,
};
use reqwest::{blocking::Client, cookie::Jar, redirect::Policy, Url};
use std::{io::Read, sync::Arc, time::Duration};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

const USER_CONF_URL: &str = "https://authserver.szu.edu.cn/personalInfo/common/getUserConf";
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

#[derive(Debug, Clone)]
struct ResponseCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    http_only: bool,
    secure: bool,
    expires: Option<String>,
    same_site: Option<String>,
    partitioned: Option<bool>,
}

pub(super) fn refresh_session(
    session: Option<AuthSessionSecret>,
    expected_student_id: &str,
) -> RefreshOutcome {
    if !is_student_id(expected_student_id) {
        return RefreshOutcome::Unavailable;
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

    let response_cookies = response.cookies().map(response_cookie).collect::<Vec<_>>();
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
                    && stored.path == cookie.path)
            });
            continue;
        }
        if let Some(existing) = existing {
            existing.value = cookie.value;
            existing.expires = cookie.expires;
            existing.http_only = Some(cookie.http_only);
            existing.secure = Some(cookie.secure);
            existing.same_site = cookie.same_site;
            existing.partitioned = cookie.partitioned;
        } else {
            session.cookies.push(StoredAuthCookie {
                name: cookie.name,
                value: cookie.value,
                domain: cookie.domain,
                path: cookie.path,
                expires: cookie.expires,
                http_only: Some(cookie.http_only),
                secure: Some(cookie.secure),
                same_site: cookie.same_site,
                partitioned: cookie.partitioned,
            });
        }
    }
}

fn response_cookie(cookie: reqwest::cookie::Cookie<'_>) -> ResponseCookie {
    let expires = cookie
        .expires()
        .map(OffsetDateTime::from)
        .and_then(|expires| expires.format(&Rfc2822).ok());
    let same_site = if cookie.same_site_lax() {
        Some("Lax".to_owned())
    } else if cookie.same_site_strict() {
        Some("Strict".to_owned())
    } else {
        None
    };
    ResponseCookie {
        name: cookie.name().to_owned(),
        value: cookie.value().to_owned(),
        domain: cookie.domain().unwrap_or(AUTH_HOST).to_ascii_lowercase(),
        path: cookie.path().unwrap_or("/").to_owned(),
        http_only: cookie.http_only(),
        secure: cookie.secure(),
        expires,
        same_site,
        partitioned: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_student_ids_without_network_access() {
        assert_eq!(refresh_session(None, "bad"), RefreshOutcome::Unavailable);
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
            student_id: "1234567890".into(),
            cookies: vec![StoredAuthCookie {
                name: "sid".into(),
                value: "old".into(),
                domain: AUTH_HOST.into(),
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
                    path: "/".into(),
                    http_only: true,
                    secure: true,
                    expires: Some("Fri, 13 Sep 2030 00:00:00 +0000".into()),
                    same_site: Some("Lax".into()),
                    partitioned: None,
                },
                ResponseCookie {
                    name: "evil".into(),
                    value: "ignored".into(),
                    domain: "example.com".into(),
                    path: "/".into(),
                    http_only: false,
                    secure: true,
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
