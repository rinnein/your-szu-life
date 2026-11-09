//! Cookie-only CDP client. Wire messages must never be logged.
use super::ops::equivalent;
use super::{CookieRecord, Error, Extensions, PartitionKey, Patch, Scope, Snapshot};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    WebSocketStream,
};

const MAX_RESPONSE: usize = 32 * 1024 * 1024;
const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct Protocol {
    params: HashSet<String>,
    partition_object: bool,
    ancestor_required: bool,
    delete_partition: bool,
}
impl Protocol {
    fn parse(value: &Value) -> Result<Self, Error> {
        let network = value["domains"]
            .as_array()
            .and_then(|a| a.iter().find(|d| d["domain"] == "Network"))
            .ok_or(Error::Unavailable)?;
        let types = network["types"].as_array().ok_or(Error::Unavailable)?;
        let cookie = types
            .iter()
            .find(|t| t["id"] == "CookieParam")
            .ok_or(Error::Unavailable)?;
        let properties = cookie["properties"].as_array().ok_or(Error::Unavailable)?;
        let params = properties
            .iter()
            .filter_map(|p| p["name"].as_str().map(str::to_owned))
            .collect();
        let partition_object = properties
            .iter()
            .find(|p| p["name"] == "partitionKey")
            .is_some_and(|p| p.get("$ref").is_some());
        let ancestor_required = types
            .iter()
            .find(|t| t["id"] == "CookiePartitionKey")
            .and_then(|t| t["properties"].as_array())
            .is_some_and(|ps| ps.iter().any(|p| p["name"] == "hasCrossSiteAncestor"));
        let delete_partition = network["commands"]
            .as_array()
            .and_then(|a| a.iter().find(|c| c["name"] == "deleteCookies"))
            .and_then(|c| c["parameters"].as_array())
            .is_some_and(|ps| ps.iter().any(|p| p["name"] == "partitionKey"));
        Ok(Self {
            params,
            partition_object,
            ancestor_required,
            delete_partition,
        })
    }
    fn partition(&self, key: &PartitionKey) -> Result<Value, Error> {
        if self.partition_object {
            if self.ancestor_required && key.has_cross_site_ancestor.is_none() {
                return Err(Error::UnsupportedPartition);
            }
            let mut value = json!({"topLevelSite":key.top_level_site});
            if self.ancestor_required {
                value["hasCrossSiteAncestor"] = json!(key
                    .has_cross_site_ancestor
                    .ok_or(Error::UnsupportedPartition)?);
            }
            Ok(value)
        } else {
            if key.has_cross_site_ancestor.is_some() {
                return Err(Error::UnsupportedPartition);
            }
            Ok(json!(key.top_level_site))
        }
    }
    fn cookie(&self, c: &CookieRecord, scope: &Scope) -> Result<Value, Error> {
        c.validate()?;
        if !scope.allows(c) {
            return Err(Error::OutOfScope);
        }
        let mut value = json!({"name":c.name,"value":c.value,"url":c.url(scope)?.as_str(),"path":c.path,
            "secure":c.secure.unwrap_or(false),"httpOnly":c.http_only.unwrap_or(false)});
        if !c.host_only {
            value["domain"] = json!(format!(".{}", c.domain.trim_start_matches('.')));
        }
        if let Some(s) = &c.same_site {
            value["sameSite"] = json!(s);
        }
        if let Some(s) = &c.expires {
            let t = time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc2822)
                .map_err(|_| Error::InvalidMetadata)?;
            value["expires"] = json!(t.unix_timestamp());
        }
        if let Some(e) = &c.extensions {
            for (name, item) in [
                ("sourceScheme", e.source_scheme.as_ref().map(|v| json!(v))),
                ("sourcePort", e.source_port.map(|v| json!(v))),
                ("priority", e.priority.as_ref().map(|v| json!(v))),
                ("sameParty", e.same_party.filter(|v| *v).map(|v| json!(v))),
            ] {
                if let Some(item) = item {
                    if !self.params.contains(name) {
                        return Err(Error::InvalidMetadata);
                    }
                    value[name] = item;
                }
            }
            if let Some(key) = &e.partition_key {
                if !self.params.contains("partitionKey") {
                    return Err(Error::UnsupportedPartition);
                }
                value["partitionKey"] = self.partition(key)?;
            }
        }
        if c.partitioned == Some(true) && value.get("partitionKey").is_none() {
            return Err(Error::UnsupportedPartition);
        }
        Ok(value)
    }
    fn deletion(&self, c: &CookieRecord) -> Result<Value, Error> {
        let mut v = json!({"name":c.name,"domain":if c.host_only {c.domain.clone()} else {format!(".{}",c.domain.trim_start_matches('.'))},"path":c.path});
        if let Some(key) = c.extensions.as_ref().and_then(|e| e.partition_key.as_ref()) {
            if !self.delete_partition {
                return Err(Error::UnsupportedPartition);
            }
            v["partitionKey"] = self.partition(key)?;
        }
        Ok(v)
    }
}

pub(super) struct Session<S> {
    ws: WebSocketStream<S>,
    next_id: u64,
    protocol: Protocol,
    alive: Box<dyn Fn() -> bool + Send>,
}
impl<S: AsyncRead + AsyncWrite + Unpin + Send> Session<S> {
    #[cfg(test)]
    pub(super) async fn from_stream(stream: S, protocol: Value) -> Result<Self, Error> {
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_RESPONSE))
            .max_frame_size(Some(MAX_RESPONSE));
        let (ws, _) = tokio::time::timeout(
            Duration::from_secs(2),
            tokio_tungstenite::client_async_with_config(
                "ws://localhost/devtools/page/cookies",
                stream,
                Some(config),
            ),
        )
        .await
        .map_err(|_| Error::Unavailable)?
        .map_err(|_| Error::Unavailable)?;
        Ok(Self {
            ws,
            next_id: 0,
            protocol: Protocol::parse(&protocol)?,
            alive: Box::new(|| true),
        })
    }
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        if !(self.alive)() {
            return Err(Error::Unavailable);
        }
        self.next_id += 1;
        let id = self.next_id;
        tokio::time::timeout(OPERATION_TIMEOUT, async {
            self.ws
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await
                .map_err(|_| Error::Unavailable)?;
            while let Some(message) = self.ws.next().await {
                let message = message.map_err(|_| Error::Unavailable)?;
                if let Message::Text(text) = message {
                    if text.len() > MAX_RESPONSE {
                        return Err(Error::Protocol);
                    }
                    let value: Value = serde_json::from_str(&text).map_err(|_| Error::Protocol)?;
                    if value["id"].as_u64() != Some(id) {
                        continue;
                    }
                    if let Some(error) = value.get("error") {
                        return Err(if error["code"].as_i64() == Some(-32601) {
                            Error::UnsupportedMethod
                        } else {
                            Error::Protocol
                        });
                    }
                    return value.get("result").cloned().ok_or(Error::Protocol);
                }
            }
            Err(Error::Unavailable)
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }
    pub(super) async fn capture(&mut self, scope: &Scope) -> Result<Snapshot, Error> {
        let mut result = self.call("Storage.getCookies", json!({})).await;
        if matches!(result, Err(Error::UnsupportedMethod)) {
            result = self.call("Network.getAllCookies", json!({})).await;
        }
        decode_cookies(&result?, scope)
    }
    pub(super) async fn apply(&mut self, scope: &Scope, patches: &[Patch]) -> Result<(), Error> {
        let before = self.capture(scope).await?;
        let mut desired = before.clone();
        for patch in patches {
            patch.cookie.validate()?;
            if !scope.allows(&patch.cookie) {
                return Err(Error::OutOfScope);
            }
            if patch.cookie.extensions.is_none() {
                let mut record = patch.cookie.clone();
                if patch.remove {
                    record.expires = Some("Thu, 01 Jan 1970 00:00:00 +0000".into());
                }
                let url = record.url(scope)?;
                super::update_http(&mut desired, scope, record, &url)?;
            } else {
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
        }
        self.replace_known(scope, &before, &desired).await
    }
    pub(super) async fn replace(&mut self, scope: &Scope, desired: &Snapshot) -> Result<(), Error> {
        desired.validate(scope)?;
        let before = self.capture(scope).await?;
        self.replace_known(scope, &before, desired).await
    }
    async fn replace_known(
        &mut self,
        scope: &Scope,
        before: &Snapshot,
        desired: &Snapshot,
    ) -> Result<(), Error> {
        desired.validate(scope)?;
        let writes: Vec<Value> = desired
            .cookies
            .iter()
            .filter(|c| !c.is_definitely_expired(time::OffsetDateTime::now_utc()))
            .map(|c| self.protocol.cookie(c, scope))
            .collect::<Result<_, _>>()?;
        let deletions: Vec<Value> = before
            .cookies
            .iter()
            .map(|c| self.protocol.deletion(c))
            .collect::<Result<_, _>>()?;
        if equivalent(before, desired) {
            return Ok(());
        }
        let mut unique = HashSet::new();
        let result = async {
            for deletion in deletions {
                if unique.insert(deletion.to_string()) {
                    self.call("Network.deleteCookies", deletion).await?;
                }
            }
            if !writes.is_empty() {
                self.call("Network.setCookies", json!({"cookies":writes}))
                    .await?;
            }
            Ok::<_, Error>(())
        }
        .await;
        // Verify uncertain writes rather than replaying them.
        let current = self
            .capture(scope)
            .await
            .map_err(|_| Error::WriteUncertain)?;
        if equivalent(&current, desired) {
            Ok(())
        } else {
            Err(result.err().unwrap_or(Error::WriteUncertain))
        }
    }
}
pub(super) fn select_http(
    snapshot: &mut Snapshot,
    scope: &Scope,
    headers: &[String],
) -> Result<(), Error> {
    if headers.len() != scope.urls.len() {
        return Err(Error::Protocol);
    }
    // Older getCookie implementations cannot expose partitioned cookies. Their
    // complete metadata comes from CDP, with the module's explicit top-level site.
    let mut selected: HashSet<_> = snapshot
        .cookies
        .iter()
        .filter(|c| {
            c.http_compatible(scope)
                && c.extensions
                    .as_ref()
                    .and_then(|e| e.partition_key.as_ref())
                    .is_some()
        })
        .map(CookieRecord::identity)
        .collect();
    for (url, header) in scope.urls.iter().zip(headers) {
        let mut values: HashMap<&str, Vec<&str>> = HashMap::new();
        for pair in header.split(';').filter(|s| !s.trim().is_empty()) {
            let (name, value) = pair.trim_start().split_once('=').ok_or(Error::Protocol)?;
            values.entry(name).or_default().push(value);
        }
        for (name, values) in values {
            let candidates: Vec<_> = snapshot
                .cookies
                .iter()
                .filter(|c| {
                    c.name == name
                        && c.http_compatible(scope)
                        && c.source_matches(url)
                        && url.host_str().is_some_and(|h| {
                            h == c.domain
                                || (!c.host_only && h.ends_with(&format!(".{}", c.domain)))
                        })
                        && (!c.secure.unwrap_or(false) || url.scheme() == "https")
                        && (url.path() == c.path
                            || (url.path().starts_with(&c.path)
                                && (c.path.ends_with('/')
                                    || url.path().as_bytes().get(c.path.len()) == Some(&b'/'))))
                })
                .collect();
            // Headers can contain parent-domain cookies excluded by this module.
            if candidates.is_empty() {
                continue;
            }
            let matched: Vec<_> = candidates
                .iter()
                .filter(|c| values.contains(&c.value.as_str()))
                .collect();
            if matched.is_empty()
                && candidates.iter().any(|c| {
                    c.extensions.as_ref().is_none_or(|e| {
                        e.source_port.is_none_or(|p| {
                            p == -1 || Some(p) == url.port_or_known_default().map(i32::from)
                        })
                    })
                })
            {
                return Err(Error::Unavailable);
            }
            selected.extend(matched.into_iter().map(|c| c.identity()));
        }
    }
    snapshot.http_keys = Some(
        snapshot
            .cookies
            .iter()
            .map(CookieRecord::identity)
            .filter(|k| selected.contains(k))
            .collect(),
    );
    Ok(())
}
pub(super) fn decode_cookies(result: &Value, scope: &Scope) -> Result<Snapshot, Error> {
    let list = result["cookies"].as_array().ok_or(Error::Protocol)?;
    let mut cookies = vec![];
    for item in list {
        let domain = item["domain"].as_str().ok_or(Error::Protocol)?;
        let mut cookie = CookieRecord {
            name: item["name"].as_str().ok_or(Error::Protocol)?.into(),
            value: String::new(),
            domain: domain.trim_start_matches('.').to_ascii_lowercase(),
            host_only: !domain.starts_with('.'),
            path: item["path"].as_str().ok_or(Error::Protocol)?.into(),
            expires: None,
            http_only: Some(item["httpOnly"].as_bool().ok_or(Error::Protocol)?),
            secure: Some(item["secure"].as_bool().ok_or(Error::Protocol)?),
            same_site: item["sameSite"].as_str().map(str::to_owned),
            partitioned: None,
            extensions: None,
        };
        // Unrelated cookies, including opaque partitions, never enter module data.
        if !scope.allows(&cookie) {
            continue;
        }
        if item["partitionKeyOpaque"].as_bool() == Some(true) {
            return Err(Error::UnsupportedPartition);
        }
        cookie.value = item["value"].as_str().ok_or(Error::Protocol)?.into();
        let partition_key = match item.get("partitionKey") {
            None | Some(Value::Null) => None,
            Some(Value::String(site)) => Some(PartitionKey {
                top_level_site: site.clone(),
                has_cross_site_ancestor: None,
            }),
            Some(Value::Object(key)) => Some(PartitionKey {
                top_level_site: key
                    .get("topLevelSite")
                    .and_then(Value::as_str)
                    .ok_or(Error::InvalidMetadata)?
                    .into(),
                has_cross_site_ancestor: key.get("hasCrossSiteAncestor").and_then(Value::as_bool),
            }),
            _ => return Err(Error::UnsupportedPartition),
        };
        cookie.partitioned = Some(partition_key.is_some());
        cookie.extensions = Some(Extensions {
            partition_key,
            source_scheme: item["sourceScheme"].as_str().map(str::to_owned),
            source_port: item["sourcePort"]
                .as_i64()
                .map(|v| i32::try_from(v).map_err(|_| Error::InvalidMetadata))
                .transpose()?,
            priority: item["priority"].as_str().map(str::to_owned),
            same_party: item["sameParty"].as_bool(),
        });
        if !item["session"].as_bool().ok_or(Error::Protocol)? {
            let expiry = item["expires"].as_f64().ok_or(Error::Protocol)?;
            if !expiry.is_finite() {
                return Err(Error::InvalidMetadata);
            }
            let date = time::OffsetDateTime::from_unix_timestamp(expiry.floor() as i64)
                .map_err(|_| Error::InvalidMetadata)?;
            cookie.expires = Some(
                date.format(&time::format_description::well_known::Rfc2822)
                    .map_err(|_| Error::InvalidMetadata)?,
            );
        }
        cookie.validate()?;
        if !cookie.is_definitely_expired(time::OffsetDateTime::now_utc()) {
            cookies.push(cookie);
        }
    }
    let mut identities = HashSet::new();
    if cookies.iter().any(|c| !identities.insert(c.identity())) {
        return Err(Error::InvalidMetadata);
    }
    Ok(Snapshot {
        cookies,
        ..Default::default()
    })
}

pub(super) async fn read_retry<T, F, Fut>(mut read: F) -> Result<T, Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    for attempt in 0..3 {
        match read().await {
            Err(Error::Unavailable) if attempt < 2 => {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            result => return result,
        }
    }
    Err(Error::Unavailable)
}

#[cfg(target_os = "android")]
async fn connect(socket: &str) -> Result<tokio::net::UnixStream, Error> {
    use std::os::android::net::SocketAddrExt;
    let socket = socket.to_owned();
    let stream = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking(move || {
            let address = std::os::unix::net::SocketAddr::from_abstract_name(socket.as_bytes())
                .map_err(|_| Error::Unavailable)?;
            let stream = std::os::unix::net::UnixStream::connect_addr(&address)
                .map_err(|_| Error::Unavailable)?;
            stream
                .set_nonblocking(true)
                .map_err(|_| Error::Unavailable)?;
            Ok::<_, Error>(stream)
        }),
    )
    .await
    .map_err(|_| Error::Unavailable)?
    .map_err(|_| Error::Unavailable)??;
    tokio::net::UnixStream::from_std(stream).map_err(|_| Error::Unavailable)
}
async fn http<S: AsyncRead + AsyncWrite + Unpin>(
    stream: &mut S,
    path: &str,
) -> Result<Value, Error> {
    use tokio::io::AsyncWriteExt;
    tokio::time::timeout(OPERATION_TIMEOUT, async {
        stream
            .write_all(
                format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .map_err(|_| Error::Unavailable)?;
        read_http_response(stream).await
    })
    .await
    .map_err(|_| Error::Unavailable)?
}
// DevTools' HTTP server keeps sockets open even when Connection: close was
// requested. Finish at the HTTP message boundary, never wait for socket EOF.
async fn read_http_response<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Value, Error> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    loop {
        if let Some(body) = http_body(&bytes, false)? {
            return decode_body(&body);
        }
        let mut chunk = [0u8; 8192];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| Error::Unavailable)?;
        if read == 0 {
            return decode_http(&bytes);
        }
        if bytes.len() + read > MAX_RESPONSE {
            return Err(Error::Protocol);
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
}
fn http_body(bytes: &[u8], eof: bool) -> Result<Option<Vec<u8>>, Error> {
    if bytes.len() > MAX_RESPONSE {
        return Err(Error::Protocol);
    }
    let Some(i) = bytes.windows(4).position(|s| s == b"\r\n\r\n") else {
        return if eof || bytes.len() > 65536 {
            Err(Error::Protocol)
        } else {
            Ok(None)
        };
    };
    let header = std::str::from_utf8(&bytes[..i]).map_err(|_| Error::Protocol)?;
    if !header
        .lines()
        .next()
        .is_some_and(|s| s.split_whitespace().nth(1) == Some("200"))
    {
        return Err(Error::Unavailable);
    }
    let mut headers = reqwest::header::HeaderMap::new();
    for line in header.lines().skip(1) {
        let (name, value) = line.split_once(':').ok_or(Error::Protocol)?;
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| Error::Protocol)?;
        if headers.contains_key(&name) {
            return Err(Error::Protocol);
        }
        headers.insert(
            name,
            reqwest::header::HeaderValue::from_str(value.trim()).map_err(|_| Error::Protocol)?,
        );
    }
    let body = &bytes[i + 4..];
    let complete = if let Some(encoding) = headers.get(reqwest::header::TRANSFER_ENCODING) {
        if encoding != "chunked" || headers.contains_key(reqwest::header::CONTENT_LENGTH) {
            return Err(Error::Protocol);
        }
        chunked_body(body)?
    } else if let Some(length) = headers.get(reqwest::header::CONTENT_LENGTH) {
        let length: usize = length
            .to_str()
            .map_err(|_| Error::Protocol)?
            .parse()
            .map_err(|_| Error::Protocol)?;
        if length > MAX_RESPONSE - i - 4 {
            return Err(Error::Protocol);
        }
        if body.len() < length {
            None
        } else {
            Some(body[..length].to_vec())
        }
    } else if eof {
        Some(body.to_vec())
    } else {
        None
    };
    if eof && complete.is_none() {
        return Err(Error::Protocol);
    }
    let Some(body) = complete else {
        return Ok(None);
    };
    match headers
        .get(reqwest::header::CONTENT_ENCODING)
        .map(|h| h.to_str())
    {
        None | Some(Ok("identity")) => Ok(Some(body)),
        Some(Ok("gzip")) => {
            use std::io::Read;
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(body.as_slice())
                .take((MAX_RESPONSE + 1) as u64)
                .read_to_end(&mut decoded)
                .map_err(|_| Error::Protocol)?;
            if decoded.len() > MAX_RESPONSE {
                return Err(Error::Protocol);
            }
            Ok(Some(decoded))
        }
        _ => Err(Error::Protocol),
    }
}
fn chunked_body(body: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let mut result = Vec::new();
    let mut position = 0;
    loop {
        let Some(end) = body[position..].windows(2).position(|s| s == b"\r\n") else {
            return Ok(None);
        };
        let text =
            std::str::from_utf8(&body[position..position + end]).map_err(|_| Error::Protocol)?;
        let length = usize::from_str_radix(text.split(';').next().unwrap_or(""), 16)
            .map_err(|_| Error::Protocol)?;
        position += end + 2;
        if length == 0 {
            return if body[position..].starts_with(b"\r\n")
                || body[position..].windows(4).any(|s| s == b"\r\n\r\n")
            {
                Ok(Some(result))
            } else {
                Ok(None)
            };
        }
        if length > MAX_RESPONSE - result.len() {
            return Err(Error::Protocol);
        }
        let end = position
            .checked_add(length)
            .and_then(|p| p.checked_add(2))
            .ok_or(Error::Protocol)?;
        if end > body.len() {
            return Ok(None);
        }
        if &body[end - 2..end] != b"\r\n" {
            return Err(Error::Protocol);
        }
        result.extend_from_slice(&body[position..end - 2]);
        position = end;
    }
}
fn decode_body(body: &[u8]) -> Result<Value, Error> {
    serde_json::from_slice(body).map_err(|_| Error::Protocol)
}
fn decode_http(bytes: &[u8]) -> Result<Value, Error> {
    decode_body(&http_body(bytes, true)?.ok_or(Error::Protocol)?)
}
#[cfg(target_os = "android")]
pub(super) async fn open(
    socket: &str,
    window: &tauri::WebviewWindow,
) -> Result<Session<tokio::net::UnixStream>, Error> {
    if socket != format!("webview_devtools_remote_{}", std::process::id()) {
        return Err(Error::OutOfScope);
    }
    let target = window.clone();
    open_with(
        || connect(socket),
        Box::new(move || super::android::alive(&target)),
    )
    .await
}
async fn open_with<S, F, Fut>(
    mut connect: F,
    alive: Box<dyn Fn() -> bool + Send>,
) -> Result<Session<S>, Error>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<S, Error>>,
{
    let targets = http(&mut connect().await?, "/json/list")
        .await
        .inspect_err(|e| {
            log::warn!("WebView Cookie CDP failure: stage=discovery, category={e:?}");
        })?;

    let path = targets
        .as_array()
        .and_then(|a| {
            a.iter()
                .filter(|t| t["type"] == "page")
                .find_map(|t| t["webSocketDebuggerUrl"].as_str())
        })
        .ok_or(Error::Unavailable)?;
    let url = tauri::Url::parse(path).map_err(|_| Error::Protocol)?;
    if url.scheme() != "ws"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || !url.path().starts_with("/devtools/page/")
    {
        return Err(Error::OutOfScope);
    }
    let protocol = http(&mut connect().await?, "/json/protocol")
        .await
        .inspect_err(|e| {
            log::warn!("WebView Cookie CDP failure: stage=protocol-discovery, category={e:?}");
        })?;
    let stream = connect().await?;
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_RESPONSE))
        .max_frame_size(Some(MAX_RESPONSE));
    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(2),
        tokio_tungstenite::client_async_with_config(url.as_str(), stream, Some(config)),
    )
    .await
    .map_err(|_| Error::Unavailable)?
    .map_err(|_| Error::Unavailable)?;
    Ok(Session {
        ws,
        next_id: 0,
        protocol: Protocol::parse(&protocol)?,
        alive,
    })
}

#[cfg(test)]
mod tests;
