//! macOS 系统代理探测：解析 `scutil --proxy` 的输出。
//!
//! reqwest 在 macOS 上本来也能读系统代理，但统一拦截器关掉了那套机制
//! （为了把「要不要绕过」的判断收在一处），所以这里自己读一遍。

use crate::netproxy::system::parse_proxy_value;
use reqwest::Url;

/// `scheme` 为 `https` / `http`，决定优先取哪个协议的代理配置。
#[cfg(target_os = "macos")]
pub(crate) fn system_proxy(scheme: &str) -> Option<Url> {
    let output = std::process::Command::new("scutil")
        .arg("--proxy")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    parse_scutil(&text, scheme)
}

/// `scutil --proxy` 输出的是 `KEY : VALUE` 扁平行（嵌套的数组行不再缩进对齐）。
fn parse_scutil(text: &str, scheme: &str) -> Option<Url> {
    let field = |key: &str| -> Option<String> {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.trim() == key)
                .then(|| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
    };
    let enabled = |key: &str| field(key).is_some_and(|value| value == "1");

    // 先按目标协议取，取不到再退回另一个；最后才是 SOCKS。
    let (primary, fallback) = if scheme == "https" {
        (
            ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
            ("HTTPEnable", "HTTPProxy", "HTTPPort"),
        )
    } else {
        (
            ("HTTPEnable", "HTTPProxy", "HTTPPort"),
            ("HTTPSEnable", "HTTPSProxy", "HTTPSPort"),
        )
    };
    for (enable, host_key, port_key) in [primary, fallback] {
        if !enabled(enable) {
            continue;
        }
        if let Some(url) = endpoint(&field, host_key, port_key, "http") {
            return Some(url);
        }
    }
    if enabled("SOCKSEnable") {
        return endpoint(&field, "SOCKSProxy", "SOCKSPort", "socks5h");
    }
    None
}

fn endpoint(
    field: &impl Fn(&str) -> Option<String>,
    host_key: &str,
    port_key: &str,
    default_scheme: &str,
) -> Option<Url> {
    let host = field(host_key)?;
    let port = field(port_key)?;
    parse_proxy_value(&format!("{host}:{port}"), default_scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTTPS_ONLY: &str = r#"<dictionary> {
  ExceptionsList : <array> {
    0 : *.local
    1 : 169.254/16
  }
  FTPPassive : 1
  HTTPEnable : 0
  HTTPPort : 0
  HTTPProxy : 
  HTTPSEnable : 1
  HTTPSPort : 7890
  HTTPSProxy : 127.0.0.1
  ProxyAutoConfigEnable : 0
  SOCKSEnable : 0
  SOCKSPort : 0
  SOCKSProxy : 
}
"#;

    #[test]
    fn reads_the_enabled_https_proxy() {
        for scheme in ["https", "http"] {
            assert_eq!(
                parse_scutil(HTTPS_ONLY, scheme).map(|url| url.to_string()),
                Some("http://127.0.0.1:7890/".into()),
                "{scheme}"
            );
        }
    }

    #[test]
    fn reads_a_socks_proxy_as_socks5h() {
        let text = "<dictionary> {\n  HTTPEnable : 0\n  SOCKSEnable : 1\n  SOCKSPort : 1080\n  SOCKSProxy : 10.0.0.2\n}\n";
        assert_eq!(
            parse_scutil(text, "http").map(|url| url.to_string()),
            Some("socks5h://10.0.0.2:1080".into())
        );
    }

    #[test]
    fn nothing_enabled_means_no_proxy() {
        let text = "<dictionary> {\n  HTTPEnable : 0\n  HTTPPort : 0\n  HTTPProxy : \n  HTTPSEnable : 0\n}\n";
        assert!(parse_scutil(text, "http").is_none());
        assert!(parse_scutil("", "https").is_none());
    }
}