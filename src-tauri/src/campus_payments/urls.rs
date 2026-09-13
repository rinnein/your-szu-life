use super::PAYMENTS_PATH;
use tauri::Url;

fn has_no_credentials(url: &Url) -> bool {
    url.username().is_empty() && url.password().is_none()
}

pub(super) fn is_portal_origin(url: &Url) -> bool {
    has_no_credentials(url)
        && url.scheme() == "https"
        && url.host_str() == Some("self.szu.edu.cn")
        && url.port_or_known_default() == Some(443)
}

pub(super) fn is_login_url(url: &Url) -> bool {
    is_portal_origin(url)
        && url.path() == "/self/nav_login"
        && url.query().is_none()
        && url.fragment().is_none()
}

pub(super) fn is_target_url(url: &Url) -> bool {
    is_portal_origin(url) && url.path() == "/self/nav_operators"
}

pub(super) fn same_application_origin(url: &Url, application: &Url) -> bool {
    has_no_credentials(url)
        && url.scheme() == application.scheme()
        && url.host_str() == application.host_str()
        && url.port_or_known_default() == application.port_or_known_default()
}

pub(super) fn application_return_url(current: &Url, dev_url: Option<&Url>) -> Option<Url> {
    if !has_no_credentials(current) || current.path() != PAYMENTS_PATH {
        return None;
    }
    let production = (current.scheme() == "tauri"
        && current.host_str() == Some("localhost")
        && current.port().is_none())
        || (current.scheme() == "http"
            && current.host_str() == Some("tauri.localhost")
            && current.port_or_known_default() == Some(80));
    let development = dev_url.is_some_and(|url| {
        matches!(url.scheme(), "http" | "https") && same_application_origin(current, url)
    });
    if !production && !development {
        return None;
    }
    let mut url = current.clone();
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}
