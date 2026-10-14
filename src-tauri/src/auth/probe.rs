use super::{
    model::is_student_id, AUTH_HOST, AUTH_LOGIN_URL, AUTH_MOBILE_PROFILE_URL, AUTH_PROFILE_URL,
};

pub(super) fn authentication_url(user_agent: Option<&str>) -> tauri::Url {
    let ua = user_agent.unwrap_or_default().to_ascii_lowercase();
    let mobile = cfg!(target_os = "android")
        || ["android", "iphone", "ipad", "ipod", "mobile"]
            .iter()
            .any(|marker| ua.contains(marker));
    let profile = if mobile {
        AUTH_MOBILE_PROFILE_URL
    } else {
        AUTH_PROFILE_URL
    };
    let mut url = tauri::Url::parse(AUTH_LOGIN_URL).expect("fixed official login URL");
    url.query_pairs_mut().append_pair("service", profile);
    url
}

pub(super) fn is_auth_login_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    let path = url.path().trim_end_matches('/');
    url.scheme() == "https"
        && url.host_str() == Some(AUTH_HOST)
        && url.port_or_known_default() == Some(443)
        && (path == "/authserver/login" || path.starts_with("/authserver/login;"))
}

pub(super) fn is_authserver_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    url.scheme() == "https" && url.host_str() == Some(AUTH_HOST)
}

pub(super) fn is_auth_profile_url(raw: &str) -> bool {
    let Ok(url) = raw.parse::<tauri::Url>() else {
        return false;
    };
    let path = url.path().trim_end_matches('/');
    is_authserver_url(raw) && (path == "/personalInfo" || path.starts_with("/personalInfo/"))
}

pub(super) fn decode_callback_payload(payload: &str) -> Option<serde_json::Value> {
    let mut parsed = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    if let Some(encoded) = parsed.as_str() {
        parsed = serde_json::from_str(encoded).ok()?;
    }
    Some(parsed)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedUserIdentity {
    pub(crate) student_id: String,
    pub(crate) name: Option<String>,
}

pub(super) fn normalized_user_name(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim();
        (!value.is_empty() && value.chars().count() <= 80).then(|| value.to_owned())
    })
}

pub(super) fn probe_identity(payload: &str) -> Option<VerifiedUserIdentity> {
    let parsed = decode_callback_payload(payload)?;
    let text = parsed.get("text")?.as_str()?;
    let user_conf = serde_json::from_str::<serde_json::Value>(text).ok()?;
    let student_id = ["/datas/uid", "/data/uid"].into_iter().find_map(|path| {
        user_conf
            .pointer(path)
            .and_then(|value| value.as_str())
            .filter(|id| is_student_id(id))
            .map(str::to_owned)
    })?;

    // SZU's current response exposes the name as `res.data.cn`. Keep that
    // exact field first, while accepting the known compatibility aliases. Do
    // not recursively scan or persist the raw official response.
    let name = [
        "/data/cn",
        "/datas/cn",
        "/datas/name",
        "/datas/userName",
        "/datas/username",
        "/datas/user_name",
        "/datas/realName",
        "/datas/real_name",
        "/datas/xm",
        "/datas/user/name",
        "/datas/user/userName",
        "/datas/user/realName",
        "/datas/userInfo/name",
        "/datas/userInfo/userName",
        "/datas/userInfo/realName",
        "/datas/userInfo/xm",
    ]
    .into_iter()
    .find_map(|path| {
        normalized_user_name(user_conf.pointer(path).and_then(|value| value.as_str()))
    });

    Some(VerifiedUserIdentity { student_id, name })
}

pub(super) fn auth_probe_reached_verified_endpoint(payload: &str) -> bool {
    let Some(parsed) = decode_callback_payload(payload) else {
        return false;
    };
    parsed
        .get("ok")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        && parsed
            .get("status")
            .and_then(|value| value.as_u64())
            .is_some_and(|status| (200..300).contains(&status))
        && parsed
            .get("url")
            .and_then(|value| value.as_str())
            .is_some_and(is_auth_profile_url)
}

pub(super) fn auth_probe_succeeded(payload: &str) -> bool {
    auth_probe_reached_verified_endpoint(payload) && probe_identity(payload).is_some()
}
