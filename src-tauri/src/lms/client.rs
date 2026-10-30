use reqwest::{Client as Http, Method, StatusCode, Url};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::{sync::Arc, time::Duration};

/// 畅课站点根地址。
pub(super) const LMS_ORIGIN: &str = "https://lms.szu.edu.cn/";
/// authserver CAS 登录入口，畅课未登录时会被重定向到这里。
pub(super) const AUTH_LOGIN: &str = "https://authserver.szu.edu.cn/authserver/login";

const MAX_REDIRECTS: usize = 10;
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// 会话引导结果。
#[derive(Debug)]
pub(super) enum Bootstrap {
    /// 会话可用，附带你落在的畅课页面 HTML（内含 globalData）。
    Established(String),
    /// 被重定向到 authserver 登录表单，需要先完成应用统一认证。
    NeedsAuth,
}

/// API 请求错误。`session_expired` 为真表示畅课会话失效，可尝试重新引导。
#[derive(Debug)]
pub(super) struct ApiError {
    pub message: String,
    pub session_expired: bool,
}

impl ApiError {
    fn local(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            session_expired: false,
        }
    }
}

pub(super) struct Client {
    http: Http,
    pub(super) jar: Arc<CookieStoreMutex>,
    lms: Url,
    auth: Url,
}

impl Client {
    /// `cookies` 是之前保存的畅课会话 jar（JSON，仅含 lms 域 cookie）；
    /// `sso_cookies` 是来自统一认证的 Set-Cookie 字符串，注入到 authserver 域。
    pub(super) fn new(
        lms_base: &str,
        auth_base: &str,
        cookies: Option<&str>,
        sso_cookies: &[String],
    ) -> Result<Self, String> {
        let lms = normalize_base(lms_base, "畅课地址无效")?;
        let auth = normalize_base(auth_base, "统一认证地址无效")?;
        let mut store = if cookies
            .map(|c| !c.is_empty())
            .unwrap_or(false)
        {
            cookie_store::serde::json::load_all(std::io::Cursor::new(cookies.unwrap()))
                .map_err(|_| "畅课会话数据损坏，将重新建立会话".to_string())
                .unwrap_or_default()
        } else {
            cookie_store::CookieStore::default()
        };
        for raw in sso_cookies {
            let _ = store.insert_raw(
                &cookie_store::RawCookie::parse(raw.clone())
                    .map_err(|_| "统一认证会话数据无效".to_string())?,
                &auth,
            );
        }
        let jar = Arc::new(CookieStoreMutex::new(store));
        let builder = crate::netproxy::client_builder()
            .cookie_provider(jar.clone())
            // CAS 跳转由模块自己逐跳跟随，便于识别登录表单并限定跳转目标。
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("SZULife/0.3 LMS");
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()?;
        Ok(Self {
            http: builder.build().map_err(|_| "无法建立畅课连接".to_string())?,
            jar,
            lms,
            auth,
        })
    }

    /// 保存 jar 中属于畅课域的 cookie（不含 authserver 会话，避免重复保存主凭据）。
    pub(super) fn cookies(&self) -> Result<String, String> {
        use cookie_store::CookieDomain;
        let jar = self.jar.lock().map_err(|_| "畅课会话不可用".to_string())?;
        let lms_host = self.lms.host_str().unwrap_or_default().to_owned();
        let in_lms_scope = |domain: &CookieDomain| match domain {
            CookieDomain::HostOnly(host) => host == &lms_host,
            CookieDomain::Suffix(suffix) => {
                suffix.trim_start_matches('.').trim_end_matches('.') == lms_host
            }
            _ => false,
        };
        let mut filtered = cookie_store::CookieStore::default();
        for cookie in jar.iter_any() {
            if in_lms_scope(&cookie.domain) && !cookie.is_expired() {
                let _ = filtered.insert_raw(&cookie.clone().into(), &self.lms);
            }
        }
        let mut bytes = vec![];
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(&filtered, &mut bytes)
            .map_err(|_| "无法保存畅课会话".to_string())?;
        String::from_utf8(bytes).map_err(|_| "会话编码无效".into())
    }

    /// 单次 GET（不跟随重定向），返回状态码、最终 URL、Location 与响应体。
    async fn fetch(
        &self,
        url: Url,
    ) -> Result<(StatusCode, Url, Option<String>, String), String> {
        let mut response = self
            .http
            .get(url.clone())
            .header("Referer", self.lms.as_str())
            .send()
            .await
            .map_err(|_| "无法连接畅课，请检查校园网或代理设置".to_string())?;
        let status = response.status();
        let final_url = response.url().clone();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "畅课响应读取中断".to_string())?
        {
            if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES as usize {
                return Err("畅课响应过大".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((
            status,
            final_url,
            location,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }

    fn is_lms(&self, url: &Url) -> bool {
        url.host_str() == self.lms.host_str()
    }

    /// 跳转目标是否可信：lms / authserver 两个主 host，或深大自己的任意子域。
    /// 畅课的 CAS 链路并不直达 authserver——中间经 `identity.szu.edu.cn`
    /// （Keycloak）broker 中转，因此必须放行整个 szu.edu.cn 域；
    /// 校外域一律拒绝，防止跟随任意重定向。
    pub(super) fn is_trusted_redirect(&self, url: &Url) -> bool {
        let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
            return false;
        };
        if host == self.lms.host_str().unwrap_or_default()
            || host == self.auth.host_str().unwrap_or_default()
        {
            return true;
        }
        host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")
    }

    /// 建立或恢复畅课会话：访问畅课首页并手动跟随 CAS 重定向。
    /// 会话有效时直接落在首页；会话失效但 authserver SSO 有效时经
    /// service ticket 自动重建；两者都失效时返回 [`Bootstrap::NeedsAuth`]。
    pub(super) async fn bootstrap(&self) -> Result<Bootstrap, String> {
        let mut url = self.lms.clone();
        for _ in 0..=MAX_REDIRECTS {
            let (status, final_url, location, text) = self.fetch(url).await?;
            if status.is_redirection() {
                let Some(location) = location else {
                    return Err("畅课登录跳转缺少目标地址".into());
                };
                let next = final_url
                    .join(&location)
                    .map_err(|_| "畅课跳转地址无效".to_string())?;
                if !self.is_trusted_redirect(&next) {
                    return Err(format!(
                        "畅课登录跳转目标不受信任: {}",
                        next.host_str().unwrap_or_default()
                    ));
                }
                url = next;
                continue;
            }
            if !status.is_success() {
                return Err(format!("畅课服务返回 HTTP {}", status.as_u16()));
            }
            if !self.is_lms(&final_url) {
                // SSO 流程最终停在统一认证侧（authserver 登录表单或 Keycloak
                // 登录页），说明统一认证会话已失效，需要重新认证。
                return Ok(Bootstrap::NeedsAuth);
            }
            // 落地页可能没有 globalData（登录壳/跳转页，实测 SSO 结束后前端
            // 再切到 /user/index），缺的时候补拉用户资料页。
            let html = if text.contains("globalData") {
                text
            } else {
                let profile_url = self
                    .lms
                    .join("user/index")
                    .map_err(|_| "畅课地址无效".to_string())?;
                let (profile_status, profile_landing, _, profile_html) =
                    self.fetch(profile_url).await?;
                if !profile_status.is_success() || !self.is_lms(&profile_landing) {
                    return Ok(Bootstrap::NeedsAuth);
                }
                profile_html
            };
            return Ok(Bootstrap::Established(html));
        }
        Err("畅课登录跳转次数过多".into())
    }

    /// GET 一个畅课 JSON 接口。会话失效（3xx/401）时 `session_expired` 为真。
    pub(super) async fn get_json(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, ApiError> {
        let url = self
            .lms
            .join(path.trim_start_matches('/'))
            .map_err(|_| ApiError::local("接口地址无效"))?;
        let response = self
            .http
            .request(Method::GET, url)
            .query(query)
            .header("Referer", self.lms.as_str())
            .send()
            .await
            .map_err(|_| ApiError::local("无法连接畅课，请检查校园网或代理设置"))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ApiError::local("畅课响应读取中断"))?;
        if status.is_redirection() || status == StatusCode::UNAUTHORIZED {
            return Err(ApiError {
                message: "畅课会话已失效".into(),
                session_expired: true,
            });
        }
        if !status.is_success() {
            return Err(ApiError::local(format!("畅课服务返回 HTTP {}", status.as_u16())));
        }
        serde_json::from_slice(&bytes).map_err(|_| ApiError::local("畅课接口响应格式不兼容"))
    }
}

fn normalize_base(raw: &str, message: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| message.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(message.into());
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}
