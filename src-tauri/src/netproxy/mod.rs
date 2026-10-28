//! 全局网络代理设置。
//!
//! 本模块是全应用**唯一**的 reqwest 客户端出口，也是**唯一**判断「这次请求要不要
//! 绕过代理」的地方：
//!
//! - 所有出网客户端都必须经由 [`client_builder`] / [`blocking_client_builder`] 构建；
//! - 每次请求的代理与绕过判断都由 [`resolve_proxy`](fn@resolve_proxy) 按目标地址给出，
//!   业务模块不需要（也不应该）自己决定谁是内网、谁该直连；
//! - 校内域名的 DNS 覆盖由同一个工厂注入，**与代理模式无关**：不走代理时同样
//!   先问校内 DNS；即使配的是 SOCKS5，也会把远端解析降级为本机解析来套用这条规则。
//!
//! 设置存在加密凭据文档里（键 `fixed.network.proxy`），随「设置迁移」导出。
//! 因为判断是请求时做的，保存后立刻对已存在的客户端生效，无需重启或重建。

#[cfg(target_os = "android")]
mod android;
mod cidr;
pub(crate) mod commands;
mod dns;
#[cfg(all(
    unix,
    not(any(target_os = "android", target_os = "macos", target_os = "ios"))
))]
mod linux;
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(any(target_os = "windows", test))]
mod windows;
pub(crate) mod mcp;
pub(crate) mod model;
mod system;

pub(crate) use model::{ProxyProbeView, ProxySettings, ProxySettingsInput, ProxySettingsView};

use crate::storage::{CredentialStore, SharedCredentialStore};
use reqwest::{Proxy, Url};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

/// 跨窗口同步用的事件名。
pub(crate) const EVENT: &str = "proxy-settings-changed";

/// 连通性测试的默认目标。选一个国内几乎处处可达的小文件，
/// 这样「关闭 / 自动」模式下也能验证客户端本身是否通。
const PROBE_URL: &str = "https://www.baidu.com/favicon.ico";

static CACHE: OnceLock<RwLock<ProxySettings>> = OnceLock::new();

/// 托管状态。只持有凭据句柄，设置本体在进程级缓存里，
/// 因为客户端构建点拿不到 Tauri 的 `State`。
pub(crate) struct ProxyStore {
    pub(crate) credentials: SharedCredentialStore,
}

impl ProxyStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self { credentials }
    }
}

fn cache() -> &'static RwLock<ProxySettings> {
    CACHE.get_or_init(|| RwLock::new(ProxySettings::default()))
}

/// 当前生效的设置。尚未初始化（或测试环境）时为默认的「自动」。
pub(crate) fn current() -> ProxySettings {
    cache()
        .read()
        .map(|value| value.clone())
        .unwrap_or_default()
}

/// 启动时把 vault 里的设置发布到进程缓存。
pub(crate) fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.try_state::<ProxyStore>().ok_or("代理设置状态未注册")?;
    let settings = model::load(store.credentials.as_ref())?;
    publish(settings);
    Ok(())
}

pub(crate) fn save(
    credentials: &dyn CredentialStore,
    app: &tauri::AppHandle,
    input: ProxySettingsInput,
) -> Result<ProxySettingsView, String> {
    let settings = model::merge_input(credentials, input)?;
    credentials.save_proxy_settings(&settings)?;
    publish(settings.clone());
    let view = ProxySettingsView::from(&settings);
    let _ = app.emit(EVENT, view.clone());
    Ok(view)
}

fn publish(value: ProxySettings) {
    if let Ok(mut cache) = cache().write() {
        *cache = value;
    }
}

/// 全应用唯一的异步客户端出口。
///
/// 这里注入两件与「怎么出网」有关的事：代理策略（含绕过判断），以及校内域名的
/// DNS 覆盖。DNS 覆盖与代理模式无关 —— 关闭代理、或目标被判定为直连时，校内
/// 域名依然由校内 DNS 解析。
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .proxy(dynamic_proxy())
        .dns_resolver(std::sync::Arc::new(dns::CampusResolver))
}

/// 全应用唯一的同步客户端出口。约定与 [`client_builder`] 相同。
pub(crate) fn blocking_client_builder() -> reqwest::blocking::ClientBuilder {
    reqwest::blocking::Client::builder()
        .proxy(dynamic_proxy())
        .dns_resolver(std::sync::Arc::new(dns::CampusResolver))
}

/// 请求时决策的代理拦截器。
///
/// 装上它就意味着 reqwest 自己的环境变量/平台代理处理被关掉，全部改由
/// [`resolve_proxy`] 决定 —— 这也正是「绕过逻辑只存在于一处」的前提。
fn dynamic_proxy() -> Proxy {
    Proxy::custom(|target: &Url| resolve_proxy(target))
}

/// 单次请求的代理决策：返回 `None` 表示这次请求直连。
fn resolve_proxy(target: &Url) -> Option<Url> {
    resolve_for(&current(), target).map(|(proxy, _)| proxy)
}

/// 决策与来源。分开返回是为了让「只有环境变量代理才受 `no_proxy` 影响」这条
/// 规则可以被准确表达，而不是把两件事混在一个布尔里。
fn resolve_for(settings: &ProxySettings, target: &Url) -> Option<(Url, Source)> {
    let (proxy, source) = match settings.mode.as_str() {
        model::MODE_OFF => return None,
        model::MODE_MANUAL => (
            settings.proxy_url()?,
            // 用户手填的地址就按用户的意思用，不再套用环境变量的 no_proxy。
            Source::Manual,
        ),
        _ => match system::platform_proxy(target.scheme()) {
            Some(url) => (url, Source::Platform),
            None => (system::env_proxy(target)?, Source::Environment),
        },
    };
    if model::bypasses_proxy(target, settings) {
        return None;
    }
    if source == Source::Environment && system::env_bypasses(target) {
        return None;
    }
    Some((
        enforce_campus_dns(proxy, target, dns::campus_dns_available()),
        source,
    ))
}

/// 校内域名必须由本机的校内 DNS 解析，因此即便配的是 SOCKS5，
/// 也要把「远端解析」（`socks5h`）降级成「本机解析」（`socks5`）——
/// 只有这样解析才会经过 [`dns::CampusResolver`] 问到校内 DNS。
///
/// 这条规则的优先级高于 SOCKS5 自身的默认行为，且与代理来源无关
/// （手动填写或系统代理都照此处理）。
fn enforce_campus_dns(proxy: Url, target: &Url, campus_dns_available: bool) -> Url {
    if !campus_dns_available || proxy.scheme() != "socks5h" {
        return proxy;
    }
    if !target.host_str().is_some_and(model::is_campus_domain) {
        return proxy;
    }
    let mut local = proxy.clone();
    // 改不了就保持 socks5h，不因为这条规则把请求弄坏。
    if local.set_scheme("socks5").is_err() {
        return proxy;
    }
    local
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Manual,
    /// 系统/桌面代理设置。
    Platform,
    /// `http_proxy` 一类环境变量。
    Environment,
}

/// aria2 的命令行代理注入方式。
///
/// aria2 不共用 reqwest 客户端，因此「怎么把代理告诉它」也由本模块决定，
/// 下载模块只负责把结果拼进命令行。
pub(crate) enum Aria2Proxy {
    /// 关闭：显式禁用，连 aria2 自己会读的环境变量代理也清掉。
    Disabled,
    /// 不干预，交给 aria2 按自身环境变量/系统设置处理。
    Environment,
    /// 手动模式下的 HTTP 代理地址。
    Manual(String),
}

pub(crate) fn aria2_proxy() -> Aria2Proxy {
    let settings = current();
    match settings.mode.as_str() {
        model::MODE_OFF => Aria2Proxy::Disabled,
        model::MODE_MANUAL => match settings.aria2_proxy() {
            Some(url) => Aria2Proxy::Manual(url),
            None => {
                // SOCKS5（或主机端口没填全）：aria2 不支持 SOCKS 代理，
                // 只能保持它自己的网络设置，前端会就这一点提示用户。
                log::warn!("aria2 不支持 SOCKS 代理，应用内下载不会经由该代理");
                Aria2Proxy::Environment
            }
        },
        _ => Aria2Proxy::Environment,
    }
}

/// 连通性测试。用**待保存**的设置建一次性客户端，因此不需要先落盘。
///
/// 必须在阻塞线程上调用（`reqwest::blocking` 不能在异步运行时里构建）。
pub(crate) fn test(
    credentials: &dyn CredentialStore,
    input: ProxySettingsInput,
    url: Option<&str>,
) -> Result<ProxyProbeView, String> {
    let settings = model::merge_input(credentials, input)?;
    let target = url.unwrap_or(PROBE_URL);
    let parsed = Url::parse(target).map_err(|_| "测试地址无效".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("测试地址必须是 http(s) 地址".into());
    }
    let proxy = resolve_for(&settings, &parsed).map(|(proxy, _)| proxy);
    let builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        // 连通性测试也走同一套 DNS 规则：校内域名优先问校内 DNS。
        .dns_resolver(std::sync::Arc::new(dns::CampusResolver))
        .user_agent("your-szu-life/0.1");
    // 测试用的是尚未保存的候选配置，因此不能走全局拦截器，只能显式指定。
    let builder = match proxy.and_then(|url| Proxy::all(url).ok()) {
        Some(proxy) => builder.proxy(proxy),
        None => builder.no_proxy(),
    };
    let client = builder.build().map_err(|_| "无法构建测试客户端".to_owned())?;
    let started = Instant::now();
    let response = client
        .get(parsed.clone())
        .send()
        .map_err(classify_probe_error)?;
    Ok(ProxyProbeView {
        target: parsed.to_string(),
        status: response.status().as_u16(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// reqwest 的错误文本可能带上完整的代理地址（含凭据），因此只回分类结果。
fn classify_probe_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        return "连接超时，请检查代理地址与端口".into();
    }
    if error.is_connect() {
        return "无法连接，请检查代理是否可用".into();
    }
    "测试请求失败".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testing::MemoryCredentialStore;

    fn memory(settings: Option<ProxySettings>) -> std::sync::Arc<MemoryCredentialStore> {
        let store = std::sync::Arc::new(MemoryCredentialStore::default());
        if let Some(settings) = settings {
            store.save_proxy_settings(&settings).unwrap();
        }
        store
    }

    fn manual() -> ProxySettings {
        ProxySettings {
            mode: model::MODE_MANUAL.into(),
            protocol: model::PROTOCOL_HTTP.into(),
            host: "proxy.example".into(),
            port: 7890,
            ..ProxySettings::default()
        }
    }

    fn target(value: &str) -> Url {
        Url::parse(value).unwrap()
    }

    #[test]
    fn current_defaults_to_auto_before_initialization() {
        // 测试进程不会初始化 Tauri 状态，缓存应保持默认值。
        assert_eq!(current().mode, model::MODE_AUTO);
        assert!(current().validate().is_ok());
    }

    #[test]
    fn every_mode_produces_a_usable_builder() {
        for mode in [
            model::MODE_OFF,
            model::MODE_AUTO,
            model::MODE_MANUAL,
            "unexpected",
        ] {
            let settings = ProxySettings {
                mode: mode.into(),
                ..manual()
            };
            let _ = resolve_for(&settings, &target("https://github.com/"));
            let _ = resolve_for(&settings, &target("http://192.168.1.1/"));
            let builder = reqwest::Client::builder().proxy(Proxy::custom(move |url: &Url| {
                resolve_for(&settings, url).map(|(proxy, _)| proxy)
            }));
            assert!(builder.build().is_ok(), "{mode}");
        }
    }

    #[test]
    fn off_never_resolves_a_proxy() {
        let settings = ProxySettings {
            mode: model::MODE_OFF.into(),
            campus_through_proxy: true,
            ..manual()
        };
        assert!(resolve_for(&settings, &target("https://github.com/")).is_none());
        assert!(resolve_for(&settings, &target("http://172.30.255.42/")).is_none());
    }

    #[test]
    fn manual_resolves_the_configured_proxy_for_public_targets_only() {
        let settings = manual();
        let (proxy, source) = resolve_for(&settings, &target("https://github.com/")).unwrap();
        assert_eq!(proxy.as_str(), "http://proxy.example:7890/");
        assert!(source == Source::Manual);
        assert!(resolve_for(&settings, &target("https://net.szu.edu.cn/")).is_none());
        assert!(resolve_for(&settings, &target("http://127.0.0.1:54001/")).is_none());
    }

    #[test]
    fn incomplete_manual_config_falls_back_to_direct() {
        let settings = ProxySettings {
            host: String::new(),
            ..manual()
        };
        assert!(resolve_for(&settings, &target("https://github.com/")).is_none());
    }

    #[test]
    fn campus_toggle_routes_whitelisted_addresses_through_the_proxy() {
        let mut settings = manual();
        // 172.25.100.105 在 campus-network.json 白名单里（172.25.0.0/19）。
        let target_url = target("http://172.25.100.105:8010/");
        assert!(resolve_for(&settings, &target_url).is_none());
        settings.campus_through_proxy = true;
        let (proxy, _) = resolve_for(&settings, &target_url).unwrap();
        assert_eq!(proxy.host_str(), Some("proxy.example"));
    }

    #[test]
    fn campus_domains_resolve_locally_so_the_campus_dns_applies() {
        let campus = target("https://authserver.szu.edu.cn/authserver/login");
        let public = target("https://github.com/");
        let socks = Url::parse("socks5h://proxy.example:1080").unwrap();

        // 校内 DNS 可用时把远端解析降级为本机解析。
        assert_eq!(
            enforce_campus_dns(socks.clone(), &campus, true).scheme(),
            "socks5"
        );
        // 校内 DNS 不可用则维持 SOCKS5 的默认行为。
        assert_eq!(
            enforce_campus_dns(socks.clone(), &campus, false).scheme(),
            "socks5h"
        );
        // 非校内域名不受影响。
        assert_eq!(
            enforce_campus_dns(socks.clone(), &public, true).scheme(),
            "socks5h"
        );
        // 非 SOCKS 代理的地址原样保留。
        let http = Url::parse("http://proxy.example:7890").unwrap();
        assert_eq!(enforce_campus_dns(http, &campus, true).scheme(), "http");
        // 账号密码等其它部分不受改动影响。
        let with_auth = Url::parse("socks5h://user:s3cret@proxy.example:1080").unwrap();
        let downgraded = enforce_campus_dns(with_auth, &campus, true);
        assert_eq!(downgraded.username(), "user");
        assert_eq!(downgraded.port(), Some(1080));
    }

    #[test]
    fn socks5_mode_resolves_campus_domains_locally() {
        // 校内域名只有开了「校内网也走代理」才会经过代理，那时才谈得上
        // SOCKS5 用本机还是远端解析。
        let settings = ProxySettings {
            protocol: model::PROTOCOL_SOCKS5.into(),
            campus_through_proxy: true,
            ..manual()
        };
        let (proxy, _) = resolve_for(&settings, &target("https://authserver.szu.edu.cn/")).unwrap();
        assert_eq!(proxy.scheme(), "socks5");
        let (proxy, _) = resolve_for(&settings, &target("https://github.com/")).unwrap();
        assert_eq!(proxy.scheme(), "socks5h");
    }

    #[test]
    fn test_rejects_non_http_targets() {
        let store = memory(None);
        let result = test(
            store.as_ref(),
            ProxySettingsInput {
                mode: model::MODE_OFF.into(),
                protocol: model::PROTOCOL_HTTP.into(),
                host: String::new(),
                port: 0,
                username: String::new(),
                password: None,
                campus_through_proxy: None,
            },
            Some("ftp://example.com/"),
        );
        assert_eq!(result.unwrap_err(), "测试地址必须是 http(s) 地址");
    }

    #[test]
    fn load_and_merge_read_the_vault_entry() {
        let settings = ProxySettings {
            mode: model::MODE_MANUAL.into(),
            protocol: model::PROTOCOL_SOCKS5.into(),
            host: "proxy.example".into(),
            port: 1080,
            username: "user".into(),
            password: "<SECRET_9e1e6b1e0e16a14d>".into(),
            campus_through_proxy: true,
        };
        let store = memory(Some(settings.clone()));
        assert_eq!(model::load(store.as_ref()).unwrap(), settings);

        let merged = model::merge_input(
            store.as_ref(),
            ProxySettingsInput {
                mode: model::MODE_MANUAL.into(),
                protocol: model::PROTOCOL_SOCKS5.into(),
                host: "proxy.example".into(),
                port: 1080,
                username: "user".into(),
password: None,
                campus_through_proxy: Some(true),
            },
        )
        .unwrap();
        assert_eq!(merged.password, "<SECRET_9e1e6b1e0e16a14d>");
        assert!(merged.campus_through_proxy);
    }

    #[test]
    fn merge_rejects_invalid_manual_settings_without_writing() {
        let store = memory(None);
        let result = model::merge_input(
            store.as_ref(),
            ProxySettingsInput {
                mode: model::MODE_MANUAL.into(),
                protocol: model::PROTOCOL_HTTP.into(),
                host: String::new(),
                port: 7890,
                username: String::new(),
                password: None,
                campus_through_proxy: None,
            },
        );
        assert!(result.is_err());
        assert!(model::load(store.as_ref()).unwrap().validate().is_ok());
    }
}