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
