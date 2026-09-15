use super::model::*;
use reqwest::{blocking::Client, Url};
use std::{io::Read, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RequestKind {
    Json,
    Github,
    Image,
}

pub(super) struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub has_next_page: bool,
    pub retry_at: Option<u64>,
}

impl HttpResponse {
    pub fn successful(self, kind: RequestKind) -> Result<Self, ContentError> {
        if (200..300).contains(&self.status) {
            return Ok(self);
        }
        let mut error = if kind == RequestKind::Github && matches!(self.status, 403 | 429) {
            ContentError::new(ErrorKind::RateLimited, "GitHub 请求受限，请稍后再试。")
        } else if kind == RequestKind::Github && self.status == 404 {
            ContentError::new(ErrorKind::Http, "GitHub 仓库不存在或不是公开仓库。")
        } else {
            ContentError::new(ErrorKind::Http, format!("服务器返回 HTTP {}", self.status))
        };
        error.retry_at = self.retry_at;
        Err(error)
    }
}

pub(super) trait PublicHttp: Send + Sync {
    fn get(&self, url: &Url, kind: RequestKind, limit: usize)
        -> Result<HttpResponse, ContentError>;
}

pub(super) struct PublicClient {
    client: Result<Client, ContentError>,
}

impl PublicClient {
    pub fn new(version: &str) -> Self {
        let client = Self::build(version);
        Self { client }
    }

    fn build(version: &str) -> Result<Client, ContentError> {
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()
            .map_err(|_| ContentError::new(ErrorKind::Unavailable, "无法初始化系统 TLS 验证器"))?;
        Client::builder()
            .user_agent(format!("your-szu-life/{version}"))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            // Use the platform/environment proxy policy; never the portal client's no_proxy.
            .cookie_store(false)
            .referer(false)
            // A redirect is a non-success response, not permission to proxy another image URL.
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .map_err(|_| ContentError::new(ErrorKind::Unavailable, "无法初始化公共内容网络客户端"))
    }
}

impl PublicHttp for PublicClient {
    fn get(
        &self,
        url: &Url,
        kind: RequestKind,
        limit: usize,
    ) -> Result<HttpResponse, ContentError> {
        web_url(url.as_str())?;
        let accept = match kind {
            RequestKind::Json => "application/json",
            RequestKind::Github => "application/vnd.github+json",
            RequestKind::Image => "image/png,image/jpeg,image/gif,image/webp",
        };
        let response = self
            .client
            .as_ref()
            .map_err(Clone::clone)?
            .get(url.clone())
            .header("Accept", accept)
            .send()
            .map_err(|_| ContentError::new(ErrorKind::Network, "请求失败或超时，请检查网络"))?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let has_next_page = response
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|header| {
                header.split(',').any(|part| {
                    part.split(';')
                        .skip(1)
                        .any(|parameter| parameter.trim() == "rel=\"next\"")
                })
            });
        let retry_at = response
            .headers()
            .get("x-ratelimit-reset")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .and_then(|seconds| seconds.checked_mul(1000));
        if response
            .content_length()
            .is_some_and(|size| size > limit as u64)
        {
            return Err(ContentError::new(ErrorKind::TooLarge, "响应超过大小限制"));
        }
        let mut body = Vec::new();
        response
            .take(limit as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| ContentError::new(ErrorKind::Network, "读取响应失败或超时"))?;
        if body.len() > limit {
            return Err(ContentError::new(ErrorKind::TooLarge, "响应超过大小限制"));
        }
        Ok(HttpResponse {
            status,
            body,
            content_type,
            has_next_page,
            retry_at,
        })
    }
}
