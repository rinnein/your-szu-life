mod document;
mod script;

use super::model::DormPortalSessionInfo;

pub(super) fn decode_portal_html(bytes: &[u8], content_type: Option<&str>) -> String {
    // Charset sniffing needs only ASCII markup; never replace the original body before decoding it.
    let ascii: String = bytes
        .iter()
        .map(|byte| if byte.is_ascii() { *byte as char } else { ' ' })
        .collect();
    let encoding = content_type
        .and_then(document::content_type_charset)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .or_else(|| {
            document::meta_charset(&ascii)
                .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        });
    if let Some(encoding) = encoding {
        return encoding.decode(bytes).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(utf8) => utf8.to_owned(),
        Err(_) => encoding_rs::GB18030.decode(bytes).0.into_owned(),
    }
}

pub(super) fn extract_portal_value(html: &str, name: &str) -> Option<String> {
    script::values(html).get(name).and_then(script::Value::text)
}

pub(super) fn extract_portal_session_info(html: &str) -> DormPortalSessionInfo {
    let values = script::values(html);
    let text = |name| values.get(name).and_then(script::Value::text);
    let number = |name| values.get(name).and_then(script::Value::number);
    DormPortalSessionInfo {
        online_username: text("NID"),
        campus_card_number: text("uid"),
        online_duration: number("oltime"),
        online_flow: number("olflow"),
        login_ip: text("lip"),
        login_started_at: text("stime"),
        login_expires_at: text("etime"),
        charset: document::meta_charset(html),
    }
}

pub(super) fn has_online_portal_marker(html: &str) -> bool {
    html.contains("Dr.COMWebLoginID_1.htm")
}
pub(super) fn has_login_portal_marker(html: &str) -> bool {
    html.contains("Dr.COMWebLoginID_0.htm")
}
pub(super) fn has_online_session_fields(session: &DormPortalSessionInfo) -> bool {
    session.online_username.is_some()
        || session.campus_card_number.is_some()
        || session.login_ip.is_some()
        || session.login_started_at.is_some()
        || session.login_expires_at.is_some()
}

pub(super) fn valid_ipv4(value: &str) -> bool {
    let parts: Vec<_> = value.trim().split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| part.parse::<u8>().is_ok())
        && value != "000.000.000.000"
        && value != "0.0.0.0"
}

pub(super) fn extract_terminal_ip(html: &str) -> Option<String> {
    ["v4ip", "v46ip", "ss5"]
        .into_iter()
        .filter_map(|key| extract_portal_value(html, key))
        .find(|value| valid_ipv4(value))
}

#[cfg(test)]
pub(super) fn parse_jsonp_response(body: &str) -> Result<serde_json::Value, String> {
    let body = body.trim().trim_start_matches('﻿');
    let (callback, _) = body.split_once('(').ok_or("门户响应不是 JSONP")?;
    parse_jsonp_callback(body, callback.trim())
}

pub(super) fn parse_jsonp_callback(
    body: &str,
    callback: &str,
) -> Result<serde_json::Value, String> {
    if callback.is_empty()
        || !callback.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic()
                || matches!(byte, b'_' | b'$')
                || (index > 0 && byte.is_ascii_digit())
        })
    {
        return Err("门户回调名称无效".into());
    }
    let body = body.trim().trim_start_matches('﻿').trim();
    let body = body.strip_suffix(';').unwrap_or(body).trim_end();
    let json = body
        .strip_prefix(callback)
        .and_then(|rest| rest.trim_start().strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or("门户回调不匹配或包含额外内容")?;
    serde_json::from_str(json.trim()).map_err(|_| "门户 JSON 数据无效".into())
}

pub(super) fn meta_refresh_target(html: &str) -> Option<String> {
    document::meta_refresh_target(html)
}

pub(super) fn config_literals(
    html: &str,
) -> Option<std::collections::HashMap<String, serde_json::Value>> {
    Some(
        script::object_values(html, "CONFIG")?
            .into_iter()
            .map(|(key, value)| {
                let value = match value {
                    script::Value::Text(text) => serde_json::Value::String(text),
                    script::Value::Number(number) => serde_json::Value::from(number),
                    script::Value::Bool(value) => serde_json::Value::Bool(value),
                };
                (key, value)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests;
