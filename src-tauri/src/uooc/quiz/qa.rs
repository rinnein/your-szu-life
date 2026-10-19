use super::model::{Answers, Kind, OptionItem, Question, Review};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(?:[a-zA-Z/][a-zA-Z0-9:-]*\b[^>]*|!--[\s\S]*?--)>").unwrap()
});

/// 去除字符串中的所有 HTML 标签并解析常见实体字符，保证输出纯文本
pub(crate) fn strip_html_tags(raw: &str) -> String {
    let stripped = HTML_TAG_RE.replace_all(raw, " ");
    let unescaped = stripped
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    unescaped.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) const EMBEDDING_DIMENSIONS: usize = 512;

fn deserialize_flex_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct FlexStringVisitor;
    impl<'de> serde::de::Visitor<'de> for FlexStringVisitor {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string, number, or boolean")
        }
        fn visit_str<E>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_string<E>(self, v: String) -> Result<String, E> {
            Ok(v)
        }
        fn visit_i64<E>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_u64<E>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_f64<E>(self, v: f64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_bool<E>(self, v: bool) -> Result<String, E> {
            Ok(v.to_string())
        }
    }
    deserializer.deserialize_any(FlexStringVisitor)
}

fn deserialize_opt_flex_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(val.map(|v| match v {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        other => other.to_string(),
    }))
}

fn deserialize_flex_values<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw_list = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let string_list = raw_list
        .into_iter()
        .map(|v| match v {
            serde_json::Value::String(s) => s,
            serde_json::Value::Bool(b) => {
                if b {
                    "A".to_string()
                } else {
                    "B".to_string()
                }
            }
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        })
        .collect();
    Ok(string_list)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaOption {
    #[serde(default, deserialize_with = "deserialize_flex_string")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_flex_string")]
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaAnswer {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_flex_values")]
    pub values: Vec<String>,
    #[serde(default, rename = "isCorrect")]
    pub is_correct: bool,
    #[serde(default)]
    pub confidence: Option<f64>,
}

impl QaAnswer {
    #[allow(dead_code)]
    pub fn string_values(&self) -> Vec<String> {
        self.values.clone()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaQuestion {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub tid: Option<String>,
    #[serde(default, rename = "questionId", deserialize_with = "deserialize_opt_flex_string")]
    pub question_id: Option<String>,
    #[serde(default, rename = "type")]
    pub question_type: Option<String>,
    #[serde(default)]
    pub stem: Option<String>,
    #[serde(default, rename = "stemHash")]
    pub stem_hash: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<QaOption>>,
    #[serde(default)]
    pub answers: Vec<QaAnswer>,
    #[serde(default, rename = "stemEmbedding")]
    pub stem_embedding: Option<Vec<f32>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaTaskGroup {
    pub tid: String,
    #[serde(default)]
    pub cid: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct QaTaskResponse {
    #[serde(default, rename = "taskGroup")]
    pub task_group: Option<QaTaskGroup>,
    #[serde(default)]
    pub questions: Vec<QaQuestion>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct QaUploadOption {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct QaUploadQuestion {
    pub question_id: String,
    #[serde(rename = "type")]
    pub question_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QaUploadOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blanks: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correct_answers: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrong_answers: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct QaBatchUploadPayload {
    pub tid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course_name: Option<String>,
    pub questions: Vec<QaUploadQuestion>,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct QaUploadActionResult {
    pub question_id: String,
    pub action: String,
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct QaBatchUploadResponse {
    pub ok: bool,
    #[serde(default)]
    pub results: Vec<QaUploadActionResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalBankAnswer {
    #[serde(default)]
    pub id: Option<i64>,
    pub values: Vec<serde_json::Value>,
    #[serde(rename = "isCorrect")]
    pub is_correct: bool,
    #[serde(default = "default_answer_source")]
    pub source: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_answer_source() -> String {
    "local".into()
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalBankQuestion {
    pub cid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_name: Option<String>,
    pub tid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    pub question_id: String,
    #[serde(rename = "type")]
    pub question_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<QaUploadOption>>,
    #[serde(default)]
    pub blanks: usize,
    #[serde(default)]
    pub answers: Vec<LocalBankAnswer>,
    #[serde(default)]
    pub correct_answers: Vec<String>,
    pub uploaded: bool,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BankMatchItem {
    pub tid: String,
    pub question_id: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BankMatchPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<BankMatchItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_question_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct MissingOnServerItem {
    pub cid: Option<String>,
    pub tid: Option<String>,
    #[serde(alias = "question_id")]
    pub question_id: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub(crate) struct BankMatchResponse {
    pub ok: bool,
    #[serde(default, alias = "missing_on_server")]
    pub missing_on_server: Vec<MissingOnServerItem>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CachedVectorQuestion {
    pub id: Option<i64>,
    pub tid: String,
    pub cid: Option<String>,
    pub question_id: String,
    pub question_type: String,
    pub stem: String,
    pub embedding: Vec<f32>,
    pub options: Option<Vec<QaUploadOption>>,
    pub answers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EmbeddingModelStatus {
    pub status: String,
    pub cached_count: usize,
    pub total_dimension: usize,
    pub updated_at_ms: i64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaSearchResultItem {
    pub question_id: String,
    pub tid: Option<String>,
    pub cid: Option<String>,
    pub stem: String,
    pub question_type: String,
    pub similarity: f64,
    pub answers: Vec<String>,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaAccountStatus {
    pub api_key: Option<String>,
    pub is_anonymous: bool,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub permission_level: String,
    pub can_overwrite: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaDeviceCodeResponse {
    #[serde(alias = "device_code")]
    pub device_code: String,
    #[serde(alias = "user_code")]
    pub user_code: String,
    #[serde(alias = "verification_uri")]
    pub verification_uri: String,
    #[serde(alias = "verification_uri_complete")]
    pub verification_uri_complete: String,
    #[serde(alias = "expires_in")]
    pub expires_in: u64,
    #[serde(alias = "interval")]
    pub interval: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaDeviceTokenPollResult {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaHttpRequest {
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct QaHttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

pub(crate) fn http_client_for_url(url: &str) -> Result<reqwest::Client, String> {
    #[cfg(target_os = "android")]
    crate::platform_tls::initialize_platform_verifier()?;

    let mut builder = reqwest::Client::builder().timeout(Duration::from_secs(15));
    if let Ok(parsed) = reqwest::Url::parse(url) {
        if let Some(host) = parsed.host_str() {
            let h = host.to_lowercase();
            if h == "localhost"
                || h == "127.0.0.1"
                || h == "::1"
                || h == "0.0.0.0"
                || h.starts_with("192.168.")
                || h.starts_with("10.")
                || h.starts_with("172.")
                || h.ends_with(".local")
            {
                builder = builder.no_proxy();
            }
        }
    } else {
        builder = builder.no_proxy();
    }

    builder.build().map_err(|e| format!("无法构建 HTTP 客户端: {e}"))
}

pub(crate) fn http_client() -> Result<reqwest::Client, String> {
    http_client_for_url("http://localhost")
}

pub(crate) async fn execute_qa_http_request(
    request: QaHttpRequest,
) -> Result<QaHttpResponse, String> {
    let client = http_client_for_url(&request.url)?;
    let method = match request.method.to_uppercase().as_str() {
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        _ => reqwest::Method::GET,
    };

    let mut req = client.request(method, &request.url);
    if let Some(headers) = request.headers {
        for (k, v) in headers {
            if let (Ok(name), Ok(val)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(&v),
            ) {
                req = req.header(name, val);
            }
        }
    }

    if let Some(body) = request.body {
        req = req.body(body);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("题库网络请求失败 ({}): {e}", request.url))?;

    let status = resp.status().as_u16();
    let status_text = resp.status().canonical_reason().unwrap_or("").to_string();
    let mut resp_headers = BTreeMap::new();
    for (k, v) in resp.headers() {
        if let Ok(v_str) = v.to_str() {
            resp_headers.insert(k.as_str().to_string(), v_str.to_string());
        }
    }
    let body = resp.text().await.unwrap_or_default();

    Ok(QaHttpResponse {
        status,
        status_text,
        headers: resp_headers,
        body,
    })
}

pub(crate) async fn get_qa_account_status(
    server_url: &str,
    api_key: Option<&str>,
) -> Result<QaAccountStatus, String> {
    let Some(key) = api_key.filter(|k| !k.trim().is_empty()) else {
        return Ok(QaAccountStatus {
            api_key: None,
            is_anonymous: true,
            user_id: None,
            user_name: None,
            permission_level: "unregistered".into(),
            can_overwrite: false,
        });
    };

    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/automation/tasks?limit=1");
    let client = http_client()?;
    let resp = client.get(&url).header("x-api-key", key.trim()).send().await;

    match resp {
        Ok(res) if res.status().is_success() => {
            let data: serde_json::Value = res.json().await.unwrap_or_default();
            let account = &data["account"];
            let level = account["permissionLevel"].as_str().unwrap_or("anonymous");
            let is_anon = account["isAnonymous"].as_bool().unwrap_or(true);
            let can_overwrite = level == "authenticated" || level == "admin";

            Ok(QaAccountStatus {
                api_key: Some(key.to_string()),
                is_anonymous: is_anon,
                user_id: account["userId"].as_str().map(str::to_string),
                user_name: account["userName"].as_str().map(str::to_string),
                permission_level: level.to_string(),
                can_overwrite,
            })
        }
        _ => Ok(QaAccountStatus {
            api_key: Some(key.to_string()),
            is_anonymous: true,
            user_id: None,
            user_name: None,
            permission_level: "unknown".into(),
            can_overwrite: false,
        }),
    }
}

pub(crate) fn resolve_auth_base(server_url: &str) -> String {
    let base = server_url.trim().trim_end_matches('/');
    if base.ends_with("/api/auth") {
        base.to_string()
    } else if base.ends_with("/api") {
        format!("{base}/auth")
    } else {
        format!("{base}/api/auth")
    }
}

pub(crate) fn resolve_api_base(server_url: &str) -> String {
    let base = server_url.trim().trim_end_matches('/');
    if base.ends_with("/api/auth") {
        base.trim_end_matches("/auth").to_string()
    } else if base.ends_with("/api") {
        base.to_string()
    } else {
        format!("{base}/api")
    }
}

pub(crate) async fn request_device_code(server_url: &str) -> Result<QaDeviceCodeResponse, String> {
    let auth_base = resolve_auth_base(server_url);
    let url = format!("{auth_base}/device/code");
    let client = http_client()?;
    let payload = serde_json::json!({
        "client_id": "your-szu-life",
        "scope": "read write",
    });
    let resp = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("请求设备码失败（无法连接到服务 {url}）: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        let detail = if err_text.trim().is_empty() {
            format!("HTTP {status}")
        } else if let Ok(json) = serde_json::from_str::<serde_json::Value>(&err_text) {
            let desc = json["error_description"]
                .as_str()
                .or_else(|| json["message"].as_str())
                .or_else(|| json["error"].as_str())
                .unwrap_or(&err_text);
            format!("HTTP {status}: {desc}")
        } else {
            format!("HTTP {status}: {err_text}")
        };
        return Err(format!("申请设备码服务端返回错误 ({detail})，请求地址: {url}"));
    }

    let mut code_resp = resp
        .json::<QaDeviceCodeResponse>()
        .await
        .map_err(|e| format!("解析设备码响应失败: {e}"))?;

    let root_origin = {
        if let Ok(parsed) = reqwest::Url::parse(server_url) {
            let port_part = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
            format!("{}://{}{port_part}", parsed.scheme(), parsed.host_str().unwrap_or("localhost"))
        } else {
            server_url.trim().trim_end_matches('/').to_string()
        }
    };

    if code_resp.verification_uri.starts_with('/') {
        code_resp.verification_uri = format!("{root_origin}{}", code_resp.verification_uri);
    }
    if code_resp.verification_uri_complete.starts_with('/') {
        code_resp.verification_uri_complete = format!("{root_origin}{}", code_resp.verification_uri_complete);
    }

    Ok(code_resp)
}

pub(crate) async fn poll_device_token(
    server_url: &str,
    device_code: &str,
) -> Result<QaDeviceTokenPollResult, String> {
    let auth_base = resolve_auth_base(server_url);
    let token_url = format!("{auth_base}/device/token");
    let client = http_client()?;
    let payload = serde_json::json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "device_code": device_code,
        "client_id": "your-szu-life",
    });
    let resp = client
        .post(&token_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("轮询设备授权状态失败: {e}"))?;

    let status = resp.status();
    if status == reqwest::StatusCode::BAD_REQUEST {
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let error_code = body["error"].as_str().unwrap_or("unknown_error");
        if error_code == "authorization_pending" || error_code == "slow_down" {
            return Ok(QaDeviceTokenPollResult {
                status: "pending".into(),
                api_key: None,
                error: None,
            });
        }
        let desc = body["error_description"].as_str().unwrap_or(error_code);
        return Ok(QaDeviceTokenPollResult {
            status: "error".into(),
            api_key: None,
            error: Some(format!("授权未完成或已失效: {desc}")),
        });
    }

    if !status.is_success() {
        let err_text = resp.text().await.unwrap_or_default();
        return Ok(QaDeviceTokenPollResult {
            status: "error".into(),
            api_key: None,
            error: Some(format!("轮询返回异常状态 HTTP {status}: {err_text}")),
        });
    }

    let token_data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 Token 响应失败: {e}"))?;

    if let Some(error_code) = token_data["error"].as_str() {
        if error_code == "authorization_pending" || error_code == "slow_down" {
            return Ok(QaDeviceTokenPollResult {
                status: "pending".into(),
                api_key: None,
                error: None,
            });
        }
        let desc = token_data["error_description"].as_str().unwrap_or(error_code);
        return Ok(QaDeviceTokenPollResult {
            status: "error".into(),
            api_key: None,
            error: Some(format!("授权失败: {desc}")),
        });
    }

    let access_token = token_data["access_token"]
        .as_str()
        .ok_or_else(|| "响应中缺少 access_token".to_string())?;

    // Request API Key creation with the access_token
    let api_key_url = format!("{auth_base}/api-key/create");
    let create_payload = serde_json::json!({
        "name": "your-szu-life",
        "prefix": "uooc_",
        "expiresIn": 31536000,
    });

    let key_resp = client
        .post(&api_key_url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {access_token}"))
        .json(&create_payload)
        .send()
        .await
        .map_err(|e| format!("获取 API Key 失败: {e}"))?;

    let key_status = key_resp.status();
    if !key_status.is_success() {
        let err_text = key_resp.text().await.unwrap_or_default();
        return Err(format!("通过设备凭据创建 API Key 失败 (HTTP {key_status}): {err_text}"));
    }

    let key_data: serde_json::Value = key_resp
        .json()
        .await
        .map_err(|e| format!("解析 API Key 响应失败: {e}"))?;

    let key = key_data["key"]
        .as_str()
        .or_else(|| key_data["apiKey"].as_str())
        .ok_or_else(|| "创建响应中缺少 key 字段".to_string())?;

    Ok(QaDeviceTokenPollResult {
        status: "success".into(),
        api_key: Some(key.to_string()),
        error: None,
    })
}

/// Normalize stem according to uooc-qa-share specification:
/// strip HTML tags, strip HTML entities, collapse whitespace, trim, lowercase.
pub(crate) fn normalize_stem(raw: &str) -> String {
    let mut without_html = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            without_html.push(ch);
        }
    }

    let mut without_entities = String::with_capacity(without_html.len());
    let mut chars = without_html.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '&' {
            let mut entity = String::new();
            let mut found_semicolon = false;
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ';' {
                    chars.next();
                    found_semicolon = true;
                    break;
                } else if next_ch.is_alphabetic() && entity.len() < 10 {
                    entity.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            if found_semicolon && !entity.is_empty() {
                without_entities.push(' ');
            } else {
                without_entities.push('&');
                without_entities.push_str(&entity);
            }
        } else {
            without_entities.push(ch);
        }
    }

    without_entities
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Compute SHA-256 hash of normalized stem.
pub(crate) fn hash_stem(raw: &str) -> String {
    let normalized = normalize_stem(raw);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compute a deterministic 512-dimensional L2-normalized semantic feature vector.
pub(crate) fn compute_local_embedding(text: &str) -> Vec<f32> {
    let normalized = normalize_stem(text);
    let mut vector = vec![0.0_f32; EMBEDDING_DIMENSIONS];
    if normalized.is_empty() {
        vector[0] = 1.0;
        return vector;
    }

    let chars: Vec<char> = normalized.chars().collect();
    let n = chars.len();

    // 1-gram, 2-gram, 3-gram feature projection
    for gram_len in 1..=3 {
        if n < gram_len {
            continue;
        }
        for i in 0..=(n - gram_len) {
            let sub: String = chars[i..i + gram_len].iter().collect();
            let mut hasher = Sha256::new();
            hasher.update(sub.as_bytes());
            let hash_bytes = hasher.finalize();

            // Project each n-gram into 4 independent positions with alternating weights
            for seed in 0..4 {
                let offset = seed * 8;
                let chunk = &hash_bytes[offset..offset + 4];
                let idx = u32::from_le_bytes(chunk.try_into().unwrap()) as usize % EMBEDDING_DIMENSIONS;
                let sign = if (hash_bytes[offset + 4] & 1) == 0 { 1.0_f32 } else { -1.0_f32 };
                let weight = match gram_len {
                    1 => 1.0_f32,
                    2 => 1.5_f32,
                    _ => 2.0_f32,
                };
                vector[idx] += sign * weight;
            }
        }
    }

    // L2 normalize vector
    let norm_sq: f32 = vector.iter().map(|v| v * v).sum();
    if norm_sq > 0.0 {
        let norm = norm_sq.sqrt();
        for val in &mut vector {
            *val /= norm;
        }
    } else {
        vector[0] = 1.0;
    }

    vector
}

/// Cosine similarity between two unit vectors (dot product).
pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    (dot as f64).clamp(-1.0, 1.0)
}

/// Automatically acquire or verify an API key via /api/auth/anonymous-key.
pub(crate) async fn ensure_api_key(server_url: &str, stored_key: Option<&str>) -> Option<String> {
    if let Some(key) = stored_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let auth_base = resolve_auth_base(server_url);
    let url = format!("{auth_base}/anonymous-key");
    let client = http_client().ok()?;
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let data = resp.json::<serde_json::Value>().await.ok()?;
    data.get("apiKey")
        .or_else(|| data.get("key"))
        .or_else(|| data.get("token"))
        .or_else(|| data.get("data").and_then(|d| d.get("apiKey").or_else(|| d.get("key"))))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Query QA platform for questions and known answers of a task group.
pub(crate) async fn query_task_answers(
    server_url: &str,
    api_key: Option<&str>,
    tid: &str,
    cid: Option<&str>,
) -> Result<Vec<QaQuestion>, String> {
    let api_base = resolve_api_base(server_url);
    let client = http_client()?;

    // 1. 如果指定了 cid，优先请求带课程作用域的接口 /api/courses/{cid}/tasks/{tid}
    if let Some(c) = cid.filter(|s| !s.trim().is_empty()) {
        let course_url = format!("{api_base}/courses/{c}/tasks/{tid}");
        let mut req = client.get(&course_url);
        if let Some(key) = api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                req = req.header("x-api-key", trimmed);
            }
        }
        if let Ok(resp) = req.send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(data) = serde_json::from_slice::<QaTaskResponse>(&bytes) {
                        if data.error.is_none() && !data.questions.is_empty() {
                            return Ok(data.questions);
                        }
                    }
                }
            }
        }
    }

    // 2. 回退请求通用题单接口 /api/tasks/{tid}（跨课程、全题库收录的标准入口）
    let task_url = format!("{api_base}/tasks/{tid}");
    let mut req = client.get(&task_url);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求题库共享平台失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("题库请求失败，状态码: {}", resp.status()));
    }

    let resp_bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取题库响应内容失败: {e}"))?;
    let resp_text = String::from_utf8_lossy(&resp_bytes);

    let data: QaTaskResponse = match serde_json::from_slice(&resp_bytes) {
        Ok(d) => d,
        Err(e) => {
            return Err(format!("解析题库数据失败: {e}，响应原内容: {resp_text}"));
        }
    };

    // 如果服务端返回业务上的未找到，表示服务端题库暂未收录该题单，连接本身是成功的
    if let Some(ref err) = data.error {
        if err.contains("not found") {
            return Ok(vec![]);
        }
    }

    Ok(data.questions)
}

/// Upload question results in batch to /api/upload/batch.
pub(crate) async fn upload_batch(
    server_url: &str,
    api_key: Option<&str>,
    payload: &QaBatchUploadPayload,
    service: Option<&crate::uooc::Service>,
) -> Result<QaBatchUploadResponse, String> {
    let api_base = resolve_api_base(server_url);
    let url = format!("{api_base}/upload/batch");

    let payload_json = serde_json::to_string_pretty(payload)
        .or_else(|_| serde_json::to_string(payload))
        .unwrap_or_default();
    if let Some(svc) = service {
        svc.log_debug(
            "bank",
            format!(
                "[uooc-qa-share POST] 发送题库批量上传请求: URL={url}, tid={}, cid={:?}, task_name={:?}, 包含题目数={}\n请求体详情 (Payload):\n{payload_json}",
                payload.tid,
                payload.cid,
                payload.task_name,
                payload.questions.len(),
            ),
        );
    }

    let client = http_client()?;
    let mut req = client.post(&url).json(payload);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let err_msg = format!("上报题库数据失败: {e}");
            if let Some(svc) = service {
                svc.log_debug(
                    "bank",
                    format!("[uooc-qa-share POST] 请求发生错误: URL={url}, 错误={err_msg}"),
                );
            }
            return Err(err_msg);
        }
    };

    let status = resp.status();
    let resp_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            let err_msg = format!("读取上报题库响应内容失败: {e}");
            if let Some(svc) = service {
                svc.log_debug(
                    "bank",
                    format!("[uooc-qa-share POST] 读取响应失败: URL={url}, HTTP状态码={status}, 错误={err_msg}"),
                );
            }
            return Err(err_msg);
        }
    };

    let resp_text = String::from_utf8_lossy(&resp_bytes);
    if let Some(svc) = service {
        svc.log_debug(
            "bank",
            format!(
                "[uooc-qa-share POST] 收到响应: URL={url}, HTTP状态码={}, 响应体:\n{resp_text}",
                status.as_u16(),
            ),
        );
    }

    if !status.is_success() {
        return Err(format!("上报题库数据被拒绝，状态码: {}, 响应: {}", status, resp_text));
    }

    let data = serde_json::from_slice::<QaBatchUploadResponse>(&resp_bytes)
        .map_err(|e| format!("解析上报结果失败: {e}，响应原内容: {resp_text}"))?;
    Ok(data)
}

#[derive(Default)]
pub(crate) struct MatchResult {
    pub matched_answers: Answers,
    pub wrong_options: BTreeMap<String, Vec<String>>,
    pub reviews: Vec<Review>,
}

/// Match local paper questions with questions from the QA platform.
pub(crate) fn match_answers(
    qa_questions: &[QaQuestion],
    local_questions: &[Question],
) -> MatchResult {
    let mut result = MatchResult::default();

    for local_q in local_questions {
        let local_hash = hash_stem(&local_q.text);
        let local_norm = normalize_stem(&local_q.text);

        // Find best match in qa_questions:
        // 1. Direct question_id equality (or substring)
        // 2. stemHash equality
        // 3. normalized stem equality
        let matched = qa_questions.iter().find(|qa| {
            if let Some(ref qid) = qa.question_id {
                if qid == &local_q.id
                    || local_q.id.contains(qid.as_str())
                    || qid.contains(local_q.id.as_str())
                {
                    return true;
                }
            }
            if let Some(ref sh) = qa.stem_hash {
                if sh == &local_hash {
                    return true;
                }
            }
            if let Some(ref stem) = qa.stem {
                if normalize_stem(stem) == local_norm {
                    return true;
                }
            }
            false
        });

        let Some(qa) = matched else {
            continue;
        };

        // Extract correct answers
        let correct_ans = qa.answers.iter().find(|a| a.is_correct && !a.values.is_empty());
        let qa_opts = qa.options.as_deref();
        if let Some(correct) = correct_ans {
            let mapped_values = map_option_values(&correct.values, &local_q.options, qa_opts);
            if !mapped_values.is_empty() || local_q.kind == Kind::Text {
                let ans = if local_q.kind == Kind::Text {
                    correct.values.clone()
                } else {
                    mapped_values
                };
                result.matched_answers.insert(local_q.id.clone(), ans.clone());
                result.reviews.push(Review {
                    question_id: local_q.id.clone(),
                    answers: ans,
                    probabilities: Default::default(),
                    message: "题库命中标准答案".into(),
                });
            }
        }

        // Extract wrong answers (pruning candidates)
        let wrongs: Vec<String> = qa
            .answers
            .iter()
            .filter(|a| !a.is_correct)
            .flat_map(|a| map_option_values(&a.values, &local_q.options, qa_opts))
            .collect();
        if !wrongs.is_empty() {
            let mut unique_wrongs = wrongs.clone();
            unique_wrongs.sort();
            unique_wrongs.dedup();
            result
                .wrong_options
                .insert(local_q.id.clone(), unique_wrongs.clone());

            // 逻辑排除推导：
            // 1. 若为判断题（2个选项），确认到 1 个错误答案，自动确定另外一个为唯一正确答案；
            // 2. 若为单选题（N个选项），确认到 N - 1 个错误选项，自动确定剩下的唯一个为唯一正确答案！
            let total_opts = local_q.options.len();
            let is_judge = local_q.kind == Kind::Single && total_opts == 2 && {
                let opt0 = local_q.options[0].text.trim().to_lowercase();
                let opt1 = local_q.options[1].text.trim().to_lowercase();
                (opt0.contains("正确") || opt0.contains("对") || opt0 == "t" || opt0 == "true" || opt0 == "a")
                    && (opt1.contains("错误") || opt1.contains("错") || opt1 == "f" || opt1 == "false" || opt1 == "b")
            };
            let is_single = local_q.kind == Kind::Single;

            let auto_deduced = if !result.matched_answers.contains_key(&local_q.id) {
                if (is_judge && !unique_wrongs.is_empty()) || (is_single && total_opts >= 2 && unique_wrongs.len() >= total_opts - 1) {
                    local_q.options.iter().find(|o| !unique_wrongs.contains(&o.id))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(deduced_opt) = auto_deduced {
                let ans = vec![deduced_opt.id.clone()];
                result.matched_answers.insert(local_q.id.clone(), ans.clone());
                result.reviews.push(Review {
                    question_id: local_q.id.clone(),
                    answers: ans,
                    probabilities: Default::default(),
                    message: if is_judge {
                        "题库已排除错误选项，自动推导出唯一正确答案".into()
                    } else {
                        format!("题库已排除 {} 个错误选项，自动推导出唯一正确答案", unique_wrongs.len())
                    },
                });
            } else if !result.matched_answers.contains_key(&local_q.id) {
                let label_text = unique_wrongs
                    .iter()
                    .map(|id| {
                        local_q
                            .options
                            .iter()
                            .find(|o| o.id == *id)
                            .map(|o| o.text.as_str())
                            .unwrap_or(id.as_str())
                    })
                    .collect::<Vec<_>>()
                    .join("、");
                result.reviews.push(Review {
                    question_id: local_q.id.clone(),
                    answers: unique_wrongs.clone(),
                    probabilities: Default::default(),
                    message: format!("题库已排除错误选项：{label_text}，请勿选择"),
                });
            }
        }
    }

    result
}

pub(crate) fn map_answers_to_option_ids(
    answers: &[String],
    options: &[OptionItem],
) -> Vec<String> {
    let mut mapped = Vec::new();
    for ans in answers {
        let ans_trim = ans.trim();
        if ans_trim.is_empty() {
            continue;
        }
        if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(ans_trim) || o.text.trim() == ans_trim) {
            if !mapped.contains(&opt.id) {
                mapped.push(opt.id.clone());
            }
        } else {
            mapped.push(ans_trim.to_string());
        }
    }
    mapped
}

fn map_option_values(
    values: &[String],
    local_options: &[OptionItem],
    qa_options: Option<&[QaOption]>,
) -> Vec<String> {
    if local_options.is_empty() {
        return values.to_vec();
    }
    let mut mapped = Vec::new();
    for v in values {
        let v_trim = v.trim();
        if v_trim.is_empty() {
            continue;
        }

        // 1. 优先直接文本比对（防选项乱序核心机制）
        let norm_v = normalize_stem(v_trim);
        let mut matched_opt = local_options.iter().find(|o| {
            let norm_o = normalize_stem(&o.text);
            if norm_o == norm_v || o.text.trim() == v_trim {
                return true;
            }
            if norm_v.len() >= 4 && (norm_o.contains(&norm_v) || norm_v.contains(&norm_o)) {
                return true;
            }
            false
        });

        // 2. 若 v 未直接命中本地选项文本，且题库提供了旧选项列表：
        // 在题库旧选项中查找 id 等于 v 的项，取出其文本再与本地选项做文本比对
        if matched_opt.is_none() {
            if let Some(qa_opts) = qa_options {
                if let Some(old_qa_opt) = qa_opts.iter().find(|qo| qo.id.eq_ignore_ascii_case(v_trim)) {
                    let norm_old = normalize_stem(&old_qa_opt.text);
                    matched_opt = local_options.iter().find(|o| {
                        let norm_o = normalize_stem(&o.text);
                        norm_o == norm_old || o.text.trim() == old_qa_opt.text.trim()
                    });
                }
            }
        }

        // 3. 命中选项文本，取出当前选项在当前试卷中的真实 ID
        if let Some(opt) = matched_opt {
            if !mapped.contains(&opt.id) {
                mapped.push(opt.id.clone());
            }
            continue;
        }

        // 4. 回退匹配：匹配选项精确 ID
        if let Some(opt) = local_options.iter().find(|o| o.id == v_trim || o.id.eq_ignore_ascii_case(v_trim)) {
            if !mapped.contains(&opt.id) {
                mapped.push(opt.id.clone());
            }
            continue;
        }

        // 5. 回退匹配：按字母下标（A -> 0, B -> 1 等）
        if v_trim.len() == 1 {
            let ch = v_trim.chars().next().unwrap().to_ascii_uppercase();
            if ch.is_ascii_alphabetic() {
                let idx = (ch as u8 - b'A') as usize;
                if idx < local_options.len() {
                    let opt_id = local_options[idx].id.clone();
                    if !mapped.contains(&opt_id) {
                        mapped.push(opt_id);
                    }
                    continue;
                }
            }
        }

        // 6. 判断题语义比对
        if v_trim == "正确" || v_trim == "对" || v_trim.eq_ignore_ascii_case("true") || v_trim.eq_ignore_ascii_case("t") {
            if let Some(opt) = local_options.iter().find(|o| o.text.contains("正确") || o.text.contains("对")) {
                if !mapped.contains(&opt.id) {
                    mapped.push(opt.id.clone());
                }
                continue;
            }
        } else if v_trim == "错误" || v_trim == "错" || v_trim.eq_ignore_ascii_case("false") || v_trim.eq_ignore_ascii_case("f") {
            if let Some(opt) = local_options.iter().find(|o| o.text.contains("错误") || o.text.contains("错")) {
                if !mapped.contains(&opt.id) {
                    mapped.push(opt.id.clone());
                }
                continue;
            }
        }
    }
    mapped
}

/// 提取选项文本内容作为答案载体，实现跨批次、跨账号防选项乱序
pub(crate) fn extract_answers_content(
    answers: &Answers,
    questions: &[Question],
) -> Answers {
    let mut result = Answers::new();
    for q in questions {
        if let Some(ans_list) = answers.get(&q.id) {
            let content_list: Vec<String> = ans_list
                .iter()
                .map(|val| {
                    if let Some(opt) = q.options.iter().find(|o| o.id == *val || o.id.eq_ignore_ascii_case(val)) {
                        if !opt.text.trim().is_empty() {
                            return opt.text.trim().to_string();
                        }
                    }
                    val.clone()
                })
                .collect();
            result.insert(q.id.clone(), content_list);
        }
    }
    result
}

/// Build batch upload payload when quiz is confirmed passed.
#[allow(dead_code)]
pub(crate) fn build_passed_payload(
    cid: &str,
    course_name: Option<&str>,
    task_id: &str,
    task_title: &str,
    questions: &[Question],
    answers: &Answers,
) -> QaBatchUploadPayload {
    let upload_questions = questions
        .iter()
        .map(|q| {
            let correct = answers.get(&q.id).map(|v| v.as_slice());
            build_qa_upload_question(q, correct, None, false)
        })
        .collect();

    QaBatchUploadPayload {
        tid: task_id.to_string(),
        cid: Some(cid.to_string()),
        task_name: Some(task_title.to_string()),
        course_name: course_name.map(str::to_string),
        questions: upload_questions,
    }
}

/// Build batch upload payload when quiz submission failed.
pub(crate) fn build_failed_payload(
    cid: &str,
    course_name: Option<&str>,
    task_id: &str,
    task_title: &str,
    questions: &[Question],
    answers: &Answers,
) -> QaBatchUploadPayload {
    let upload_questions = questions
        .iter()
        .map(|q| {
            let wrong = answers.get(&q.id).map(|v| v.as_slice());
            build_qa_upload_question(q, None, wrong, false)
        })
        .collect();

    QaBatchUploadPayload {
        tid: task_id.to_string(),
        cid: Some(cid.to_string()),
        task_name: Some(task_title.to_string()),
        course_name: course_name.map(str::to_string),
        questions: upload_questions,
    }
}

// ─────────────────────────────────────────────────────────────
// 本地题目 ID 库管理与自动匹配上传 (Bank Matching & Sync)
// ─────────────────────────────────────────────────────────────

pub(crate) fn bank_key(tid: &str, question_id: &str) -> String {
    format!("{tid}:{question_id}")
}

pub(crate) fn load_local_bank(bank_path: &Path) -> BTreeMap<String, LocalBankQuestion> {
    crate::uooc::persistence::read::<BTreeMap<String, LocalBankQuestion>>(bank_path)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save_local_bank(
    bank_path: &Path,
    bank: &BTreeMap<String, LocalBankQuestion>,
) -> Result<(), String> {
    crate::uooc::persistence::write(bank_path, bank)
}

fn normalize_answers_for_upload(
    q_type: &str,
    clean_opts: &[QaUploadOption],
    raw_answers: &[String],
) -> Option<Vec<serde_json::Value>> {
    if raw_answers.is_empty() {
        return None;
    }

    if q_type == "judge" {
        // 判断题规范：永远只有 A. 正确、B. 错误 两个选项，答案统一以唯一的 boolean 项存储（正确为 [true]，错误为 [false]）
        for item in raw_answers {
            let s = item.trim().to_lowercase();
            if s == "true"
                || s == "1"
                || s == "a"
                || s == "正确"
                || s == "对"
                || s == "t"
                || s.starts_with("a.")
                || s.starts_with("a、")
            {
                return Some(vec![serde_json::Value::Bool(true)]);
            }
            if s == "false"
                || s == "0"
                || s == "b"
                || s == "错误"
                || s == "错"
                || s == "f"
                || s.starts_with("b.")
                || s.starts_with("b、")
            {
                return Some(vec![serde_json::Value::Bool(false)]);
            }
        }
        return Some(vec![serde_json::Value::Bool(true)]);
    }

    if q_type == "single" || q_type == "multiple" {
        // 单选/多选题答案：仅需保存对应 options[].id 值的选项（如 ["A"] 或 ["A", "C"]），严禁在答案项中保存实际选项文本内容
        let mut chosen_ids = Vec::new();
        for raw in raw_answers {
            let raw_trim = raw.trim();
            if raw_trim.is_empty() {
                continue;
            }

            // 1. 直接匹配 option.id（如 "A"）
            if let Some(opt) = clean_opts.iter().find(|o| o.id.eq_ignore_ascii_case(raw_trim)) {
                if !chosen_ids.contains(&opt.id) {
                    chosen_ids.push(opt.id.clone());
                }
                continue;
            }

            // 2. 匹配选项文本内容
            let raw_clean = strip_html_tags(raw_trim);
            if let Some(opt) = clean_opts.iter().find(|o| {
                o.text.trim() == raw_clean
                    || o.text.replace(' ', "") == raw_clean.replace(' ', "")
            }) {
                if !chosen_ids.contains(&opt.id) {
                    chosen_ids.push(opt.id.clone());
                }
                continue;
            }

            // 3. 前缀匹配如 "A. 选项A内容" 或 "A、选项A内容"
            if let Some(caps) = Regex::new(r"^([A-Za-z0-9]+)[.、:：\s]").ok().and_then(|re| re.captures(raw_trim)) {
                let candidate = &caps[1];
                if let Some(opt) = clean_opts.iter().find(|o| o.id.eq_ignore_ascii_case(candidate)) {
                    if !chosen_ids.contains(&opt.id) {
                        chosen_ids.push(opt.id.clone());
                    }
                    continue;
                }
            }

            // 4. 字母解析（如单选 "A" 或多选无空格拼接 "ABC"）
            if raw_trim.len() <= 6 && raw_trim.chars().all(|c| c.is_ascii_alphabetic()) {
                for ch in raw_trim.chars() {
                    let letter = ch.to_ascii_uppercase().to_string();
                    if clean_opts.iter().any(|o| o.id == letter) {
                        if !chosen_ids.contains(&letter) {
                            chosen_ids.push(letter);
                        }
                    }
                }
            }
        }

        if chosen_ids.is_empty() {
            None
        } else {
            Some(chosen_ids.into_iter().map(serde_json::Value::String).collect())
        }
    } else {
        // 填空题 (text)：保存纯文本数组
        let text_vals: Vec<serde_json::Value> = raw_answers
            .iter()
            .map(|s| strip_html_tags(s))
            .filter(|s| !s.is_empty())
            .map(serde_json::Value::String)
            .collect();
        if text_vals.is_empty() {
            None
        } else {
            Some(text_vals)
        }
    }
}

/// Helper to construct a `QaUploadQuestion` adhering to uooc-qa-share specifications:
/// 1. HTML tags in stem and options are strictly stripped.
/// 2. Choice answers (single/multiple) MUST contain only option IDs (e.g. ["A"], ["B"]), never option text.
/// 3. Judge questions MUST have options A. 正确, B. 错误, and answers must be a single boolean [true] or [false].
/// 4. Text questions MUST NOT have options, and blanks matches answer count.
pub(crate) fn build_qa_upload_question(
    q: &Question,
    correct_answers: Option<&[String]>,
    wrong_answers: Option<&[String]>,
    with_embedding: bool,
) -> QaUploadQuestion {
    let clean_stem = strip_html_tags(&q.text);

    let mut q_type = match q.kind {
        Kind::Multiple => "multiple",
        Kind::Text => "text",
        Kind::Single => "single",
    };

    if q.kind == Kind::Single && q.options.len() == 2 {
        let opt0 = strip_html_tags(&q.options[0].text).to_lowercase();
        let opt1 = strip_html_tags(&q.options[1].text).to_lowercase();
        let is_judge = (opt0.contains("正确") || opt0.contains("对") || opt0 == "t" || opt0 == "true" || opt0 == "a")
            && (opt1.contains("错误") || opt1.contains("错") || opt1 == "f" || opt1 == "false" || opt1 == "b");
        if is_judge {
            q_type = "judge";
        }
    }

    let (clean_opts, options_for_payload) = if q_type == "judge" {
        let opts = vec![
            QaUploadOption { id: "A".into(), text: "正确".into() },
            QaUploadOption { id: "B".into(), text: "错误".into() },
        ];
        (opts.clone(), Some(opts))
    } else if q_type == "text" {
        (Vec::new(), None)
    } else {
        let mut opts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (idx, opt) in q.options.iter().enumerate() {
            let opt_id = strip_html_tags(&opt.id);
            let opt_id = if opt_id.is_empty() {
                ((b'A' + idx as u8) as char).to_string()
            } else {
                opt_id
            };
            let opt_text = strip_html_tags(&opt.text);
            if !opt_text.is_empty() && seen.insert(opt_id.clone()) {
                opts.push(QaUploadOption { id: opt_id, text: opt_text });
            }
        }
        let payload_opts = if opts.len() >= 2 { Some(opts.clone()) } else { None };
        (opts, payload_opts)
    };

    let norm_correct = correct_answers.and_then(|ans| normalize_answers_for_upload(q_type, &clean_opts, ans));
    let norm_wrong = wrong_answers.and_then(|ans| normalize_answers_for_upload(q_type, &clean_opts, ans));

    let blanks = if q_type == "text" {
        Some(norm_correct.as_ref().map_or(q.blanks.max(1), |a| a.len()))
    } else {
        None
    };

    let embedding = if with_embedding && !clean_stem.is_empty() {
        Some(compute_local_embedding(&clean_stem))
    } else {
        None
    };

    QaUploadQuestion {
        question_id: q.id.clone(),
        question_type: q_type.to_string(),
        stem: if clean_stem.is_empty() { None } else { Some(clean_stem) },
        options: options_for_payload,
        blanks,
        correct_answers: norm_correct,
        wrong_answers: norm_wrong,
        embedding,
    }
}

pub(crate) fn build_upload_question_from_local_bank(q: &LocalBankQuestion) -> QaUploadQuestion {
    let clean_stem = q.stem.as_deref().map(strip_html_tags);

    let mut q_type = match q.question_type.as_str() {
        "11" | "multiple" => "multiple",
        "20" | "judge" => "judge",
        "30" | "text" => "text",
        _ => "single",
    };

    if q_type == "single" {
        if let Some(ref opts) = q.options {
            if opts.len() == 2 {
                let opt0 = strip_html_tags(&opts[0].text).to_lowercase();
                let opt1 = strip_html_tags(&opts[1].text).to_lowercase();
                if (opt0.contains("正确") || opt0.contains("对") || opt0 == "t" || opt0 == "true" || opt0 == "a")
                    && (opt1.contains("错误") || opt1.contains("错") || opt1 == "f" || opt1 == "false" || opt1 == "b")
                {
                    q_type = "judge";
                }
            }
        }
    }

    let (clean_opts, options_for_payload) = if q_type == "judge" {
        let opts = vec![
            QaUploadOption { id: "A".into(), text: "正确".into() },
            QaUploadOption { id: "B".into(), text: "错误".into() },
        ];
        (opts.clone(), Some(opts))
    } else if q_type == "text" {
        (Vec::new(), None)
    } else {
        let mut opts = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(ref raw_opts) = q.options {
            for (idx, opt) in raw_opts.iter().enumerate() {
                let opt_id = strip_html_tags(&opt.id);
                let opt_id = if opt_id.is_empty() {
                    ((b'A' + idx as u8) as char).to_string()
                } else {
                    opt_id
                };
                let opt_text = strip_html_tags(&opt.text);
                if !opt_text.is_empty() && seen.insert(opt_id.clone()) {
                    opts.push(QaUploadOption { id: opt_id, text: opt_text });
                }
            }
        }
        let payload_opts = if opts.len() >= 2 { Some(opts.clone()) } else { None };
        (opts, payload_opts)
    };

    let norm_correct = if let Some(ans_obj) = q.answers.iter().find(|a| a.is_correct) {
        let raw_strs: Vec<String> = ans_obj.values.iter().map(|v| match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(b) => if *b { "A".to_string() } else { "B".to_string() },
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        }).collect();
        normalize_answers_for_upload(q_type, &clean_opts, &raw_strs)
    } else {
        normalize_answers_for_upload(q_type, &clean_opts, &q.correct_answers)
    };

    let blanks = if q_type == "text" {
        Some(norm_correct.as_ref().map_or(q.blanks.max(1), |a| a.len()))
    } else {
        None
    };

    let embedding = clean_stem.as_deref().map(compute_local_embedding);

    QaUploadQuestion {
        question_id: q.question_id.clone(),
        question_type: q_type.to_string(),
        stem: clean_stem,
        options: options_for_payload,
        blanks,
        correct_answers: norm_correct,
        wrong_answers: None,
        embedding,
    }
}

/// Helper to construct a `QaBatchUploadPayload` from questions and answers.
pub(crate) fn build_batch_upload_payload(
    tid: &str,
    cid: Option<&str>,
    task_name: Option<&str>,
    course_name: Option<&str>,
    questions: &[Question],
    answers: &Answers,
) -> QaBatchUploadPayload {
    let upload_questions: Vec<QaUploadQuestion> = questions
        .iter()
        .map(|q| {
            let q_answers = answers.get(&q.id).map(|v| v.as_slice());
            build_qa_upload_question(q, q_answers, None, true)
        })
        .collect();

    QaBatchUploadPayload {
        tid: tid.to_string(),
        cid: cid.map(str::to_string),
        task_name: task_name.map(str::to_string),
        course_name: course_name.map(str::to_string),
        questions: upload_questions,
    }
}

/// 录入已通过并确认答案的测验试题到本地题库表，标记 uploaded 状态
pub(crate) fn record_passed_quiz(
    bank_path: &Path,
    cid: &str,
    task_id: &str,
    questions: &[Question],
    answers: &Answers,
    course_name: Option<&str>,
) -> Result<usize, String> {
    let mut bank = load_local_bank(bank_path);
    let mut count = 0;
    let now = chrono::Utc::now().timestamp_millis();
    let content_answers = extract_answers_content(answers, questions);

    for q in questions {
        let key = bank_key(task_id, &q.id);
        let existing = bank.get(&key);
        let ans = content_answers
            .get(&q.id)
            .cloned()
            .or_else(|| existing.map(|e| e.correct_answers.clone()))
            .unwrap_or_default();

        let already_uploaded = existing
            .map(|old| old.uploaded && old.correct_answers == ans && !ans.is_empty())
            .unwrap_or(false);

        let c_name = course_name
            .map(str::to_string)
            .or_else(|| existing.and_then(|e| e.course_name.clone()));

        let q_type_str = match q.kind {
            Kind::Single => {
                if q.options.len() == 2 {
                    let opt0 = q.options[0].text.trim().to_lowercase();
                    let opt1 = q.options[1].text.trim().to_lowercase();
                    if (opt0.contains("正确") || opt0.contains("对") || opt0 == "t" || opt0 == "true" || opt0 == "a")
                        && (opt1.contains("错误") || opt1.contains("错") || opt1 == "f" || opt1 == "false" || opt1 == "b")
                    {
                        "judge"
                    } else {
                        "single"
                    }
                } else {
                    "single"
                }
            }
            Kind::Multiple => "multiple",
            Kind::Text => "text",
        };

        let mut bank_answers = Vec::new();
        if !ans.is_empty() {
            let values: Vec<serde_json::Value> = if q_type_str == "judge" {
                ans.iter().map(|s| {
                    let st = s.trim().to_lowercase();
                    let b = st == "a" || st == "true" || st == "正确" || st == "对" || st == "t" || st.starts_with("a.");
                    serde_json::Value::Bool(b)
                }).collect()
            } else if q_type_str == "single" || q_type_str == "multiple" {
                ans.iter().map(|s| {
                    if let Some(opt) = q.options.iter().find(|o| o.text.trim() == s.trim() || o.id.eq_ignore_ascii_case(s.trim())) {
                        serde_json::Value::String(opt.id.clone())
                    } else {
                        serde_json::Value::String(s.clone())
                    }
                }).collect()
            } else {
                ans.iter().map(|s| serde_json::Value::String(s.clone())).collect()
            };

            bank_answers.push(LocalBankAnswer {
                id: None,
                values,
                is_correct: true,
                source: "local".into(),
                confidence: 1.0,
            });
        }

        let record = LocalBankQuestion {
            cid: cid.to_string(),
            course_name: c_name,
            tid: task_id.to_string(),
            task_name: None,
            question_id: q.id.clone(),
            question_type: q_type_str.into(),
            stem: if q.text.trim().is_empty() { None } else { Some(strip_html_tags(&q.text)) },
            options: if q.options.is_empty() {
                None
            } else {
                Some(
                    q.options
                        .iter()
                        .map(|o| QaUploadOption {
                            id: strip_html_tags(&o.id),
                            text: strip_html_tags(&o.text),
                        })
                        .collect(),
                )
            },
            blanks: q.blanks,
            answers: bank_answers,
            correct_answers: ans,
            uploaded: already_uploaded,
            updated_at_ms: now,
        };
        bank.insert(key, record);
        count += 1;
    }

    save_local_bank(bank_path, &bank)?;
    Ok(count)
}

/// 针对本地维护的确认答案题目列表，调用 POST /api/automation/match 进行服务端缺失确认，
/// 并自动将服务端缺失（或本地未上传）的题库打包上传至 /api/upload/batch
pub(crate) async fn sync_missing_to_server(
    bank_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    cid: Option<&str>,
    tid: Option<&str>,
    service: Option<&crate::uooc::Service>,
) -> Result<usize, String> {
    let mut bank = load_local_bank(bank_path);
    if bank.is_empty() {
        return Ok(0);
    }

    let candidate_keys: Vec<String> = bank
        .iter()
        .filter(|(_, q)| {
            if let Some(target_tid) = tid {
                if q.tid != target_tid {
                    return false;
                }
            } else if let Some(target_cid) = cid {
                if q.cid != target_cid {
                    return false;
                }
            }
            true
        })
        .map(|(k, _)| k.clone())
        .collect();

    if candidate_keys.is_empty() {
        return Ok(0);
    }

    let match_items: Vec<BankMatchItem> = candidate_keys
        .iter()
        .filter_map(|k| bank.get(k))
        .map(|q| BankMatchItem {
            tid: q.tid.clone(),
            question_id: q.question_id.clone(),
        })
        .collect();

    let local_qids: Vec<String> = candidate_keys
        .iter()
        .filter_map(|k| bank.get(k))
        .map(|q| q.question_id.clone())
        .collect();

    let payload = BankMatchPayload {
        cid: cid.map(str::to_string),
        tid: tid.map(str::to_string),
        items: Some(match_items.clone()),
        local_question_ids: Some(local_qids),
    };

    let api_base = resolve_api_base(server_url);
    let url = format!("{api_base}/automation/match");

    let payload_json = serde_json::to_string_pretty(&payload)
        .or_else(|_| serde_json::to_string(&payload))
        .unwrap_or_default();
    if let Some(svc) = service {
        svc.log_debug(
            "bank",
            format!(
                "[uooc-qa-share POST] 发送题目缺失比对请求: URL={url}, cid={:?}, tid={:?}, 比对题目数量={}\n请求体详情 (Payload):\n{payload_json}",
                cid,
                tid,
                match_items.len(),
            ),
        );
    }

    let client = http_client()?;
    let mut req = client.post(&url).json(&payload);
    if let Some(key) = api_key {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            req = req.header("x-api-key", trimmed);
        }
    }

    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            let err_msg = format!("匹配题库请求失败: {e}");
            if let Some(svc) = service {
                svc.log_debug(
                    "bank",
                    format!("[uooc-qa-share POST] 比对请求发生错误: URL={url}, 错误={err_msg}"),
                );
            }
            return Err(err_msg);
        }
    };

    let status = resp.status();
    let resp_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            let err_msg = format!("读取比对响应失败: {e}");
            if let Some(svc) = service {
                svc.log_debug(
                    "bank",
                    format!("[uooc-qa-share POST] 读取比对响应失败: URL={url}, HTTP状态码={status}, 错误={err_msg}"),
                );
            }
            return Err(err_msg);
        }
    };

    let resp_text = String::from_utf8_lossy(&resp_bytes);
    if let Some(svc) = service {
        svc.log_debug(
            "bank",
            format!(
                "[uooc-qa-share POST] 收到比对响应: URL={url}, HTTP状态码={}, 响应体:\n{resp_text}",
                status.as_u16(),
            ),
        );
    }

    if !status.is_success() {
        return Err(format!("匹配题库响应失败，状态码: {}, 响应: {}", status, resp_text));
    }

    let match_res: BankMatchResponse = serde_json::from_slice(&resp_bytes)
        .map_err(|e| format!("解析匹配题库响应失败: {e}，响应原内容: {resp_text}"))?;

    // 汇总服务端缺失以及本地尚未成功标记上传的题目 key
    let mut to_upload_keys = std::collections::HashSet::new();
    for missing in &match_res.missing_on_server {
        let missing_tid = missing.tid.as_deref().or(tid);
        if let Some(t) = missing_tid {
            to_upload_keys.insert(bank_key(t, &missing.question_id));
        } else {
            for k in &candidate_keys {
                if let Some(q) = bank.get(k) {
                    if q.question_id == missing.question_id {
                        to_upload_keys.insert(k.clone());
                    }
                }
            }
        }
    }

    // 同时包含本地已记录但 uploaded == false 的题目
    for k in &candidate_keys {
        if let Some(q) = bank.get(k) {
            if !q.uploaded {
                to_upload_keys.insert(k.clone());
            }
        }
    }

    if to_upload_keys.is_empty() {
        return Ok(0);
    }

    // 按 tid 分组打包批量上传
    let mut by_tid: BTreeMap<String, Vec<LocalBankQuestion>> = BTreeMap::new();
    for k in &to_upload_keys {
        if let Some(q) = bank.get(k) {
            by_tid.entry(q.tid.clone()).or_default().push(q.clone());
        }
    }

    let mut uploaded_count = 0;
    for (t, qs) in by_tid {
        let first_cid = qs.first().map(|q| q.cid.clone());
        let first_course_name = qs.first().and_then(|q| q.course_name.clone());
        let upload_payload = QaBatchUploadPayload {
            tid: t,
            cid: first_cid,
            task_name: None,
            course_name: first_course_name,
            questions: qs
                .iter()
                .map(build_upload_question_from_local_bank)
                .collect(),
        };

        if let Ok(res) = upload_batch(server_url, api_key, &upload_payload, service).await {
            if res.ok {
                for q in qs {
                    let k = bank_key(&q.tid, &q.question_id);
                    if let Some(item) = bank.get_mut(&k) {
                        item.uploaded = true;
                        uploaded_count += 1;
                    }
                }
            }
        }
    }

    let _ = save_local_bank(bank_path, &bank);
    Ok(uploaded_count)
}

/// 手动全量上传本地题库的所有题目至服务端，按 (cid, tid) 分组打包提交
pub(crate) async fn upload_all_local_bank(
    bank_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    service: Option<&crate::uooc::Service>,
) -> Result<usize, String> {
    let mut bank = load_local_bank(bank_path);
    if bank.is_empty() {
        return Ok(0);
    }

    let mut grouped: BTreeMap<(String, String), Vec<LocalBankQuestion>> = BTreeMap::new();
    for (_, q) in bank.iter() {
        grouped
            .entry((q.cid.clone(), q.tid.clone()))
            .or_default()
            .push(q.clone());
    }

    let mut total_uploaded = 0;
    for ((cid, tid), questions) in grouped {
        let first_course_name = questions.first().and_then(|q| q.course_name.clone());
        let upload_questions: Vec<QaUploadQuestion> = questions
            .iter()
            .map(build_upload_question_from_local_bank)
            .collect();

        let payload = QaBatchUploadPayload {
            tid: tid.clone(),
            cid: Some(cid),
            task_name: None,
            course_name: first_course_name,
            questions: upload_questions,
        };

        let res = upload_batch(server_url, api_key, &payload, service)
            .await
            .map_err(|e| format!("全量上传任务 {tid} 失败: {e}"))?;

        if res.ok {
            for q in questions {
                let k = bank_key(&q.tid, &q.question_id);
                if let Some(item) = bank.get_mut(&k) {
                    item.uploaded = true;
                    total_uploaded += 1;
                }
            }
        }
    }

    let _ = save_local_bank(bank_path, &bank);
    Ok(total_uploaded)
}

/// 解析 UOOC 官方 /exam/getTaskPaper 接口返回的标准无混淆题单（含全局真实单题 ID、选项和答案）
pub(crate) fn parse_task_paper(raw: &[u8]) -> Option<(Vec<Question>, Answers)> {
    let val: serde_json::Value = serde_json::from_slice(raw).ok()?;
    parse_task_paper_json(&val)
}

pub(crate) fn parse_task_paper_json(val: &serde_json::Value) -> Option<(Vec<Question>, Answers)> {
    let list = val["data"]["questions"]
        .as_array()
        .or_else(|| val["questions"].as_array())
        .or_else(|| val["data"].as_array())?;

    let mut questions = Vec::new();
    let mut answers = BTreeMap::new();

    for (index, item) in list.iter().enumerate() {
        let qid = item["id"]
            .as_i64()
            .map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(str::to_string))
            .or_else(|| item["question_id"].as_i64().map(|n| n.to_string()))
            .or_else(|| item["question_id"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{index}"));

        let stem = item["question"]
            .as_str()
            .or_else(|| item["stem"].as_str())
            .or_else(|| item["title"].as_str())
            .or_else(|| item["content"].as_str())
            .unwrap_or_default()
            .to_string();

        let type_str = item["type"].as_str().unwrap_or("");
        let kind = match type_str {
            "11" | "multiple" => Kind::Multiple,
            "20" | "judge" => Kind::Single,
            "30" | "text" => Kind::Text,
            _ => Kind::Single,
        };

        let mut options = Vec::new();
        if let Some(opts) = item["options_app"].as_array().or_else(|| item["options"].as_array()) {
            for (opt_idx, opt) in opts.iter().enumerate() {
                let opt_id = opt["key"]
                    .as_str()
                    .or_else(|| opt["id"].as_str())
                    .or_else(|| opt["value"].as_str().filter(|v| v.len() <= 2))
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        let letter = (b'A' + opt_idx as u8) as char;
                        letter.to_string()
                    });
                let opt_text = opt["value"]
                    .as_str()
                    .or_else(|| opt["text"].as_str())
                    .or_else(|| opt["title"].as_str())
                    .unwrap_or_default()
                    .to_string();
                options.push(OptionItem {
                    id: opt_id,
                    text: opt_text,
                });
            }
        }

        let mut correct = Vec::new();
        let ans_val = &item["answer"];
        let ans_arr = ans_val
            .as_array()
            .or_else(|| item["answers"].as_array())
            .or_else(|| item["standard_answer"].as_array());

        if let Some(arr) = ans_arr {
            for a in arr {
                if let Some(s) = a.as_str() {
                    correct.push(s.to_string());
                } else if let Some(n) = a.as_i64() {
                    correct.push(n.to_string());
                }
            }
        } else if let Some(s) = ans_val.as_str().or_else(|| item["standard_answer"].as_str()) {
            for part in s.split(|c| c == ',' || c == '，' || c == '、' || c == ' ') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    correct.push(trimmed.to_string());
                }
            }
        }

        // 防选项乱序处理：若答案包含字母标签，将其解析提取为对应的选项文本内容
        let text_answers: Vec<String> = correct
            .iter()
            .map(|ans_key| {
                if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(ans_key)) {
                    if !opt.text.trim().is_empty() {
                        return opt.text.trim().to_string();
                    }
                }
                ans_key.clone()
            })
            .collect();

        if !text_answers.is_empty() {
            answers.insert(qid.clone(), text_answers);
        }

        questions.push(Question {
            id: qid,
            text: stem,
            kind,
            options,
            blanks: 0,
            images: Vec::new(),
        });
    }

    if questions.is_empty() {
        None
    } else {
        Some((questions, answers))
    }
}

/// 将 DOM 解析的题目（携带表单提交 binding）与 getTaskPaper 解析的真实题单对齐
/// 从而将 DOM 题目 ID 规范化为真实单题数字 ID，同时纯净化题干与选项文本
pub(crate) fn align_paper_with_task_paper(
    paper_questions: &mut [Question],
    task_paper_questions: &[Question],
) {
    if paper_questions.is_empty() || task_paper_questions.is_empty() {
        return;
    }

    let mut used_tp_indices = std::collections::HashSet::new();
    let total_dom = paper_questions.len();
    let total_tp = task_paper_questions.len();

    for dom_q in paper_questions.iter_mut() {
        let dom_norm = normalize_stem(&dom_q.text);

        // 1. 尝试通过题干匹配
        let mut matched_idx = None;
        for (tp_idx, tp_q) in task_paper_questions.iter().enumerate() {
            if used_tp_indices.contains(&tp_idx) {
                continue;
            }
            let tp_norm = normalize_stem(&tp_q.text);
            if tp_norm == dom_norm
                || (!tp_norm.is_empty() && (dom_norm.contains(&tp_norm) || tp_norm.contains(&dom_norm)))
            {
                matched_idx = Some(tp_idx);
                break;
            }
        }

        // 2. 如果未通过题干匹配上，且题目数量相同，使用未使用的同位置索引
        if matched_idx.is_none() && total_dom == total_tp {
            for (tp_idx, _) in task_paper_questions.iter().enumerate() {
                if !used_tp_indices.contains(&tp_idx) {
                    matched_idx = Some(tp_idx);
                    break;
                }
            }
        }

        if let Some(tp_idx) = matched_idx {
            used_tp_indices.insert(tp_idx);
            let tp_q = &task_paper_questions[tp_idx];

            // 更新为全局真实单题数字 ID
            dom_q.id = tp_q.id.clone();

            // 若 tp_q 拥有纯净题干，补充题干
            if !tp_q.text.is_empty() {
                dom_q.text = tp_q.text.clone();
            }

            // 更新选项文本：通过 ID 或相对顺序把纯净文本对齐进 DOM 选项
            for dom_opt in dom_q.options.iter_mut() {
                if let Some(tp_opt) = tp_q.options.iter().find(|o| o.id.eq_ignore_ascii_case(&dom_opt.id)) {
                    if !tp_opt.text.is_empty() {
                        dom_opt.text = tp_opt.text.clone();
                    }
                }
            }
        }
    }
}

/// 解析 /exam/view 返回的已提交测验数据（支持 JSON 和 HTML 两种形态）
pub(crate) fn parse_exam_view(raw: &[u8]) -> Option<(Vec<Question>, Answers)> {
    if let Ok(val) = serde_json::from_slice::<serde_json::Value>(raw) {
        // 优先使用专门的 parse_exam_view_json 解析包含已批改答案/用户提交答案的试卷数据
        if let Some((questions, answers)) = parse_exam_view_json(&val) {
            if !questions.is_empty() && !answers.is_empty() {
                return Some((questions, answers));
            }
            if !questions.is_empty() {
                if let Some((tp_qs, tp_ans)) = parse_task_paper_json(&val) {
                    if !tp_ans.is_empty() {
                        return Some((tp_qs, tp_ans));
                    }
                }
                return Some((questions, answers));
            }
        }
        if let Some((questions, answers)) = parse_task_paper_json(&val) {
            if !questions.is_empty() {
                return Some((questions, answers));
            }
        }
    }

    if let Ok(html_str) = String::from_utf8(raw.to_vec()) {
        if html_str.contains('<') {
            return parse_exam_view_html(&html_str);
        }
    }

    None
}

fn parse_exam_view_json(val: &serde_json::Value) -> Option<(Vec<Question>, Answers)> {
    let list = val["data"]["questions"]
        .as_array()
        .or_else(|| val["questions"].as_array())
        .or_else(|| val["data"]["list"].as_array())
        .or_else(|| val["data"]["paper"]["questions"].as_array())
        .or_else(|| val["data"]["paper"]["question_list"].as_array())
        .or_else(|| val["paper"]["questions"].as_array())
        .or_else(|| val["data"].as_array())?;

    let mut questions = Vec::new();
    let mut answers = BTreeMap::new();

    for (index, item) in list.iter().enumerate() {
        let qid = item["id"]
            .as_i64()
            .map(|n| n.to_string())
            .or_else(|| item["id"].as_str().map(str::to_string))
            .or_else(|| item["question_id"].as_i64().map(|n| n.to_string()))
            .or_else(|| item["question_id"].as_str().map(str::to_string))
            .unwrap_or_else(|| format!("q{index}"));

        let stem = item["stem"]
            .as_str()
            .or_else(|| item["title"].as_str())
            .or_else(|| item["content"].as_str())
            .or_else(|| item["question"].as_str())
            .unwrap_or_default()
            .to_string();

        let type_str = item["type"]
            .as_str()
            .or_else(|| item["question_type"].as_str())
            .unwrap_or("single");

        let kind = match type_str {
            "11" | "multiple" => Kind::Multiple,
            "20" | "judge" => Kind::Single,
            "30" | "text" => Kind::Text,
            _ => Kind::Single,
        };

        let mut options = Vec::new();
        if let Some(opts) = item["options_app"].as_array().or_else(|| item["options"].as_array()) {
            for (opt_idx, opt) in opts.iter().enumerate() {
                let opt_id = opt["key"]
                    .as_str()
                    .or_else(|| opt["id"].as_str())
                    .or_else(|| opt["value"].as_str().filter(|v| v.len() <= 2))
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| {
                        let letter = (b'A' + opt_idx as u8) as char;
                        letter.to_string()
                    });
                let opt_text = opt["value"]
                    .as_str()
                    .or_else(|| opt["text"].as_str())
                    .or_else(|| opt["title"].as_str())
                    .or_else(|| opt["content"].as_str())
                    .unwrap_or_default()
                    .to_string();
                options.push(OptionItem {
                    id: opt_id,
                    text: opt_text,
                });
            }
        }

        let mut correct = Vec::new();

        // 1. 优先提取显式标准答案
        let std_arr = item["standard_answer"]
            .as_array()
            .or_else(|| item["standardAnswer"].as_array())
            .or_else(|| item["right_answer"].as_array())
            .or_else(|| item["rightAnswer"].as_array())
            .or_else(|| item["correct_answers"].as_array())
            .or_else(|| item["correct_answer"].as_array())
            .or_else(|| item["correctAnswer"].as_array())
            .or_else(|| item["answers"].as_array())
            .or_else(|| item["answer"].as_array());

        if let Some(arr) = std_arr {
            for a in arr {
                if let Some(s) = a.as_str() {
                    correct.push(s.to_string());
                } else if let Some(n) = a.as_i64() {
                    correct.push(n.to_string());
                }
            }
        } else if let Some(s) = item["standard_answer"]
            .as_str()
            .or_else(|| item["standardAnswer"].as_str())
            .or_else(|| item["right_answer"].as_str())
            .or_else(|| item["rightAnswer"].as_str())
            .or_else(|| item["correct_answer"].as_str())
            .or_else(|| item["correctAnswer"].as_str())
            .or_else(|| item["answer"].as_str())
        {
            for part in s.split(|c| c == ',' || c == '，' || c == '、' || c == ' ' || c == ';') {
                let trimmed = part.trim().trim_matches(|c: char| !c.is_alphanumeric());
                if !trimmed.is_empty() {
                    if trimmed.chars().all(|c| c.is_ascii_alphabetic())
                        && trimmed.len() > 1
                        && trimmed.len() <= 6
                        && !options.iter().any(|o| o.id == trimmed)
                    {
                        for ch in trimmed.chars() {
                            correct.push(ch.to_ascii_uppercase().to_string());
                        }
                    } else {
                        correct.push(trimmed.to_string());
                    }
                }
            }
        }

        // 2. 提取用户提交的答案（判对或无独立标答时作为有效答案来源）
        if correct.is_empty() {
            let is_right = item["is_right"]
                .as_i64()
                .map(|n| n != 0)
                .or_else(|| item["is_right"].as_str().map(|s| s == "1" || s == "true"))
                .or_else(|| item["is_correct"].as_bool())
                .or_else(|| item["is_correct"].as_i64().map(|n| n != 0))
                .or_else(|| item["score"].as_f64().map(|s| s > 0.0))
                .unwrap_or(true);

            if is_right {
                let user_arr = item["user_answer"]
                    .as_array()
                    .or_else(|| item["userAnswer"].as_array())
                    .or_else(|| item["user_answers"].as_array())
                    .or_else(|| item["my_answer"].as_array())
                    .or_else(|| item["myAnswer"].as_array())
                    .or_else(|| item["student_answer"].as_array())
                    .or_else(|| item["submit_answer"].as_array());

                if let Some(arr) = user_arr {
                    for a in arr {
                        if let Some(s) = a.as_str() {
                            correct.push(s.to_string());
                        } else if let Some(n) = a.as_i64() {
                            correct.push(n.to_string());
                        }
                    }
                } else if let Some(s) = item["user_answer"]
                    .as_str()
                    .or_else(|| item["userAnswer"].as_str())
                    .or_else(|| item["my_answer"].as_str())
                    .or_else(|| item["myAnswer"].as_str())
                    .or_else(|| item["student_answer"].as_str())
                    .or_else(|| item["submit_answer"].as_str())
                {
                    for part in s.split(|c| c == ',' || c == '，' || c == '、' || c == ' ' || c == ';') {
                        let trimmed = part.trim().trim_matches(|c: char| !c.is_alphanumeric());
                        if !trimmed.is_empty() {
                            if trimmed.chars().all(|c| c.is_ascii_alphabetic())
                                && trimmed.len() > 1
                                && trimmed.len() <= 6
                                && !options.iter().any(|o| o.id == trimmed)
                            {
                                for ch in trimmed.chars() {
                                    correct.push(ch.to_ascii_uppercase().to_string());
                                }
                            } else {
                                correct.push(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        // 提取文本答案以对冲乱序
        let text_answers: Vec<String> = correct
            .iter()
            .map(|ans_key| {
                if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(ans_key)) {
                    if !opt.text.trim().is_empty() {
                        return opt.text.trim().to_string();
                    }
                }
                ans_key.clone()
            })
            .collect();

        if !text_answers.is_empty() {
            answers.insert(qid.clone(), text_answers);
        }

        questions.push(Question {
            id: qid,
            text: stem,
            kind,
            options,
            blanks: 0,
            images: Vec::new(),
        });
    }

    if questions.is_empty() {
        None
    } else {
        Some((questions, answers))
    }
}

fn parse_exam_view_html(html: &str) -> Option<(Vec<Question>, Answers)> {
    let doc = scraper::Html::parse_document(html);
    let container_sel = scraper::Selector::parse(".queContainer,.que-item,.question-item,.ti-item,.test-item").ok()?;
    let stem_sel = scraper::Selector::parse(".ti-q,.que-title,.question-title,.stem").ok()?;
    let option_sel = scraper::Selector::parse(".ti-a,label,.option,.option-item").ok()?;
    let input_sel = scraper::Selector::parse("input[type=radio],input[type=checkbox]").ok()?;
    let answer_sel = scraper::Selector::parse(".rightAnswer,.right-answer,.correct-answer,.da-an,.daan,.standard-answer,.ti-daan,.standardAnswer").ok()?;
    let user_ans_sel = scraper::Selector::parse(".userAnswer,.user-answer,.myAnswer,.my-answer").ok()?;

    let mut questions = Vec::new();
    let mut answers = BTreeMap::new();

    for (index, elem) in doc.select(&container_sel).enumerate() {
        let qid = elem
            .value()
            .attr("id")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("q{index}"));

        let stem = elem
            .select(&stem_sel)
            .next()
            .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .unwrap_or_default();

        let mut options = Vec::new();
        let mut has_checkbox = false;

        for (opt_idx, opt_elem) in elem.select(&option_sel).enumerate() {
            let input = opt_elem.select(&input_sel).next();
            if let Some(inp) = input {
                if inp.value().attr("type") == Some("checkbox") {
                    has_checkbox = true;
                }
            }
            let opt_id = input
                .and_then(|inp| inp.value().attr("value"))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    let letter = (b'A' + opt_idx as u8) as char;
                    letter.to_string()
                });
            let opt_text = opt_elem.text().collect::<Vec<_>>().join(" ").trim().to_string();
            options.push(OptionItem {
                id: opt_id,
                text: opt_text,
            });
        }

        let kind = if has_checkbox {
            Kind::Multiple
        } else if !options.is_empty() {
            Kind::Single
        } else {
            Kind::Text
        };

        let mut correct = Vec::new();

        // 1. 优先从专门的标准答案标签提取
        if let Some(ans_elem) = elem.select(&answer_sel).next() {
            let ans_text = ans_elem.text().collect::<Vec<_>>().join(" ");
            extract_answer_tokens(&ans_text, &options, &mut correct);
        }

        // 2. 检查整个题干区域文本中的模式（如 "正确答案：A"、"标准答案：B"）
        if correct.is_empty() {
            let full_text = elem.text().collect::<Vec<_>>().join(" ");
            extract_answer_tokens(&full_text, &options, &mut correct);
        }

        // 3. 检查带有 class="right" / "correct" / "is-correct" / "dui" 的选项
        if correct.is_empty() {
            for opt_elem in elem.select(&option_sel) {
                let classes = opt_elem.value().classes().collect::<Vec<_>>();
                if classes.iter().any(|c| *c == "right" || *c == "correct" || *c == "is-correct" || *c == "dui") {
                    let input = opt_elem.select(&input_sel).next();
                    if let Some(val) = input.and_then(|i| i.value().attr("value")) {
                        correct.push(val.to_string());
                    }
                }
            }
        }

        // 4. 提取用户提交的答案（当题目判对或有作答记录时作为补充）
        if correct.is_empty() {
            let full_text = elem.text().collect::<Vec<_>>().join(" ");
            let is_judged_right = full_text.contains("回答正确")
                || full_text.contains("得分")
                || elem.select(&scraper::Selector::parse(".icon-dui,.dui,.icon-right,.icon-check").unwrap()).next().is_some();

            if is_judged_right {
                for opt_elem in elem.select(&option_sel) {
                    let input = opt_elem.select(&input_sel).next();
                    if let Some(inp) = input {
                        if inp.value().attr("checked").is_some() {
                            if let Some(val) = inp.value().attr("value") {
                                correct.push(val.to_string());
                            }
                        }
                    }
                }
                if correct.is_empty() {
                    if let Some(user_elem) = elem.select(&user_ans_sel).next() {
                        let u_text = user_elem.text().collect::<Vec<_>>().join(" ");
                        extract_user_answer_tokens(&u_text, &options, &mut correct);
                    }
                }
            }
        }

        if !correct.is_empty() {
            // 防选项乱序处理：若答案包含字母标签，将其解析提取为对应的选项文本内容
            let text_answers: Vec<String> = correct
                .iter()
                .map(|ans_key| {
                    if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(ans_key)) {
                        if !opt.text.trim().is_empty() {
                            return opt.text.trim().to_string();
                        }
                    }
                    ans_key.clone()
                })
                .collect();
            answers.insert(qid.clone(), text_answers);
        }

        questions.push(Question {
            id: qid,
            text: stem,
            kind,
            options,
            blanks: 0,
            images: Vec::new(),
        });
    }

    if questions.is_empty() {
        None
    } else {
        Some((questions, answers))
    }
}

fn extract_answer_tokens(text: &str, options: &[OptionItem], out: &mut Vec<String>) {
    let markers = ["正确答案", "标准答案", "参考答案", "正确选项", "判定答案", "答案"];
    for marker in &markers {
        if let Some(pos) = text.find(marker) {
            let after = &text[pos + marker.len()..];
            let cleaned = after.trim_start_matches(|c: char| c == ':' || c == '：' || c.is_whitespace() || c == '【' || c == '[' || c == '“' || c == '"');

            let end_idx = cleaned
                .find(|c: char| c == '\n' || c == '\r' || c == '；' || c == ';')
                .unwrap_or(cleaned.len().min(120));
            let segment = &cleaned[..end_idx];

            // 优先截取括号注释前的内容（如 "B (1983年)" 中的 "B"）
            let before_paren = segment.split(|c| c == '(' || c == '（').next().unwrap_or(segment).trim();
            let to_parse = if !before_paren.is_empty() { before_paren } else { segment };

            let mut extracted = Vec::new();
            for raw_word in to_parse.split(|c: char| {
                c == ',' || c == '，' || c == '、' || c == ' ' || c == '\t' || c == '；' || c == ';' || c == '/'
            }) {
                let trimmed = raw_word.trim().trim_matches(|c: char| !c.is_alphanumeric());
                if trimmed.is_empty() {
                    continue;
                }
                // 字母型选项（单选 A 或多选如 ABCD）
                if trimmed.chars().all(|c| c.is_ascii_alphabetic()) && trimmed.len() <= 6 {
                    if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(trimmed)) {
                        if !extracted.contains(&opt.id) {
                            extracted.push(opt.id.clone());
                        }
                    } else {
                        for ch in trimmed.chars() {
                            let letter = ch.to_ascii_uppercase().to_string();
                            if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(&letter)) {
                                if !extracted.contains(&opt.id) {
                                    extracted.push(opt.id.clone());
                                }
                            } else if !extracted.contains(&letter) {
                                extracted.push(letter);
                            }
                        }
                    }
                } else if trimmed == "正确" || trimmed == "对" || trimmed == "T" || trimmed == "true" {
                    if let Some(opt) = options.iter().find(|o| o.text.contains("正确") || o.text.contains("对")) {
                        if !extracted.contains(&opt.id) {
                            extracted.push(opt.id.clone());
                        }
                    } else if !extracted.contains(&"A".to_string()) {
                        extracted.push("A".to_string());
                    }
                } else if trimmed == "错误" || trimmed == "错" || trimmed == "F" || trimmed == "false" {
                    if let Some(opt) = options.iter().find(|o| o.text.contains("错误") || o.text.contains("错")) {
                        if !extracted.contains(&opt.id) {
                            extracted.push(opt.id.clone());
                        }
                    } else if !extracted.contains(&"B".to_string()) {
                        extracted.push("B".to_string());
                    }
                } else if let Some(opt) = options.iter().find(|o| {
                    !o.text.trim().is_empty() && (o.text.trim() == trimmed || trimmed.contains(&o.text.trim()))
                }) {
                    if !extracted.contains(&opt.id) {
                        extracted.push(opt.id.clone());
                    }
                }
            }

            if !extracted.is_empty() {
                for ans in extracted {
                    if !out.contains(&ans) {
                        out.push(ans);
                    }
                }
                return;
            }
        }
    }
}

fn extract_user_answer_tokens(text: &str, options: &[OptionItem], out: &mut Vec<String>) {
    let markers = ["我的答案", "考生答案", "学生答案", "你的答案", "所选答案"];
    for marker in &markers {
        if let Some(pos) = text.find(marker) {
            let after = &text[pos + marker.len()..];
            let cleaned = after.trim_start_matches(|c: char| c == ':' || c == '：' || c.is_whitespace() || c == '【' || c == '[');
            for word in cleaned.split(|c: char| c == ',' || c == '，' || c == '、' || c == ' ' || c == ';' || c == '\n') {
                let trimmed = word.trim().trim_matches(|c: char| !c.is_alphanumeric());
                if !trimmed.is_empty() {
                    if trimmed.chars().all(|c| c.is_ascii_alphabetic()) && trimmed.len() <= 6 {
                        if let Some(opt) = options.iter().find(|o| o.id.eq_ignore_ascii_case(trimmed)) {
                            if !out.contains(&opt.id) {
                                out.push(opt.id.clone());
                            }
                        } else {
                            for ch in trimmed.chars() {
                                let letter = ch.to_ascii_uppercase().to_string();
                                if !out.contains(&letter) {
                                    out.push(letter);
                                }
                            }
                        }
                    }
                }
            }
            if !out.is_empty() {
                return;
            }
        }
    }
}

/// 将从 /exam/view 解析出的试题答案对齐到规范的单题列表（支持按 ID、题干或顺序匹配）
pub(crate) fn align_view_answers_to_questions(
    view_questions: &[Question],
    view_answers: &Answers,
    target_questions: &[Question],
    out_answers: &mut Answers,
) {
    for (v_idx, vq) in view_questions.iter().enumerate() {
        let Some(ans_list) = view_answers.get(&vq.id) else {
            continue;
        };
        if ans_list.is_empty() {
            continue;
        }

        // 寻找 target_questions 中的对应题目：1. id 匹配 2. 题干匹配 3. 顺序索引匹配
        let v_norm = normalize_stem(&vq.text);
        let matched_tq = target_questions
            .iter()
            .find(|tq| tq.id == vq.id || tq.id.contains(&vq.id) || vq.id.contains(&tq.id))
            .or_else(|| {
                target_questions.iter().find(|tq| {
                    let t_norm = normalize_stem(&tq.text);
                    !t_norm.is_empty()
                        && (t_norm == v_norm || t_norm.contains(&v_norm) || v_norm.contains(&t_norm))
                })
            })
            .or_else(|| {
                if target_questions.len() == view_questions.len() {
                    target_questions.get(v_idx)
                } else {
                    None
                }
            });

        if let Some(tq) = matched_tq {
            let mut resolved_ans = Vec::new();
            for ans_item in ans_list {
                let ans_trim = ans_item.trim();
                // 优先看 ans_item 是否是 vq 的选项字母或文本，转换到对应选项的文本内容
                if let Some(v_opt) = vq
                    .options
                    .iter()
                    .find(|o| o.id.eq_ignore_ascii_case(ans_trim) || o.text.trim() == ans_trim)
                {
                    if !v_opt.text.trim().is_empty() {
                        resolved_ans.push(v_opt.text.trim().to_string());
                        continue;
                    }
                }
                if let Some(t_opt) = tq
                    .options
                    .iter()
                    .find(|o| o.id.eq_ignore_ascii_case(ans_trim) || o.text.trim() == ans_trim)
                {
                    if !t_opt.text.trim().is_empty() {
                        resolved_ans.push(t_opt.text.trim().to_string());
                        continue;
                    }
                }
                resolved_ans.push(ans_trim.to_string());
            }
            if !resolved_ans.is_empty() {
                out_answers.insert(tq.id.clone(), resolved_ans);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
// 本地题库向量缓存与向量检索 (Vector Cache & Search)
// ─────────────────────────────────────────────────────────────

pub(crate) fn load_vector_cache(cache_path: &Path) -> Vec<CachedVectorQuestion> {
    crate::uooc::persistence::read::<Vec<CachedVectorQuestion>>(cache_path)
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn save_vector_cache(
    cache_path: &Path,
    vectors: &[CachedVectorQuestion],
) -> Result<(), String> {
    crate::uooc::persistence::write(cache_path, &vectors)
}

pub(crate) fn get_vector_status(cache_path: &Path) -> EmbeddingModelStatus {
    let list = load_vector_cache(cache_path);
    let count = list.len();
    EmbeddingModelStatus {
        status: if count > 0 { "ready".into() } else { "empty".into() },
        cached_count: count,
        total_dimension: EMBEDDING_DIMENSIONS,
        updated_at_ms: chrono::Utc::now().timestamp_millis(),
        message: if count > 0 {
            format!("本地已缓存 {count} 条题目向量，支持高速离线与远程向量检索")
        } else {
            "尚未下载题库向量，点击下方按钮开始下载".into()
        },
    }
}

/// 从服务端批量拉取题目并缓存 512 维向量
pub(crate) async fn download_qa_vectors(
    cache_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    cids: &[String],
) -> Result<EmbeddingModelStatus, String> {
    let mut cached = load_vector_cache(cache_path);
    let mut seen_ids = std::collections::HashSet::new();
    for item in &cached {
        seen_ids.insert(format!("{}:{}", item.tid, item.question_id));
    }

    let base = server_url.trim_end_matches('/');
    let client = http_client()?;

    // 1. 如果提供了课程列表，优先按课程抓取题组
    let mut tids_to_fetch = std::collections::HashSet::new();
    for cid in cids {
        let url = format!("{base}/api/courses/{cid}");
        let mut req = client.get(&url);
        if let Some(key) = api_key {
            req = req.header("x-api-key", key);
        }
        if let Ok(resp) = req.send().await {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(groups) = data["taskGroups"].as_array() {
                    for g in groups {
                        if let Some(tid) = g["tid"].as_str() {
                            tids_to_fetch.insert((cid.clone(), tid.to_string()));
                        }
                    }
                }
            }
        }
    }

    // 2. 同时从 /api/automation/tasks 发现所有缺失或现存的题组
    let auto_url = format!("{base}/api/automation/tasks?type=all&limit=100");
    let mut auto_req = client.get(&auto_url);
    if let Some(key) = api_key {
        auto_req = auto_req.header("x-api-key", key);
    }
    if let Ok(resp) = auto_req.send().await {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(tasks) = data["tasks"].as_array() {
                for t in tasks {
                    let cid = t["cid"].as_str().unwrap_or("").to_string();
                    if let Some(tid) = t["tid"].as_str() {
                        tids_to_fetch.insert((cid, tid.to_string()));
                    }
                }
            }
        }
    }

    // 3. 逐个题组拉取题目并生成/记录向量
    for (cid, tid) in tids_to_fetch {
        if let Ok(qs) = query_task_answers(server_url, api_key, &tid, if cid.is_empty() { None } else { Some(&cid) }).await {
            for q in qs {
                let Some(ref qid) = q.question_id else { continue };
                let key = format!("{tid}:{qid}");
                if seen_ids.insert(key) {
                    let stem_text = q.stem.clone().unwrap_or_default();
                    let embedding = match q.stem_embedding {
                        Some(ref v) if v.len() == EMBEDDING_DIMENSIONS => v.clone(),
                        _ => compute_local_embedding(&stem_text),
                    };

                    let answers = q
                        .answers
                        .iter()
                        .filter(|a| a.is_correct)
                        .flat_map(|a| a.values.clone())
                        .collect();

                    cached.push(CachedVectorQuestion {
                        id: q.id,
                        tid: tid.clone(),
                        cid: if cid.is_empty() { None } else { Some(cid.clone()) },
                        question_id: qid.clone(),
                        question_type: q.question_type.unwrap_or_else(|| "single".into()),
                        stem: stem_text,
                        embedding,
                        options: q.options.map(|opts| {
                            opts.into_iter()
                                .map(|o| QaUploadOption { id: o.id, text: o.text })
                                .collect()
                        }),
                        answers,
                    });
                }
            }
        }
    }

    save_vector_cache(cache_path, &cached)?;
    Ok(get_vector_status(cache_path))
}

/// 本地完成 512 维向量计算后搜索题目（结合本地向量缓存余弦相似度与服务端远程搜索）
pub(crate) async fn search_questions_with_local_embedding(
    cache_path: &Path,
    server_url: &str,
    api_key: Option<&str>,
    query: &str,
    limit: usize,
) -> Result<Vec<QaSearchResultItem>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    // 1. 本地计算 512 维特征向量
    let local_vector = compute_local_embedding(trimmed);

    let mut results: Vec<QaSearchResultItem> = Vec::new();
    let mut seen_qids = std::collections::HashSet::new();

    // 2. 本地已下载缓存的向量库检索 (余弦相似度计算)
    let cached = load_vector_cache(cache_path);
    for item in &cached {
        let sim = cosine_similarity(&local_vector, &item.embedding);
        if sim > 0.3 || normalize_stem(&item.stem).contains(&normalize_stem(trimmed)) {
            seen_qids.insert(item.question_id.clone());
            results.push(QaSearchResultItem {
                question_id: item.question_id.clone(),
                tid: Some(item.tid.clone()),
                cid: item.cid.clone(),
                stem: item.stem.clone(),
                question_type: item.question_type.clone(),
                similarity: sim,
                answers: item.answers.clone(),
                source: "本地题库向量检索".into(),
            });
        }
    }

    // 3. 远程调用 POST /api/questions/search 加速搜索（传递本地向量，服务端免计算直接余弦排序）
    let base = server_url.trim_end_matches('/');
    let url = format!("{base}/api/questions/search");
    let client = http_client()?;
    let search_payload = serde_json::json!({
        "q": trimmed,
        "embedding": local_vector,
        "limit": limit.min(20),
    });

    let mut req = client.post(&url).json(&search_payload);
    if let Some(key) = api_key {
        let trimmed_key = key.trim();
        if !trimmed_key.is_empty() {
            req = req.header("x-api-key", trimmed_key);
        }
    }

    if let Ok(resp) = req.send().await {
        if resp.status().is_success() {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                if let Some(list) = data["results"].as_array() {
                    for row in list {
                        let qid = row["questionId"]
                            .as_str()
                            .or_else(|| row["question_id"].as_str())
                            .or_else(|| row["id"].as_i64().map(|id| id.to_string().leak() as &str))
                            .unwrap_or("")
                            .to_string();

                        if !qid.is_empty() && !seen_qids.contains(&qid) {
                            seen_qids.insert(qid.clone());
                            let sim = row["similarity"].as_f64().unwrap_or(0.85);
                            let stem = row["stem"].as_str().unwrap_or("").to_string();
                            let q_type = row["type"].as_str().unwrap_or("single").to_string();
                            results.push(QaSearchResultItem {
                                question_id: qid,
                                tid: row["tid"].as_str().map(str::to_string),
                                cid: row["cid"].as_str().map(str::to_string),
                                stem,
                                question_type: q_type,
                                similarity: sim,
                                answers: vec![],
                                source: "服务端向量加速检索".into(),
                            });
                        }
                    }
                }
            }
        }
    }

    // 按相似度倒序排序
    results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_tags_removes_tags_and_preserves_clean_text() {
        assert_eq!(strip_html_tags("<p>题干内容</p>"), "题干内容");
        assert_eq!(strip_html_tags("选项A<br/>"), "选项A");
        assert_eq!(strip_html_tags("<span style='color:red;'>高亮</span>"), "高亮");
        assert_eq!(strip_html_tags("<!-- 注释 -->题干"), "题干");
        assert_eq!(
            strip_html_tags("若 x < y 且 z > 0，则下列结论正确的是？"),
            "若 x < y 且 z > 0，则下列结论正确的是？"
        );
    }

    #[test]
    fn build_qa_upload_question_normalizes_to_option_ids_and_strips_html() {
        // 1. Single choice: HTML stripped from stem and options, answers normalized to option IDs
        let q_choice = Question {
            id: "101".into(),
            text: "<p>以下关于矩阵初等变换的说法中正确的是？</p>".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "<p>初等变换改变矩阵的秩</p>".into() },
                OptionItem { id: "B".into(), text: "<span>初等变换不改变矩阵的秩</span>".into() },
            ],
            blanks: 0,
            images: vec![],
        };
        // Answer given as option text content
        let answers_text = vec!["初等变换不改变矩阵的秩".to_string()];
        let upload_q = build_qa_upload_question(&q_choice, Some(&answers_text), None, false);

        assert_eq!(upload_q.stem.as_deref(), Some("以下关于矩阵初等变换的说法中正确的是？"));
        let opts = upload_q.options.unwrap();
        assert_eq!(opts[0].text, "初等变换改变矩阵的秩");
        assert_eq!(opts[1].text, "初等变换不改变矩阵的秩");
        // MUST BE option ID ["B"], NOT text!
        assert_eq!(upload_q.correct_answers, Some(vec![serde_json::Value::String("B".into())]));

        // 2. Judge question: answer normalized to boolean [true] or [false]
        let q_judge = Question {
            id: "102".into(),
            text: "<p>所有初等函数在其定义域内均连续。</p>".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "正确".into() },
                OptionItem { id: "B".into(), text: "错误".into() },
            ],
            blanks: 0,
            images: vec![],
        };
        let answers_judge = vec!["正确".to_string()];
        let upload_judge = build_qa_upload_question(&q_judge, Some(&answers_judge), None, false);
        assert_eq!(upload_judge.question_type, "judge");
        assert_eq!(upload_judge.correct_answers, Some(vec![serde_json::Value::Bool(true)]));

        // 3. Text question: no options, blanks set
        let q_text = Question {
            id: "103".into(),
            text: "<p>微积分基本公式又称作____公式。</p>".into(),
            kind: Kind::Text,
            options: vec![],
            blanks: 1,
            images: vec![],
        };
        let answers_fill = vec!["牛顿-莱布尼茨".to_string()];
        let upload_text = build_qa_upload_question(&q_text, Some(&answers_fill), None, false);
        assert_eq!(upload_text.question_type, "text");
        assert!(upload_text.options.is_none());
        assert_eq!(upload_text.blanks, Some(1));
        assert_eq!(upload_text.correct_answers, Some(vec![serde_json::Value::String("牛顿-莱布尼茨".into())]));
    }

    #[test]
    fn stem_normalization_and_hashing_matches_specification() {
        let raw = "<p> 关于微积分基本定理，下列说法正确的是？ &nbsp; </p>";
        let normalized = normalize_stem(raw);
        assert_eq!(normalized, "关于微积分基本定理，下列说法正确的是？");
        let hash = hash_stem(raw);
        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_stem("关于微积分基本定理，下列说法正确的是？"));
    }

    #[test]
    fn local_embedding_vector_has_correct_dimension_and_unit_norm() {
        let text1 = "高等数学微积分定理分析";
        let text2 = "高等数学微积分定理证明";
        let text3 = "近代史纲要重大历史事件总结";

        let vec1 = compute_local_embedding(text1);
        let vec2 = compute_local_embedding(text2);
        let vec3 = compute_local_embedding(text3);

        assert_eq!(vec1.len(), EMBEDDING_DIMENSIONS);
        assert_eq!(vec2.len(), EMBEDDING_DIMENSIONS);
        assert_eq!(vec3.len(), EMBEDDING_DIMENSIONS);

        let norm1: f32 = vec1.iter().map(|v| v * v).sum();
        assert!((norm1.sqrt() - 1.0).abs() < 1e-4);

        let sim12 = cosine_similarity(&vec1, &vec2);
        let sim13 = cosine_similarity(&vec1, &vec3);

        // 相似主题的文本余弦相似度应当高于不同主题文本
        assert!(sim12 > sim13);
    }

    #[test]
    fn match_answers_extracts_correct_and_wrong_answers() {
        let qa_questions = vec![
            QaQuestion {
                id: Some(1),
                tid: Some("t1".into()),
                question_id: Some("q1".into()),
                question_type: Some("single".into()),
                stem: Some("1 + 1 = ?".into()),
                stem_hash: Some(hash_stem("1 + 1 = ?")),
                options: Some(vec![
                    QaOption { id: "opt-a".into(), text: "2".into() },
                    QaOption { id: "opt-b".into(), text: "3".into() },
                ]),
                answers: vec![
                    QaAnswer {
                        id: Some(10),
                        values: vec!["opt-a".into()],
                        is_correct: true,
                        confidence: Some(1.0),
                    },
                ],
                stem_embedding: None,
            },
            QaQuestion {
                id: Some(2),
                tid: Some("t1".into()),
                question_id: Some("q2".into()),
                question_type: Some("single".into()),
                stem: Some("<p>地球是平的吗？</p>".into()),
                stem_hash: Some(hash_stem("地球是平的吗？")),
                options: None,
                answers: vec![
                    QaAnswer {
                        id: Some(11),
                        values: vec!["A".into()],
                        is_correct: false,
                        confidence: Some(0.5),
                    },
                ],
                stem_embedding: None,
            },
        ];

        let local_questions = vec![
            Question {
                id: "q1".into(),
                text: "1 + 1 = ?".into(),
                kind: Kind::Single,
                options: vec![
                    OptionItem { id: "opt-a".into(), text: "2".into() },
                    OptionItem { id: "opt-b".into(), text: "3".into() },
                ],
                blanks: 0,
                images: vec![],
            },
            Question {
                id: "q2".into(),
                text: "地球是平的吗？".into(),
                kind: Kind::Single,
                options: vec![
                    OptionItem { id: "A".into(), text: "是".into() },
                    OptionItem { id: "B".into(), text: "否".into() },
                ],
                blanks: 0,
                images: vec![],
            },
        ];

        let result = match_answers(&qa_questions, &local_questions);
        assert_eq!(result.matched_answers.get("q1"), Some(&vec!["opt-a".into()]));
        // q2 为单选/判断且 A 被排除，通过逻辑排除法自动推导 B 为唯一正确答案
        assert_eq!(result.matched_answers.get("q2"), Some(&vec!["B".into()]));
        assert_eq!(result.wrong_options.get("q2"), Some(&vec!["A".into()]));
        assert!(result.reviews.iter().any(|r| r.question_id == "q1" && r.message.contains("命中标准答案")));
        assert!(result.reviews.iter().any(|r| r.question_id == "q2" && r.message.contains("自动推导")));
    }

    #[tokio::test]
    async fn local_bank_and_sync_missing_to_server() {
        use crate::uooc::tests::Server;
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let bank_path = dir.path().join("qa-bank.json");

        let mut answers = Answers::new();
        answers.insert("q1".into(), vec!["A".into()]);
        let questions = vec![Question {
            id: "q1".into(),
            text: "1 + 1 = ?".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "2".into() },
                OptionItem { id: "B".into(), text: "3".into() },
            ],
            blanks: 0,
            images: vec![],
        }];

        // 1. Record passed quiz into local bank
        let recorded = record_passed_quiz(&bank_path, "200", "100", &questions, &answers, None).unwrap();
        assert_eq!(recorded, 1);
        let bank = load_local_bank(&bank_path);
        assert_eq!(bank.len(), 1);
        assert_eq!(bank.get("100:q1").unwrap().uploaded, false);

        // 2. Mock server for match and batch upload
        let match_calls = Arc::new(AtomicUsize::new(0));
        let match_calls_clone = match_calls.clone();
        let upload_calls = Arc::new(AtomicUsize::new(0));
        let upload_calls_clone = upload_calls.clone();

        let server = Server::new(move |request| {
            if request.path == "/api/automation/match" && request.method == "POST" {
                match_calls_clone.fetch_add(1, Ordering::SeqCst);
                return json!({
                    "ok": true,
                    "missingOnServer": [
                        { "cid": "200", "tid": "100", "questionId": "q1" }
                    ]
                });
            }
            if request.path == "/api/upload/batch" && request.method == "POST" {
                upload_calls_clone.fetch_add(1, Ordering::SeqCst);
                let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
                assert_eq!(body["tid"], "100");
                assert_eq!(body["questions"][0]["question_id"], "q1");
                return json!({
                    "ok": true,
                    "results": [{ "question_id": "q1", "action": "added_correct" }]
                });
            }
            json!({ "error": "not found" })
        });

        let uploaded_count = sync_missing_to_server(&bank_path, &server.base, Some("test_key"), Some("200"), Some("100"), None)
            .await
            .unwrap();
        assert_eq!(uploaded_count, 1);
        assert_eq!(match_calls.load(Ordering::SeqCst), 1);
        assert_eq!(upload_calls.load(Ordering::SeqCst), 1);

        // Check local bank marked as uploaded
        let updated_bank = load_local_bank(&bank_path);
        assert_eq!(updated_bank.get("100:q1").unwrap().uploaded, true);
    }

    #[tokio::test]
    async fn device_auth_and_api_key_retrieval_flow() {
        use crate::uooc::tests::Server;
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let poll_count = Arc::new(AtomicUsize::new(0));
        let poll_count_clone = poll_count.clone();

        let server = Server::new(move |request| {
            if request.path == "/api/auth/device/code" && request.method == "POST" {
                return json!({
                    "device_code": "dev-code-12345",
                    "user_code": "ABCD-1234",
                    "verification_uri": "http://localhost:4321/device",
                    "verification_uri_complete": "http://localhost:4321/device?user_code=ABCD-1234",
                    "expires_in": 1800,
                    "interval": 5
                });
            }
            if request.path == "/api/auth/device/token" && request.method == "POST" {
                let call = poll_count_clone.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    // First poll is pending (simulate 400 Bad Request with error: authorization_pending)
                    // Server::new returns 200 by default, so we can check bytes below or return error field
                    return json!({ "error": "authorization_pending" });
                }
                return json!({
                    "access_token": "mock-access-token-xyz",
                    "token_type": "Bearer",
                    "expires_in": 2678400
                });
            }
            if request.path == "/api/auth/api-key/create" && request.method == "POST" {
                assert!(request.headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("authorization") && v.contains("mock-access-token-xyz")));
                return json!({
                    "key": "uooc_device_auth_key_8888",
                    "id": "key_id_1"
                });
            }
            json!({ "error": "unmatched route" })
        });

        // 1. Request device code
        let code_res = request_device_code(&server.base).await.unwrap();
        assert_eq!(code_res.device_code, "dev-code-12345");
        assert_eq!(code_res.user_code, "ABCD-1234");
        assert_eq!(code_res.verification_uri_complete, "http://localhost:4321/device?user_code=ABCD-1234");

        // 2. Poll token endpoint: first call returns pending
        let poll1 = poll_device_token(&server.base, &code_res.device_code).await.unwrap();
        assert_eq!(poll1.status, "pending");

        // 3. Second call returns success with created API Key
        let token_res = poll_device_token(&server.base, &code_res.device_code).await.unwrap();
        assert_eq!(token_res.status, "success");
        assert_eq!(token_res.api_key.as_deref(), Some("uooc_device_auth_key_8888"));
    }

    #[tokio::test]
    async fn manual_upload_all_local_bank() {
        use crate::uooc::tests::Server;
        use serde_json::json;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let bank_path = dir.path().join("qa-bank.json");

        let mut answers1 = Answers::new();
        answers1.insert("q1".into(), vec!["A".into()]);
        let questions1 = vec![Question {
            id: "q1".into(),
            text: "测试单选1".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "选项A".into() },
                OptionItem { id: "B".into(), text: "选项B".into() },
            ],
            blanks: 0,
            images: vec![],
        }];
        record_passed_quiz(&bank_path, "10", "101", &questions1, &answers1, None).unwrap();

        let mut answers2 = Answers::new();
        answers2.insert("q2".into(), vec!["B".into(), "C".into()]);
        let questions2 = vec![Question {
            id: "q2".into(),
            text: "测试多选2".into(),
            kind: Kind::Multiple,
            options: vec![
                OptionItem { id: "A".into(), text: "A".into() },
                OptionItem { id: "B".into(), text: "B".into() },
                OptionItem { id: "C".into(), text: "C".into() },
            ],
            blanks: 0,
            images: vec![],
        }];
        record_passed_quiz(&bank_path, "10", "102", &questions2, &answers2, None).unwrap();

        let upload_calls = Arc::new(AtomicUsize::new(0));
        let upload_calls_clone = upload_calls.clone();

        let server = Server::new(move |request| {
            if request.path == "/api/upload/batch" && request.method == "POST" {
                upload_calls_clone.fetch_add(1, Ordering::SeqCst);
                return json!({ "ok": true, "results": [] });
            }
            json!({ "error": "not found" })
        });

        let uploaded_count = upload_all_local_bank(&bank_path, &server.base, Some("mock-key"), None).await.unwrap();
        assert_eq!(uploaded_count, 2);
        assert_eq!(upload_calls.load(Ordering::SeqCst), 2); // 2 different tids (101 and 102)

        let bank = load_local_bank(&bank_path);
        assert!(bank.get("101:q1").unwrap().uploaded);
        assert!(bank.get("102:q2").unwrap().uploaded);
    }

    #[test]
    fn parse_exam_view_json_and_html_variants() {
        // Test JSON format (with anti-confusion option text extraction)
        let json_data = serde_json::json!({
            "code": 1,
            "data": {
                "questions": [
                    {
                        "id": 2057971114,
                        "stem": "关于微积分基本定理",
                        "type": "single",
                        "options_app": [
                            { "key": "A", "value": "连续函数一定可积" },
                            { "key": "B", "value": "可积函数一定连续" }
                        ],
                        "answer": "A"
                    }
                ]
            }
        });
        let raw_json = serde_json::to_vec(&json_data).unwrap();
        let (qs, ans) = parse_exam_view(&raw_json).unwrap();
        assert_eq!(qs.len(), 1);
        assert_eq!(qs[0].id, "2057971114");
        // Anti-confusion ensures answer is stored as the option text content
        assert_eq!(ans.get("2057971114"), Some(&vec!["连续函数一定可积".into()]));

        // Test HTML format with .queContainer
        let html = r#"
            <div class="exam-review">
                <div class="queContainer" id="que_101">
                    <div class="ti-q">深圳大学建校于哪一年？</div>
                    <label class="ti-a"><input type="radio" value="1980">1980年</label>
                    <label class="ti-a"><input type="radio" value="1983">1983年</label>
                    <div class="rightAnswer">正确答案：B (1983年)</div>
                </div>
            </div>
        "#;
        let (qs_html, ans_html) = parse_exam_view(html.as_bytes()).unwrap();
        assert_eq!(qs_html.len(), 1);
        assert_eq!(qs_html[0].id, "que_101");
        assert_eq!(ans_html.get("que_101"), Some(&vec!["B".into()]));
    }

    #[test]
    fn anti_confusion_text_matching_against_shuffled_options() {
        // 模拟题库中收录的答案：存放的是文本内容 "ALphaGo围棋比赛大获全胜"（或旧选项在 C）
        let qa_questions = vec![QaQuestion {
            id: Some(1),
            tid: Some("t1".into()),
            question_id: Some("2057971114".into()),
            question_type: Some("single".into()),
            stem: Some("人工智能发展的第三次高潮期的典型代表是（   ）。".into()),
            stem_hash: Some(hash_stem("人工智能发展的第三次高潮期的典型代表是（   ）。")),
            options: Some(vec![
                QaOption { id: "A".into(), text: "美国达特茅斯学院“侃谈会”的举办".into() },
                QaOption { id: "B".into(), text: "国际互联网技术的发展".into() },
                QaOption { id: "C".into(), text: "ALphaGo围棋比赛大获全胜".into() },
            ]),
            answers: vec![
                QaAnswer {
                    id: Some(101),
                    // 题库中存的是文本内容（防混淆标准载体）
                    values: vec!["ALphaGo围棋比赛大获全胜".into()],
                    is_correct: true,
                    confidence: Some(1.0),
                },
                QaAnswer {
                    id: Some(102),
                    // 题库中已排除错误选项
                    values: vec!["国际互联网技术的发展".into()],
                    is_correct: false,
                    confidence: Some(0.5),
                },
            ],
            stem_embedding: None,
        }];

        // 当前试卷打开时，UOOC 选项顺序被随机打乱："ALphaGo" 被打乱到了 A，"国际互联网" 被打乱到了 C
        let local_shuffled_questions = vec![Question {
            id: "2057971114".into(),
            text: "人工智能发展的第三次高潮期的典型代表是（   ）。".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "current_A".into(), text: "ALphaGo围棋比赛大获全胜".into() },
                OptionItem { id: "current_B".into(), text: "美国达特茅斯学院“侃谈会”的举办".into() },
                OptionItem { id: "current_C".into(), text: "国际互联网技术的发展".into() },
            ],
            blanks: 0,
            images: vec![],
        }];

        let result = match_answers(&qa_questions, &local_shuffled_questions);
        // 核心验证：通过文本内容反查，正确选中国当前的真实 ID "current_A"，彻底对冲乱序
        assert_eq!(result.matched_answers.get("2057971114"), Some(&vec!["current_A".into()]));
        // 核心验证：错误选项排除也准确反查到了当前被打乱为 "current_C" 的选项
        assert_eq!(result.wrong_options.get("2057971114"), Some(&vec!["current_C".into()]));
    }

    #[test]
    fn parse_and_align_task_paper_with_real_id() {
        let tp_json = serde_json::json!({
            "code": 1,
            "data": {
                "questions": [
                    {
                        "id": 2057971114,
                        "type": "10",
                        "question": "1 + 1 = ?",
                        "options_app": [
                            { "key": "A", "value": "2" },
                            { "key": "B", "value": "3" }
                        ],
                        "answer": "A"
                    }
                ]
            }
        });
        let raw = serde_json::to_vec(&tp_json).unwrap();
        let (tp_questions, tp_answers) = parse_task_paper(&raw).unwrap();
        assert_eq!(tp_questions.len(), 1);
        assert_eq!(tp_questions[0].id, "2057971114");
        assert_eq!(tp_answers.get("2057971114"), Some(&vec!["2".into()]));

        // DOM 解析出的临时 ID（如 q0:answers[1]）
        let mut dom_questions = vec![Question {
            id: "q0:answers[1]".into(),
            text: "1. 1 + 1 = ?".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "A".into(), text: "选项 2".into() },
                OptionItem { id: "B".into(), text: "选项 3".into() },
            ],
            blanks: 0,
            images: vec![],
        }];

        align_paper_with_task_paper(&mut dom_questions, &tp_questions);
        // 题目 ID 对齐为服务端的真实单题 ID
        assert_eq!(dom_questions[0].id, "2057971114");
        // 题干纯净化为 getTaskPaper 里的标准题干
        assert_eq!(dom_questions[0].text, "1 + 1 = ?");
    }

    #[test]
    fn extract_answers_content_converts_ids_to_text() {
        let questions = vec![Question {
            id: "q10".into(),
            text: "测试题".into(),
            kind: Kind::Single,
            options: vec![
                OptionItem { id: "opt_1".into(), text: "正确答案文本".into() },
                OptionItem { id: "opt_2".into(), text: "错误答案文本".into() },
            ],
            blanks: 0,
            images: vec![],
        }];

        let mut answers = Answers::new();
        answers.insert("q10".into(), vec!["opt_1".into()]);

        let content_answers = extract_answers_content(&answers, &questions);
        assert_eq!(content_answers.get("q10"), Some(&vec!["正确答案文本".into()]));
    }
}
