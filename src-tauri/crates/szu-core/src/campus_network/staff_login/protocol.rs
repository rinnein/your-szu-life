use crate::campus_network::staff::{staff_url, StaffPortalConfig};
use base64::{
    alphabet::Alphabet,
    engine::{general_purpose, GeneralPurpose},
    Engine,
};
use hmac::{Hmac, Mac};
use md5::Md5;
use reqwest::Url;
use serde::Serialize;
use sha1::{Digest, Sha1};

const SRUN_ALPHABET: &str = "LVoJPiCN2R8G90yg+hmFHuacZ1OWMnrsSTXkYpUq/3dlbfKwv6xztjI7DeBE45QA";

#[derive(Serialize)]
struct UserInfo<'a> {
    username: &'a str,
    password: &'a str,
    ip: &'a str,
    acid: &'a str,
    enc_ver: &'static str,
}
fn words(value: &str, with_length: bool) -> Vec<u32> {
    // Portal.js uses charCodeAt and JS string length, not UTF-8 bytes.
    let units: Vec<_> = value.encode_utf16().collect();
    let mut words = vec![0; units.len().div_ceil(4)];
    for (index, unit) in units.iter().enumerate() {
        words[index / 4] |= (*unit as u32) << ((index % 4) * 8);
    }
    if with_length {
        words.push(units.len() as u32);
    }
    words
}
pub(super) fn encode_info(
    username: &str,
    password: &str,
    config: &StaffPortalConfig,
    token: &str,
) -> String {
    let json = serde_json::to_string(&UserInfo {
        username,
        password,
        ip: &config.ip,
        acid: &config.acid,
        enc_ver: "srun_bx1",
    })
    .expect("string-only protocol object");
    let mut v = words(&json, true);
    let mut key = words(token, false);
    key.resize(key.len().max(4), 0);
    let last = v.len() - 1;
    let mut z = v[last];
    let mut sum = 0u32;
    for _ in 0..(6 + 52 / v.len()) {
        sum = sum.wrapping_add(0x9e3779b9);
        let e = ((sum >> 2) & 3) as usize;
        for p in 0..=last {
            let y = v[if p == last { 0 } else { p + 1 }];
            let m = ((z >> 5) ^ (y << 2))
                .wrapping_add((y >> 3) ^ (z << 4) ^ (sum ^ y))
                .wrapping_add(key[(p & 3) ^ e] ^ z);
            v[p] = v[p].wrapping_add(m);
            z = v[p];
        }
    }
    let bytes: Vec<u8> = v.into_iter().flat_map(u32::to_le_bytes).collect();
    let alphabet = Alphabet::new(SRUN_ALPHABET).expect("portal alphabet");
    format!(
        "{{SRBX1}}{}",
        GeneralPurpose::new(&alphabet, general_purpose::PAD).encode(bytes)
    )
}

pub(super) fn login_url(
    username: &str,
    password: &str,
    config: &StaffPortalConfig,
    nas_ip: &str,
    token: &str,
    now: u64,
) -> (Url, String) {
    // HMAC-MD5 is required by Srun's wire protocol, not used for stored passwords.
    let mut mac =
        Hmac::<Md5>::new_from_slice(token.as_bytes()).expect("HMAC accepts any key length");
    mac.update(password.as_bytes());
    let hmd5: String = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let info = encode_info(username, password, config, token);
    let checksum = format!(
        "{:x}",
        Sha1::digest(
            format!(
                "{token}{username}{token}{hmd5}{token}{}{token}{}{token}200{token}1{token}{info}",
                config.acid, config.ip
            )
            .as_bytes()
        )
    );
    let callback = format!("szuLifeLogin_{now}");
    let (os, name) = if cfg!(target_os = "windows") {
        ("Windows", "Windows")
    } else if cfg!(target_os = "macos") {
        ("Mac OS", "Mac OS")
    } else if cfg!(target_os = "android") {
        ("Android", "Android")
    } else {
        ("Linux", "Linux")
    };
    let mut url = staff_url("/cgi-bin/srun_portal");
    url.query_pairs_mut()
        .append_pair("callback", &callback)
        .append_pair("action", "login")
        .append_pair("username", username)
        .append_pair("password", &format!("{{MD5}}{hmd5}"))
        .append_pair("os", os)
        .append_pair("name", name)
        .append_pair("nas_ip", nas_ip)
        .append_pair("double_stack", "0")
        .append_pair("chksum", &checksum)
        .append_pair("info", &info)
        .append_pair("ac_id", &config.acid)
        .append_pair("ip", &config.ip)
        .append_pair("n", "200")
        .append_pair("type", "1")
        .append_pair("captchaId", "")
        .append_pair("captchaVal", "")
        .append_pair("_", &now.to_string());
    (url, callback)
}

pub(super) fn remember_value(username: &str, password: &str) -> Result<String, &'static str> {
    #[derive(Serialize)]
    struct Remember<'a> {
        username: &'a str,
        password: &'a str,
    }
    let json =
        serde_json::to_string(&Remember { username, password }).expect("string-only cookie object");
    // The deployed jquery-base64 encoder rejects UTF-16 code units above 255.
    let bytes: Result<Vec<u8>, _> = json.encode_utf16().map(u8::try_from).collect();
    let bytes =
        bytes.map_err(|_| "当前密码字符不兼容门户的记住密码功能，请选择应用自动登录模式")?;
    Ok(general_purpose::STANDARD.encode(bytes))
}

pub(super) fn credential_stamp(username: &str, password: &str) -> String {
    // Equality/version marker kept in the keyring; never emitted or used as an auth verifier.
    format!(
        "{:x}",
        Sha1::digest(serde_json::to_vec(&(username, password)).expect("strings"))
    )
}

#[cfg(test)]
mod tests;
