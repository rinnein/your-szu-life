use serde::{Deserialize, Serialize};
use std::fmt;
use tauri::{webview::cookie::Cookie, Url};
use time::{format_description::well_known::Rfc2822, OffsetDateTime};

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PartitionKey {
    pub top_level_site: String,
    pub has_cross_site_ancestor: Option<bool>,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Extensions {
    pub partition_key: Option<PartitionKey>,
    pub source_scheme: Option<String>,
    pub source_port: Option<i32>,
    pub priority: Option<String>,
    pub same_party: Option<bool>,
}

/// Secret data. Debug output deliberately omits names, values and domains.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CookieRecord {
    pub name: String,
    pub value: String,
    pub domain: String,
    #[serde(default)]
    pub host_only: bool,
    pub path: String,
    pub expires: Option<String>,
    pub http_only: Option<bool>,
    pub secure: Option<bool>,
    pub same_site: Option<String>,
    pub partitioned: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Extensions>,
}
impl fmt::Debug for CookieRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CookieRecord(<redacted>)")
    }
}
impl CookieRecord {
    pub fn from_cookie(cookie: Cookie<'static>, host: &str) -> Self {
        Self {
            domain: cookie
                .domain()
                .unwrap_or(host)
                .trim_start_matches('.')
                .to_ascii_lowercase(),
            host_only: cookie.domain().is_none(),
            name: cookie.name().into(),
            value: cookie.value().into(),
            path: cookie.path().unwrap_or("/").into(),
            expires: cookie
                .max_age()
                .map(|age| OffsetDateTime::now_utc().saturating_add(age))
                .or_else(|| cookie.expires_datetime())
                .and_then(|d| d.format(&Rfc2822).ok()),
            http_only: cookie.http_only(),
            secure: cookie.secure(),
            same_site: cookie.same_site().map(|s| s.to_string()),
            partitioned: cookie.partitioned(),
            extensions: None,
        }
    }
    pub fn from_stored(cookie: &cookie_store::Cookie<'_>) -> Option<Self> {
        let (domain, host_only) = match &cookie.domain {
            cookie_store::CookieDomain::HostOnly(host) => (host.as_str(), true),
            cookie_store::CookieDomain::Suffix(host) => (host.as_str(), false),
            _ => return None,
        };
        // cookie_store's From<Cookie> conversion drops Secure, HttpOnly,
        // SameSite and Partitioned. Clone its raw cookie instead.
        let mut raw = std::ops::Deref::deref(cookie).clone().into_owned();
        if host_only {
            raw.unset_domain();
        } else {
            raw.set_domain(domain.to_owned());
        }
        raw.set_path(cookie.path.as_ref().to_owned());
        raw.set_max_age(None);
        match cookie.expires {
            cookie_store::CookieExpiration::AtUtc(date) => raw.set_expires(date),
            cookie_store::CookieExpiration::SessionEnd => raw.unset_expires(),
        }
        Some(Self::from_cookie(raw, domain))
    }
    #[cfg(any(target_os = "android", test))]
    pub fn from_android(raw: &str) -> Result<Self, super::Error> {
        let domain = raw
            .split(';')
            .skip(1)
            .filter_map(|s| s.trim().split_once('='))
            .find(|(k, _)| k.eq_ignore_ascii_case("domain"))
            .map(|(_, v)| v.trim())
            .ok_or(super::Error::InvalidMetadata)?;
        let cookie = Cookie::parse(raw.to_owned()).map_err(|_| super::Error::InvalidMetadata)?;
        if cookie.path().is_none() {
            return Err(super::Error::InvalidMetadata);
        }
        let mut record = Self::from_cookie(cookie.into_owned(), domain);
        record.host_only = !domain.starts_with('.');
        record.validate()?;
        Ok(record)
    }
    pub fn validate(&self) -> Result<(), super::Error> {
        if self.name.is_empty()
            || self.domain.is_empty()
            || !self.path.starts_with('/')
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || self.value.contains(';')
            || self.value.bytes().any(|b| b.is_ascii_control())
            || self.domain.contains([';', '/', ':', '\r', '\n'])
            || self.path.contains([';', '\r', '\n'])
        {
            return Err(super::Error::InvalidMetadata);
        }
        if self
            .same_site
            .as_deref()
            .is_some_and(|s| !matches!(s, "None" | "Lax" | "Strict"))
        {
            return Err(super::Error::InvalidMetadata);
        }
        if let Some(e) = &self.extensions {
            if e.source_port
                .is_some_and(|p| p != -1 && !(1..=65535).contains(&p))
                || e.source_scheme
                    .as_deref()
                    .is_some_and(|s| !matches!(s, "Unset" | "NonSecure" | "Secure"))
                || e.priority
                    .as_deref()
                    .is_some_and(|s| !matches!(s, "Low" | "Medium" | "High"))
            {
                return Err(super::Error::InvalidMetadata);
            }
            if let Some(key) = &e.partition_key {
                let url =
                    Url::parse(&key.top_level_site).map_err(|_| super::Error::InvalidMetadata)?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.path() != "/"
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || url.port().is_some()
                {
                    return Err(super::Error::InvalidMetadata);
                }
            }
        }
        if self
            .expires
            .as_deref()
            .is_some_and(|s| OffsetDateTime::parse(s, &Rfc2822).is_err())
        {
            return Err(super::Error::InvalidMetadata);
        }
        Ok(())
    }
    pub fn to_webview_cookie(&self) -> Result<Cookie<'static>, String> {
        self.validate().map_err(String::from)?;
        let mut raw = format!("{}={}; Path={}", self.name, self.value, self.path);
        if !self.host_only {
            raw.push_str(&format!("; Domain={}", self.domain));
        }
        if self.secure == Some(true) {
            raw.push_str("; Secure");
        }
        if self.http_only == Some(true) {
            raw.push_str("; HttpOnly");
        }
        if let Some(s) = &self.same_site {
            raw.push_str(&format!("; SameSite={s}"));
        }
        if self.partitioned == Some(true) {
            raw.push_str("; Partitioned");
        }
        if let Some(s) = &self.expires {
            raw.push_str(&format!("; Expires={s}"));
        }
        Cookie::parse(raw)
            .map(Cookie::into_owned)
            .map_err(|_| super::Error::InvalidMetadata.into())
    }
    pub fn is_definitely_expired(&self, now: OffsetDateTime) -> bool {
        self.expires
            .as_deref()
            .and_then(|s| OffsetDateTime::parse(s, &Rfc2822).ok())
            .is_some_and(|d| d <= now)
    }
    pub fn identity(
        &self,
    ) -> (
        String,
        String,
        String,
        bool,
        Option<PartitionKey>,
        Option<String>,
        Option<i32>,
    ) {
        let e = self.extensions.as_ref();
        (
            self.name.clone(),
            self.domain.clone(),
            self.path.clone(),
            self.host_only,
            e.and_then(|e| e.partition_key.clone()),
            e.and_then(|e| e.source_scheme.clone()),
            e.and_then(|e| e.source_port),
        )
    }
    pub fn url(&self, scope: &super::Scope) -> Result<Url, super::Error> {
        let base = scope
            .urls
            .iter()
            .find(|u| {
                u.host_str().is_some_and(|h| {
                    h == self.domain
                        || (!self.host_only && h.ends_with(&format!(".{}", self.domain)))
                })
            })
            .ok_or(super::Error::OutOfScope)?;
        let mut url = base.clone();
        url.set_path(&self.path);
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }
    pub fn source_matches(&self, url: &Url) -> bool {
        self.extensions.as_ref().is_none_or(|e| {
            e.source_port
                .is_none_or(|p| p == -1 || Some(p) == url.port_or_known_default().map(i32::from))
                && e.source_scheme.as_deref().is_none_or(|s| {
                    s == "Unset"
                        || s == if url.scheme() == "https" {
                            "Secure"
                        } else {
                            "NonSecure"
                        }
                })
        })
    }
    pub fn matches_url(&self, url: &Url) -> bool {
        let host = url.host_str().unwrap_or("");
        (host == self.domain || (!self.host_only && host.ends_with(&format!(".{}", self.domain))))
            && (!self.secure.unwrap_or(false) || url.scheme() == "https")
            && self.source_matches(url)
            && (url.path() == self.path
                || (url.path().starts_with(&self.path)
                    && (self.path.ends_with('/')
                        || url.path().as_bytes().get(self.path.len()) == Some(&b'/'))))
    }
    pub fn http_compatible(&self, scope: &super::Scope) -> bool {
        let key = self
            .extensions
            .as_ref()
            .and_then(|e| e.partition_key.as_ref());
        match key {
            Some(key) => {
                key.top_level_site.trim_end_matches('/') == scope.top_level_site
                    && key.has_cross_site_ancestor != Some(true)
            }
            None => self.partitioned != Some(true),
        }
    }
}

pub type Identity = (
    String,
    String,
    String,
    bool,
    Option<PartitionKey>,
    Option<String>,
    Option<i32>,
);
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub cookies: Vec<CookieRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_keys: Option<Vec<Identity>>,
}
impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        if self.cookies.len() != other.cookies.len() {
            return false;
        }
        let records: std::collections::HashMap<_, _> =
            other.cookies.iter().map(|c| (c.identity(), c)).collect();
        self.cookies
            .iter()
            .all(|c| records.get(&c.identity()).is_some_and(|other| *other == c))
    }
}
impl Eq for Snapshot {}
impl fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieSnapshot")
            .field("count", &self.cookies.len())
            .finish()
    }
}
impl Snapshot {
    pub fn validate(&self, scope: &super::Scope) -> Result<(), super::Error> {
        let mut identities = std::collections::HashSet::new();
        for cookie in &self.cookies {
            if !identities.insert(cookie.identity()) {
                return Err(super::Error::InvalidMetadata);
            }
            if cookie
                .extensions
                .as_ref()
                .and_then(|e| e.partition_key.as_ref())
                .is_some()
                && cookie.partitioned != Some(true)
            {
                return Err(super::Error::InvalidMetadata);
            }
            cookie.validate()?;
            if !scope.allows(cookie) {
                return Err(super::Error::OutOfScope);
            }
            if cookie.partitioned == Some(true)
                && cookie
                    .extensions
                    .as_ref()
                    .and_then(|e| e.partition_key.as_ref())
                    .is_none()
            {
                return Err(super::Error::UnsupportedPartition);
            }
        }
        if let Some(keys) = &self.http_keys {
            if keys.iter().any(|key| {
                !self
                    .cookies
                    .iter()
                    .any(|c| c.identity() == *key && c.http_compatible(scope))
            }) {
                return Err(super::Error::InvalidMetadata);
            }
        }
        Ok(())
    }
    pub fn http_cookies<'a>(
        &'a self,
        scope: &'a super::Scope,
    ) -> impl Iterator<Item = &'a CookieRecord> {
        self.cookies.iter().filter(move |c| {
            scope.allows(c)
                && self
                    .http_keys
                    .as_ref()
                    .is_none_or(|keys| keys.contains(&c.identity()))
                && c.http_compatible(scope)
                && !c.is_definitely_expired(OffsetDateTime::now_utc())
        })
    }
}

#[cfg(any(target_os = "android", test))]
#[derive(Clone)]
pub struct Patch {
    pub cookie: CookieRecord,
    pub remove: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEnvelope {
    pub version: u8,
    pub http_jar: String,
    pub webview: Snapshot,
    pub scope: super::Scope,
}
impl SessionEnvelope {
    pub fn decode(raw: &str) -> Result<Option<Self>, String> {
        if !raw.trim_start().starts_with('{') {
            return Ok(None);
        }
        let value = match serde_json::Deserializer::from_str(raw)
            .into_iter::<serde_json::Value>()
            .next()
        {
            Some(Ok(value)) => value,
            _ => return Ok(None),
        };
        if value.get("webview").is_none() {
            return Ok(None);
        }
        let result: Self = serde_json::from_str(raw).map_err(|_| "会话数据格式无效")?;
        if result.version != 1 {
            return Err("不支持的会话数据版本".into());
        }
        let urls: Vec<_> = result.scope.urls.iter().map(|u| u.as_str()).collect();
        let scope = super::Scope::new(result.scope.kind, &urls).map_err(String::from)?;
        if scope.top_level_site != result.scope.top_level_site {
            return Err(super::Error::OutOfScope.into());
        }
        result.webview.validate(&scope).map_err(String::from)?;
        Ok(Some(result))
    }
    pub fn encode(
        http_jar: String,
        webview: Snapshot,
        scope: super::Scope,
    ) -> Result<String, String> {
        webview.validate(&scope).map_err(String::from)?;
        serde_json::to_string(&Self {
            version: 1,
            http_jar,
            webview,
            scope,
        })
        .map_err(|_| "会话数据保存失败".into())
    }
}
