//! 网络代理设置：仅描述「要不要用代理、用哪个代理」，不含任何 IO。

use crate::storage::CredentialStore;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use super::cidr;

pub(crate) const MODE_OFF: &str = "off";
pub(crate) const MODE_AUTO: &str = "auto";
pub(crate) const MODE_MANUAL: &str = "manual";

pub(crate) const PROTOCOL_HTTP: &str = "http";
pub(crate) const PROTOCOL_SOCKS5: &str = "socks5";

const MODES: [&str; 3] = [MODE_OFF, MODE_AUTO, MODE_MANUAL];
const PROTOCOLS: [&str; 2] = [PROTOCOL_HTTP, PROTOCOL_SOCKS5];

/// 落盘到凭据文档的代理设置。
///
/// 与外观、通知这类「设备偏好」不同，代理设置属于可迁移的用户配置，
/// 因此存在加密凭据文档里（键 `fixed.network.proxy`），并随「设置迁移」导出。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxySettings {
    /// `off` | `auto` | `manual`
    pub mode: String,
    /// `http` | `socks5`
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    /// 校内网（172.16.0.0/12 与校内门户）是否也走代理。
    /// 默认关闭：这些地址通常只有校园网内可达，送进代理只会得到一个误导性的失败。
    pub campus_through_proxy: bool,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            // 默认「自动」：与 reqwest 自身的环境变量/系统代理行为完全一致。
            mode: MODE_AUTO.into(),
            protocol: PROTOCOL_HTTP.into(),
            host: String::new(),
            port: 8080,
            username: String::new(),
            password: String::new(),
            campus_through_proxy: false,
        }
    }
}

/// 发给前端的视图。**不含密码**，只回一个「是否已设置」的标志。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxySettingsView {
    pub mode: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub has_password: bool,
    pub campus_through_proxy: bool,
}

impl From<&ProxySettings> for ProxySettingsView {
    fn from(settings: &ProxySettings) -> Self {
        Self {
            mode: settings.mode.clone(),
            protocol: settings.protocol.clone(),
            host: settings.host.clone(),
            port: settings.port,
            username: settings.username.clone(),
            has_password: !settings.password.is_empty(),
            campus_through_proxy: settings.campus_through_proxy,
        }
    }
}

/// 前端提交的修改。`password` 为 `None` 表示「沿用已保存的密码」，
/// 避免把密码回传到 WebView 再原样传回来。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxySettingsInput {
    pub mode: String,
    pub protocol: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: Option<String>,
    /// 校内网是否也走代理。省略表示沿用已保存的值。
    #[serde(default)]
    pub campus_through_proxy: Option<bool>,
}

impl ProxySettingsInput {
    /// 合并已有设置。清空用户名即视为清空密码，否则省略密码表示沿用旧值。
    pub(crate) fn merge(self, existing: ProxySettings) -> ProxySettings {
        let password = if self.username.is_empty() {
            String::new()
        } else {
            self.password.unwrap_or(existing.password)
        };
        ProxySettings {
            mode: self.mode,
            protocol: self.protocol,
            host: self.host,
            port: self.port,
            username: self.username,
            password,
            campus_through_proxy: self
                .campus_through_proxy
                .unwrap_or(existing.campus_through_proxy),
        }
    }
}

/// 连通性测试结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProxyProbeView {
    /// 实际请求的目标地址。
    pub target: String,
    pub status: u16,
    pub elapsed_ms: u64,
}

impl ProxySettings {
    /// 拒绝越界取值，避免损坏的凭据文档把网络锁死。
    ///
    /// `off` / `auto` 不依赖主机与端口，因此只在 `manual` 下校验这两个字段。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !MODES.contains(&self.mode.as_str()) {
            return Err(format!("未知的代理模式：{}", self.mode));
        }
        if !PROTOCOLS.contains(&self.protocol.as_str()) {
            return Err(format!("未知的代理协议：{}", self.protocol));
        }
        if self.mode != MODE_MANUAL {
            return Ok(());
        }
        if self.host.is_empty() {
            return Err("请填写代理服务器地址".into());
        }
        if self.host.contains("://")
            || self.host.contains('/')
            || self.host.chars().any(char::is_whitespace)
        {
            return Err("代理服务器地址只需主机名或 IP，不要带协议或路径".into());
        }
        if self.port == 0 {
            return Err("代理端口需在 1 到 65535 之间".into());
        }
        Ok(())
    }

    /// 手动模式下的代理地址，供 reqwest 使用；其他模式返回 `None`。
    ///
    /// SOCKS5 映射为 `socks5h://`，让 DNS 也由代理远端解析 —— 这正是
    /// 需要用 SOCKS 的场景（本地 DNS 不可信或被污染）。
    pub(crate) fn proxy_url(&self) -> Option<Url> {
        if self.mode != MODE_MANUAL || self.validate().is_err() {
            return None;
        }
        let scheme = if self.protocol == PROTOCOL_SOCKS5 {
            "socks5h"
        } else {
            "http"
        };
        let mut url = Url::parse(&format!(
            "{scheme}://{}:{}",
            authority_host(&self.host),
            self.port
        ))
        .ok()?;
        if !self.username.is_empty() {
            url.set_username(&self.username).ok()?;
            url.set_password(Some(&self.password)).ok()?;
        }
        Some(url)
    }

    /// 手动 HTTP 代理给 aria2 用的一行参数值，其他情况返回 `None`。
    ///
    /// aria2 不支持 SOCKS 代理，因此 SOCKS5 模式这里不做注入（下载将直连，
    /// 前端会就这一点给出常驻提示）。凭据按原文拼进 userinfo：aria2 不解析
    /// 百分号转义，用编码后的值当密码会直接认证失败。
    pub(crate) fn aria2_proxy(&self) -> Option<String> {
        if self.mode != MODE_MANUAL || self.protocol != PROTOCOL_HTTP {
            return None;
        }
        self.validate().ok()?;
        let auth = if self.username.is_empty() {
            String::new()
        } else {
            format!("{}:{}@", self.username, self.password)
        };
        Some(format!(
            "http://{auth}{}:{}",
            authority_host(&self.host),
            self.port
        ))
    }
}

/// 拼 URL 的 authority 部分：IPv6 字面量必须带方括号。
pub(crate) fn authority_host(host: &str) -> String {
    let trimmed = host.trim().trim_matches(['[', ']']);
    if trimmed.contains(':') {
        format!("[{trimmed}]")
    } else {
        trimmed.to_owned()
    }
}

/// 一组「按地址或域名匹配」的目标，用于校内网白名单与强制直连名单。
#[derive(Default)]
struct AddressSet {
    /// 预解析的网段，避免每次请求重新解析 CIDR 文本。
    network: cidr::Compiled,
    /// 域名，例如 `net.szu.edu.cn`；按精确或子域后缀匹配。
    hosts: Vec<String>,
}

impl AddressSet {
    fn matches(&self, host: &str) -> bool {
        let host = normalize_host(host);
        if self.matches_domain(&host) {
            return true;
        }
        host.parse::<std::net::IpAddr>()
            .is_ok_and(|ip| self.network.matches(ip))
    }

    /// 只查域名条目（`+.szu.edu.cn` 这类），不看网段。
    fn matches_domain(&self, host: &str) -> bool {
        let host = normalize_host(host);
        self.hosts
            .iter()
            .any(|entry| host_matches_suffix(&host, entry))
    }
}

/// 项目根目录的 `campus-network.json`。
///
/// 三类内容：`alwaysDirect` 永不走代理（校内门户只在校园网内可达，代理到不了），
/// `cidrs` + `hosts` 是校内网名单，是否走代理由设置里的 `campus_through_proxy`
/// 开关决定，`dns` 是校内域名专用的主 DNS。
struct CampusNetwork {
    always_direct: AddressSet,
    campus: AddressSet,
    /// 校内域名专用的 DNS 服务器。空列表表示不做覆盖，全部交给系统解析器。
    dns: Vec<std::net::IpAddr>,
}

impl Default for CampusNetwork {
    fn default() -> Self {
        Self {
            always_direct: AddressSet::default(),
            campus: AddressSet::default(),
            dns: Vec::new(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CampusNetworkSource {
    /// 永不走代理的地址：`172.30.255.42` 这类裸地址，或 `net.szu.edu.cn` 这类域名。
    #[serde(default)]
    always_direct: Vec<String>,
    /// 校内网在用网段，形如 `172.16.0.0/12`。
    #[serde(default)]
    cidrs: Vec<String>,
    /// 校内网域名，`+.szu.edu.cn` 表示本域及其全部子域。
    #[serde(default)]
    hosts: Vec<String>,
    /// 校内域名要先问的 DNS 服务器地址。
    #[serde(default)]
    dns: Vec<String>,
}

fn compile_set(entries: &[String]) -> AddressSet {
    let mut set = AddressSet::default();
    for entry in entries {
        // 域名直接留着做后缀匹配，其余交给网段解析。
        if entry
            .trim()
            .parse::<std::net::IpAddr>()
            .is_err()
            && entry.trim().split_once('/').is_none()
        {
            set.hosts.push(entry.clone());
        }
    }
    set.network = cidr::compile(entries);
    set
}

impl From<CampusNetworkSource> for CampusNetwork {
    fn from(source: CampusNetworkSource) -> Self {
        // 网段与域名是同一份「校内网名单」的两种写法，合成一个集合。
        let mut campus = source.cidrs;
        campus.extend(source.hosts);
        Self {
            always_direct: compile_set(&source.always_direct),
            campus: compile_set(&campus),
            // 只有能当作地址用的条目才是 DNS 服务器。
            dns: source
                .dns
                .iter()
                .filter_map(|entry| {
                    let parsed = entry.trim().parse::<std::net::IpAddr>();
                    if parsed.is_err() {
                        log::warn!("campus-network.json 里的 DNS 条目不是地址，已忽略: {entry}");
                    }
                    parsed.ok()
                })
                .collect(),
        }
    }
}

/// 校内域名专用的 DNS 服务器。空列表表示不做覆盖，全部交给系统解析器。
pub(crate) fn campus_dns_servers() -> &'static [std::net::IpAddr] {
    &campus_network().dns
}

/// 该域名是否属于校内域名 —— 这些域名要先问校内 DNS。
///
/// 只看 `hosts` 里的域名条目：`alwaysDirect` 里可能混有非校内的域名，
/// 那些不该被送去校内 DNS。
pub(crate) fn is_campus_domain(host: &str) -> bool {
    campus_network().campus.matches_domain(host)
}

fn campus_network() -> &'static CampusNetwork {
    static NETWORK: OnceLock<CampusNetwork> = OnceLock::new();
    NETWORK.get_or_init(|| {
        serde_json::from_str::<CampusNetworkSource>(include_str!("../../../campus-network.json"))
            .map(CampusNetwork::from)
            .unwrap_or_else(|error| {
                log::warn!("campus-network.json 无法解析，校内网白名单按空处理: {error}");
                CampusNetwork::default()
            })
    })
}

/// 域名条目按「本域及其子域」匹配。
///
/// `+.szu.edu.cn`、`.szu.edu.cn`、`szu.edu.cn` 三种写法等价；带点的边界保证
/// `notszu.edu.cn` 这种同后缀域名不会被误判。
fn host_matches_suffix(host: &str, entry: &str) -> bool {
    let entry = entry
        .trim()
        .trim_start_matches('+')
        .trim_start_matches('.')
        .to_ascii_lowercase();
    !entry.is_empty() && (host == entry || host.ends_with(&format!(".{entry}")))
}

/// 该目标是否绕过代理。
///
/// 这是全应用**唯一**一处判断「要不要绕过代理」的地方：各个业务模块只负责构建
/// 客户端，绕不绕由这里按目标地址决定。
pub(crate) fn bypasses_proxy(target: &Url, settings: &ProxySettings) -> bool {
    let Some(host) = target.host_str() else {
        // 没有主机名就无从判断，宁可不走代理。
        return true;
    };
    let host = normalize_host(host);
    if host.is_empty() {
        return true;
    }
    let campus = campus_network();
    // 校内门户只能直连：它只在校园网内可达，代理到不了；这条优先于开关。
    if campus.always_direct.matches(&host) {
        return true;
    }
    // 本机与链路本地地址永远直连：代理到不了，也不该被代理。
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    // 校内网在用网段：由开关决定是否也交给代理。
    if campus.campus.matches(&host) {
        return !settings.campus_through_proxy;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => {
            if ip.is_loopback() || ip.is_unspecified() || ip.is_link_local() {
                return true;
            }
            // 其余私有网段直连：它们本来就出不了公网，交给代理只会得到一个
            // 与真实原因无关的失败。
            ip.is_private()
        }
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unspecified(),
        // 域名按公网处理。
        Err(_) => false,
    }
}

fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// 读取已保存的设置；从未保存过时返回默认值。
pub(crate) fn load(credentials: &dyn CredentialStore) -> Result<ProxySettings, String> {
    Ok(credentials.load_proxy_settings()?.unwrap_or_default())
}

/// 保存前的合并入口，供命令与 MCP 共用。
pub(crate) fn merge_input(
    credentials: &dyn CredentialStore,
    input: ProxySettingsInput,
) -> Result<ProxySettings, String> {
    let settings = input.merge(load(credentials)?);
    settings.validate()?;
    Ok(settings)
}

pub(crate) fn view(credentials: &dyn CredentialStore) -> Result<ProxySettingsView, String> {
    Ok(ProxySettingsView::from(&load(credentials)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual(protocol: &str) -> ProxySettings {
        ProxySettings {
            mode: MODE_MANUAL.into(),
            protocol: protocol.into(),
            host: "proxy.example".into(),
            port: 7890,
            username: String::new(),
            password: String::new(),
            campus_through_proxy: false,
        }
    }

    fn input(mode: &str, username: &str, password: Option<String>) -> ProxySettingsInput {
        ProxySettingsInput {
            mode: mode.into(),
            protocol: PROTOCOL_HTTP.into(),
            host: "proxy.example".into(),
            port: 7890,
            username: username.into(),
            password,
            campus_through_proxy: None,
        }
    }

    #[test]
    fn default_is_valid_and_uses_no_explicit_proxy() {
        let settings = ProxySettings::default();
        assert_eq!(settings.mode, MODE_AUTO);
        assert!(settings.validate().is_ok());
        assert!(settings.proxy_url().is_none());
        assert!(settings.aria2_proxy().is_none());
    }

    #[test]
    fn off_and_auto_ignore_host_and_port() {
        let mut settings = ProxySettings::default();
        settings.mode = MODE_OFF.into();
        settings.host = String::new();
        settings.port = 0;
        assert!(settings.validate().is_ok());
        assert!(settings.proxy_url().is_none());
    }

    #[test]
    fn rejects_unknown_mode_or_protocol() {
        let mut settings = manual(PROTOCOL_HTTP);
        settings.mode = "sometimes".into();
        assert!(settings.validate().is_err());
        let mut settings = manual("socks4");
        settings.protocol = "socks4".into();
        assert!(settings.validate().is_err());
    }

    #[test]
    fn manual_requires_a_plain_host() {
        for host in ["", "http://proxy.example", "proxy.example/", "pro xy"] {
            let mut settings = manual(PROTOCOL_HTTP);
            settings.host = host.into();
            assert!(settings.validate().is_err(), "should reject {host}");
        }
        let mut settings = manual(PROTOCOL_HTTP);
        settings.port = 0;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn http_proxy_url_carries_percent_encoded_credentials() {
        let mut settings = manual(PROTOCOL_HTTP);
        settings.username = "user@corp".into();
        settings.password = "p@ss word".into();
        let url = settings.proxy_url().unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("proxy.example"));
        assert_eq!(url.port(), Some(7890));
        // Url 把 userinfo 按百分号转义保存；hyper-util 拼 Proxy-Authorization 时会解码，
        // 因此这里断言的是转义后的形式。
        assert_eq!(url.username(), "user%40corp");
        let encoded = url.password().unwrap_or_default();
        assert!(encoded.contains("%40"), "{encoded}");
        assert!(encoded.contains("%20"), "{encoded}");
    }

    #[test]
    fn socks5_maps_to_socks5h_for_remote_dns() {
        let settings = manual(PROTOCOL_SOCKS5);
        let url = settings.proxy_url().unwrap();
        assert_eq!(url.scheme(), "socks5h");
    }

    #[test]
    fn aria2_gets_a_raw_http_proxy_and_nothing_for_socks5() {
        let mut settings = manual(PROTOCOL_HTTP);
        settings.username = "u".into();
        settings.password = "p".into();
        assert_eq!(
            settings.aria2_proxy().as_deref(),
            Some("http://u:p@proxy.example:7890")
        );
        assert!(manual(PROTOCOL_SOCKS5).aria2_proxy().is_none());
        let mut automatic = ProxySettings::default();
        automatic.mode = MODE_AUTO.into();
        assert!(automatic.aria2_proxy().is_none());
    }

    #[test]
    fn ipv6_hosts_are_bracketed() {
        let mut settings = manual(PROTOCOL_HTTP);
        settings.host = "fd00::1".into();
        assert!(settings.proxy_url().is_some());
        assert_eq!(
            settings.aria2_proxy().as_deref(),
            Some("http://[fd00::1]:7890")
        );
    }

    #[test]
    fn bypass_rule_always_skips_local_and_private_targets() {
        let settings = ProxySettings::default();
        for target in [
            "http://localhost/",
            "http://127.0.0.1:54001/jsonrpc",
            "http://127.8.9.10/",
            "http://[::1]/",
            "http://192.168.84.3:9090/",
            "http://10.1.2.3/",
            "http://169.254.1.1/",
            "http://0.0.0.0/",
            "http://printer.local/",
        ] {
            assert!(
                bypasses_proxy(&Url::parse(target).unwrap(), &settings),
                "{target} 应直连"
            );
        }
        for target in [
            "https://proxy.example/",
            "https://www.uooc.net.cn/",
            "https://github.com/",
            "http://172.32.0.1/",
            "http://192.169.0.1/",
        ] {
            assert!(
                !bypasses_proxy(&Url::parse(target).unwrap(), &settings),
                "{target} 应走代理"
            );
        }
    }

    #[test]
    fn shipped_whitelist_parses_into_the_expected_sets() {
        // 直接断言随包发出的 campus-network.json 真的解析成功：
        // 解析失败会静默退化成空集合，那样开关就形同虚设。
        let campus = campus_network();
        assert!(
            campus.campus.matches("172.21.5.5"),
            "校内网段（172.21.0.0/16）应在白名单里"
        );
        assert!(
            campus.campus.matches("2001:250:3c00::1"),
            "校内 IPv6 前缀也应在白名单里"
        );
        assert!(
            campus.campus.matches("192.168.84.3"),
            "宿舍电费所在网段在校内清单里"
        );
        assert!(
            !campus.campus.matches("172.30.255.42"),
            "校内门户不在校内清单里，它由 alwaysDirect 单独规定"
        );
        // `+.szu.edu.cn`：本域与子域都算校内网（默认直连，开关打开才走代理）。
        for host in [
            "szu.edu.cn",
            "authserver.szu.edu.cn",
            "ehall.szu.edu.cn",
            "bkxk.szu.edu.cn",
            "webvpn.szu.edu.cn",
        ] {
            assert!(campus.campus.matches(host), "{host} 应算校内网");
        }
        assert!(!campus.campus.matches("notszu.edu.cn"));
        assert!(!campus.campus.matches("szu.edu.cn.evil.test"));
        // 门户虽然也落在 `+.szu.edu.cn` 里，但 alwaysDirect 先判，仍是直连。
        assert!(campus.always_direct.matches("net.szu.edu.cn"));
        // 门户的两种写法都只能直连。
        for host in ["172.30.255.42", "net.szu.edu.cn", "Net.SZU.edu.Cn"] {
            assert!(campus.always_direct.matches(host), "{host} 应强制直连");
        }
        assert!(!campus.always_direct.matches("github.com"));
    }

    #[test]
    fn host_entries_match_the_domain_and_its_subdomains() {
        for entry in ["+.szu.edu.cn", ".szu.edu.cn", "szu.edu.cn", "  +.SZU.edu.cn "] {
            assert!(host_matches_suffix("szu.edu.cn", entry), "{entry}");
            assert!(host_matches_suffix("www.szu.edu.cn", entry), "{entry}");
            assert!(!host_matches_suffix("notszu.edu.cn", entry), "{entry}");
            assert!(!host_matches_suffix("szu.edu.cn.evil.test", entry), "{entry}");
            assert!(!host_matches_suffix("www.other.edu.cn", entry), "{entry}");
        }
        assert!(!host_matches_suffix("www.szu.edu.cn", ""));
        assert!(!host_matches_suffix("www.szu.edu.cn", "+."));
    }

    #[test]
    fn portal_is_direct_regardless_of_the_toggle() {
        for toggle in [false, true] {
            let settings = ProxySettings {
                mode: MODE_MANUAL.into(),
                campus_through_proxy: toggle,
                ..ProxySettings::default()
            };
            for target in ["http://172.30.255.42:801/eportal/", "https://net.szu.edu.cn/"] {
                assert!(
                    bypasses_proxy(&Url::parse(target).unwrap(), &settings),
                    "{target} 开关为 {toggle} 时也必须直连"
                );
            }
        }
    }

    #[test]
    fn campus_targets_follow_the_toggle() {
        let mut settings = ProxySettings {
            mode: MODE_MANUAL.into(),
            ..ProxySettings::default()
        };
        // 白名单（campus-network.json 的 cidrs 与 hosts）内的地址：默认直连，开关打开后走代理。
        let campus_targets = [
            "http://172.25.100.105:8010/",
            "http://192.168.84.3:9090/",
            "https://authserver.szu.edu.cn/authserver/login",
            "https://webvpn.szu.edu.cn/",
        ];
        for target in campus_targets {
            let url = Url::parse(target).unwrap();
            assert!(bypasses_proxy(&url, &settings), "{target} 默认应直连");
        }
        settings.campus_through_proxy = true;
        for target in campus_targets {
            let url = Url::parse(target).unwrap();
            assert!(!bypasses_proxy(&url, &settings), "{target} 开启后应走代理");
        }
        settings.campus_through_proxy = false;
        // 不在白名单里的私有地址与回环地址始终直连，与开关无关。
        for target in ["http://127.0.0.1/", "http://10.0.0.1/"] {
            let url = Url::parse(target).unwrap();
            assert!(bypasses_proxy(&url, &settings), "{target}");
            settings.campus_through_proxy = true;
            assert!(bypasses_proxy(&url, &settings), "{target} 开关开启后仍直连");
            settings.campus_through_proxy = false;
        }
        // 公网站点不受开关影响。
        assert!(!bypasses_proxy(
            &Url::parse("https://github.com/").unwrap(),
            &settings
        ));
    }

    #[test]
    fn merge_keeps_the_saved_password_unless_the_username_is_cleared() {
        let existing = ProxySettings {
            password: "saved".into(),
            ..manual(PROTOCOL_HTTP)
        };
        let kept = input(MODE_MANUAL, "user", None).merge(existing.clone());
        assert_eq!(kept.password, "saved");

        let replaced = input(MODE_MANUAL, "user", Some("new".into())).merge(existing.clone());
        assert_eq!(replaced.password, "new");

        // 清空用户名即视为清空密码。
        let cleared = input(MODE_MANUAL, "", None).merge(existing);
        assert!(cleared.password.is_empty());
    }

    #[test]
    fn merge_carries_the_campus_toggle() {
        let mut submitted = input(MODE_AUTO, "", None);
        submitted.campus_through_proxy = Some(true);
        assert!(submitted.merge(ProxySettings::default()).campus_through_proxy);
        // 省略该字段时保留已保存的值，而不是悄悄关掉。
        let saved = ProxySettings {
            campus_through_proxy: true,
            ..ProxySettings::default()
        };
        assert!(input(MODE_AUTO, "", None).merge(saved).campus_through_proxy);
    }

    #[test]
    fn view_never_exposes_the_password() {
        let settings = ProxySettings {
            username: "user".into(),
            password: "secret".into(),
            campus_through_proxy: true,
            ..manual(PROTOCOL_HTTP)
        };
        let view = ProxySettingsView::from(&settings);
        assert!(view.has_password);
        assert!(view.campus_through_proxy);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("secret"), "{json}");
        assert!(json.contains("hasPassword"), "{json}");
        assert!(json.contains("campusThroughProxy"), "{json}");
    }
}