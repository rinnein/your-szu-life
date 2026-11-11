use super::CookieRecord;
use tauri::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Kind {
    Auth,
    Uooc,
    Campus,
    Webvpn,
    Gateway,
    Academic,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Scope {
    pub urls: Vec<Url>,
    pub kind: Kind,
    pub top_level_site: String,
}
impl Scope {
    pub fn new(kind: Kind, urls: &[&str]) -> Result<Self, super::Error> {
        let urls: Vec<Url> = urls
            .iter()
            .map(|u| Url::parse(u).map_err(|_| super::Error::OutOfScope))
            .collect::<Result<_, _>>()?;
        if urls.is_empty()
            || urls.iter().any(|u| {
                !matches!(u.scheme(), "http" | "https")
                    || !u.username().is_empty()
                    || u.password().is_some()
                    || u.host_str().is_none()
            })
        {
            return Err(super::Error::OutOfScope);
        }
        for u in &urls {
            let h = u.host_str().unwrap();
            let allowed = match kind {
                Kind::Auth => h == "authserver.szu.edu.cn" && u.scheme() == "https",
                Kind::Uooc => h == "www.uooc.net.cn" && u.scheme() == "https",
                Kind::Campus => matches!(h, "bkxk.szu.edu.cn" | "szu.edu.cn"),
                Kind::Webvpn | Kind::Gateway => {
                    h == "szu.edu.cn"
                        || h == "webvpn.szu.edu.cn"
                        || h.ends_with(".webvpn.szu.edu.cn")
                }
                Kind::Academic => h == "ehall.szu.edu.cn" && u.scheme() == "https",
            };
            if !allowed {
                return Err(super::Error::OutOfScope);
            }
        }
        let site = if matches!(kind, Kind::Uooc) {
            "uooc.net.cn"
        } else {
            "szu.edu.cn"
        };
        let scheme = if matches!(kind, Kind::Campus) {
            "http"
        } else {
            "https"
        };
        let top_level_site = format!("{scheme}://{site}");
        Ok(Self {
            urls,
            kind,
            top_level_site,
        })
    }
    pub fn auth() -> Self {
        Self::new(
            Kind::Auth,
            &[
                crate::auth::AUTH_LOGIN_URL,
                crate::auth::AUTH_PROFILE_URL,
                crate::auth::AUTH_MOBILE_PROFILE_URL,
                crate::auth::AUTH_USER_CONF_URL,
            ],
        )
        .expect("fixed auth scope")
    }
    pub fn uooc() -> Self {
        Self::new(
            Kind::Uooc,
            &[
                "https://www.uooc.net.cn/",
                "https://www.uooc.net.cn/user/login",
                "https://www.uooc.net.cn/home",
                "https://www.uooc.net.cn/app/download",
                "https://www.uooc.net.cn/home/member/user",
                "https://www.uooc.net.cn/home/course/list",
            ],
        )
        .expect("fixed uooc scope")
    }
    pub fn allows(&self, c: &CookieRecord) -> bool {
        let domain = c.domain.trim_start_matches('.');
        let gateway_name = matches!(
            c.name.as_str(),
            "_webvpn_key" | "webvpn_username" | "webvpn_username_NS_Sig"
        );
        let parent_name = matches!(
            c.name.as_str(),
            "route" | "insert_cookie" | "JSESSIONID" | "_WEU"
        );
        let domain_allowed = match self.kind {
            Kind::Auth => {
                (domain == "authserver.szu.edu.cn" || (!c.host_only && domain == "szu.edu.cn"))
                    && (!c.host_only || domain == "authserver.szu.edu.cn")
            }
            Kind::Uooc => {
                if c.host_only {
                    domain == "www.uooc.net.cn"
                } else {
                    matches!(domain, "www.uooc.net.cn" | "uooc.net.cn")
                }
            }
            Kind::Campus => domain == "bkxk.szu.edu.cn" || (parent_name && domain == "szu.edu.cn"),
            Kind::Webvpn => {
                domain == "bkxk.webvpn.szu.edu.cn"
                    || ((parent_name || gateway_name)
                        && matches!(domain, "szu.edu.cn" | "webvpn.szu.edu.cn"))
            }
            Kind::Gateway => {
                gateway_name
                    && (domain == "szu.edu.cn"
                        || domain == "webvpn.szu.edu.cn"
                        || domain.ends_with(".webvpn.szu.edu.cn"))
            }
            Kind::Academic => domain == "ehall.szu.edu.cn",
        };
        domain_allowed
            && self.urls.iter().any(|u| {
                let host = u.host_str().unwrap();
                let path = if u.path().is_empty() { "/" } else { u.path() };
                (host == domain || (!c.host_only && host.ends_with(&format!(".{domain}"))))
                    && (!c.secure.unwrap_or(false) || u.scheme() == "https")
                    && (path == c.path
                        || (path.starts_with(&c.path)
                            && (c.path.ends_with('/')
                                || path.as_bytes().get(c.path.len()) == Some(&b'/'))))
            })
    }
}
