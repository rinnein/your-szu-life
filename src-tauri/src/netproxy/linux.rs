//! Linux 桌面的系统代理探测（GNOME / KDE）。
//!
//! reqwest 在 Linux 上只读环境变量，读不到桌面环境的代理设置，因此这里显式探测。
//! 探测失败一律返回 `None`，让客户端保持 reqwest 的原有行为（环境变量）。

use crate::netproxy::model::authority_host;
use reqwest::Url;

pub(crate) fn system_proxy() -> Option<Url> {
    // 测试环境不探测桌面代理：避免依赖 DBus 与用户当前桌面会话，保证测试可重复。
    #[cfg(test)]
    {
        None
    }
    #[cfg(not(test))]
    {
        gnome_proxy().or_else(kde_proxy)
    }
}

#[cfg(not(test))]
fn gnome_proxy() -> Option<Url> {
    if read("gsettings", &["get", "org.gnome.system.proxy", "mode"]).as_deref() != Some("manual") {
        return None;
    }
    let host = read(
        "gsettings",
        &["get", "org.gnome.system.proxy.http", "host"],
    )?;
    let port = read(
        "gsettings",
        &["get", "org.gnome.system.proxy.http", "port"],
    )?;
    build(&host, port.parse().ok()?)
}

/// KDE 把代理写在 `kioslaverc` 里，`ProxyType` 为 1 表示手动。
#[cfg(not(test))]
fn kde_proxy() -> Option<Url> {
    for binary in ["kreadconfig6", "kreadconfig5"] {
        let Some(proxy_type) = kde_read(binary, "ProxyType") else {
            continue;
        };
        if proxy_type.trim() != "1" {
            return None;
        }
        let raw = kde_read(binary, "httpProxy")?;
        return parse_kde(&raw);
    }
    None
}

/// 值形如 `http://host:8080` 或 `host:8080`；缺端口时按 HTTP 默认代理端口兜底。
#[cfg(not(test))]
fn parse_kde(raw: &str) -> Option<Url> {
    let value = raw
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    if let Some((host, port)) = value.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            return build(host, port);
        }
    }
    build(value, 8080)
}

#[cfg(not(test))]
fn kde_read(binary: &str, key: &str) -> Option<String> {
    read(
        binary,
        &["--file", "kioslaverc", "--group", "Proxy", "--key", key],
    )
}

/// 只接受成功的、非空的输出。
#[cfg(not(test))]
fn read(binary: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(binary).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    // gsettings 对字符串返回带引号的形式，例如 `'manual'`。
    let trimmed = text.trim().trim_matches(['\'', '"']);
    if trimmed.is_empty() || trimmed == "@as []" {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn build(host: &str, port: u16) -> Option<Url> {
    if host.is_empty() || port == 0 {
        return None;
    }
    Url::parse(&format!("http://{}:{port}", authority_host(host))).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_nothing_in_tests_to_stay_hermetic() {
        assert!(system_proxy().is_none());
    }

    #[test]
    fn build_rejects_incomplete_values() {
        assert!(build("", 8080).is_none());
        assert!(build("proxy.example", 0).is_none());
        assert_eq!(
            build("proxy.example", 8080).map(|url| url.to_string()),
            Some("http://proxy.example:8080/".into())
        );
        assert_eq!(
            build("fd00::1", 8080).map(|url| url.to_string()),
            Some("http://[fd00::1]:8080/".into())
        );
    }
}