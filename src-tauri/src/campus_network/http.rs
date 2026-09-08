use reqwest::{blocking::Client, redirect::Policy, Url};
use std::{fmt, io::Read, time::Duration};

pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub(super) struct HttpResponse {
    pub(super) status: u16,
    pub(super) content_type: Option<String>,
    pub(super) location: Option<String>,
    pub(super) body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HttpError {
    Unavailable,
    Timeout,
    InvalidResponse,
    UnsafeUrl,
    Canceled,
}
impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // reqwest errors can contain the entire URL, including login credentials.
        f.write_str(match self {
            Self::Unavailable => "门户请求失败",
            Self::Timeout => "门户请求超时",
            Self::InvalidResponse => "门户响应无效或过大",
            Self::UnsafeUrl => "已拒绝不受信任的门户地址",
            Self::Canceled => "操作已取消或网络已变化",
        })
    }
}

pub(super) trait PortalHttp: Sync {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError>;
}

pub(super) struct PortalClient(Client);
impl PortalClient {
    pub(super) fn new() -> Result<Self, HttpError> {
        client_builder()
            .build()
            .map(Self)
            .map_err(|_| HttpError::Unavailable)
    }
}

pub(super) fn client_builder() -> reqwest::blocking::ClientBuilder {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(REQUEST_TIMEOUT)
        .user_agent("your-szu-life/0.1")
}

pub(super) fn allowed_url(url: &Url) -> bool {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return false;
    }
    matches!(
        (url.scheme(), url.host_str(), url.port_or_known_default()),
        ("https", Some("net.szu.edu.cn"), Some(443))
            | ("http", Some("172.30.255.42"), Some(80 | 801))
    )
}

impl PortalHttp for PortalClient {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        if !allowed_url(url) {
            return Err(HttpError::UnsafeUrl);
        }
        let response = self.0.get(url.clone()).send().map_err(|error| {
            if error.is_timeout() {
                HttpError::Timeout
            } else {
                HttpError::Unavailable
            }
        })?;
        read_response(response)
    }
}

pub(super) fn read_response(
    response: reqwest::blocking::Response,
) -> Result<HttpResponse, HttpError> {
    let mut result = HttpResponse {
        status: response.status().as_u16(),
        content_type: response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        location: response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned),
        body: Vec::new(),
    };
    response
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut result.body)
        .map_err(|_| HttpError::InvalidResponse)?;
    if result.body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(HttpError::InvalidResponse);
    }
    Ok(result)
}

/// Check cancellation/network identity before every request, not just before a multi-step probe.
pub(super) struct CheckedHttp<'a> {
    pub(super) inner: &'a dyn PortalHttp,
    pub(super) allowed: &'a (dyn Fn() -> bool + Sync),
}
impl PortalHttp for CheckedHttp<'_> {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        if !(self.allowed)() {
            return Err(HttpError::Canceled);
        }
        self.inner.get(url)
    }
}

pub(super) fn portal_url(host: &str, port: u16, path: &str) -> Url {
    Url::parse(&format!("http://{host}:{port}{path}")).expect("static portal URL")
}

// Retained for the existing explicitly ignored read-only live check.
#[cfg(test)]
pub(super) fn read_http_response_bytes(
    host: &str,
    port: u16,
    path: &str,
) -> Result<HttpResponse, String> {
    PortalClient::new()
        .and_then(|client| client.get(&portal_url(host, port, path)))
        .map_err(|error| error.to_string())
}

pub(super) struct UnavailablePortal;
impl PortalHttp for UnavailablePortal {
    fn get(&self, _url: &Url) -> Result<HttpResponse, HttpError> {
        Err(HttpError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_accepts_only_the_fixed_portal_origins() {
        for value in [
            "https://net.szu.edu.cn/",
            "http://172.30.255.42/",
            "http://172.30.255.42:801/eportal/portal/logout",
        ] {
            assert!(allowed_url(&Url::parse(value).unwrap()));
        }
        for value in [
            "http://net.szu.edu.cn/",
            "https://net.szu.edu.cn:8800/",
            "https://net.szu.edu.cn.evil.example/",
            "http://user:password@172.30.255.42/",
            "http://127.0.0.1/",
            "https://net.szu.edu.cn/#fragment",
        ] {
            assert!(!allowed_url(&Url::parse(value).unwrap()));
        }
    }

    #[test]
    fn cancelled_request_does_not_reach_the_transport() {
        let inner = crate::campus_network::testing::FakeHttp(
            |_: &Url| -> Result<HttpResponse, HttpError> {
                panic!("must not send canceled requests")
            },
        );
        let checked = CheckedHttp {
            inner: &inner,
            allowed: &|| false,
        };
        assert!(matches!(
            checked.get(&Url::parse("https://net.szu.edu.cn/").unwrap()),
            Err(HttpError::Canceled)
        ));
    }
}
