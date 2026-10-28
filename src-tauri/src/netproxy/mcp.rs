use crate::mcp::service::UnifiedSzuMcpService;
use crate::netproxy::{ProxyProbeView, ProxySettingsInput, ProxyStore};
use crate::storage::SharedCredentialStore;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetProxyParams {
    /// 代理模式：off（全部直连）/ auto（跟随系统代理）/ manual（使用下面的地址）
    pub mode: String,
    /// 代理协议：http / socks5（socks5 由代理远端解析 DNS）
    pub protocol: String,
    /// 代理服务器主机名或 IP，manual 模式必填
    #[serde(default)]
    pub host: String,
    /// 代理端口，manual 模式必填
    #[serde(default)]
    pub port: u16,
    /// 代理用户名，留空表示该代理不需要认证
    #[serde(default)]
    pub username: String,
    /// 代理密码；不填则沿用已保存的密码（改用户名会一并清空旧密码）
    #[serde(default)]
    pub password: Option<String>,
    /// 校内网（campus-network.json 白名单内的地址）是否也走代理，不填则沿用已保存值
    #[serde(default)]
    pub campus_through_proxy: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TestProxyParams {
    /// 代理模式：off / auto / manual
    pub mode: String,
    /// 代理协议：http / socks5
    pub protocol: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    /// 可选：把校内网也交给代理（按 campus-network.json 白名单判断）
    #[serde(default)]
    pub campus_through_proxy: Option<bool>,
    /// 可选的测试目标地址，缺省时使用内置的可达地址
    #[serde(default)]
    pub url: Option<String>,
}

impl UnifiedSzuMcpService {
    fn proxy_credentials(&self) -> Result<SharedCredentialStore, String> {
        Ok(self
            .app
            .try_state::<ProxyStore>()
            .ok_or("代理设置状态未注册")?
            .credentials
            .clone())
    }
}

#[tool_router(router = netproxy_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "netproxy_get_settings",
        description = "获取your-szu-life的全局网络代理设置（模式、协议、主机、端口、用户名）。出于安全考虑不返回代理密码，只返回是否已设置。对应资源 your-szu-life://data/network/proxy"
    )]
    pub async fn netproxy_get_settings(&self) -> Result<String, String> {
        let view = crate::netproxy::model::view(self.proxy_credentials()?.as_ref())?;
        serde_json::to_string_pretty(&view).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "netproxy_set_settings",
        description = "设置your-szu-life的全局网络代理。保存后立即对全部外部网络请求生效（应用更新、公告、AI、UOOC、OJ、教务/WebVPN 等）；校园网门户、宿舍电费与本地服务始终直连。对应资源 your-szu-life://data/network/proxy"
    )]
    pub async fn netproxy_set_settings(
        &self,
        Parameters(params): Parameters<SetProxyParams>,
    ) -> Result<String, String> {
        let input = ProxySettingsInput {
            mode: params.mode,
            protocol: params.protocol,
            host: params.host,
            port: params.port,
            username: params.username,
            password: params.password,
            campus_through_proxy: params.campus_through_proxy,
        };
        let view = crate::netproxy::save(self.proxy_credentials()?.as_ref(), &self.app, input)?;
        serde_json::to_string_pretty(&view).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "netproxy_test_proxy",
        description = "用给定的代理配置试连一次并返回 HTTP 状态码与耗时，用于在保存前确认代理可用。参数与 netproxy_set_settings 相同（密码沿用已保存值）。对应资源 your-szu-life://data/network/proxy"
    )]
    pub async fn netproxy_test_proxy(
        &self,
        Parameters(params): Parameters<TestProxyParams>,
    ) -> Result<String, String> {
        let credentials = self.proxy_credentials()?;
        let target = params.url.clone();
        let result: Result<ProxyProbeView, String> = tauri::async_runtime::spawn_blocking(move || {
            crate::netproxy::test(
                credentials.as_ref(),
                ProxySettingsInput {
                    mode: params.mode,
                    protocol: params.protocol,
                    host: params.host,
                    port: params.port,
                    username: params.username,
                    password: None,
                    campus_through_proxy: params.campus_through_proxy,
                },
                target.as_deref(),
            )
        })
        .await
        .map_err(|_| "代理连通性测试任务异常结束".to_owned())?;
        serde_json::to_string_pretty(&result?).map_err(|e| format!("序列化失败: {e}"))
    }
}