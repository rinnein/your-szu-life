use super::model::*;
use reqwest::{Url, blocking::Client};
use std::{io::Read, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestKind {
    Json,
    Github,
    CnbWeb,
    /// 公告/更新日志等可信内容源的图片（允许内网地址）。
    Image,
    /// 任意第三方图片（只允许公网地址，防 SSRF）。
    PublicImage,
}

pub struct HttpResponse {
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
        } else if kind == RequestKind::CnbWeb && self.status == 404 {
            ContentError::new(ErrorKind::Http, "CNB 仓库或资源不存在。")
        } else {
            ContentError::new(ErrorKind::Http, format!("服务器返回 HTTP {}", self.status))
        };
        error.retry_at = self.retry_at;
        Err(error)
    }
}

pub fn content_error(error: szu_core::resource_proxy::ResourceError) -> ContentError {
    use szu_core::resource_proxy::ResourceErrorKind;
    let kind = match error.kind {
        ResourceErrorKind::Invalid | ResourceErrorKind::Denied => ErrorKind::InvalidData,
        ResourceErrorKind::TooLarge => ErrorKind::TooLarge,
        ResourceErrorKind::Network => ErrorKind::Network,
        ResourceErrorKind::Http => ErrorKind::Http,
        ResourceErrorKind::Unavailable => ErrorKind::Unavailable,
    };
    ContentError::new(kind, error.message)
}

pub trait PublicHttp: Send + Sync {
    fn get(&self, url: &Url, kind: RequestKind, limit: usize)
    -> Result<HttpResponse, ContentError>;
}

pub struct PublicClient {
    client: Result<Client, ContentError>,
    user_agent: String,
}

impl PublicClient {
    pub fn new(version: &str) -> Self {
        Self {
            client: Self::build(version),
            user_agent: format!("your-szu-life/{version}"),
        }
    }

    fn build(version: &str) -> Result<Client, ContentError> {
        #[cfg(target_os = "android")]
        szu_core::platform_tls::initialize_platform_verifier()
            .map_err(|_| ContentError::new(ErrorKind::Unavailable, "无法初始化系统 TLS 验证器"))?;
        // 代理策略与绕过判断都在 netproxy 里，这里只描述这个客户端自己要什么。
        szu_core::netproxy::blocking_client_builder()
            .user_agent(format!("your-szu-life/{version}"))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .cookie_store(false)
            .referer(false)
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
            RequestKind::CnbWeb => "application/vnd.cnb.web+json",
            RequestKind::Image | RequestKind::PublicImage => {
                "image/png,image/jpeg,image/gif,image/webp,image/bmp,image/avif"
            }
        };
        // 图片统一走 szu-core 的资源代理：netproxy 出口、逐跳校验重定向、不携带 Cookie/Referer。
        if matches!(kind, RequestKind::Image | RequestKind::PublicImage) {
            let access = if kind == RequestKind::Image {
                szu_core::resource_proxy::ResourceAccess::Trusted
            } else {
                szu_core::resource_proxy::ResourceAccess::Public
            };
            let response = szu_core::resource_proxy::fetch_blocking(
                szu_core::resource_proxy::ResourceRequest {
                    url: url.as_str(),
                    access,
                    accept,
                    limit,
                    user_agent: &self.user_agent,
                },
            )
            .map_err(content_error)?;
            return Ok(HttpResponse {
                status: 200,
                body: response.bytes,
                content_type: response.content_type,
                has_next_page: false,
                retry_at: None,
            });
        }

        let client = self.client.as_ref().map_err(Clone::clone)?;
        let response = client
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
