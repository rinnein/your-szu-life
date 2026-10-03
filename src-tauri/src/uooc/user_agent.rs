use tauri::Url;

pub(super) const DESKTOP: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

pub(super) fn is_login_redirect(from: &Url, location: &str) -> bool {
    from.join(location).is_ok_and(|target| {
        target.origin() == from.origin()
            && target.username().is_empty()
            && target.password().is_none()
            && target.path().trim_end_matches('/') == "/user/login"
    })
}

#[cfg(any(target_os = "android", test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Profile {
    Login,
    Desktop,
}

#[cfg(any(target_os = "android", test))]
fn profile(url: &Url) -> Option<Profile> {
    if url.scheme() != "https"
        || url.host_str() != Some("www.uooc.net.cn")
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(if url.path().trim_end_matches('/') == "/user/login" {
        Profile::Login
    } else {
        Profile::Desktop
    })
}

#[cfg(any(target_os = "android", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Switch {
    pub id: u64,
    pub profile: Profile,
    pub url: Url,
}
#[cfg(any(target_os = "android", test))]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Allow,
    Block,
    Switch(Switch),
}
#[cfg(any(target_os = "android", test))]
pub(super) struct Navigation {
    applied: Profile,
    sequence: u64,
    pending: Option<Switch>,
    blocked: bool,
}
#[cfg(any(target_os = "android", test))]
impl Default for Navigation {
    fn default() -> Self {
        Self {
            applied: Profile::Login,
            sequence: 0,
            pending: None,
            blocked: false,
        }
    }
}
#[cfg(any(target_os = "android", test))]
impl Navigation {
    pub fn request(&mut self, url: &Url) -> Decision {
        if self.blocked {
            return Decision::Block;
        }
        let Some(target) = profile(url) else {
            self.pending = None;
            return Decision::Allow;
        };
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.url == *url)
        {
            return Decision::Block;
        }
        self.pending = None; // A newer navigation invalidates an earlier queued switch.
        if target == self.applied {
            return Decision::Allow;
        }
        self.sequence += 1;
        let switch = Switch {
            id: self.sequence,
            profile: target,
            url: url.clone(),
        };
        self.pending = Some(switch.clone());
        Decision::Switch(switch)
    }
    pub fn current(&self, switch: &Switch) -> bool {
        !self.blocked && self.pending.as_ref() == Some(switch)
    }
    pub fn applied(&mut self, switch: &Switch) -> bool {
        if !self.current(switch) {
            return false;
        }
        self.applied = switch.profile;
        self.pending = None;
        true
    }
    pub fn invalidate(&mut self) {
        self.blocked = true;
        self.pending = None;
    }
}

#[cfg(target_os = "android")]
pub(super) struct AndroidNavigation {
    app: tauri::AppHandle,
    label: String,
    navigation: std::sync::Mutex<Navigation>,
    original_ua: std::sync::Mutex<Option<String>>,
    valid: Box<dyn Fn() -> bool + Send + Sync>,
    failed: Box<dyn Fn() + Send + Sync>,
}

#[cfg(target_os = "android")]
impl AndroidNavigation {
    pub fn new(
        app: tauri::AppHandle,
        label: String,
        valid: impl Fn() -> bool + Send + Sync + 'static,
        failed: impl Fn() + Send + Sync + 'static,
    ) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            app,
            label,
            navigation: Default::default(),
            original_ua: Default::default(),
            valid: Box::new(valid),
            failed: Box::new(failed),
        })
    }
    pub fn navigate(self: &std::sync::Arc<Self>, url: &Url) -> bool {
        if !(self.valid)() {
            self.navigation.lock().unwrap().invalidate();
            return false;
        }
        let decision = self.navigation.lock().unwrap().request(url);
        match decision {
            Decision::Allow => true,
            Decision::Block => false,
            Decision::Switch(switch) => {
                let this = self.clone();
                // The navigation callback runs synchronously on Android's UI thread.
                // Return false first; never re-enter Wry's synchronous API from it.
                std::thread::spawn(move || this.dispatch(switch));
                false
            }
        }
    }
    fn dispatch(self: std::sync::Arc<Self>, switch: Switch) {
        use tauri::Manager;
        if !(self.valid)() || !self.navigation.lock().unwrap().current(&switch) {
            return;
        }
        let Some(window) = self.app.get_webview_window(&self.label) else {
            return;
        };
        let this = self.clone();
        if window
            .with_webview(move |platform| {
                platform.jni_handle().exec(move |env, _, webview| {
                    if !(this.valid)() || !this.navigation.lock().unwrap().current(&switch) {
                        return;
                    }
                    let result = (|| -> Result<(), jni::errors::Error> {
                        use jni::objects::{JString, JValue};
                        let settings = env
                            .call_method(
                                webview,
                                "getSettings",
                                "()Landroid/webkit/WebSettings;",
                                &[],
                            )?
                            .l()?;
                        let original = env
                            .call_method(
                                &settings,
                                "getUserAgentString",
                                "()Ljava/lang/String;",
                                &[],
                            )?
                            .l()?;
                        let original: String = env.get_string(&JString::from(original))?.into();
                        let agent = {
                            let mut saved = this.original_ua.lock().unwrap();
                            let saved = saved.get_or_insert(original);
                            match switch.profile {
                                Profile::Login => saved.clone(),
                                Profile::Desktop => DESKTOP.to_owned(),
                            }
                        };
                        if !(this.valid)() || !this.navigation.lock().unwrap().current(&switch) {
                            return Ok(());
                        }
                        // Prevent setUserAgentString from replaying an in-flight login form.
                        env.call_method(webview, "stopLoading", "()V", &[])?;
                        let agent = env.new_string(agent)?;
                        env.call_method(
                            &settings,
                            "setUserAgentString",
                            "(Ljava/lang/String;)V",
                            &[JValue::Object(&agent)],
                        )?;
                        if !this.navigation.lock().unwrap().applied(&switch) || !(this.valid)() {
                            return Ok(());
                        }
                        // No state lock is held: loadUrl may synchronously re-enter on_navigation.
                        let url = env.new_string(switch.url.as_str())?;
                        env.call_method(
                            webview,
                            "loadUrl",
                            "(Ljava/lang/String;)V",
                            &[JValue::Object(&url)],
                        )?;
                        Ok(())
                    })();
                    if env.exception_check().unwrap_or(false) {
                        let _ = env.exception_clear();
                    }
                    if result.is_err() {
                        this.fail();
                    }
                });
            })
            .is_err()
        {
            self.fail();
        }
    }
    fn fail(&self) {
        self.navigation.lock().unwrap().invalidate();
        if (self.valid)() {
            (self.failed)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_headers_and_redirects_are_scoped() {
        assert!(DESKTOP.contains("Chrome/") && DESKTOP.contains("Mozilla/5.0"));
        assert!(!DESKTOP.contains("Android") && !DESKTOP.contains("Mobile"));
        let base: Url = "https://www.uooc.net.cn/home/course/list".parse().unwrap();
        assert!(is_login_redirect(&base, "/user/login?return=/home"));
        assert!(is_login_redirect(&base, "/user/login/"));
        for redirect in [
            "/app/download",
            "/home",
            "https://evil.test/user/login",
            "https://user:pass@www.uooc.net.cn/user/login",
        ] {
            assert!(!is_login_redirect(&base, redirect));
        }
    }
    #[test]
    fn login_business_and_return_navigation_switch_before_allowing_requests() {
        let login: Url = "https://www.uooc.net.cn/user/login".parse().unwrap();
        let home: Url = "https://www.uooc.net.cn/home".parse().unwrap();
        let mut nav = Navigation::default();
        assert_eq!(nav.request(&login), Decision::Allow);
        let Decision::Switch(to_desktop) = nav.request(&home) else {
            panic!("must intercept the first business request")
        };
        assert_eq!(to_desktop.profile, Profile::Desktop);
        assert_eq!(nav.request(&home), Decision::Block);
        assert!(nav.applied(&to_desktop));
        assert_eq!(nav.request(&home), Decision::Allow);
        let Decision::Switch(to_login) = nav.request(&login) else {
            panic!("must restore the original UA")
        };
        assert_eq!(to_login.profile, Profile::Login);
        assert!(nav.applied(&to_login));
        assert_eq!(nav.request(&login), Decision::Allow);
    }
    #[test]
    fn superseded_cancelled_and_foreign_navigations_do_not_replay() {
        let mut nav = Navigation::default();
        let home: Url = "https://www.uooc.net.cn/home".parse().unwrap();
        let Decision::Switch(old) = nav.request(&home) else {
            panic!()
        };
        let next: Url = "https://www.uooc.net.cn/home/course/42".parse().unwrap();
        let Decision::Switch(latest) = nav.request(&next) else {
            panic!()
        };
        assert!(!nav.applied(&old));
        nav.invalidate();
        assert!(!nav.applied(&latest));
        assert_eq!(nav.request(&home), Decision::Block);
        let mut nav = Navigation::default();
        let Decision::Switch(pending) = nav.request(&home) else {
            panic!()
        };
        assert_eq!(
            nav.request(
                &"https://authserver.szu.edu.cn/authserver/login"
                    .parse()
                    .unwrap()
            ),
            Decision::Allow
        );
        assert!(!nav.applied(&pending));
        assert_eq!(
            nav.request(&"about:blank".parse().unwrap()),
            Decision::Allow
        );
    }
}
