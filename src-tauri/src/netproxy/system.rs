//! 系统代理与环境变量代理的来源解析。
//!
//! 因为统一拦截器关掉了 reqwest 自己的代理探测，环境变量（`http_proxy` /
//! `https_proxy` / `all_proxy` / `no_proxy`）也必须由这里接手，才能保证
//! 「绕过判断只存在于一处」。

use reqwest::Url;
use std::{
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use super::cidr;

/// 探测系统代理要起子进程或过 JNI，不能每次请求都跑一遍。
const TTL: Duration = Duration::from_secs(30);

static PLATFORM_CACHE: OnceLock<Mutex<Option<(Instant, Option<Url>)>>> = OnceLock::new();

/// 平台/桌面环境的系统代理。`scheme` 用于 Windows/macOS 这类按协议分别配置的系统。
pub(crate) fn platform_proxy(scheme: &str) -> Option<Url> {
    let cache = PLATFORM_CACHE.get_or_init(|| Mutex::new(None));
    let Ok(mut guard) = cache.lock() else {
        return detect(scheme);
    };
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < TTL {
            return value.clone();
        }
    }
    let value = detect(scheme);
    *guard = Some((Instant::now(), value.clone()));
    value
}

/// 环境变量代理。`https_proxy` 优先于 `all_proxy`，与 curl/reqwest 的习惯一致。
pub(crate) fn env_proxy(target: &Url) -> Option<Url> {
    let keys: &[&str] = if target.scheme() == "https" {
        &["https_proxy", "HTTPS_PROXY", "all_proxy", "ALL_PROXY"]
    } else {
        &["http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"]
    };
    keys.iter().find_map(|key| {
        let value = std::env::var(key).ok()?;
        parse_env_value(value.trim())
    })
}

/// 环境变量里的 `no_proxy` / `NO_PROXY` 是否覆盖该目标。
pub(crate) fn env_bypasses(target: &Url) -> bool {
    let Some(list) = std::env::var("no_proxy")
        .ok()
        .or_else(|| std::env::var("NO_PROXY").ok())
    else {
        return false;
    };
    let Some(host) = target.host_str() else {
        return true;
    };
    no_proxy_matches(host, &list)
}

/// `no_proxy` 的值可能是省略协议的 `host:port`，补上 `http://` 再解析。
fn parse_env_value(value: &str) -> Option<Url> {
    if value.is_empty() {
        return None;
    }
    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        format!("http://{value}")
    };
    let url = Url::parse(&candidate).ok()?;
    if !matches!(
        url.scheme(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        return None;
    }
    url.host_str()?;
    Some(url)
}

/// 匹配 `no_proxy` 列表：`*`、精确主机、域名后缀、`host:port`，以及 CIDR。
///
/// 这是对 reqwest 原有行为的一次近似：它同样支持这些形式，但还允许端口范围等
/// 更细的写法，这里只实现被实际用到的部分。
fn no_proxy_matches(host: &str, list: &str) -> bool {
    let host = host
        .trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase();
    list.split(',').map(str::trim).any(|entry| {
        if entry.is_empty() {
            return false;
        }
        if entry == "*" {
            return true;
        }
        let entry = entry.to_ascii_lowercase();
        if entry.contains('/') {
            return host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| cidr::contains(ip, &entry));
        }
        // `host:port` 在 no_proxy 里的端口部分在这里被忽略：代理是按协议分流的，
        // 同一个主机不会因为端口不同而需要分别判断。
        let name = entry.rsplit_once(':').map_or(entry.as_str(), |(name, _)| name);
        let name = name.trim_start_matches('.');
        if name.is_empty() {
            return false;
        }
        host == name || host.ends_with(&format!(".{name}"))
    })
}

/// 解析系统命令（`gsettings` / `kreadconfig` / `scutil` / `reg query`）返回的代理值。
///
/// 形如 `http://host:port` 或 `host:port`（按 `default_scheme` 补协议）。
#[cfg(any(target_os = "macos", target_os = "windows", test))]
pub(crate) fn parse_proxy_value(value: &str, default_scheme: &str) -> Option<Url> {
    let value = value.trim().trim_matches(['\'', '"']);
    if value.is_empty() {
        return None;
    }
    if value.contains("://") {
        parse_env_value(value)
    } else {
        parse_env_value(&format!("{default_scheme}://{value}"))
    }
}

/// 解析 Kotlin 手写 JSON 形式的系统代理探测结果。
///
/// Kotlin 侧已保证格式，这里仍做校验：JNI 边界上的异常值不应让客户端构建失败。
#[cfg(any(target_os = "android", test))]
pub(crate) fn parse_bridge_result(raw: &str) -> Result<Option<Url>, String> {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Bridge {
        /// `none` | `manual` | `unavailable`
        mode: String,
        url: Option<String>,
    }

    let parsed: Bridge =
        serde_json::from_str(raw).map_err(|error| format!("系统代理探测结果无法解析：{error}"))?;
    if parsed.mode != "manual" {
        return Ok(None);
    }
    let value = parsed.url.ok_or("系统代理探测结果缺少地址")?;
    let url = Url::parse(&value).map_err(|_| format!("系统代理探测结果含非法地址：{value}"))?;
    if !matches!(
        url.scheme(),
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    ) {
        return Err(format!("系统代理使用了不支持的协议：{}", url.scheme()));
    }
    url.host_str()
        .ok_or_else(|| format!("系统代理探测结果的地址缺少主机：{value}"))?;
    Ok(Some(url))
}

#[cfg(target_os = "android")]
fn detect(scheme: &str) -> Option<Url> {
    super::android::system_proxy(scheme).ok().flatten()
}

#[cfg(all(
    unix,
    not(any(target_os = "android", target_os = "macos", target_os = "ios"))
))]
fn detect(_scheme: &str) -> Option<Url> {
    super::linux::system_proxy()
}

#[cfg(target_os = "macos")]
fn detect(scheme: &str) -> Option<Url> {
    super::macos::system_proxy(scheme)
}

#[cfg(target_os = "windows")]
fn detect(scheme: &str) -> Option<Url> {
    super::windows::system_proxy(scheme)
}

#[cfg(target_os = "ios")]
fn detect(_scheme: &str) -> Option<Url> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_env_values_with_and_without_a_scheme() {
        assert_eq!(
            parse_env_value("127.0.0.1:7890").map(|url| url.to_string()),
            Some("http://127.0.0.1:7890/".into())
        );
        assert_eq!(
            parse_env_value("socks5://10.0.0.2:1080").map(|url| url.to_string()),
            Some("socks5://10.0.0.2:1080".into())
        );
        assert!(parse_env_value("").is_none());
        // 不支持的协议不能悄悄变成代理地址。
        assert!(parse_env_value("file:///etc/passwd").is_none());
    }

    #[test]
    fn parses_platform_command_values() {
        assert_eq!(
            parse_proxy_value("'proxy.example'", "http").map(|url| url.to_string()),
            Some("http://proxy.example/".into())
        );
        assert_eq!(
            parse_proxy_value("proxy.example:7890", "http").map(|url| url.to_string()),
            Some("http://proxy.example:7890/".into())
        );
        assert_eq!(
            parse_proxy_value("socks5://proxy.example:1080", "http").map(|url| url.to_string()),
            Some("socks5://proxy.example:1080".into())
        );
        assert!(parse_proxy_value("", "http").is_none());
        assert!(parse_proxy_value("@as []", "http").is_none());
    }

    #[test]
    fn no_proxy_matches_hosts_suffixes_ports_and_cidrs() {
        assert!(no_proxy_matches("example.com", "example.com"));
        assert!(no_proxy_matches("api.example.com", "example.com"));
        assert!(no_proxy_matches("api.example.com", ".example.com"));
        assert!(no_proxy_matches("example.com", "example.com:8080"));
        assert!(no_proxy_matches("anything.local", "*"));
        assert!(no_proxy_matches("10.1.2.3", "10.0.0.0/8"));
        assert!(no_proxy_matches("172.30.255.42", "172.16.0.0/12"));
        assert!(no_proxy_matches("192.168.84.3", "192.168.84.3"));
        assert!(no_proxy_matches("example.com", "other.test, example.com"));
        assert!(!no_proxy_matches("example.com", "notexample.com"));
        assert!(!no_proxy_matches("10.1.2.3", "10.2.0.0/16"));
        assert!(!no_proxy_matches("example.com", ""));
        assert!(!no_proxy_matches("example.com", "other.test"));
    }

    #[test]
    fn parses_a_manual_system_proxy_from_the_bridge() {
        let parsed = parse_bridge_result(r#"{"mode":"manual","url":"http://10.0.0.2:8080"}"#).unwrap();
        assert_eq!(
            parsed.map(|url| url.to_string()).as_deref(),
            Some("http://10.0.0.2:8080/")
        );
    }

    #[test]
    fn parses_absence_as_none() {
        for raw in [
            r#"{"mode":"none","url":null}"#,
            r#"{"mode":"unavailable","url":null}"#,
        ] {
            assert_eq!(parse_bridge_result(raw).unwrap(), None, "{raw}");
        }
    }

    #[test]
    fn rejects_malformed_or_dangerous_bridge_results() {
        assert!(parse_bridge_result("not json").is_err());
        assert!(parse_bridge_result(r#"{"mode":"manual"}"#).is_err());
        assert!(parse_bridge_result(r#"{"mode":"manual","url":"file:///etc/passwd"}"#).is_err());
        assert!(parse_bridge_result(r#"{"mode":"manual","url":"not-a-url"}"#).is_err());
    }
}