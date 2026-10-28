//! Windows 系统代理探测：读注册表里 Internet Settings 的代理项。
//!
//! 与 macOS 同理，统一拦截器关掉了 reqwest 自带的系统代理探测，这里自己读。
//! 用 `reg query` 而不是直接 FFI：一是与 Linux 侧的 `gsettings` 保持一致，
//! 二是不用为了一次读取引入新的 windows-sys feature。

use crate::netproxy::system::parse_proxy_value;
use reqwest::Url;

#[cfg(target_os = "windows")]
const INTERNET_SETTINGS: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

#[cfg(target_os = "windows")]
pub(crate) fn system_proxy(scheme: &str) -> Option<Url> {
    let values = reg_values("ProxyEnable")?;
    if values.get("ProxyEnable").map(String::as_str) != Some("0x1") {
        return None;
    }
    // 只认静态代理：PAC（AutoConfigURL）需要执行脚本，这里不做。
    let server = reg_values("ProxyServer")?.get("ProxyServer").cloned()?;
    parse_windows_proxy(&server, scheme)
}

#[cfg(target_os = "windows")]
fn reg_values(key: &str) -> Option<std::collections::HashMap<String, String>> {
    let output = std::process::Command::new("reg")
        .args(["query", INTERNET_SETTINGS, "/v", key])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let parsed = parse_reg_output(&text);
    (!parsed.is_empty()).then_some(parsed)
}

/// `reg query` 的数据行形如 `    ProxyEnable    REG_DWORD    0x1`。
fn parse_reg_output(text: &str) -> std::collections::HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("HKEY_") {
                return None;
            }
            let mut parts = line.split_whitespace();
            let name = parts.next()?.to_owned();
            let kind = parts.next()?;
            if !kind.starts_with("REG_") {
                return None;
            }
            let value = parts.collect::<Vec<_>>().join(" ");
            Some((name, value))
        })
        .collect()
}

/// `ProxyServer` 可能是单地址，也可能是 `http=host:port;https=host:port;ftp=...`。
fn parse_windows_proxy(value: &str, scheme: &str) -> Option<Url> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if !value.contains('=') {
        return parse_proxy_value(value, "http");
    }
    let preferred = if scheme == "https" { "https" } else { "http" };
    let mut fallback = None;
    for entry in value.split(';') {
        let Some((name, address)) = entry.split_once('=') else {
            continue;
        };
        let parsed = parse_proxy_value(address.trim(), "http");
        if name.trim().eq_ignore_ascii_case(preferred) {
            return parsed;
        }
        if fallback.is_none() {
            fallback = parsed;
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_registry_line_shape() {
        let text = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    ProxyEnable    REG_DWORD    0x1\r\n";
        let values = parse_reg_output(text);
        assert_eq!(values.get("ProxyEnable").map(String::as_str), Some("0x1"));
    }

    #[test]
    fn parses_a_single_proxy_server() {
        assert_eq!(
            parse_windows_proxy("127.0.0.1:7890", "https").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
    }

    #[test]
    fn parses_a_per_scheme_proxy_list() {
        let value = "http=127.0.0.1:7890;https=127.0.0.1:7891";
        assert_eq!(
            parse_windows_proxy(value, "https").map(|url| url.to_string()),
            Some("http://127.0.0.1:7891/".into())
        );
        assert_eq!(
            parse_windows_proxy(value, "http").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
        // 只配了 ftp 时退回第一个可用项，而不是直接失败。
        assert_eq!(
            parse_windows_proxy("ftp=127.0.0.1:7890", "http").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
    }

    #[test]
    fn empty_values_are_rejected() {
        assert!(parse_windows_proxy("", "http").is_none());
        assert!(parse_windows_proxy("   ", "http").is_none());
    }
}