//! Dify 公共聊天（WebApp API）的 provider 接入。
//!
//! 公开分享的 Dify 应用地址形如 `https://host/chatbot/<分享码>`，没有 API
//! Key，也不能选择模型。WebApp API 先用分享码换取 passport，再用 passport
//! 调用 `chat-messages`。这里把阻塞式的 `chat-messages` 包装成 rig 的
//! completion wire：Dify 只接受一段 `query`，因此系统提示与历史消息会被
//! 拼成一段文本；工具、图片等 rig 能力在传输前被降级为占位文本。

use crate::config::{ProviderConfig, Vendor, DIFY_MODEL_ID};
use rig_core::completion::options::{Mapping, OptionFields, OptionMap};
use rig_core::completion::{Accepts, CompletionRequest, FinishReason, Media, ReplayTarget};
use rig_core::error::{EncodeError, ProviderError};
use rig_core::message::{Api, Message};
use rig_core::operation::{Block, Completion, Finish};
use rig_core::providers::internal::wire;
use rig_core::wire::document::{Reassemble, Serves};
use rig_core::wire::{
    Body, Decoder, Descriptor, Encoded, Flow, Framing, Mode, Out, Wire, WireEvent, WireFrame,
};
use serde_json::{Value, json};
use std::time::Duration;

/// rig 描述符里的 provider 名称。
pub const PROVIDER_NAME: &str = "dify";

/// Dify 端用户的本地偏好键：passport 需要一个稳定的 `user_id`，否则每次
/// 拉取都会在实例上新建一个 end_user。
const END_USER_PREF: &str = "ai.dify.end_user";

/// 一个 Dify 公共聊天地址拆出的实例根地址与分享码。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// 实例根地址（可带反向代理前缀），不带尾部 `/`。
    pub api_base: String,
    /// 公开分享码（URL 最后一段）。
    pub app_code: String,
}

impl Endpoint {
    /// 接受 `/chatbot/<分享码>` 与 `/chat/<分享码>` 两种公开地址；query 与
    /// fragment 会被忽略（公开地址常带 `username`/`phone` 输入参数）。
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("请填写 Dify 公共聊天地址".into());
        }
        let url = reqwest::Url::parse(raw).map_err(|_| "Dify 公共聊天地址无效")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err("Dify 公共聊天地址需使用 HTTP 或 HTTPS，且不能包含凭据".into());
        }
        let segments: Vec<&str> = url
            .path_segments()
            .map(|segments| segments.filter(|segment| !segment.is_empty()).collect())
            .unwrap_or_default();
        let position = segments
            .iter()
            .rposition(|segment| matches!(*segment, "chatbot" | "chat"))
            .ok_or_else(|| {
                "请填写 Dify 公共聊天地址，例如 https://dify.example.com/chatbot/<分享码>"
                    .to_string()
            })?;
        if position + 2 != segments.len() {
            return Err("Dify 公共聊天地址应指向 /chatbot/<分享码>".into());
        }
        let app_code = segments[position + 1];
        if app_code.is_empty()
            || !app_code
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        {
            return Err("Dify 分享码格式无效".into());
        }
        let mut api_base = url.origin().ascii_serialization();
        for segment in &segments[..position] {
            api_base.push('/');
            api_base.push_str(segment);
        }
        Ok(Self {
            api_base,
            app_code: app_code.to_string(),
        })
    }

    /// 保存到 provider 的规范化公开地址。
    pub fn chatbot_url(&self) -> String {
        format!("{}/chatbot/{}", self.api_base.trim_end_matches('/'), self.app_code)
    }

    fn passport_url(&self) -> String {
        format!("{}/api/passport", self.api_base.trim_end_matches('/'))
    }

    fn site_url(&self) -> String {
        format!("{}/api/site", self.api_base.trim_end_matches('/'))
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat-messages", self.api_base.trim_end_matches('/'))
    }
}

/// 探测时使用的短超时客户端。
fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    #[cfg(target_os = "android")]
    szu_core::platform_tls::initialize_platform_verifier()?;
    szu_core::netproxy::client_builder()
        .timeout(timeout)
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法初始化 Dify HTTP 客户端".into())
}

fn end_user_id() -> String {
    if let Ok(Some(id)) = szu_core::preferences::load::<String>(END_USER_PREF) {
        let id = id.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    let id = format!("your-szu-life-{}", uuid::Uuid::new_v4());
    let _ = szu_core::preferences::save(END_USER_PREF, &id);
    id
}

/// 用分享码换 passport；失败时只返回 Dify 的错误摘要。
pub async fn fetch_passport(
    client: &reqwest::Client,
    endpoint: &Endpoint,
    user_id: &str,
) -> Result<String, String> {
    let response = client
        .get(endpoint.passport_url())
        .query(&[("user_id", user_id)])
        .header("X-App-Code", &endpoint.app_code)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("无法连接 Dify：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取 Dify 响应：{error}"))?;
    if !status.is_success() {
        return Err(dify_error(status, &body));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|_| "Dify 返回的 passport 格式无效".to_string())?;
    value
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "Dify 未返回 passport".to_string())
}

/// 探测一个 Dify 公共聊天：passport 成功后读取站点信息。公开聊天没有模型
/// 列表，只返回固定的伪模型名。
pub async fn probe(base_url: &str) -> Result<Vec<String>, String> {
    let endpoint = Endpoint::parse(base_url)?;
    let client = client(Duration::from_secs(20))?;
    let user_id = end_user_id();
    let passport = fetch_passport(&client, &endpoint, &user_id).await?;
    let response = client
        .get(endpoint.site_url())
        .header("X-App-Code", &endpoint.app_code)
        .header("X-App-Passport", &passport)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("无法连接 Dify：{error}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("无法读取 Dify 响应：{error}"))?;
    if !status.is_success() {
        return Err(dify_error(status, &body));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|_| "Dify 返回的站点信息格式无效".to_string())?;
    if value.get("app_id").and_then(Value::as_str).is_none() {
        return Err("Dify 返回的站点信息不完整".into());
    }
    Ok(vec![DIFY_MODEL_ID.to_string()])
}

/// 取 Dify 错误体里的 `message`，避免把整页 HTML 抛给前端。
fn dify_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| value.get("error").and_then(Value::as_str))
                .map(str::to_string)
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| {
            let body = body.trim();
            if body.is_empty() {
                "无响应内容".to_string()
            } else {
                body.chars().take(200).collect()
            }
        });
    format!("Dify 请求失败（HTTP {}）：{detail}", status.as_u16())
}

/// 把一次 completion 请求拼成 Dify 能接受的单段 `query`。Dify 公共聊天
/// 不接受 system 消息、工具与历史会话，因此系统提示依次放在最前面。
fn flatten(request: &CompletionRequest) -> String {
    let mut parts = Vec::new();
    for message in &request.chat_history {
        let text = match message.as_ref() {
            Message::System { content } => content.clone(),
            Message::User { content } => content
                .iter()
                .filter_map(|part| part.as_text())
                .collect::<Vec<_>>()
                .join("\n"),
            Message::Assistant(assistant) => assistant.text(),
        };
        let text = text.trim();
        if !text.is_empty() {
            parts.push(text.to_string());
        }
    }
    parts.join("\n\n")
}

/// 阻塞式 `POST /api/chat-messages` 的 wire。
#[derive(Clone, Debug)]
pub struct DifyChat {
    endpoint: Endpoint,
    passport: String,
    user_id: String,
    model: String,
}

impl DifyChat {
    pub fn new(
        endpoint: Endpoint,
        passport: String,
        user_id: String,
        model: impl Into<String>,
    ) -> Self {
        Self {
            endpoint,
            passport,
            user_id,
            model: model.into(),
        }
    }
}

impl Wire for DifyChat {
    type Op = Completion;
    type Payload = Encoded;
    type Frame = WireFrame;
    type Event = Value;
    type Decoder<'id> = DifyDecoder;
    type Reassembler = DifyDocument;

    fn describe(&self) -> Descriptor<'_> {
        Descriptor::new(PROVIDER_NAME)
            .model(self.model.as_str())
            .replay(self)
    }

    fn encode(&self, request: CompletionRequest, _mode: Mode) -> Result<Encoded, EncodeError> {
        let query = flatten(&request);
        if query.trim().is_empty() {
            return Err(EncodeError::request("Dify 公共聊天缺少提示词"));
        }
        let body = serde_json::to_vec(&json!({
            "inputs": {},
            "query": query,
            "response_mode": "blocking",
            "user": self.user_id,
            "auto_generate_name": false,
        }))?;
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(self.endpoint.chat_url())
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .header("x-app-code", &self.endpoint.app_code)
            .header("x-app-passport", &self.passport)
            .body(Body::Bytes(body))?;
        Ok(Encoded::new(request, Framing::Whole))
    }

    fn decoder<'id>(&self) -> Self::Decoder<'id> {
        DifyDecoder
    }
}

impl ReplayTarget for DifyChat {
    fn api(&self) -> Api {
        Api::from_static("dify.chat")
    }

    fn provider(&self) -> &str {
        PROVIDER_NAME
    }

    fn model(&self) -> &str {
        &self.model
    }

    /// 只收纯文本：图片、音视频与工具历史都由适配器降级成文本。
    fn accepts(&self, _model: &str) -> Accepts {
        Accepts {
            user_images: false,
            assistant_images: false,
            tool_result_images: false,
            tools: false,
            foreign_reasoning: false,
        }
    }

    fn encodes(&self, _model: &str, _media: Media<'_>) -> bool {
        false
    }

    fn declares_tools(&self, _request: &CompletionRequest) -> bool {
        false
    }

    fn map_options(&self, _request: &CompletionRequest, fields: OptionFields<'_>) -> OptionMap {
        OptionMap {
            reasoning: Mapping::of(fields.reasoning, omit_option),
            cache: Mapping::of(fields.cache, omit_option),
            service_tier: Mapping::of(fields.service_tier, omit_option),
            verbosity: Mapping::of(fields.verbosity, omit_option),
            parallel_tool_calls: Mapping::of(fields.parallel_tool_calls, omit_option),
            top_p: Mapping::of(fields.top_p, omit_sample),
            seed: Mapping::of(fields.seed, omit_sample),
            stop: Mapping::of_stop(fields.stop, |_| {
                Mapping::Omit("Dify 公共聊天不接受停止词")
            }),
            cache_key: Mapping::unrouted(fields.cache_key),
        }
    }
}

fn omit_option<T>(_: T) -> Mapping {
    Mapping::Omit("Dify 公共聊天不接受该生成参数")
}

fn omit_sample<T>(_: T) -> Mapping {
    Mapping::Omit("Dify 公共聊天不接受采样参数")
}

/// 阻塞式回复只有一个 JSON 文档，字段集固定；`answer` 是正文。
#[derive(Debug, Default)]
pub struct DifyDecoder;

impl<'id> Decoder<'id, Completion> for DifyDecoder {
    type Event = Value;

    fn classify(&self, frame: WireFrame) -> WireEvent<Value> {
        const MARKERS: &[&str] = &["answer", "event", "message_id", "code"];
        wire::classify_marker_keyed_frame(frame.as_str().as_ref(), MARKERS)
    }

    fn decode(&mut self, event: Value, mut out: Out<'id, Completion>) -> Result<Flow, ProviderError> {
        if event.get("code").is_some() || event.get("error").is_some() {
            return Err(ProviderError::from_provider_body(event.to_string()));
        }
        let answer = event
            .get("answer")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if answer.trim().is_empty() {
            return Err(ProviderError::from_provider_body(
                "Dify 返回了空答案".to_string(),
            ));
        }
        out.run(Block::Text, answer)?;
        out.end_run()?;
        let response_id = event
            .get("message_id")
            .and_then(Value::as_str)
            .map(str::to_string);
        Ok(out.end(Finish {
            usage: Default::default(),
            reason: Some(FinishReason::Stop),
            response_id,
            model: None,
            error: None,
        }))
    }
}

/// 把阻塞回复保留为 `raw`。
#[derive(Debug, Default)]
pub struct DifyDocument {
    document: Option<Value>,
}

impl Reassemble<Value> for DifyDocument {
    fn absorb(&mut self, event: &Value) {
        self.document = Some(event.clone());
    }

    fn finish(self) -> Value {
        self.document.unwrap_or(Value::Null)
    }
}

impl Serves<Completion> for DifyDocument {}

/// 根据保存的 provider 建立 rig 模型。passport 需要一次异步请求，因此这个
/// 入口是 async 的；其它协议仍走同步的 [`crate::model::build_provider`]。
pub async fn connect(
    provider: &ProviderConfig,
    model: &str,
) -> Result<rig_core::Model<DifyChat, rig_reqwest::ReqwestClient>, String> {
    if provider.vendor != Vendor::Dify {
        return Err("不是 Dify 公共聊天提供商".into());
    }
    let endpoint = Endpoint::parse(&provider.base_url)?;
    let user_id = end_user_id();
    let client = client(Duration::from_secs(20))?;
    let passport = fetch_passport(&client, &endpoint, &user_id).await?;
    let transport = crate::model::chat_transport()?;
    Ok(rig_core::Model::new(
        DifyChat::new(endpoint, passport, user_id, model),
        transport,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn parses_public_chatbot_urls() {
        let endpoint = Endpoint::parse("https://dify.example.com/chatbot/abc123").unwrap();
        assert_eq!(endpoint.api_base, "https://dify.example.com");
        assert_eq!(endpoint.app_code, "abc123");
        assert_eq!(endpoint.chat_url(), "https://dify.example.com/api/chat-messages");

        let endpoint =
            Endpoint::parse("https://example.com/dify/chat/abc_123?username=张三&phone=1").unwrap();
        assert_eq!(endpoint.api_base, "https://example.com/dify");
        assert_eq!(endpoint.app_code, "abc_123");
        assert_eq!(
            endpoint.chatbot_url(),
            "https://example.com/dify/chatbot/abc_123"
        );
    }

    #[test]
    fn rejects_non_chatbot_urls() {
        assert!(Endpoint::parse("").is_err());
        assert!(Endpoint::parse("https://dify.example.com").is_err());
        assert!(Endpoint::parse("https://dify.example.com/chatbot").is_err());
        assert!(Endpoint::parse("https://dify.example.com/chatbot/a/b").is_err());
        assert!(Endpoint::parse("https://user:pass@dify.example.com/chatbot/abc").is_err());
        assert!(Endpoint::parse("file:///tmp/chatbot/abc").is_err());
        assert!(Endpoint::parse("https://dify.example.com/chatbot/中文").is_err());
    }

    #[test]
    fn flattens_system_and_history_into_one_query() {
        let request = CompletionRequest::new("请回答")
            .preamble("你是答题助手");
        let query = flatten(&request);
        assert!(query.contains("你是答题助手"));
        assert!(query.contains("请回答"));
        assert!(query.find("你是答题助手") < query.find("请回答"));
    }

    #[tokio::test]
    async fn blocking_chat_round_trip() {
        use rig_core::message::AssistantMessage;
        use std::io::Write;
        use std::net::TcpListener;
        use std::sync::{Arc, Mutex};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let requests: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            seen.lock().unwrap().push(request);
            let body = r#"{"event":"message","answer":"答案是 A","message_id":"m1"}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        });

        let endpoint = Endpoint {
            api_base: format!("http://{address}"),
            app_code: "abc123".into(),
        };
        let model = rig_core::Model::new(
            DifyChat::new(endpoint, "passport".into(), "user".into(), DIFY_MODEL_ID),
            crate::model::chat_transport().unwrap(),
        );
        let response = model
            .call(CompletionRequest::new("请回答").preamble("你是助手"))
            .await
            .unwrap();
        assert_eq!(
            AssistantMessage::new(response.choice).text(),
            "答案是 A"
        );
        worker.join().unwrap();

        let request = requests.lock().unwrap()[0].clone();
        let head = request.to_ascii_lowercase();
        assert!(head.contains("post /api/chat-messages"));
        assert!(head.contains("x-app-code: abc123"));
        assert!(head.contains("x-app-passport: passport"));
        assert!(request.contains(r#""response_mode":"blocking""#));
        assert!(request.contains("你是助手"));
        assert!(request.contains("请回答"));
    }

    /// 真实网络验证：Dify 公共聊天的阻塞式 `chat-messages` 契约。默认忽略，
    /// 手动执行会消耗学校实例的模型配额。
    #[tokio::test]
    #[ignore = "真实网络与 Dify 配额：手动验证阻塞式 chat-messages 契约时运行"]
    async fn live_blocking_chat() {
        use rig_core::message::AssistantMessage;

        let provider = ProviderConfig {
            id: crate::config::BUILTIN_DIFY_PROVIDER_ID.into(),
            vendor: Vendor::Dify,
            name: "UOOC 智能小优".into(),
            api_key: String::new(),
            base_url: crate::config::BUILTIN_DIFY_BASE_URL.into(),
            builtin: true,
            app_code: Some(crate::config::BUILTIN_DIFY_APP_CODE.into()),
            models: vec![crate::config::builtin_dify_model()],
        };
        let model = connect(&provider, DIFY_MODEL_ID).await.unwrap();
        let response = model
            .call(CompletionRequest::new("请只回复：ok"))
            .await
            .unwrap();
        assert!(!AssistantMessage::new(response.choice).text().trim().is_empty());
    }

    /// 读到完整请求（含 Content-Length 指定的正文）。
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            let read = stream.read(&mut buf).unwrap_or(0);
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buf[..read]);
            let Some(head_end) = data.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let head = String::from_utf8_lossy(&data[..head_end]).to_ascii_lowercase();
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if data.len() >= head_end + 4 + length {
                break;
            }
        }
        String::from_utf8_lossy(&data).to_string()
    }
}
