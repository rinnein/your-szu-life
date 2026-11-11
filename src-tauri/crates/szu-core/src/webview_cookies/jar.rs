//! HTTP projection and weakly-owned native snapshots. Secrets stay in the vault.
use super::Error;
use super::{CookieRecord, Scope, SessionEnvelope, Snapshot};
use cookie_store::CookieStore;
#[cfg(test)]
use cookie_store::RawCookie;
use reqwest_cookie_store::CookieStoreMutex;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak},
};

struct Entry {
    owner: Weak<CookieStoreMutex>,
    snapshot: Snapshot,
    scope: Scope,
    managed_http: bool,
    failure: Option<Error>,
}
fn registry() -> &'static Mutex<HashMap<usize, Entry>> {
    static ENTRIES: OnceLock<Mutex<HashMap<usize, Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| Mutex::new(HashMap::new()))
}
fn remember(jar: &Arc<CookieStoreMutex>, snapshot: Snapshot, scope: Scope) {
    let mut entries = registry().lock().unwrap();
    entries.retain(|_, e| e.owner.strong_count() > 0);
    let managed_http = entries
        .get(&(Arc::as_ptr(jar) as usize))
        .is_some_and(|e| e.managed_http);
    entries.insert(
        Arc::as_ptr(jar) as usize,
        Entry {
            owner: Arc::downgrade(jar),
            snapshot,
            scope,
            managed_http,
            failure: None,
        },
    );
}
pub fn native_snapshot(jar: &CookieStoreMutex) -> Option<(Snapshot, Scope)> {
    let mut entries = registry().lock().unwrap();
    entries.retain(|_, e| e.owner.strong_count() > 0);
    entries
        .get(&(jar as *const CookieStoreMutex as usize))
        .map(|e| (e.snapshot.clone(), e.scope.clone()))
}
pub fn copy_snapshot(from: &CookieStoreMutex, to: &Arc<CookieStoreMutex>) {
    if let Some((snapshot, scope)) = native_snapshot(from) {
        remember(to, snapshot, scope);
    }
}
pub fn merge_native(
    target: &Arc<CookieStoreMutex>,
    source: &CookieStoreMutex,
    mut scope: Scope,
) -> Result<(), String> {
    let target_native = native_snapshot(target);
    let source_native = native_snapshot(source);
    if target_native.is_none() && source_native.is_none() {
        return Ok(());
    }
    if let Some((_, previous)) = &target_native {
        for url in &previous.urls {
            if !scope.urls.contains(url) {
                scope.urls.push(url.clone());
            }
        }
    }
    let mut snapshot = target_native.map(|(s, _)| s).unwrap_or_else(|| Snapshot {
        cookies: target
            .lock()
            .unwrap()
            .iter_unexpired()
            .filter_map(CookieRecord::from_stored)
            .filter(|c| scope.allows(c))
            .collect(),
        ..Default::default()
    });
    if snapshot.http_keys.is_none() {
        snapshot.http_keys = Some(
            snapshot
                .http_cookies(&scope)
                .map(CookieRecord::identity)
                .collect(),
        );
    }
    let additions = source_native.map(|(s, _)| s).unwrap_or_else(|| Snapshot {
        cookies: source
            .lock()
            .unwrap()
            .iter_unexpired()
            .filter_map(CookieRecord::from_stored)
            .filter(|c| scope.allows(c))
            .collect(),
        ..Default::default()
    });
    let selected: Vec<_> = additions
        .http_cookies(&scope)
        .map(CookieRecord::identity)
        .collect();
    for record in additions.cookies.into_iter().filter(|c| scope.allows(c)) {
        snapshot.cookies.retain(|c| {
            c.identity() != record.identity()
                && !(c.extensions.is_none()
                    && same_base(c, &record)
                    && c.partitioned.unwrap_or(false) == record.partitioned.unwrap_or(false))
        });
        snapshot.cookies.push(record);
    }
    if let Some(keys) = snapshot.http_keys.as_mut() {
        keys.extend(selected);
        keys.retain(|k| snapshot.cookies.iter().any(|c| c.identity() == *k));
        let mut seen = std::collections::HashSet::new();
        keys.retain(|k| seen.insert(k.clone()));
    }
    snapshot.validate(&scope).map_err(String::from)?;
    remember(target, snapshot, scope);
    Ok(())
}
pub fn project(snapshot: Snapshot, scope: Scope) -> Result<Arc<CookieStoreMutex>, String> {
    snapshot.validate(&scope).map_err(String::from)?;
    let mut store = CookieStore::default();
    let selected: Vec<_> = snapshot.http_cookies(&scope).collect();
    for c in &selected {
        if c.partitioned == Some(true)
            && selected
                .iter()
                .any(|p| same_base(c, p) && p.partitioned != Some(true))
        {
            continue;
        }
        let raw = c.to_webview_cookie()?;
        store
            .insert_raw(&raw, &c.url(&scope).map_err(String::from)?)
            .map_err(|_| String::from(Error::InvalidMetadata))?;
    }
    let jar = Arc::new(CookieStoreMutex::new(store));
    remember(&jar, snapshot, scope);
    Ok(jar)
}
pub fn load(raw: &str) -> Result<Arc<CookieStoreMutex>, String> {
    let envelope = SessionEnvelope::decode(raw)?;
    let http = envelope
        .as_ref()
        .map(|e| e.http_jar.as_str())
        .unwrap_or(raw);
    let jar = if http.is_empty() {
        CookieStore::default()
    } else {
        cookie_store::serde::json::load(http.as_bytes()).map_err(|_| "会话数据格式无效")?
    };
    let jar = Arc::new(CookieStoreMutex::new(jar));
    if let Some(e) = envelope {
        remember(&jar, e.webview, e.scope);
    }
    Ok(jar)
}
pub fn serialize(jar: &CookieStoreMutex) -> Result<String, String> {
    let mut bytes = vec![];
    let native = native_snapshot(jar);
    let managed = {
        let entries = registry().lock().unwrap();
        let entry = entries.get(&(jar as *const CookieStoreMutex as usize));
        if let Some(error) = entry.and_then(|e| e.failure) {
            return Err(error.into());
        }
        entry.is_some_and(|e| e.managed_http)
    };
    let store = jar.lock().map_err(|_| "会话锁失败")?;
    cookie_store::serde::json::save_incl_expired_and_nonpersistent(&store, &mut bytes)
        .map_err(|_| "会话保存失败")?;
    let http = String::from_utf8(bytes).map_err(|_| "会话编码失败")?;
    let Some((mut snapshot, scope)) = native else {
        return Ok(http);
    };
    if managed {
        return SessionEnvelope::encode(http, snapshot, scope);
    }
    let projected: Vec<_> = store
        .iter_unexpired()
        .filter_map(CookieRecord::from_stored)
        .collect();
    drop(store);
    // Only synchronize the active HTTP partition. All other WebView partitions
    // and their source identity remain untouched by response-cookie updates.
    let active: std::collections::HashSet<_> = snapshot
        .http_cookies(&scope)
        .map(CookieRecord::identity)
        .collect();
    snapshot.cookies.retain(|c| {
        !active.contains(&c.identity())
            || c.partitioned == Some(true)
            || projected.iter().any(|p| {
                same_base(c, p) && c.partitioned.unwrap_or(false) == p.partitioned.unwrap_or(false)
            })
    });
    for p in projected.into_iter().filter(|c| scope.allows(c)) {
        let candidates: Vec<_> = snapshot
            .cookies
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                active.contains(&c.identity())
                    && same_base(c, &p)
                    && c.partitioned.unwrap_or(false) == p.partitioned.unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        let index = if candidates.len() <= 1 {
            candidates.first().copied()
        } else {
            let unchanged: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|i| snapshot.cookies[*i].value == p.value)
                .collect();
            if !unchanged.is_empty() {
                continue;
            }
            let port = scope.urls[0].port_or_known_default().map(i32::from);
            let scheme = if scope.urls[0].scheme() == "https" {
                "Secure"
            } else {
                "NonSecure"
            };
            let matching: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|i| {
                    snapshot.cookies[*i].extensions.as_ref().is_some_and(|e| {
                        e.source_port == port && e.source_scheme.as_deref() == Some(scheme)
                    })
                })
                .collect();
            if matching.len() != 1 {
                return Err(String::from(super::Error::InvalidMetadata));
            }
            matching.first().copied()
        };
        if let Some(i) = index {
            let c = &mut snapshot.cookies[i];
            c.value = p.value;
            c.expires = p.expires;
            c.http_only = p.http_only;
            c.secure = p.secure;
            c.same_site = p.same_site;
        } else {
            if let Some(keys) = snapshot.http_keys.as_mut() {
                keys.push(p.identity());
            }
            snapshot.cookies.push(p);
        }
    }
    if let Some(keys) = snapshot.http_keys.as_mut() {
        keys.retain(|key| snapshot.cookies.iter().any(|c| c.identity() == *key));
    }
    SessionEnvelope::encode(http, snapshot, scope)
}
fn same_base(a: &CookieRecord, b: &CookieRecord) -> bool {
    a.name == b.name && a.domain == b.domain && a.path == b.path && a.host_only == b.host_only
}

/// Keeps native identities that a conventional HTTP jar cannot represent.
pub struct HttpProvider {
    jar: Arc<CookieStoreMutex>,
}
pub fn http_provider(jar: Arc<CookieStoreMutex>) -> Arc<HttpProvider> {
    if let Some(entry) = registry()
        .lock()
        .unwrap()
        .get_mut(&(Arc::as_ptr(&jar) as usize))
    {
        entry.managed_http = true;
    }
    Arc::new(HttpProvider { jar })
}
impl reqwest::cookie::CookieStore for HttpProvider {
    fn cookies(&self, url: &tauri::Url) -> Option<reqwest::header::HeaderValue> {
        let Some((snapshot, scope)) = native_snapshot(&self.jar) else {
            return reqwest::cookie::CookieStore::cookies(self.jar.as_ref(), url);
        };
        if Scope::new(scope.kind, &[url.as_str()]).is_err() {
            return reqwest::cookie::CookieStore::cookies(self.jar.as_ref(), url);
        }
        let mut cookies: Vec<_> = snapshot
            .http_cookies(&scope)
            .filter(|c| c.matches_url(url))
            .collect();
        cookies.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        if cookies.is_empty() {
            return None;
        }
        reqwest::header::HeaderValue::from_str(
            &cookies
                .iter()
                .map(|c| format!("{}={}", c.name, c.value))
                .collect::<Vec<_>>()
                .join("; "),
        )
        .ok()
    }
    fn set_cookies(
        &self,
        headers: &mut dyn Iterator<Item = &reqwest::header::HeaderValue>,
        url: &tauri::Url,
    ) {
        let headers: Vec<_> = headers.cloned().collect();
        reqwest::cookie::CookieStore::set_cookies(self.jar.as_ref(), &mut headers.iter(), url);
        let key = Arc::as_ptr(&self.jar) as usize;
        let mut entries = registry().lock().unwrap();
        let Some(entry) = entries.get_mut(&key) else {
            return;
        };
        for header in headers {
            let Ok(text) = header.to_str() else {
                continue;
            };
            let Ok(parsed) = cookie_store::Cookie::parse(text.to_owned(), url) else {
                continue;
            };
            let Some(record) = CookieRecord::from_stored(&parsed) else {
                continue;
            };
            if let Err(error) = update_http(&mut entry.snapshot, &entry.scope, record, url) {
                entry.failure = Some(error);
            }
        }
    }
}

/// Response updates carry the module's explicit HTTP context. They cannot
/// overwrite another partition or an unrelated source port.
pub fn update_http(
    snapshot: &mut Snapshot,
    scope: &Scope,
    mut record: CookieRecord,
    url: &tauri::Url,
) -> Result<(), Error> {
    record.validate()?;
    if !scope.allows(&record) {
        return Ok(());
    }
    let partitioned = record.partitioned == Some(true);
    let indices: Vec<_> = snapshot
        .cookies
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            same_base(c, &record)
                && c.http_compatible(scope)
                && c.partitioned.unwrap_or(false) == partitioned
        })
        .map(|(i, _)| i)
        .collect();
    let matching: Vec<_> = indices
        .iter()
        .copied()
        .filter(|i| snapshot.cookies[*i].source_matches(url))
        .collect();
    let index = match matching.as_slice() {
        [i] => Some(*i),
        [] => None,
        _ => return Err(Error::InvalidMetadata),
    };
    if let Some(i) = index {
        record.extensions = snapshot.cookies[i].extensions.clone();
    } else if partitioned {
        record.extensions = Some(super::model::Extensions {
            partition_key: Some(super::model::PartitionKey {
                top_level_site: scope.top_level_site.clone(),
                has_cross_site_ancestor: Some(false),
            }),
            ..Default::default()
        });
    } else if !indices.is_empty() {
        record.extensions = Some(super::model::Extensions {
            source_scheme: Some(
                if url.scheme() == "https" {
                    "Secure"
                } else {
                    "NonSecure"
                }
                .into(),
            ),
            source_port: url.port_or_known_default().map(i32::from),
            ..Default::default()
        });
    }
    if record.is_definitely_expired(time::OffsetDateTime::now_utc()) {
        if let Some(i) = index {
            snapshot.cookies.remove(i);
        }
    } else if let Some(i) = index {
        snapshot.cookies[i] = record;
    } else {
        if let Some(keys) = snapshot.http_keys.as_mut() {
            keys.push(record.identity());
        }
        snapshot.cookies.push(record);
    }
    if let Some(keys) = snapshot.http_keys.as_mut() {
        keys.retain(|key| snapshot.cookies.iter().any(|c| c.identity() == *key));
    }
    Ok(())
}

/// Reconcile the browser delta against concurrent HTTP response updates using
/// complete identities. Unchanged browser records never overwrite HTTP writes.
#[cfg(any(target_os = "android", test))]
pub fn merge_browser_snapshot(
    current: &Snapshot,
    previous_browser: &Snapshot,
    browser: &Snapshot,
    previous_native: &Snapshot,
) -> Snapshot {
    let mut result = current.clone();
    let mut identities = std::collections::HashSet::new();
    for c in previous_browser
        .cookies
        .iter()
        .chain(browser.cookies.iter())
    {
        identities.insert(c.identity());
    }
    for identity in identities {
        let find = |s: &Snapshot| s.cookies.iter().find(|c| c.identity() == identity).cloned();
        let before = find(previous_browser);
        let next = find(browser);
        if before == next || find(current) != find(previous_native) {
            continue;
        }
        result.cookies.retain(|c| c.identity() != identity);
        if let Some(cookie) = next {
            result.cookies.push(cookie);
        }
    }
    if let Some(keys) = result.http_keys.as_mut() {
        if let Some(browser_keys) = &browser.http_keys {
            keys.extend(browser_keys.iter().cloned());
        }
        keys.retain(|key| result.cookies.iter().any(|c| c.identity() == *key));
        let mut seen = std::collections::HashSet::new();
        keys.retain(|key| seen.insert(key.clone()));
    }
    result
}
#[cfg(any(target_os = "android", test))]
pub fn store_snapshot(
    target: &Arc<CookieStoreMutex>,
    snapshot: Snapshot,
    scope: Scope,
) -> Result<(), String> {
    let projected = project(snapshot.clone(), scope.clone())?;
    *target.lock().map_err(|_| "会话锁失败")? = projected.lock().map_err(|_| "会话锁失败")?.clone();
    remember(target, snapshot, scope);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        storage::{testing::MemoryCredentialStore, CredentialStore},
        webview_cookies::{Extensions, PartitionKey},
    };
    fn row(value: &str, site: Option<&str>) -> CookieRecord {
        let mut row = CookieRecord::from_android(&format!(
            "SESSION={value}; Domain=www.uooc.net.cn; Path=/; Secure; HttpOnly"
        ))
        .unwrap();
        if let Some(site) = site {
            row.partitioned = Some(true);
            row.extensions = Some(Extensions {
                partition_key: Some(PartitionKey {
                    top_level_site: site.into(),
                    has_cross_site_ancestor: Some(false),
                }),
                ..Default::default()
            });
        }
        row
    }
    #[test]
    fn snapshots_survive_vault_roundtrip_without_exposing_other_partitions_to_http() {
        let snapshot = Snapshot {
            cookies: vec![
                row("ordinary", None),
                row("other", Some("https://another.example")),
            ],
            ..Default::default()
        };
        let jar = project(snapshot.clone(), Scope::uooc()).unwrap();
        assert_eq!(jar.lock().unwrap().iter_unexpired().count(), 1);
        let saved = serialize(&jar).unwrap();
        let vault = MemoryCredentialStore::default();
        vault.save_uooc_session(&saved).unwrap();
        let restored = load(&vault.load_uooc_session().unwrap().unwrap()).unwrap();
        let got = native_snapshot(&restored).unwrap().0;
        assert_eq!(got, snapshot);
        assert_eq!(restored.lock().unwrap().iter_unexpired().count(), 1);
        vault.delete_uooc_session().unwrap();
        assert!(vault.load_uooc_session().unwrap().is_none());
    }
    #[test]
    fn http_updates_do_not_change_another_partition() {
        let snapshot = Snapshot {
            cookies: vec![
                row("old", None),
                row("other", Some("https://another.example")),
            ],
            ..Default::default()
        };
        let jar = project(snapshot, Scope::uooc()).unwrap();
        jar.lock()
            .unwrap()
            .insert_raw(
                &RawCookie::parse("SESSION=new; Path=/; Secure; HttpOnly").unwrap(),
                &"https://www.uooc.net.cn/".parse().unwrap(),
            )
            .unwrap();
        let saved = SessionEnvelope::decode(&serialize(&jar).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(saved.webview.cookies.len(), 2);
        assert!(saved.webview.cookies.iter().any(|c| c.value == "other"));
        assert!(saved.webview.cookies.iter().any(|c| c.value == "new"));
    }
    #[test]
    fn source_identities_are_not_collapsed_by_http_serialization() {
        let mut a = row("one", None);
        a.extensions = Some(Extensions {
            source_scheme: Some("Secure".into()),
            source_port: Some(443),
            ..Default::default()
        });
        let mut b = a.clone();
        b.value = "two".into();
        b.extensions.as_mut().unwrap().source_port = Some(8443);
        let snapshot = Snapshot {
            cookies: vec![a, b],
            ..Default::default()
        };
        let jar = project(snapshot.clone(), Scope::uooc()).unwrap();
        let saved = SessionEnvelope::decode(&serialize(&jar).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(saved.webview, snapshot);
    }
    #[test]
    fn old_cookie_json_lines_load_without_an_envelope_alias() {
        let jar = Arc::new(CookieStoreMutex::new(CookieStore::default()));
        for name in ["one", "two"] {
            jar.lock()
                .unwrap()
                .insert_raw(
                    &RawCookie::parse(format!("{name}=value; Path=/")).unwrap(),
                    &"https://www.uooc.net.cn/".parse().unwrap(),
                )
                .unwrap();
        }
        let raw = serialize(&jar).unwrap();
        assert!(SessionEnvelope::decode(&raw).unwrap().is_none());
        assert_eq!(
            load(&raw).unwrap().lock().unwrap().iter_unexpired().count(),
            2
        );
    }
    #[test]
    fn malformed_or_foreign_snapshot_does_not_replace_saved_data() {
        let vault = MemoryCredentialStore::default();
        let snapshot = Snapshot {
            cookies: vec![row("original", None)],
            ..Default::default()
        };
        let valid = SessionEnvelope::encode("".into(), snapshot, Scope::uooc()).unwrap();
        vault.save_uooc_session(&valid).unwrap();
        assert!(SessionEnvelope::decode(&(valid.clone() + "{}")).is_err());
        let mut bad: serde_json::Value = serde_json::from_str(&valid).unwrap();
        bad["webview"]["cookies"][0]["domain"] = serde_json::json!("authserver.szu.edu.cn");
        assert!(load(&bad.to_string()).is_err());
        assert_eq!(vault.load_uooc_session().unwrap(), Some(valid));
    }
    #[test]
    fn same_name_partition_and_ordinary_survive_http_and_vault_roundtrip() {
        let snapshot = Snapshot {
            cookies: vec![
                row("ordinary", None),
                row("own", Some("https://uooc.net.cn")),
                row("foreign", Some("https://another.example")),
            ],
            ..Default::default()
        };
        let jar = project(snapshot.clone(), Scope::uooc()).unwrap();
        assert_eq!(
            SessionEnvelope::decode(&serialize(&jar).unwrap())
                .unwrap()
                .unwrap()
                .webview,
            snapshot
        );
        let provider = http_provider(jar.clone());
        let url = "https://www.uooc.net.cn/".parse().unwrap();
        assert_eq!(
            reqwest::cookie::CookieStore::cookies(provider.as_ref(), &url)
                .unwrap()
                .to_str()
                .unwrap(),
            "SESSION=ordinary; SESSION=own"
        );
        let saved = serialize(&jar).unwrap();
        let restored = load(&saved).unwrap();
        let restored_provider = http_provider(restored);
        assert_eq!(
            reqwest::cookie::CookieStore::cookies(provider.as_ref(), &url),
            reqwest::cookie::CookieStore::cookies(restored_provider.as_ref(), &url)
        );
    }
    #[test]
    fn responses_update_and_delete_only_the_current_partition_and_source() {
        let mut regular = row("ordinary", None);
        regular.extensions = Some(Extensions {
            source_scheme: Some("Secure".into()),
            source_port: Some(443),
            ..Default::default()
        });
        let mut other_port = regular.clone();
        other_port.value = "port".into();
        other_port.extensions.as_mut().unwrap().source_port = Some(8443);
        let jar = project(
            Snapshot {
                cookies: vec![
                    regular,
                    other_port,
                    row("own", Some("https://uooc.net.cn")),
                    row("foreign", Some("https://another.example")),
                ],
                ..Default::default()
            },
            Scope::uooc(),
        )
        .unwrap();
        let provider = http_provider(jar.clone());
        let url = "https://www.uooc.net.cn/".parse().unwrap();
        for header in [
            "SESSION=new; Path=/; Secure; HttpOnly",
            "SESSION=; Path=/; Secure; HttpOnly; Partitioned",
        ] {
            let header = reqwest::header::HeaderValue::from_str(header).unwrap();
            reqwest::cookie::CookieStore::set_cookies(
                provider.as_ref(),
                &mut [&header].into_iter(),
                &url,
            );
        }
        let snapshot = SessionEnvelope::decode(&serialize(&jar).unwrap())
            .unwrap()
            .unwrap()
            .webview;
        assert_eq!(snapshot.cookies.len(), 4);
        assert!(snapshot.cookies.iter().any(|c| c.value == "new"));
        assert!(snapshot
            .cookies
            .iter()
            .any(|c| c.value.is_empty() && c.partitioned == Some(true)));
        let deletion =
            reqwest::header::HeaderValue::from_static("SESSION=gone; Path=/; Secure; Max-Age=0");
        reqwest::cookie::CookieStore::set_cookies(
            provider.as_ref(),
            &mut [&deletion].into_iter(),
            &url,
        );
        let snapshot = SessionEnvelope::decode(&serialize(&jar).unwrap())
            .unwrap()
            .unwrap()
            .webview;
        assert_eq!(snapshot.cookies.len(), 3);
        assert!(snapshot.cookies.iter().any(|c| c.value == "port"));
        assert!(snapshot.cookies.iter().any(|c| c.value == "foreign"));
    }
    #[test]
    fn response_default_path_and_foreign_domain_follow_cookie_rules() {
        let jar = project(Snapshot::default(), Scope::uooc()).unwrap();
        let provider = http_provider(jar.clone());
        let url = "https://www.uooc.net.cn/home/member/user".parse().unwrap();
        let headers = [
            reqwest::header::HeaderValue::from_static("sid=fixture; Secure; HttpOnly"),
            reqwest::header::HeaderValue::from_static("evil=fixture; Domain=other.example; Path=/"),
        ];
        reqwest::cookie::CookieStore::set_cookies(provider.as_ref(), &mut headers.iter(), &url);
        let snapshot = native_snapshot(&jar).unwrap().0;
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(snapshot.cookies[0].path, "/home/member");
        assert!(snapshot.cookies[0].host_only);
    }
    #[test]
    fn browser_delta_preserves_concurrent_http_updates_and_other_partitions() {
        let initial = Snapshot {
            cookies: vec![
                row("old", None),
                row("partition", Some("https://another.example")),
            ],
            ..Default::default()
        };
        let mut current = initial.clone();
        current.cookies[0].value = "http".into();
        let mut browser = initial.clone();
        browser.cookies[0].value = "webview".into();
        browser.cookies[1].http_only = Some(false);
        let merged =
            crate::webview_cookies::merge_browser_snapshot(&current, &initial, &browser, &initial);
        assert!(merged.cookies.iter().any(|c| c.value == "http"));
        assert!(merged
            .cookies
            .iter()
            .any(|c| c.value == "partition" && c.http_only == Some(false)));
        let target = project(current, Scope::uooc()).unwrap();
        let _provider = http_provider(target.clone());
        crate::webview_cookies::store_snapshot(&target, merged.clone(), Scope::uooc()).unwrap();
        assert_eq!(
            SessionEnvelope::decode(&serialize(&target).unwrap())
                .unwrap()
                .unwrap()
                .webview,
            merged
        );
    }
}
