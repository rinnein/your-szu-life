use super::*;
use std::sync::{Arc, Mutex};
use tokio_tungstenite::accept_async;

fn protocol(object: bool) -> Value {
    let mut props: Vec<_> = [
        "name",
        "value",
        "url",
        "domain",
        "path",
        "secure",
        "httpOnly",
        "sameSite",
        "expires",
        "priority",
        "sameParty",
        "sourceScheme",
        "sourcePort",
    ]
    .into_iter()
    .map(|name| json!({"name":name}))
    .collect();
    props.push(if object {
        json!({"name":"partitionKey","$ref":"CookiePartitionKey"})
    } else {
        json!({"name":"partitionKey","type":"string"})
    });
    json!({"domains":[{"domain":"Network","types":[{"id":"CookieParam","properties":props},{"id":"CookiePartitionKey","properties":if object{json!([{"name":"hasCrossSiteAncestor"}])}else{json!([])}}],"commands":[{"name":"deleteCookies","parameters":[{"name":"partitionKey"}]}]}]})
}
fn cookie(domain: &str, name: &str, value: &str) -> Value {
    json!({"name":name,"value":value,"domain":domain,"path":"/","expires":-1,"session":true,"httpOnly":true,"secure":true,"sourceScheme":"Secure","sourcePort":443,"priority":"Medium","sameParty":false})
}
async fn mock(
    initial: Vec<Value>,
    object: bool,
    storage: bool,
    drop_write_response: bool,
) -> (
    Session<tokio::net::UnixStream>,
    Arc<Mutex<Vec<Value>>>,
    tokio::task::JoinHandle<()>,
) {
    let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    a.set_nonblocking(true).unwrap();
    b.set_nonblocking(true).unwrap();
    let (a, b) = (
        tokio::net::UnixStream::from_std(a).unwrap(),
        tokio::net::UnixStream::from_std(b).unwrap(),
    );
    let state = Arc::new(Mutex::new(initial));
    let saved = state.clone();
    let server = serve(b, saved, storage, drop_write_response);
    let session = Session::from_stream(a, protocol(object)).await.unwrap();
    (session, state, server)
}
fn serve<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
    b: S,
    saved: Arc<Mutex<Vec<Value>>>,
    storage: bool,
    drop_write_response: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ws = accept_async(b).await.unwrap();
        while let Some(Ok(Message::Text(text))) = ws.next().await {
            let request: Value = serde_json::from_str(&text).unwrap();
            let id = request["id"].clone();
            let params = &request["params"];
            let result = match request["method"].as_str().unwrap() {
                "Storage.getCookies" if !storage => {
                    ws.send(Message::Text(
                        json!({"id":id,"error":{"code":-32601}}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                    continue;
                }
                "Storage.getCookies" | "Network.getAllCookies" => {
                    json!({"cookies":saved.lock().unwrap().clone()})
                }
                "Network.deleteCookies" => {
                    saved.lock().unwrap().retain(|c| {
                        !(c["name"] == params["name"]
                            && c["domain"] == params["domain"]
                            && c["path"] == params["path"]
                            && c.get("partitionKey") == params.get("partitionKey"))
                    });
                    json!({})
                }
                "Network.setCookies" => {
                    let mut jar = saved.lock().unwrap();
                    for param in params["cookies"].as_array().unwrap() {
                        let url = tauri::Url::parse(param["url"].as_str().unwrap()).unwrap();
                        let domain = param["domain"].as_str().unwrap_or(url.host_str().unwrap());
                        let mut item = cookie(
                            domain,
                            param["name"].as_str().unwrap(),
                            param["value"].as_str().unwrap(),
                        );
                        item["path"] = param["path"].clone();
                        item["secure"] = param["secure"].clone();
                        item["httpOnly"] = param["httpOnly"].clone();
                        for key in [
                            "sameSite",
                            "partitionKey",
                            "sourceScheme",
                            "sourcePort",
                            "priority",
                            "sameParty",
                        ] {
                            if let Some(v) = param.get(key) {
                                item[key] = v.clone();
                            }
                        }
                        if let Some(v) = param.get("expires") {
                            item["expires"] = v.clone();
                            item["session"] = json!(false);
                        }
                        jar.push(item);
                    }
                    drop(jar);
                    if drop_write_response {
                        continue;
                    }
                    json!({})
                }
                _ => json!({}),
            };
            if ws
                .send(Message::Text(
                    json!({"id":id,"result":result}).to_string().into(),
                ))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}
#[tokio::test]
async fn reads_old_and_new_protocols_without_database_schema_checks() {
    for object in [false, true] {
        let mut partitioned = cookie("www.uooc.net.cn", "SESSION", "partition");
        partitioned["partitionKey"] = if object {
            json!({"topLevelSite":"https://another.example","hasCrossSiteAncestor":true})
        } else {
            json!("https://another.example")
        };
        let ordinary = cookie("www.uooc.net.cn", "SESSION", "ordinary");
        let (mut client, _, server) =
            mock(vec![ordinary, partitioned], object, !object, false).await;
        let snapshot = client.capture(&Scope::uooc()).await.unwrap();
        assert_eq!(snapshot.cookies.len(), 2);
        assert!(snapshot.cookies[0].identity() != snapshot.cookies[1].identity());
        assert_eq!(snapshot.http_cookies(&Scope::uooc()).count(), 1);
        server.abort();
    }
}
#[tokio::test]
async fn restores_cross_partition_and_host_only_cookies_without_domain_shadows() {
    for object in [false, true] {
        let mut partitioned = cookie("www.uooc.net.cn", "SESSION", "partition");
        partitioned["partitionKey"] = if object {
            json!({"topLevelSite":"https://another.example","hasCrossSiteAncestor":true})
        } else {
            json!("https://another.example")
        };
        let initial = vec![
            cookie("www.uooc.net.cn", "SESSION", "host"),
            cookie(".uooc.net.cn", "SESSION", "domain"),
            partitioned,
        ];
        let (mut client, state, server) = mock(initial, object, true, false).await;
        let snapshot = client.capture(&Scope::uooc()).await.unwrap();
        client
            .replace(&Scope::uooc(), &Snapshot::default())
            .await
            .unwrap();
        client.replace(&Scope::uooc(), &snapshot).await.unwrap();
        assert_eq!(state.lock().unwrap().len(), 3);
        assert_eq!(
            state
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c["domain"] == "www.uooc.net.cn")
                .count(),
            2
        );
        assert_eq!(client.capture(&Scope::uooc()).await.unwrap(), snapshot);
        server.abort();
    }
}
#[tokio::test]
async fn scoped_clear_leaves_other_modules_and_partitions_intact() {
    let mut part = cookie("authserver.szu.edu.cn", "SESSION", "partition");
    part["partitionKey"] =
        json!({"topLevelSite":"https://another.example","hasCrossSiteAncestor":true});
    let (mut client, state, server) = mock(
        vec![
            cookie("authserver.szu.edu.cn", "SESSION", "auth"),
            part,
            cookie("www.uooc.net.cn", "SESSION", "uooc"),
        ],
        true,
        true,
        false,
    )
    .await;
    client
        .replace(&Scope::auth(), &Snapshot::default())
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().len(), 1);
    assert_eq!(state.lock().unwrap()[0]["domain"], "www.uooc.net.cn");
    server.abort();
}
#[tokio::test]
async fn opaque_cookies_outside_scope_do_not_block_login() {
    let mut unrelated = cookie("other.example", "SESSION", "hidden");
    unrelated["partitionKeyOpaque"] = json!(true);
    let (mut client, _, server) = mock(
        vec![unrelated, cookie("www.uooc.net.cn", "SESSION", "ok")],
        true,
        true,
        false,
    )
    .await;
    assert_eq!(
        client.capture(&Scope::uooc()).await.unwrap().cookies.len(),
        1
    );
    server.abort();
    let mut related = cookie("www.uooc.net.cn", "SESSION", "hidden");
    related["partitionKeyOpaque"] = json!(true);
    let (mut client, _, server) = mock(vec![related], true, true, false).await;
    assert_eq!(
        client.capture(&Scope::uooc()).await.unwrap_err(),
        Error::UnsupportedPartition
    );
    server.abort();
}
#[tokio::test]
async fn independent_sources_are_not_deduplicated() {
    let a = cookie("www.uooc.net.cn", "SESSION", "one");
    let mut b = a.clone();
    b["sourcePort"] = json!(8443);
    b["value"] = json!("two");
    let (mut client, _, server) = mock(vec![a, b], true, true, false).await;
    let snapshot = client.capture(&Scope::uooc()).await.unwrap();
    assert_eq!(snapshot.cookies.len(), 2);
    client
        .replace(&Scope::uooc(), &Snapshot::default())
        .await
        .unwrap();
    client.replace(&Scope::uooc(), &snapshot).await.unwrap();
    assert_eq!(client.capture(&Scope::uooc()).await.unwrap(), snapshot);
    server.abort();
}
#[tokio::test]
async fn http_cookie_patch_does_not_overwrite_another_partition() {
    let mut part = cookie("www.uooc.net.cn", "SESSION", "partition");
    part["partitionKey"] =
        json!({"topLevelSite":"https://another.example","hasCrossSiteAncestor":true});
    let (mut client, _, server) = mock(
        vec![cookie("www.uooc.net.cn", "SESSION", "old"), part],
        true,
        true,
        false,
    )
    .await;
    let raw =
        CookieRecord::from_android("SESSION=new; Domain=www.uooc.net.cn; Path=/; Secure; HttpOnly")
            .unwrap();
    client
        .apply(
            &Scope::uooc(),
            &[Patch {
                cookie: raw,
                remove: false,
            }],
        )
        .await
        .unwrap();
    let got = client.capture(&Scope::uooc()).await.unwrap();
    assert_eq!(got.cookies.len(), 2);
    assert!(got.cookies.iter().any(|c| c.value == "partition"));
    assert!(got.cookies.iter().any(|c| c.value == "new"));
    server.abort();
}
#[tokio::test]
async fn all_business_scopes_support_cookie_validation() {
    for (kind, url, domain, name, secure) in [
        (
            super::super::Kind::Auth,
            "https://authserver.szu.edu.cn/authserver/login",
            "authserver.szu.edu.cn",
            "SESSION",
            true,
        ),
        (
            super::super::Kind::Uooc,
            "https://www.uooc.net.cn/home/member/user",
            "www.uooc.net.cn",
            "SESSION",
            true,
        ),
        (
            super::super::Kind::Campus,
            "http://bkxk.szu.edu.cn/",
            "bkxk.szu.edu.cn",
            "JSESSIONID",
            false,
        ),
        (
            super::super::Kind::Webvpn,
            "https://bkxk.webvpn.szu.edu.cn/",
            "bkxk.webvpn.szu.edu.cn",
            "JSESSIONID",
            true,
        ),
        (
            super::super::Kind::Gateway,
            "https://webvpn.szu.edu.cn/",
            "webvpn.szu.edu.cn",
            "_webvpn_key",
            true,
        ),
        (
            super::super::Kind::Academic,
            "https://ehall.szu.edu.cn/",
            "ehall.szu.edu.cn",
            "JSESSIONID",
            true,
        ),
    ] {
        let mut item = cookie(domain, name, "fixture");
        item["secure"] = json!(secure);
        let scope = Scope::new(kind, &[url]).unwrap();
        let (mut client, _, server) = mock(vec![item], true, true, false).await;
        let snapshot = client.capture(&scope).await.unwrap();
        assert_eq!(snapshot.cookies.len(), 1);
        assert_eq!(snapshot.http_cookies(&scope).count(), 1);
        let jar = crate::webview_cookies::project(snapshot.clone(), scope.clone()).unwrap();
        assert_eq!(
            jar.lock()
                .unwrap()
                .get_request_values(&tauri::Url::parse(url).unwrap())
                .count(),
            1
        );
        client.replace(&scope, &snapshot).await.unwrap();
        server.abort();
    }
}
#[tokio::test]
async fn missing_write_acknowledgement_is_verified_without_replaying() {
    let (mut client, state, server) = mock(vec![], true, true, true).await;
    let record =
        CookieRecord::from_android("SESSION=new; Domain=www.uooc.net.cn; Path=/; Secure; HttpOnly")
            .unwrap();
    client
        .replace(
            &Scope::uooc(),
            &Snapshot {
                cookies: vec![record],
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(state.lock().unwrap().len(), 1);
    server.abort();
}
#[tokio::test]
async fn cancelled_window_cannot_restore_a_session() {
    let original = cookie("www.uooc.net.cn", "SESSION", "unchanged");
    let (mut client, state, server) = mock(vec![original.clone()], true, true, false).await;
    let desired = client.capture(&Scope::uooc()).await.unwrap();
    client.alive = Box::new(|| false);
    assert_eq!(
        client.replace(&Scope::uooc(), &desired).await,
        Err(Error::Unavailable)
    );
    assert_eq!(*state.lock().unwrap(), vec![original]);
    server.abort();
}
#[test]
fn malformed_discovery_and_foreign_scope_are_rejected() {
    assert_eq!(
        decode_http(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"),
        Ok(json!({}))
    );
    assert_eq!(
        decode_http(b"HTTP/1.1 404 No\r\n\r\n{}"),
        Err(Error::Unavailable)
    );
    assert!(Scope::new(super::super::Kind::Uooc, &["https://other.example/"]).is_err());
    assert!(Scope::new(
        super::super::Kind::Auth,
        &["https://user@authserver.szu.edu.cn/"]
    )
    .is_err());
}

#[test]
fn native_headers_select_equal_values_by_source_and_keep_own_partition() {
    let mut other_port = cookie("www.uooc.net.cn", "SESSION", "same");
    other_port["sourcePort"] = json!(8443);
    let mut own = cookie("www.uooc.net.cn", "SESSION", "own");
    own["partitionKey"] =
        json!({"topLevelSite":"https://uooc.net.cn","hasCrossSiteAncestor":false});
    let scope = Scope::uooc();
    let mut snapshot = decode_cookies(
        &json!({"cookies":[cookie("www.uooc.net.cn", "SESSION", "same"), other_port, own]}),
        &scope,
    )
    .unwrap();
    select_http(
        &mut snapshot,
        &scope,
        &vec!["SESSION=same".into(); scope.urls.len()],
    )
    .unwrap();
    assert_eq!(snapshot.http_cookies(&scope).count(), 2);
    assert_eq!(
        select_http(&mut snapshot, &scope, &[]),
        Err(Error::Protocol)
    );
    assert_eq!(
        select_http(
            &mut snapshot,
            &scope,
            &vec!["SESSION=changed".into(); scope.urls.len()]
        ),
        Err(Error::Unavailable)
    );
}
#[tokio::test]
async fn incompatible_partition_format_fails_before_any_deletion() {
    let original = cookie("www.uooc.net.cn", "SESSION", "saved");
    let (mut client, state, server) = mock(vec![original.clone()], false, true, false).await;
    let mut desired = client.capture(&Scope::uooc()).await.unwrap();
    desired.cookies[0].partitioned = Some(true);
    desired.cookies[0]
        .extensions
        .as_mut()
        .unwrap()
        .partition_key = Some(PartitionKey {
        top_level_site: "https://uooc.net.cn".into(),
        has_cross_site_ancestor: Some(false),
    });
    assert_eq!(
        client.replace(&Scope::uooc(), &desired).await,
        Err(Error::UnsupportedPartition)
    );
    assert_eq!(*state.lock().unwrap(), vec![original]);
    server.abort();
}

#[tokio::test]
async fn temporary_reads_retry_three_times_but_metadata_errors_do_not() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let attempts = AtomicUsize::new(0);
    let result = read_retry(|| async {
        if attempts.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    })
    .await;
    assert_eq!(result, Ok(()));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    for error in [
        Error::Unavailable,
        Error::InvalidMetadata,
        Error::UnsupportedPartition,
        Error::Protocol,
    ] {
        attempts.store(0, Ordering::SeqCst);
        let result: Result<(), Error> = read_retry(|| async {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(error)
        })
        .await;
        assert_eq!(result, Err(error));
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            if error == Error::Unavailable { 3 } else { 1 }
        );
    }
}
#[test]
fn empty_expired_invalid_and_duplicate_snapshots_are_distinguished() {
    let scope = Scope::uooc();
    assert!(decode_cookies(&json!({"cookies":[]}), &scope)
        .unwrap()
        .cookies
        .is_empty());
    let mut expired = cookie("www.uooc.net.cn", "SESSION", "expired");
    expired["session"] = json!(false);
    expired["expires"] = json!(1);
    assert!(decode_cookies(&json!({"cookies":[expired]}), &scope)
        .unwrap()
        .cookies
        .is_empty());
    let mut missing = cookie("www.uooc.net.cn", "SESSION", "bad");
    missing.as_object_mut().unwrap().remove("httpOnly");
    assert_eq!(
        decode_cookies(&json!({"cookies":[missing]}), &scope).unwrap_err(),
        Error::Protocol
    );
    let original = cookie("www.uooc.net.cn", "SESSION", "fixture");
    assert_eq!(
        decode_cookies(&json!({"cookies":[original.clone(),original]}), &scope).unwrap_err(),
        Error::InvalidMetadata
    );
}

#[test]
fn invalid_metadata_does_not_become_an_empty_or_ordinary_session() {
    let scope = Scope::uooc();
    for (field, invalid) in [
        ("name", json!("bad name")),
        ("value", json!("bad\tvalue")),
        ("session", Value::Null),
        ("priority", json!("Unknown")),
    ] {
        let mut row = cookie("www.uooc.net.cn", "SESSION", "fixture");
        row[field] = invalid;
        assert!(decode_cookies(&json!({"cookies":[row]}), &scope).is_err());
    }
    let mut record =
        CookieRecord::from_android("SESSION=fixture; Domain=www.uooc.net.cn; Path=/").unwrap();
    record.partitioned = Some(true);
    assert_eq!(
        Snapshot {
            cookies: vec![record],
            ..Default::default()
        }
        .validate(&scope),
        Err(Error::UnsupportedPartition)
    );
}

#[tokio::test]
async fn discovery_http_does_not_wait_for_persistent_connections_to_close() {
    use tokio::io::AsyncWriteExt;
    for chunked in [false, true] {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let payload = if chunked {
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n0\r\n\r\n".as_slice()
        } else {
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: keep-alive\r\n\r\n{}".as_slice()
        };
        server.write_all(payload).await.unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(500), read_http_response(&mut client))
                .await
                .unwrap()
                .unwrap(),
            json!({})
        );
        // `server` deliberately remains open through the assertion.
        drop(server);
    }
    assert_eq!(
        decode_http(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n{}"),
        Err(Error::Protocol)
    );
    assert_eq!(
        decode_http(b"HTTP/1.1 200 OK\r\nContent-Length: 9999999999\r\n\r\n"),
        Err(Error::Protocol)
    );
}
fn pair() -> (tokio::net::UnixStream, tokio::net::UnixStream) {
    let (a, b) = std::os::unix::net::UnixStream::pair().unwrap();
    a.set_nonblocking(true).unwrap();
    b.set_nonblocking(true).unwrap();
    (
        tokio::net::UnixStream::from_std(a).unwrap(),
        tokio::net::UnixStream::from_std(b).unwrap(),
    )
}
fn discovery_reply(
    body: Value,
    path: &'static str,
) -> (tokio::net::UnixStream, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (a, mut b) = pair();
    let server = tokio::spawn(async move {
        let mut request = vec![];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut bytes = [0u8; 1024];
            let n = b.read(&mut bytes).await.unwrap();
            assert!(n > 0);
            request.extend_from_slice(&bytes[..n]);
        }
        assert!(std::str::from_utf8(&request)
            .unwrap()
            .starts_with(&format!("GET {path} HTTP/1.1")));
        let body = body.to_string();
        b.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        // The real Chromium server does not close after replying.
        let mut extra = [0u8; 1];
        let _ = b.read(&mut extra).await;
    });
    (a, server)
}
#[test]
fn missing_androidx_api_can_discover_capture_restore_and_clear_with_real_fake_cdp() {
    use crate::storage::{testing::MemoryCredentialStore, CredentialStore};
    use crate::webview_cookies::ops::{self, Transport};
    use std::collections::VecDeque;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (client, servers) = runtime.block_on(async {
        let (list, list_server) = discovery_reply(
            json!([{"type":"page","webSocketDebuggerUrl":"ws://localhost/devtools/page/cookies"}]),
            "/json/list",
        );
        let (schema, schema_server) = discovery_reply(protocol(true), "/json/protocol");
        let (ws, peer) = pair();
        let state = Arc::new(Mutex::new(vec![cookie(
            "authserver.szu.edu.cn",
            "JSESSIONID",
            "virtual",
        )]));
        let server = serve(peer, state, true, false);
        let mut streams = VecDeque::from([list, schema, ws]);
        let client = tokio::time::timeout(
            Duration::from_secs(2),
            open_with(
                || std::future::ready(Ok(streams.pop_front().unwrap())),
                Box::new(|| true),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        (client, vec![list_server, schema_server, server])
    });
    struct Legacy {
        runtime: tokio::runtime::Runtime,
        client: Session<tokio::net::UnixStream>,
    }
    impl Transport for Legacy {
        fn native_available(&mut self) -> bool {
            false
        }
        fn capture_native(&mut self, _: &Scope) -> Result<Snapshot, Error> {
            panic!("unavailable AndroidX getter must not be called")
        }
        fn write_native(&mut self, _: &Scope, _: &[Patch]) -> Result<(), Error> {
            panic!("full legacy transport must use CDP")
        }
        fn capture_cdp(&mut self, s: &Scope) -> Result<Snapshot, Error> {
            self.runtime.block_on(self.client.capture(s))
        }
        fn apply_cdp(&mut self, s: &Scope, p: &[Patch]) -> Result<(), Error> {
            self.runtime.block_on(self.client.apply(s, p))
        }
        fn replace_cdp(&mut self, s: &Scope, p: &Snapshot) -> Result<(), Error> {
            self.runtime.block_on(self.client.replace(s, p))
        }
    }
    let mut transport = Legacy { runtime, client };
    let scope = Scope::auth();
    let vault = MemoryCredentialStore::default();
    let snapshot = ops::capture(&mut transport, &scope).unwrap();
    let secret = crate::auth::session::AuthSessionSecret {
        student_id: "1234567890".into(),
        format_version: 4,
        cookies: snapshot.cookies.clone(),
        http_keys: snapshot.http_keys.clone(),
    };
    vault.save_session(&secret).unwrap();
    ops::replace(&mut transport, &scope, &Snapshot::default()).unwrap();
    assert!(ops::capture(&mut transport, &scope)
        .unwrap()
        .cookies
        .is_empty());
    let saved = vault.load_session().unwrap().unwrap();
    ops::replace(
        &mut transport,
        &scope,
        &Snapshot {
            cookies: saved.cookies,
            http_keys: saved.http_keys,
        },
    )
    .unwrap();
    assert_eq!(ops::capture(&mut transport, &scope).unwrap(), snapshot);
    ops::replace(&mut transport, &scope, &Snapshot::default()).unwrap();
    vault.delete_session().unwrap();
    assert!(ops::capture(&mut transport, &scope)
        .unwrap()
        .cookies
        .is_empty());
    for server in servers {
        server.abort();
    }
}
