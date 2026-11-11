//! The production operation policy is tested independently from JNI and CDP.
use super::{CookieRecord, Error, Patch, Scope, Snapshot};
use std::collections::HashSet;

pub(super) trait Transport {
    fn native_available(&mut self) -> bool;
    fn capture_native(&mut self, scope: &Scope) -> Result<Snapshot, Error>;
    fn write_native(&mut self, scope: &Scope, patches: &[Patch]) -> Result<(), Error>;
    fn capture_cdp(&mut self, scope: &Scope) -> Result<Snapshot, Error>;
    fn apply_cdp(&mut self, scope: &Scope, patches: &[Patch]) -> Result<(), Error>;
    fn replace_cdp(&mut self, scope: &Scope, desired: &Snapshot) -> Result<(), Error>;
}
fn ordinary(cookie: &CookieRecord) -> bool {
    cookie.partitioned != Some(true)
        && cookie
            .extensions
            .as_ref()
            .is_none_or(|e| e == &super::model::Extensions::default())
}
fn needs_cdp(error: Error) -> bool {
    matches!(
        error,
        Error::UnsupportedMethod | Error::UnsupportedPartition
    )
}
fn native_read(transport: &mut impl Transport, scope: &Scope) -> Result<Snapshot, Error> {
    for attempt in 0..3 {
        match transport.capture_native(scope) {
            Err(Error::Unavailable) if attempt < 2 => {
                std::thread::sleep(std::time::Duration::from_millis(50))
            }
            result => return result,
        }
    }
    Err(Error::Unavailable)
}
pub(super) fn capture(transport: &mut impl Transport, scope: &Scope) -> Result<Snapshot, Error> {
    if transport.native_available() {
        match native_read(transport, scope) {
            Ok(snapshot) => return Ok(snapshot),
            Err(error) if needs_cdp(error) => {}
            Err(error) => return Err(error),
        }
    }
    transport.capture_cdp(scope)
}
pub(super) fn apply(
    transport: &mut impl Transport,
    scope: &Scope,
    patches: &[Patch],
) -> Result<(), Error> {
    if patches.is_empty() {
        return Ok(());
    }
    if !transport.native_available() || !patches.iter().all(|p| ordinary(&p.cookie)) {
        return transport.apply_cdp(scope, patches);
    }
    let before = match native_read(transport, scope) {
        Ok(snapshot) => snapshot,
        Err(error) if needs_cdp(error) => return transport.apply_cdp(scope, patches),
        Err(error) => return Err(error),
    };
    let mut desired = before.clone();
    for patch in patches {
        desired
            .cookies
            .retain(|c| c.identity() != patch.cookie.identity());
        if !patch.remove
            && !patch
                .cookie
                .is_definitely_expired(time::OffsetDateTime::now_utc())
        {
            desired.cookies.push(patch.cookie.clone());
        }
    }
    native_replace(transport, scope, &before, &desired)
}
pub(super) fn replace(
    transport: &mut impl Transport,
    scope: &Scope,
    desired: &Snapshot,
) -> Result<(), Error> {
    desired.validate(scope)?;
    if !transport.native_available() || !desired.cookies.iter().all(ordinary) {
        return transport.replace_cdp(scope, desired);
    }
    let before = match native_read(transport, scope) {
        Ok(snapshot) => snapshot,
        Err(error) if needs_cdp(error) => return transport.replace_cdp(scope, desired),
        Err(error) => return Err(error),
    };
    native_replace(transport, scope, &before, desired)
}
fn native_replace(
    transport: &mut impl Transport,
    scope: &Scope,
    before: &Snapshot,
    desired: &Snapshot,
) -> Result<(), Error> {
    if equivalent(before, desired) {
        return Ok(());
    }
    let mut patches: Vec<_> = before
        .cookies
        .iter()
        .filter(|old| {
            !desired
                .cookies
                .iter()
                .any(|new| new.identity() == old.identity())
        })
        .map(|cookie| Patch {
            cookie: cookie.clone(),
            remove: true,
        })
        .collect();
    patches.extend(
        desired
            .cookies
            .iter()
            .filter(|c| {
                !c.is_definitely_expired(time::OffsetDateTime::now_utc())
                    && !before.cookies.iter().any(|old| record_matches(old, c))
            })
            .map(|cookie| Patch {
                cookie: cookie.clone(),
                remove: false,
            }),
    );
    let write = transport.write_native(scope, &patches);
    // Confirmation stays native for ordinary cookies; no DevTools dependency.
    // A lost acknowledgement never causes the operation to be replayed.
    let current = match native_read(transport, scope) {
        Ok(snapshot) => snapshot,
        Err(error) if needs_cdp(error) => transport
            .capture_cdp(scope)
            .map_err(|_| Error::WriteUncertain)?,
        Err(_) => return Err(Error::WriteUncertain),
    };
    if equivalent(&current, desired) {
        Ok(())
    } else {
        Err(write.err().unwrap_or(Error::WriteUncertain))
    }
}
pub(super) fn equivalent(a: &Snapshot, b: &Snapshot) -> bool {
    let actual: Vec<_> = a
        .cookies
        .iter()
        .filter(|c| !c.is_definitely_expired(time::OffsetDateTime::now_utc()))
        .collect();
    let expected: Vec<_> = b
        .cookies
        .iter()
        .filter(|c| !c.is_definitely_expired(time::OffsetDateTime::now_utc()))
        .collect();
    if actual.len() != expected.len() {
        return false;
    }
    let mut used = HashSet::new();
    expected.iter().all(|want| {
        actual.iter().enumerate().any(|(i, got)| {
            if used.contains(&i) || !record_matches(got, want) {
                return false;
            }
            used.insert(i);
            true
        })
    })
}
pub(super) fn record_matches(got: &CookieRecord, want: &CookieRecord) -> bool {
    let expiry = |c: &CookieRecord| {
        c.expires
            .as_deref()
            .and_then(|s| {
                time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc2822).ok()
            })
            .map(|t| t.unix_timestamp())
    };
    if got.name != want.name
        || got.domain != want.domain
        || got.path != want.path
        || got.host_only != want.host_only
        || got.value != want.value
        || expiry(got) != expiry(want)
        || got.http_only.unwrap_or(false) != want.http_only.unwrap_or(false)
        || got.secure.unwrap_or(false) != want.secure.unwrap_or(false)
        || got.same_site != want.same_site
    {
        return false;
    }
    let a = got.extensions.as_ref();
    let b = want.extensions.as_ref();
    let ak = a.and_then(|e| e.partition_key.as_ref());
    let bk = b.and_then(|e| e.partition_key.as_ref());
    let partitions = match (ak, bk) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.top_level_site == b.top_level_site
                && b.has_cross_site_ancestor
                    .is_none_or(|v| a.has_cross_site_ancestor == Some(v))
        }
        _ => false,
    };
    partitions
        && b.is_none_or(|b| {
            b.source_scheme
                .as_ref()
                .is_none_or(|v| a.and_then(|e| e.source_scheme.as_ref()) == Some(v))
                && b.source_port
                    .is_none_or(|v| a.and_then(|e| e.source_port) == Some(v))
                && b.priority
                    .as_ref()
                    .is_none_or(|v| a.and_then(|e| e.priority.as_ref()) == Some(v))
                && b.same_party
                    .is_none_or(|v| a.and_then(|e| e.same_party).unwrap_or(false) == v)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{testing::MemoryCredentialStore, CredentialStore};
    use std::collections::VecDeque;
    struct Fake {
        native: bool,
        cdp: bool,
        cookies: Snapshot,
        calls: Vec<&'static str>,
        read_errors: VecDeque<Error>,
        lose_ack: bool,
        reject_write: bool,
    }
    impl Fake {
        fn modern() -> Self {
            Self {
                native: true,
                cdp: false,
                cookies: Snapshot::default(),
                calls: vec![],
                read_errors: VecDeque::new(),
                lose_ack: false,
                reject_write: false,
            }
        }
    }
    impl Transport for Fake {
        fn native_available(&mut self) -> bool {
            self.native
        }
        fn capture_native(&mut self, _: &Scope) -> Result<Snapshot, Error> {
            self.calls.push("native-read");
            if let Some(e) = self.read_errors.pop_front() {
                return Err(e);
            }
            if !self.native {
                return Err(Error::UnsupportedMethod);
            }
            Ok(self.cookies.clone())
        }
        fn write_native(&mut self, _: &Scope, patches: &[Patch]) -> Result<(), Error> {
            self.calls.push("native-write");
            if self.reject_write {
                return Err(Error::WriteUncertain);
            }
            for patch in patches {
                self.cookies
                    .cookies
                    .retain(|c| c.identity() != patch.cookie.identity());
                if !patch.remove {
                    self.cookies.cookies.push(patch.cookie.clone());
                }
            }
            if self.lose_ack {
                Err(Error::WriteUncertain)
            } else {
                Ok(())
            }
        }
        fn capture_cdp(&mut self, _: &Scope) -> Result<Snapshot, Error> {
            self.calls.push("cdp-read");
            if self.cdp {
                Ok(self.cookies.clone())
            } else {
                Err(Error::Unavailable)
            }
        }
        fn apply_cdp(&mut self, _: &Scope, _: &[Patch]) -> Result<(), Error> {
            self.calls.push("cdp-apply");
            if self.cdp {
                Ok(())
            } else {
                Err(Error::Unavailable)
            }
        }
        fn replace_cdp(&mut self, _: &Scope, desired: &Snapshot) -> Result<(), Error> {
            self.calls.push("cdp-replace");
            if !self.cdp {
                return Err(Error::Unavailable);
            }
            self.cookies = desired.clone();
            Ok(())
        }
    }
    fn session() -> Snapshot {
        Snapshot { cookies:vec![CookieRecord::from_android("JSESSIONID=virtual; Domain=authserver.szu.edu.cn; Path=/; Secure; HttpOnly; SameSite=Lax").unwrap()], ..Default::default() }
    }
    #[test]
    fn modern_complete_login_restore_and_reinitialization_never_require_cdp() {
        let scope = Scope::auth();
        let vault = MemoryCredentialStore::default();
        let mut transport = Fake::modern();
        let saved = session();
        replace(&mut transport, &scope, &saved).unwrap();
        let captured = capture(&mut transport, &scope).unwrap();
        assert_eq!(captured, saved);
        let secret = crate::auth::session::AuthSessionSecret {
            student_id: "1234567890".into(),
            format_version: 4,
            cookies: captured.cookies,
            http_keys: None,
        };
        vault.save_session(&secret).unwrap();
        replace(&mut transport, &scope, &Snapshot::default()).unwrap();
        let loaded = vault.load_session().unwrap().unwrap();
        replace(
            &mut transport,
            &scope,
            &Snapshot {
                cookies: loaded.cookies,
                http_keys: loaded.http_keys,
            },
        )
        .unwrap();
        assert_eq!(capture(&mut transport, &scope).unwrap(), saved);
        // Explicit initialization must be able to clean the native session before forgetting it.
        replace(&mut transport, &scope, &Snapshot::default()).unwrap();
        vault.delete_session().unwrap();
        assert!(transport.cookies.cookies.is_empty());
        assert!(vault.load_session().unwrap().is_none());
        assert!(transport
            .calls
            .iter()
            .all(|call| call.starts_with("native-")));
    }
    #[test]
    fn modern_apply_readback_confirms_lost_ack_without_cdp_or_replay() {
        let mut transport = Fake::modern();
        transport.lose_ack = true;
        let cookie = session().cookies.remove(0);
        apply(
            &mut transport,
            &Scope::auth(),
            &[Patch {
                cookie,
                remove: false,
            }],
        )
        .unwrap();
        assert_eq!(
            transport
                .calls
                .iter()
                .filter(|c| **c == "native-write")
                .count(),
            1
        );
        assert!(transport.calls.iter().all(|c| c.starts_with("native-")));
    }
    #[test]
    fn native_read_failures_retry_only_transient_errors_and_do_not_force_cdp() {
        let mut transport = Fake::modern();
        transport.cookies = session();
        transport
            .read_errors
            .extend([Error::Unavailable, Error::Unavailable]);
        assert!(capture(&mut transport, &Scope::auth()).is_ok());
        assert_eq!(transport.calls.len(), 3);
        for error in [Error::InvalidMetadata, Error::Unavailable] {
            transport.calls.clear();
            transport.read_errors = VecDeque::from(vec![error; 3]);
            assert_eq!(capture(&mut transport, &Scope::auth()).unwrap_err(), error);
            assert_eq!(
                transport.calls.len(),
                if error == Error::Unavailable { 3 } else { 1 }
            );
            assert_eq!(transport.cookies, session());
        }
    }
    #[test]
    fn missing_api_and_partitioned_records_use_cdp_but_do_not_guess_attributes() {
        let mut transport = Fake::modern();
        transport.native = false;
        transport.cdp = true;
        transport.cookies = session();
        assert!(capture(&mut transport, &Scope::auth()).is_ok());
        replace(&mut transport, &Scope::auth(), &Snapshot::default()).unwrap();
        assert_eq!(transport.calls, vec!["cdp-read", "cdp-replace"]);
        transport.native = true;
        transport.read_errors.push_back(Error::UnsupportedPartition);
        capture(&mut transport, &Scope::auth()).unwrap();
        assert_eq!(&transport.calls[2..], &["native-read", "cdp-read"]);
        let mut extended = session();
        extended.cookies[0].extensions = Some(super::super::Extensions {
            source_port: Some(8443),
            ..Default::default()
        });
        replace(&mut transport, &Scope::auth(), &extended).unwrap();
        assert_eq!(transport.calls.last(), Some(&"cdp-replace"));
    }
    #[test]
    fn failed_native_cleanup_keeps_saved_session_and_empty_apply_is_a_noop() {
        let vault = MemoryCredentialStore::default();
        let mut transport = Fake::modern();
        transport.cookies = session();
        transport.reject_write = true;
        let original = crate::auth::session::AuthSessionSecret {
            student_id: "1234567890".into(),
            format_version: 4,
            cookies: session().cookies,
            http_keys: None,
        };
        vault.save_session(&original).unwrap();
        assert_eq!(
            replace(&mut transport, &Scope::auth(), &Snapshot::default()),
            Err(Error::WriteUncertain)
        );
        assert_eq!(vault.load_session().unwrap(), Some(original));
        assert_eq!(transport.cookies, session());
        transport.calls.clear();
        apply(&mut transport, &Scope::auth(), &[]).unwrap();
        assert!(transport.calls.is_empty());
    }
}
