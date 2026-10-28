use reqwest::{cookie::CookieStore as _, Client as Http, Method, Url};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) struct Client {
    pub http: Http,
    pub jar: Arc<CookieStoreMutex>,
    pub base: Url,
    pub cancel: CancellationToken,
}
#[derive(Debug)]
pub(super) struct Error {
    pub message: String,
    pub status: Option<u16>,
    pub uncertain: bool,
    pub response: Option<String>,
}
impl Error {
    pub fn local(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            status: None,
            uncertain: false,
            response: None,
        }
    }

    pub fn display(&self) -> String {
        let status = self
            .status
            .map(|value| format!(" HTTP {value}"))
            .unwrap_or_default();
        match self.response.as_deref() {
            Some(response) if !response.is_empty() => {
                format!("{}{status}\n原始响应：\n{response}", self.message)
            }
            _ => format!("{}{status}", self.message),
        }
    }
}

fn response_text(bytes: &[u8]) -> String {
    const MAX_RESPONSE_BYTES: usize = 64 * 1024;
    let truncated = bytes.len() > MAX_RESPONSE_BYTES;
    let mut text =
        String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_RESPONSE_BYTES)]).into_owned();
    if truncated {
        text.push_str("\n[响应已截断]");
    }
    text
}
pub(super) fn normalize(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| "OJ 地址无效")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("请输入不含账户、查询参数和片段的 HTTP(S) 地址".into());
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}
impl Client {
    pub fn new(base: &str, cookies: &str) -> Result<Self, String> {
        let base = normalize(base)?;
        let store = if cookies.is_empty() {
            cookie_store::CookieStore::default()
        } else {
            cookie_store::serde::json::load_all(std::io::Cursor::new(cookies))
                .map_err(|_| "OJ 会话数据无效")?
        };
        let jar = Arc::new(CookieStoreMutex::new(store));
        let builder = crate::netproxy::client_builder()
            .cookie_provider(jar.clone())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .user_agent("SZULife/0.3 OJ");
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()?;
        Ok(Self {
            // OJ 站点地址由用户填写：内网/校内网是否绕过代理交给 netproxy 判断。
            http: builder.build().map_err(|_| "无法建立 OJ 连接")?,
            jar,
            base,
            cancel: CancellationToken::new(),
        })
    }
    pub fn cookies(&self) -> Result<String, String> {
        let mut bytes = vec![];
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(
            &*self.jar.lock().map_err(|_| "会话不可用")?,
            &mut bytes,
        )
        .map_err(|_| "无法保存 OJ 会话")?;
        String::from_utf8(bytes).map_err(|_| "会话编码无效".into())
    }
    pub async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value, Error> {
        if self.cancel.is_cancelled() {
            return Err(Error::local("账号配置已变化，请重新打开"));
        }
        let url = self
            .base
            .join(&format!("api/{}", path.trim_start_matches('/')))
            .map_err(|_| Error::local("接口地址无效"))?;
        let write = method != Method::GET;
        let mut request = self
            .http
            .request(method, url.clone())
            .query(query)
            .header("Referer", self.base.as_str())
            .header("Origin", self.base.origin().ascii_serialization());
        if write {
            if let Some(cookie) = self
                .jar
                .cookies(&url)
                .and_then(|h| h.to_str().ok().map(str::to_owned))
            {
                if let Some(csrf) = cookie
                    .split(';')
                    .filter_map(|s| s.trim().strip_prefix("csrftoken="))
                    .next()
                {
                    request = request.header("X-CSRFToken", csrf)
                }
            }
        }
        if let Some(body) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(body).map_err(|_| Error::local("请求格式无效"))?)
        }
        let run = async {
            let mut response = request.send().await.map_err(|_| Error {
                message: if write {
                    "请求结果待确认，请先查询提交记录"
                } else {
                    "无法连接 OJ，请检查校园网或 VPN"
                }
                .into(),
                status: None,
                uncertain: write,
                response: None,
            })?;
            let status = response.status();
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await.map_err(|_| Error {
                message: "响应中断，请检查操作结果".into(),
                status: None,
                uncertain: write,
                response: None,
            })? {
                if bytes.len() + chunk.len() > 12 * 1024 * 1024 {
                    return Err(Error {
                        message: "OJ 响应过大，请检查操作结果".into(),
                        status: None,
                        uncertain: write,
                        response: None,
                    });
                }
                bytes.extend_from_slice(&chunk);
            }
            if !status.is_success() {
                return Err(Error {
                    message: match status.as_u16() {
                        401 => "OJ 登录已失效",
                        403 => "OJ 拒绝了请求，请检查权限或考试限制",
                        404 => "OJ 接口或资源不可访问",
                        429 => "请求过于频繁，请稍后重试",
                        _ => "OJ 服务返回错误",
                    }
                    .into(),
                    status: Some(status.as_u16()),
                    uncertain: write && (status.is_server_error() || status.is_redirection()),
                    response: Some(response_text(&bytes)),
                });
            }
            serde_json::from_slice(&bytes).map_err(|_| Error {
                message: "OJ 响应格式不兼容".into(),
                status: None,
                uncertain: write,
                response: Some(response_text(&bytes)),
            })
        };
        tokio::select! { _=self.cancel.cancelled()=>Err(Error::local("操作已取消")), result=run=>result }
    }
    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, Error> {
        self.request(Method::GET, path, query, None).await
    }
    pub async fn list(&self, path: &str, query: &[(&str, String)]) -> Result<Vec<Value>, Error> {
        let mut all = vec![];
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..200 {
            let mut q = query.to_vec();
            q.push(("offset", all.len().to_string()));
            q.push(("limit", "100".into()));
            let value = self.get(path, &q).await?;
            if let Some(rows) = value.as_array() {
                return Ok(rows.clone());
            }
            let rows = value["results"]
                .as_array()
                .ok_or_else(|| Error::local("OJ 列表格式不兼容"))?;
            let count = value["count"]
                .as_u64()
                .ok_or_else(|| Error::local("OJ 分页总数缺失"))? as usize;
            if rows.is_empty() && all.len() < count {
                return Err(Error::local("OJ 分页不完整，保留上次数据"));
            }
            for row in rows {
                let id = super::protocol::id(&row["id"])?;
                if !seen.insert(id) {
                    return Err(Error::local("OJ 分页重复，保留上次数据"));
                }
                all.push(row.clone());
            }
            if all.len() >= count {
                return Ok(all);
            }
        }
        Err(Error::local("OJ 列表超出分页限制"))
    }
}
