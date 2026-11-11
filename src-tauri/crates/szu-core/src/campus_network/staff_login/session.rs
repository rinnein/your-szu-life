use crate::campus_network::http::{
    allowed_url, client_builder, read_response, HttpError, HttpResponse, PortalHttp,
};
use reqwest::{blocking::Client, cookie::Jar, Url};
use std::sync::Arc;

pub(super) trait StaffSession: PortalHttp {
    fn remember(&self, value: Option<&str>) -> Result<(), HttpError>;
}

/// One operation, one cookie jar. No browser store, global jar, disk serialization or Debug output.
pub(super) struct StaffSessionClient {
    client: Client,
    jar: Arc<Jar>,
}
impl StaffSessionClient {
    pub(super) fn new() -> Result<Self, HttpError> {
        let jar = Arc::new(Jar::default());
        let client = client_builder()
            .cookie_provider(jar.clone())
            .build()
            .map_err(|_| HttpError::Unavailable)?;
        Ok(Self { client, jar })
    }
}
impl PortalHttp for StaffSessionClient {
    fn get(&self, url: &Url) -> Result<HttpResponse, HttpError> {
        if !allowed_url(url) || url.scheme() != "https" || url.host_str() != Some("net.szu.edu.cn")
        {
            return Err(HttpError::UnsafeUrl);
        }
        let response = self.client.get(url.clone()).send().map_err(|error| {
            if error.is_timeout() {
                HttpError::Timeout
            } else {
                HttpError::Unavailable
            }
        })?;
        read_response(response)
    }
}
impl StaffSession for StaffSessionClient {
    fn remember(&self, value: Option<&str>) -> Result<(), HttpError> {
        let url = crate::campus_network::staff::staff_url("/");
        // Same wire value/path as Portal.remember. Secure attributes only narrow native handling.
        let cookie = match value {
            Some(value) => {
                format!("remember={value}; Max-Age=604800; Path=/; Secure; SameSite=Lax")
            }
            None => "remember=; Max-Age=0; Path=/; Secure; SameSite=Lax".into(),
        };
        self.jar.add_cookie_str(&cookie, &url);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
