//! Decode AndroidX CookieManagerCompat's canonical cookie-info representation.
use super::{CookieRecord, Error, Scope, Snapshot};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
enum Reply {
    Ok { cookies: Vec<Vec<String>> },
    Error { code: Error },
}
pub fn decode(raw: &str, scope: &Scope) -> Result<Snapshot, Error> {
    let rows = match serde_json::from_str::<Reply>(raw).map_err(|_| Error::Protocol)? {
        Reply::Ok { cookies } => cookies,
        Reply::Error { code } => return Err(code),
    };
    if rows.len() != scope.urls.len() {
        return Err(Error::Protocol);
    }
    let mut cookies = Vec::new();
    let mut known = HashMap::new();
    for list in rows {
        let mut within_url = HashSet::new();
        for raw in list {
            let cookie = CookieRecord::from_android(&raw)?;
            if !scope.allows(&cookie) {
                continue;
            }
            if cookie.partitioned == Some(true) {
                return Err(Error::UnsupportedPartition);
            }
            if raw.split(';').skip(1).any(|s| {
                let name = s.trim().split('=').next().unwrap_or("");
                [
                    "Priority",
                    "SameParty",
                    "SourceScheme",
                    "SourcePort",
                    "PartitionKey",
                ]
                .iter()
                .any(|known| name.eq_ignore_ascii_case(known))
            }) {
                return Err(Error::UnsupportedMethod);
            }
            if cookie.is_definitely_expired(time::OffsetDateTime::now_utc()) {
                continue;
            }
            let identity = cookie.identity();
            if !within_url.insert(identity.clone()) {
                return Err(Error::UnsupportedMethod);
            }
            if let Some(previous) = known.get(&identity) {
                // Duplicate rows within one URL may be distinct source identities
                // that AndroidX cannot express. Across URLs, unchanged rows repeat.
                if previous != &cookie {
                    // Native metadata cannot distinguish source-bound identities
                    // from a changing snapshot. Let CDP resolve the full identity.
                    return Err(Error::UnsupportedMethod);
                }
                continue;
            }
            known.insert(identity, cookie.clone());
            cookies.push(cookie);
        }
    }
    let snapshot = Snapshot {
        cookies,
        ..Default::default()
    };
    snapshot.validate(scope)?;
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn canonical_host_domain_and_path_rows_survive_cross_url_deduplication() {
        let scope = Scope::auth();
        let rows = vec![
            "sid=host; Domain=authserver.szu.edu.cn; Path=/; Secure; HttpOnly",
            "sid=domain; Domain=.authserver.szu.edu.cn; Path=/; Secure; HttpOnly",
        ];
        let input = json!({"status":"ok","cookies":vec![rows;scope.urls.len()]}).to_string();
        let got = decode(&input, &scope).unwrap();
        assert_eq!(got.cookies.len(), 2);
        assert!(got.cookies.iter().any(|c| c.host_only));
        assert!(got.cookies.iter().any(|c| !c.host_only));
    }
    #[test]
    fn empty_sessions_are_valid_but_partition_and_hidden_source_duplicates_require_cdp() {
        let scope = Scope::auth();
        assert!(decode(
            &json!({"status":"ok","cookies":vec![Vec::<String>::new();scope.urls.len()]})
                .to_string(),
            &scope
        )
        .unwrap()
        .cookies
        .is_empty());
        for (rows, error) in [
            (
                vec!["sid=part; Domain=authserver.szu.edu.cn; Path=/; Secure; Partitioned"],
                Error::UnsupportedPartition,
            ),
            (
                vec![
                    "sid=one; Domain=authserver.szu.edu.cn; Path=/",
                    "sid=two; Domain=authserver.szu.edu.cn; Path=/",
                ],
                Error::UnsupportedMethod,
            ),
        ] {
            let input = json!({"status":"ok","cookies":vec![rows;scope.urls.len()]}).to_string();
            assert_eq!(decode(&input, &scope).unwrap_err(), error);
        }
        assert_eq!(
            decode(r#"{"status":"error","code":"Unavailable"}"#, &scope).unwrap_err(),
            Error::Unavailable
        );
    }
}
