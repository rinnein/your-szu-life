use super::model::{parse_semester_list, Envelope, LiveStreamSource};
use base64::Engine as _;
use reqwest::{Client as Http, Method, StatusCode, Url};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

/// 督导巡课站点根地址。
pub const JXXK_ORIGIN: &str = "https://jxxk.szu.edu.cn/";
/// authserver CAS 登录入口，巡课未登录时会被重定向到这里。
pub const AUTH_LOGIN: &str = "https://authserver.szu.edu.cn/authserver/login";

const MAX_REDIRECTS: usize = 15;
const MAX_HTML_BYTES: u64 = 4 * 1024 * 1024;
/// 流代理单次响应上限（m3u8 索引与分片都远小于该值）。
const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;
/// token cookie 的基础名（完整名 = 基础名 + "_" + COLLEGE_UNIVERSITY_DOMAIN 值）。
const TOKEN_COOKIE: &str = "COLLEGE_UNIVERSITY_TOKEN";
/// 记录 token cookie 域后缀的 cookie 名。
const DOMAIN_COOKIE: &str = "COLLEGE_UNIVERSITY_DOMAIN";
/// 业务错误码：登录会话已过期。
const CODE_SESSION_EXPIRED: i64 = -10000;

/// 会话引导结果。
#[derive(Debug)]
pub enum Bootstrap {
    /// 会话可用（cookie jar 中已有 token）。
    Established,
    /// 被重定向到 authserver 登录表单，需要先完成应用统一认证。
    NeedsAuth,
}

/// API 请求错误。
#[derive(Debug)]
pub struct ApiError {
    pub message: String,
}

impl ApiError {
    fn local(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub struct Client {
    http: Http,
    pub jar: Arc<CookieStoreMutex>,
    jxxk: Url,
    auth: Url,
    /// 业务请求所需的 token（JWT），从 cookie jar 中的 token cookie 提取。
    token: Mutex<Option<String>>,
}

impl Client {
    /// `cookies` 是之前保存的巡课会话 jar（JSON，仅含 jxxk 域 cookie）；
    /// `sso` 保留统一认证的原生 Cookie 身份，只投影当前 HTTP 上下文。
    pub fn new(
        jxxk_base: &str,
        auth_base: &str,
        cookies: Option<&str>,
        sso: Option<&CookieStoreMutex>,
    ) -> Result<Self, String> {
        let jxxk = normalize_base(jxxk_base, "巡课地址无效")?;
        let auth = normalize_base(auth_base, "统一认证地址无效")?;
        let mut store = if cookies.map(|c| !c.is_empty()).unwrap_or(false) {
            cookie_store::serde::json::load_all(std::io::Cursor::new(cookies.unwrap()))
                .map_err(|_| "巡课会话数据损坏，将重新建立会话".to_string())
                .unwrap_or_default()
        } else {
            cookie_store::CookieStore::default()
        };
        if let Some(sso) = sso {
            for cookie in sso
                .lock()
                .map_err(|_| "统一认证会话锁失败")?
                .iter_unexpired()
            {
                let _ = store.insert(cookie.clone(), &auth);
            }
        }
        let jar = Arc::new(CookieStoreMutex::new(store));
        if let Some(sso) = sso {
            szu_core::webview_cookies::copy_snapshot(sso, &jar);
        }
        let builder = szu_core::netproxy::client_builder()
            .cookie_provider(szu_core::webview_cookies::http_provider(jar.clone()))
            // CAS 跳转由模块自己逐跳跟随，便于识别登录表单并限定跳转目标。
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("SZULife/0.3 JXXK");
        #[cfg(target_os = "android")]
        szu_core::platform_tls::initialize_platform_verifier()?;
        let client = Self {
            http: builder
                .build()
                .map_err(|_| "无法建立巡课连接".to_string())?,
            jar,
            jxxk,
            auth,
            token: Mutex::new(None),
        };
        client.reload_token_from_jar();
        Ok(client)
    }

    pub fn has_token(&self) -> bool {
        self.token.lock().map(|t| t.is_some()).unwrap_or(false)
    }

    /// 当前校园网状态下巡课请求实际使用的 origin（overrides 的 origin 形态改写目标）。
    /// 校外/裸主机形态的规则只影响 DNS，返回 `None`。
    fn effective_endpoint(&self) -> Option<Url> {
        szu_core::netproxy::origin_override_target(self.jxxk.host_str()?)
    }

    /// 把 cookie 写进 jar，同时落在公开 origin 与改写后的 origin 上：
    /// 请求经 `netproxy::rewrite_origin` 后实际连的是改写目标，cookie 必须
    /// 归属到它才会被发送；公开 origin 上的副本保证会话跨网络环境迁移时
    /// 仍能恢复（token 从 jar 恢复按名字扫描，不区分域）。返回是否有写入成功。
    fn insert_cookie(&self, raw: &str) -> bool {
        let endpoint = self.effective_endpoint();
        let mut targets: Vec<&Url> = vec![&self.jxxk];
        if let Some(target) = &endpoint {
            targets.push(target);
        }
        let mut inserted = false;
        if let Ok(mut jar) = self.jar.lock() {
            for target in targets {
                if let Ok(parsed) = cookie_store::RawCookie::parse(raw.to_owned()) {
                    if jar.insert_raw(&parsed, target).is_ok() {
                        inserted = true;
                    }
                }
            }
        }
        inserted
    }

    /// 判断当前 token 是否即将到期（距过期不足 15 分钟，或无 exp 字段）。
    /// 只有确实需要续期时才触发网络 refreshToken，避免频繁刷新打爆平台或并发冲突。
    pub fn token_needs_refresh(&self) -> bool {
        let Some(token) = self.token() else {
            return false;
        };
        token_needs_refresh_for(&token)
    }

    pub fn token(&self) -> Option<String> {
        self.token.lock().ok().and_then(|t| t.clone())
    }

    /// 从 jar 中的 token cookie 恢复内存 token；过期 token 视为不存在，
    /// 避免「有旧 token 就跳过 CAS 重建」的死循环。
    pub fn reload_token_from_jar(&self) {
        let domain_suffix = self.jar_cookie_value(DOMAIN_COOKIE).unwrap_or_default();
        let name = format!("{TOKEN_COOKIE}_{domain_suffix}");
        let fresh = self
            .jar_cookie_value(&name)
            .filter(|value| token_is_fresh(value));
        *self.token.lock().unwrap() = fresh;
    }

    fn jar_cookie_value(&self, name: &str) -> Option<String> {
        let jar = self.jar.lock().ok()?;
        for cookie in jar.iter_any() {
            if cookie.name() == name && !cookie.is_expired() {
                return Some(cookie.value().to_owned());
            }
        }
        None
    }

    /// 把新 token 写回 jar 的 token cookie（域后缀沿用 COLLEGE_UNIVERSITY_DOMAIN），
    /// 保证会话持久化后重启仍能恢复。
    pub fn store_token(&self, token: &str) {
        *self.token.lock().unwrap() = Some(token.to_owned());
        let suffix = self.jar_cookie_value(DOMAIN_COOKIE).unwrap_or_default();
        // Max-Age 与 JWT 有效期一致，cookie_store 重载后生命周期语义不漂移。
        let max_age = token_exp(token)
            .map(|exp| (exp - chrono::Utc::now().timestamp()).max(1).to_string())
            .unwrap_or_else(|| "7200".to_string());
        let raw = format!("{TOKEN_COOKIE}_{suffix}={token}; Path=/; HttpOnly; Max-Age={max_age}");
        self.insert_cookie(&raw);
    }

    /// 保存 jar 中适用于巡课请求的 cookie（不含 authserver 会话，避免重复
    /// 保存主凭据）。用 `matches` 而非域名精确比较：平台的 token cookie
    /// 可能带父域（Domain=szu.edu.cn），按 host 精确过滤会把会话丢掉；
    /// 校内改写目标 origin 上的 cookie 同样要保留，否则校内建立的会话
    /// 重启后无法恢复。
    pub fn cookies(&self) -> Result<String, String> {
        let endpoint = self.effective_endpoint();
        let jar = self.jar.lock().map_err(|_| "巡课会话不可用".to_string())?;
        let mut filtered = cookie_store::CookieStore::default();
        for cookie in jar.iter_any() {
            if !cookie.is_expired() && cookie.matches(&self.jxxk) {
                let _ = filtered.insert_raw(&cookie.clone().into(), &self.jxxk);
            } else if let Some(endpoint) = &endpoint {
                if !cookie.is_expired() && cookie.matches(endpoint) {
                    let _ = filtered.insert_raw(&cookie.clone().into(), endpoint);
                }
            }
        }
        let mut bytes = vec![];
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(&filtered, &mut bytes)
            .map_err(|_| "无法保存巡课会话".to_string())?;
        String::from_utf8(bytes).map_err(|_| "会话编码无效".into())
    }

    /// 单次 GET（不跟随重定向），返回状态码、最终 URL、Location、Set-Cookie 头与响应体。
    async fn fetch(
        &self,
        url: Url,
    ) -> Result<(StatusCode, Url, Option<String>, Vec<String>, String), String> {
        let mut url = url;
        szu_core::netproxy::rewrite_origin(&mut url);
        let mut response = self
            .http
            .get(url.clone())
            .header("Referer", self.jxxk.as_str())
            .send()
            .await
            .map_err(|e| {
                log::warn!("[jxxk] 连接巡课平台失败 {url}: {e}");
                "无法连接巡课平台，请检查校园网或代理设置".to_string()
            })?;
        let status = response.status();
        let final_url = response.url().clone();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let set_cookies: Vec<String> = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .map(str::to_owned)
            .collect();
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "巡课平台响应读取中断".to_string())?
        {
            if bytes.len() + chunk.len() > MAX_HTML_BYTES as usize {
                return Err("巡课平台响应过大".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((
            status,
            final_url,
            location,
            set_cookies,
            String::from_utf8_lossy(&bytes).into_owned(),
        ))
    }

    fn is_jxxk(&self, url: &Url) -> bool {
        if url.host_str() == self.jxxk.host_str() {
            return true;
        }
        // 校内 origin 改写后请求落在目标 origin 上，落地判定必须同时认它。
        self.effective_endpoint()
            .is_some_and(|endpoint| same_origin(&endpoint, url))
    }

    /// 跳转目标是否可信：巡课 / authserver 两个主 host，或深大自己的任意子域。
    /// 与畅课一致，CAS 链路经 `identity.szu.edu.cn`（Keycloak）中转。
    pub fn is_trusted_redirect(&self, url: &Url) -> bool {
        let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
            return false;
        };
        if host == self.jxxk.host_str().unwrap_or_default()
            || host == self.auth.host_str().unwrap_or_default()
        {
            return true;
        }
        // 校内改写目标（如 172.31.63.8）也是巡课自己的入口。
        if self
            .effective_endpoint()
            .is_some_and(|endpoint| endpoint.host_str() == Some(host))
        {
            return true;
        }
        host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")
    }

    /// 建立或恢复巡课会话，手动跟随 CAS 重定向。
    ///
    /// 平台首页是公开的（未登录也返回 200），不会触发 SSO 跳转；真正的 SSO
    /// 入口是 `GET /university-sso/ava/sso`——官方登录页（/university-sso/ava/login）
    /// 只是设置 fromUrl cookie 后用 JS 跳到这里，带票据回来后由该端点下发
    /// token cookie。成功判据是最终落在巡课域且 cookie jar 中出现新鲜 token。
    pub async fn bootstrap(&self) -> Result<Bootstrap, String> {
        self.reload_token_from_jar();
        if self.has_token() {
            return Ok(Bootstrap::Established);
        }
        self.seed_from_url_cookie();
        let mut url = self
            .jxxk
            .join("university-sso/ava/sso")
            .map_err(|_| "巡课 SSO 地址无效".to_string())?;
        for hop in 0..=MAX_REDIRECTS {
            let (status, final_url, location, set_cookies, text) = self.fetch(url.clone()).await?;
            log::info!(
                "[jxxk] bootstrap hop {hop}: GET {url} -> HTTP {} 落地 {} Location={} Set-Cookie={}",
                status.as_u16(),
                final_url.host_str().unwrap_or_default(),
                location.as_deref().unwrap_or("-"),
                if set_cookies.is_empty() {
                    "-".to_string()
                } else {
                    set_cookies.join(" | ")
                }
            );
            if status.is_redirection() {
                let Some(location) = location else {
                    return Err("巡课登录跳转缺少目标地址".into());
                };
                let next = final_url
                    .join(&location)
                    .map_err(|_| "巡课跳转地址无效".to_string())?;
                if !self.is_trusted_redirect(&next) {
                    return Err(format!(
                        "巡课登录跳转目标不受信任: {}",
                        next.host_str().unwrap_or_default()
                    ));
                }
                url = next;
                continue;
            }
            if !status.is_success() {
                return Err(format!("巡课平台返回 HTTP {}", status.as_u16()));
            }
            if !self.is_jxxk(&final_url) {
                // 停在 authserver 登录表单或 Keycloak 页面：统一认证会话已失效。
                return Ok(Bootstrap::NeedsAuth);
            }
            // 平台可能在页面脚本里 setCookie 下发 token（而非 Set-Cookie 头）。
            let absorbed = self.absorb_script_cookies(&text);
            self.reload_token_from_jar();
            if self.has_token() {
                log::info!(
                    "[jxxk] bootstrap: 从页面脚本提取到 cookie：{}",
                    absorbed.join("、")
                );
                return Ok(Bootstrap::Established);
            }
            // 中转页也可能把 JWT 内嵌在脚本里（document.cookie 拼接写入），
            // setCookie 模式抓不到，这里直接按 JWT 形态提取。
            if let Some(jwt) = extract_jwt_token(&text) {
                self.store_token(&jwt);
                log::info!(
                    "[jxxk] bootstrap: 从页面正文提取到 token（{} 字符）",
                    jwt.len()
                );
                return Ok(Bootstrap::Established);
            }
            // 中转页常用 JS 跳转推进登录流程（票据校验后回弹 sso 的「第二次
            // 进入」就是靠它完成），等价于浏览器跟随 location.href。
            let Some(js_target) = extract_script_redirect(&text) else {
                // 平台错误页（如「单点登录失败：feign接口失败：当前超过最大登录数」）
                // 说明票据校验已通过、是平台侧拒绝建会话，必须如实上报而非按未认证处理。
                if let Some(reason) = extract_error_page_message(&text) {
                    log::warn!("[jxxk] bootstrap: 平台拒绝建立会话：{reason}");
                    return Err(format!("巡课平台拒绝登录：{reason}"));
                }
                let dump_path = std::env::temp_dir().join(format!("jxxk_bootstrap_hop{hop}.html"));
                if let Err(err) = std::fs::write(&dump_path, &text) {
                    log::warn!("[jxxk] bootstrap: 页面正文转储失败: {err}");
                } else {
                    log::warn!(
                        "[jxxk] bootstrap: 未取得 token，最终页面正文已转储到 {}（{} 字节）",
                        dump_path.display(),
                        text.len()
                    );
                }
                log::warn!(
                    "[jxxk] bootstrap: 落地巡课域但未取得 token（{}）（脚本 cookie：{}；页面无 JS 跳转），按未认证处理",
                    final_url.as_str(),
                    absorbed.join("、")
                );
                return Ok(Bootstrap::NeedsAuth);
            };
            let next = final_url
                .join(&js_target)
                .map_err(|_| "巡课页面 JS 跳转地址无效".to_string())?;
            if !self.is_trusted_redirect(&next) {
                log::warn!(
                    "[jxxk] bootstrap: 页面 JS 跳转目标不受信任: {}",
                    next.host_str().unwrap_or_default()
                );
                return Ok(Bootstrap::NeedsAuth);
            }
            log::info!("[jxxk] bootstrap: 跟随页面 JS 跳转 -> {}", next.as_str());
            url = next;
        }
        Err("巡课登录跳转次数过多".into())
    }

    /// 把 sso 中转页脚本里的 setCookie 调用写入 cookie jar，返回写入的 cookie 名。
    fn absorb_script_cookies(&self, html: &str) -> Vec<String> {
        let mut names = Vec::new();
        for (name, value) in extract_script_cookies(html) {
            let raw = format!("{name}={value}; Path=/");
            if self.insert_cookie(&raw) {
                names.push(name);
            }
        }
        names
    }

    /// 模拟官方登录页的行为：确保 fromUrl cookie 存在，SSO 成功后平台才会
    /// 跳回应用首页。
    fn seed_from_url_cookie(&self) {
        if self.jar_cookie_value("fromUrl").is_some() {
            return;
        }
        let raw = format!("fromUrl={}; Path=/", url_escape(self.jxxk.as_str()));
        self.insert_cookie(&raw);
    }

    /// 用旧 token 换新 token（平台在到期前 900 秒会走同样的刷新）。
    pub async fn refresh_token(&self) -> Result<(), String> {
        let Some(old) = self.token() else {
            return Err("当前没有可刷新的巡课 token".into());
        };
        let url = self
            .jxxk
            .join("university-user/public/refreshToken")
            .map_err(|_| "刷新地址无效".to_string())?;
        let mut url = url;
        szu_core::netproxy::rewrite_origin(&mut url);
        let response = self
            .http
            .post(url.clone())
            .header("token", old)
            .header("Referer", self.jxxk.as_str())
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| {
                log::warn!("[jxxk] 刷新 token 连接失败 {url}: {e}");
                "无法连接巡课平台刷新 token".to_string()
            })?;
        if !response.status().is_success() {
            return Err(format!(
                "刷新巡课 token 失败: HTTP {}",
                response.status().as_u16()
            ));
        }
        let envelope: Envelope = response
            .json()
            .await
            .map_err(|_| "刷新巡课 token 响应格式不兼容".to_string())?;
        let token = envelope
            .data
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| "刷新巡课 token 未返回新凭据".to_string())?;
        self.store_token(token);
        Ok(())
    }

    /// GET 一个巡课 JSON 接口，自动携带 token 请求头。
    /// 返回统一包裹的 `data` 字段；会话过期时 `session_expired` 为真。
    pub async fn get_json(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Value, ApiError> {
        let url = self
            .jxxk
            .join(path.trim_start_matches('/'))
            .map_err(|_| ApiError::local("接口地址无效"))?;
        let mut url = url;
        szu_core::netproxy::rewrite_origin(&mut url);
        let query_clone = query
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        let display_url = format!("{url}?{query_clone}");
        let mut request = self
            .http
            .request(Method::GET, url)
            .query(query)
            .header("Referer", self.jxxk.as_str());
        if let Some(token) = self.token() {
            request = request.header("token", token);
        }
        let response = request.send().await.map_err(|e| {
            log::warn!("[jxxk] GET 请求失败 {display_url}: {e}");
            ApiError::local("无法连接巡课平台，请检查校园网或代理设置")
        })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ApiError::local("巡课平台响应读取中断"))?;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ApiError::local("巡课会话已失效"));
        }
        if !status.is_success() {
            return Err(ApiError::local(format!(
                "巡课平台返回 HTTP {}",
                status.as_u16()
            )));
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::local("巡课接口响应格式不兼容"))?;
        if envelope.code_is(CODE_SESSION_EXPIRED) {
            return Err(ApiError::local(envelope.message));
        }
        if !envelope.success {
            return Err(ApiError::local(if envelope.message.is_empty() {
                format!("巡课接口返回错误码 {:?}", envelope.code)
            } else {
                envelope.message
            }));
        }
        Ok(envelope.data)
    }

    /// GET 巡课域内的相对路径（自动带 token 头），返回响应体与 Content-Type。
    pub async fn fetch_path(&self, path: &str) -> Result<(Vec<u8>, String), String> {
        let url = self
            .jxxk
            .join(path.trim_start_matches('/'))
            .map_err(|_| "接口地址无效".to_string())?;
        self.fetch_stream(&url).await
    }

    /// 拉取学年学期列表。
    pub async fn get_semesters(&self) -> Result<Vec<super::model::Semester>, String> {
        let value = self
            .post_json(
                "university-basic/public/schoolYearSemester/pageByCondition",
                &serde_json::json!({
                    "pageIndex": 1,
                    "pageSize": 50,
                }),
            )
            .await
            .map_err(|e| e.message)?;
        parse_semester_list(&value)
    }

    /// POST 一个巡课 JSON 接口（目前仅学期列表与 token 刷新使用）。
    async fn post_json(&self, path: &str, body: &Value) -> Result<Value, ApiError> {
        let url = self
            .jxxk
            .join(path.trim_start_matches('/'))
            .map_err(|_| ApiError::local("接口地址无效"))?;
        let mut url = url;
        szu_core::netproxy::rewrite_origin(&mut url);
        let display_url = url.to_string();
        let mut request = self
            .http
            .request(Method::POST, url)
            .header("Referer", self.jxxk.as_str())
            .json(body);
        if let Some(token) = self.token() {
            request = request.header("token", token);
        }
        let response = request.send().await.map_err(|e| {
            log::warn!("[jxxk] POST 请求失败 {display_url}: {e}");
            ApiError::local("无法连接巡课平台，请检查校园网或代理设置")
        })?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ApiError::local("巡课平台响应读取中断"))?;
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(ApiError::local("巡课会话已失效"));
        }
        if !status.is_success() {
            return Err(ApiError::local(format!(
                "巡课平台返回 HTTP {}",
                status.as_u16()
            )));
        }
        let envelope: Envelope = serde_json::from_slice(&bytes)
            .map_err(|_| ApiError::local("巡课接口响应格式不兼容"))?;
        if envelope.code_is(CODE_SESSION_EXPIRED) {
            return Err(ApiError::local(envelope.message));
        }
        if !envelope.success {
            return Err(ApiError::local(if envelope.message.is_empty() {
                format!("巡课接口返回错误码 {:?}", envelope.code)
            } else {
                envelope.message
            }));
        }
        Ok(envelope.data)
    }

    /// GET 一个巡课域的绝对 URL（流代理用），返回响应体与 Content-Type。
    /// 仅放行深大域与校内合法目标，防止被当作任意代理使用。
    pub async fn fetch_stream(&self, url: &Url) -> Result<(Vec<u8>, String), String> {
        let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
            return Err("流地址缺少主机名".into());
        };
        if !super::stream::trusted_target(url) {
            return Err(format!("流地址不受信任: {host}"));
        }
        // 信任校验按原始 URL 做，实际连接走改写后的 origin（如校内 http 入口）。
        let mut url = url.clone();
        szu_core::netproxy::rewrite_origin(&mut url);
        let mut request = self
            .http
            .get(url.clone())
            .header("Referer", self.jxxk.as_str())
            .timeout(Duration::from_secs(60));
        // protect 前缀的媒体接口（如 AI 字幕 VTT）需要 token 头。
        if let Some(token) = self.token() {
            request = request.header("token", token);
        }
        let response = request.send().await.map_err(|e| {
            log::warn!("[jxxk] 拉取巡课媒体失败 {url}: {e}");
            "拉取巡课媒体数据失败".to_string()
        })?;
        if !response.status().is_success() {
            return Err(format!(
                "巡课媒体服务返回 HTTP {}",
                response.status().as_u16()
            ));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "巡课媒体数据读取中断".to_string())?
            .to_vec();
        if bytes.len() > MAX_STREAM_BYTES {
            return Err("巡课媒体数据过大".into());
        }
        Ok((bytes, content_type))
    }
}

/// 两个 URL 是否同 origin（scheme、host、端口按 scheme 默认值归一）。
fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str().map(|h| h.trim_end_matches('.'))
            == b.host_str().map(|h| h.trim_end_matches('.'))
        && a.port_or_known_default() == b.port_or_known_default()
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

/// 从课次直播对象 JSON 中提取回放 mediaVideoId（mediaVideoIds[0]）。
pub fn extract_media_video_id(live: &Value) -> Option<String> {
    live.get("mediaVideoIds")
        .and_then(|v| v.as_array())
        .and_then(|ids| ids.first())
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// best-effort 地从直播流接口响应中提取播放地址。
/// 从直播流接口（university-live/protect/live/getLiveStream）提取播放地址。
/// 奥威亚平台典型返回：数组或含 streams 的对象；字段含 hls / flv / wsFlv 与 signalName / streamName / streamLabel。
pub fn parse_live_streams(data: &Value) -> Vec<LiveStreamSource> {
    // 优先采用 HLS（m3u8 协议，前端 hls.js 原生支持），其次兼容各种 FLV/直接播放链接
    const URL_KEYS: [&str; 7] = ["hls", "playUrl", "hlsUrl", "url", "flv", "flvUrl", "wsFlv"];
    const LABEL_KEYS: [&str; 5] = ["signalName", "streamName", "name", "label", "tag"];
    let mut sources = Vec::new();
    let mut collect = |item: &Value| {
        // 如果明确标注该流不可用（ifAvaliable == false），则略过
        if item.get("ifAvaliable").and_then(Value::as_bool) == Some(false) {
            return;
        }
        let mut url: Option<String> = None;
        for key in URL_KEYS {
            if let Some(value) = item.get(key).and_then(Value::as_str) {
                if !value.is_empty() {
                    url = Some(value.to_owned());
                    break;
                }
            }
        }
        let Some(url) = url else { return };
        let mut label = LABEL_KEYS
            .iter()
            .find_map(|key| item.get(*key).and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        // 若无显式名称，尝试从 streamLabel / streamNo 推导人性化标签
        if label.is_none() {
            if let Some(stream_label) = item.get("streamLabel").and_then(Value::as_str) {
                label = match stream_label {
                    "TP" => Some("教师画面".to_string()),
                    "SP" => Some("学生画面".to_string()),
                    "PC" => Some("电脑画面".to_string()),
                    "PRIMARY" => Some("主画面".to_string()),
                    "HD" => Some("高清画面".to_string()),
                    "SD" => Some("标清画面".to_string()),
                    other => Some(other.to_string()),
                };
            }
        }
        if label.is_none() {
            if let Some(stream_no) = item.get("streamNo").and_then(Value::as_i64) {
                label = match stream_no {
                    0 => Some("主画面".to_string()),
                    1 => Some("教师画面".to_string()),
                    2 => Some("学生画面".to_string()),
                    4 => Some("标清画面".to_string()),
                    30 => Some("电脑画面".to_string()),
                    n => Some(format!("画面 {n}")),
                };
            }
        }
        if !sources.iter().any(|s: &LiveStreamSource| s.url == url) {
            sources.push(LiveStreamSource { url, label });
        }
    };
    let candidates: [Option<&Value>; 4] = [
        Some(data),
        data.get("streams"),
        data.get("content"),
        data.get("list"),
    ];
    for candidate in candidates.into_iter().flatten() {
        if let Some(items) = candidate.as_array() {
            for item in items {
                if item.is_object() {
                    collect(item);
                }
            }
        }
    }
    sources
}

/// 从 SSO 中转页 HTML 提取 `setCookie('name', 'value', ...)` 调用。
/// 平台在票据校验完成后通过页面脚本（而非 Set-Cookie 响应头）下发 token。
pub fn extract_script_cookies(html: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut search = html;
    while let Some(pos) = search.find("setCookie") {
        search = &search[pos + "setCookie".len()..];
        let Some(after_open) = search.trim_start().strip_prefix("(") else {
            continue;
        };
        let Some((name, after_name)) = read_quoted(after_open) else {
            continue;
        };
        let Some((value, after_value)) = read_quoted(after_name) else {
            continue;
        };
        search = after_value;
        if !name.is_empty() && !value.is_empty() {
            pairs.push((name, value));
        }
    }
    pairs
}

fn read_quoted(s: &str) -> Option<(String, &str)> {
    // 实参之间以逗号分隔；首个实参前没有逗号，后续实参前需要先跳过。
    let trimmed = s.trim_start().trim_start_matches(',').trim_start();
    let quote = trimmed.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut out = String::new();
    let mut chars = trimmed[1..].char_indices();
    while let Some((idx, ch)) = chars.next() {
        if ch == quote {
            let end = 1 + idx + ch.len_utf8();
            return Some((out, &trimmed[end..]));
        }
        if ch == '\\' {
            match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, other)) => out.push(other),
                None => break,
            }
            continue;
        }
        out.push(ch);
    }
    None
}

/// 从页面正文提取 JWT 形态的 token（三段式 eyJ 开头）。中转页把 token 内嵌
/// 在脚本里再以 document.cookie 拼接写入时，setCookie 模式无法覆盖。
pub fn extract_jwt_token(html: &str) -> Option<String> {
    let mut search = html;
    while let Some(pos) = search.find("eyJ") {
        let rest = &search[pos..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'))
            .unwrap_or(rest.len());
        let candidate = &rest[..end];
        let segments: Vec<&str> = candidate.split('.').collect();
        if segments.len() == 3
            && segments.iter().all(|seg| !seg.is_empty())
            && candidate.len() >= 40
        {
            return Some(candidate.to_owned());
        }
        if end == 0 {
            break;
        }
        search = &rest[end..];
    }
    None
}

/// 从中转页 HTML 提取 JS/meta 跳转目标（location.href/replace 与 meta refresh）。
pub fn extract_script_redirect(html: &str) -> Option<String> {
    for marker in [
        "window.location.href=",
        "window.location.href =",
        "location.href=",
        "location.href =",
        "location.replace(",
    ] {
        let mut search = html;
        while let Some(pos) = search.find(marker) {
            search = &search[pos + marker.len()..];
            if let Some((target, _)) = read_quoted(search) {
                let trimmed = target.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_owned());
                }
            }
        }
    }
    // <meta http-equiv="refresh" content="0;url=...">
    if let Some(pos) = html.find("http-equiv=\"refresh\"") {
        let rest = &html[pos..];
        if let Some(url_pos) = rest.find("url=") {
            let tail = rest[url_pos + 4..].trim_start();
            let end = tail.find(['"', '>']).unwrap_or(tail.len());
            let target = tail[..end].trim().trim_end_matches('"');
            if !target.is_empty() {
                return Some(target.to_owned());
            }
        }
    }
    None
}

/// 从平台错误页提取提示文案。错误页形如
/// `<p class="nodata-tips">单点登录失败：feign接口失败：当前超过最大登录数</p>`。
/// 样式表里会先出现 `.nodata-tips { ... }` 规则，因此逐个出现位置向后取
/// 标签间文本，取第一个非空者。
pub fn extract_error_page_message(html: &str) -> Option<String> {
    let mut search = html;
    while let Some(pos) = search.find("nodata-tips") {
        let rest = &search[pos..];
        let Some(start) = rest.find('>') else {
            return None;
        };
        let tail = &rest[start + 1..];
        let text = match tail.find('<') {
            Some(end) => tail[..end].trim(),
            None => tail.trim(),
        };
        if !text.is_empty() {
            return Some(text.to_owned());
        }
        search = tail;
    }
    None
}

/// 从 JWT payload 提取 exp（秒级 Unix 时间）；解析失败返回 None。
fn token_exp(token: &str) -> Option<i64> {
    let payload_b64 = token.split('.').nth(1)?;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    value.get("exp")?.as_i64()
}

/// 判断 JWT token 是否仍未过期（payload 中的 exp，秒级 Unix 时间）。
/// 无 exp 字段或解析失败时按未过期处理，交由接口层的刷新/重建兜底。
fn token_is_fresh(token: &str) -> bool {
    match token_exp(token) {
        Some(exp) => chrono::Utc::now().timestamp() < exp,
        // 无 exp 或解析失败按未过期处理，交由接口层的刷新/重建兜底。
        None => true,
    }
}

pub fn token_needs_refresh_for(token: &str) -> bool {
    match token_exp(token) {
        Some(exp) => chrono::Utc::now().timestamp() + 900 >= exp,
        None => true,
    }
}

/// 等价于 JS encodeURIComponent 的最小实现（用于 fromUrl cookie 值）。
fn url_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_freshness_follows_exp() {
        let past = chrono::Utc::now().timestamp() - 3600;
        let future = chrono::Utc::now().timestamp() + 3600;
        let encode = |exp: i64| {
            let payload = serde_json::json!({ "exp": exp }).to_string();
            format!(
                "h.{}.s",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes())
            )
        };
        assert!(token_is_fresh(&encode(future)));
        assert!(!token_is_fresh(&encode(past)));
        // 无 exp / 非 JWT 形状一律视为新鲜，交给接口层兜底。
        assert!(token_is_fresh("no-exp-payload"));
        assert!(token_is_fresh("garbage"));
    }

    #[test]
    fn url_escape_matches_encode_uri_component() {
        assert_eq!(
            url_escape("https://jxxk.szu.edu.cn/#/"),
            "https%3A%2F%2Fjxxk.szu.edu.cn%2F%23%2F"
        );
    }

    #[test]
    fn error_page_message_is_extracted() {
        // 错误页的 <style> 里先出现 .nodata-tips 规则，提取必须跳过它。
        let html = "<html><title>错误提示</title><style>\
                    .nodata-tips {\n        font-size: 16px;\n    }\
                    </style>\
                    <p class=\"nodata-tips\">单点登录失败：feign接口失败：当前超过最大登录数</p>\
                    </html>";
        assert_eq!(
            extract_error_page_message(html).as_deref(),
            Some("单点登录失败：feign接口失败：当前超过最大登录数")
        );
        assert_eq!(extract_error_page_message("<p>没有提示段落</p>"), None);
        assert_eq!(extract_error_page_message(""), None);
    }
}
