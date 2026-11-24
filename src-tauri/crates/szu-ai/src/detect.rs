use crate::config::{
    base_candidates, normalize_base_url, DetectionView, Protocol, Vendor, MAX_MODELS,
};
use crate::model::detection_transport;
use futures_util::future::join_all;
use rig_core::model::ModelList;
use rig_core::providers::{
    anthropic::AnthropicConfig, gemini::GeminiConfig, ollama::OllamaConfig, openai::OpenAIConfig,
};
use rig_reqwest::ReqwestClient;
use std::time::Duration;

const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// 从 baseUrl + key 自动探测协议。已知 host 只用来排列探测优先级；最终仍以
/// provider 自己的模型列表/凭据校验是否成功为准。
pub async fn detect_with_key(
    base_url: &str,
    api_key: &str,
    protocol: Option<Protocol>,
) -> Result<DetectionView, String> {
    let protocols = candidate_protocols(base_url, protocol);
    let transport = detection_transport()?;
    let probes = protocols.iter().copied().map(|protocol| {
        let transport = transport.clone();
        async move {
            let result = tokio::time::timeout(
                PROBE_TIMEOUT,
                probe_protocol(protocol, base_url, api_key, transport),
            )
            .await;
            let result = match result {
                Ok(result) => result,
                Err(_) => Err("探测超时".to_string()),
            };
            (protocol, result)
        }
    });
    let mut results = join_all(probes).await;
    // join_all 保持输入顺序；按 candidate_protocols 的优先级取第一个成功项。
    for (protocol, result) in &results {
        if let Ok((base_url, models, verified)) = result {
            let vendor = Vendor::infer(*protocol, base_url);
            let models = if vendor == Vendor::Dify {
                let endpoint = crate::dify::Endpoint::parse(base_url)?;
                vec![crate::config::dify_model(
                    &endpoint.app_code,
                    &endpoint.chatbot_url(),
                )]
            } else {
                models
                    .iter()
                    .map(|model| crate::capabilities::model_config(vendor, model, base_url))
                    .collect()
            };
            return Ok(DetectionView {
                protocol: *protocol,
                vendor,
                base_url: base_url.clone(),
                models,
                verified: *verified,
            });
        }
    }
    let details = results
        .drain(..)
        .map(|(protocol, result)| {
            let error = result.err().unwrap_or_else(|| "未知错误".into());
            format!("{}: {error}", protocol.label())
        })
        .collect::<Vec<_>>()
        .join("；");
    Err(format!(
        "无法自动识别协议，请检查地址、Key 或手动选择协议。{details}"
    ))
}

fn candidate_protocols(base_url: &str, manual: Option<Protocol>) -> Vec<Protocol> {
    if let Some(protocol) = manual {
        return vec![protocol];
    }
    // `/chatbot/<分享码>` 或 `/chat/<分享码>` 只可能是 Dify 公共聊天，先于
    // 其它按 host 猜测的规则命中。
    if crate::dify::Endpoint::parse(base_url).is_ok() {
        return vec![Protocol::DifyChat];
    }
    let url = reqwest::Url::parse(base_url.trim()).ok();
    let host = url
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let port = url.as_ref().and_then(|url| url.port());
    if host.contains("anthropic") {
        return vec![
            Protocol::Anthropic,
            Protocol::OpenAiCompatible,
            Protocol::Gemini,
            Protocol::Ollama,
        ];
    }
    if host.contains("generativelanguage") || base_url.contains("v1beta") {
        return vec![
            Protocol::Gemini,
            Protocol::OpenAiCompatible,
            Protocol::Anthropic,
            Protocol::Ollama,
        ];
    }
    if port == Some(11434) || host == "localhost" || host == "127.0.0.1" {
        return vec![
            Protocol::Ollama,
            Protocol::OpenAiCompatible,
            Protocol::Anthropic,
            Protocol::Gemini,
        ];
    }
    vec![
        Protocol::OpenAiCompatible,
        Protocol::Anthropic,
        Protocol::Gemini,
        Protocol::Ollama,
    ]
}

async fn probe_protocol(
    protocol: Protocol,
    raw_base_url: &str,
    api_key: &str,
    transport: ReqwestClient,
) -> Result<(String, Vec<String>, bool), String> {
    let base_url = normalize_base_url(raw_base_url, protocol)?;
    let mut last_error = String::new();
    for candidate in base_candidates(protocol, &base_url) {
        match probe_one(protocol, &candidate, api_key, transport.clone()).await {
            Ok((models, verified)) => return Ok((candidate, models, verified)),
            Err(error) => last_error = error,
        }
    }
    Err(if last_error.is_empty() {
        "未找到可用端点".into()
    } else {
        last_error
    })
}

async fn probe_one(
    protocol: Protocol,
    base_url: &str,
    api_key: &str,
    transport: ReqwestClient,
) -> Result<(Vec<String>, bool), String> {
    let key = api_key.trim();
    match protocol {
        Protocol::OpenAiCompatible => {
            let client = OpenAIConfig::new(key)
                .with_base_url(base_url)
                .connect(transport);
            match client.list_models().await {
                Ok(models) => Ok((models_of(models), true)),
                Err(error) => match client.verify().await {
                    Ok(()) => Ok((Vec::new(), true)),
                    Err(_) => Err(brief(error.to_string())),
                },
            }
        }
        Protocol::Anthropic => {
            let client = AnthropicConfig::new(key)
                .with_base_url(base_url)
                .connect(transport);
            match client.list_models().await {
                Ok(models) => Ok((models_of(models), true)),
                Err(error) => match client.verify().await {
                    Ok(()) => Ok((Vec::new(), true)),
                    Err(_) => Err(brief(error.to_string())),
                },
            }
        }
        Protocol::Gemini => {
            let client = GeminiConfig::new(key)
                .with_base_url(base_url)
                .connect(transport);
            match client.list_models().await {
                Ok(models) => Ok((models_of(models), true)),
                Err(error) => match client.verify().await {
                    Ok(()) => Ok((Vec::new(), true)),
                    Err(_) => Err(brief(error.to_string())),
                },
            }
        }
        Protocol::Ollama => {
            let client = OllamaConfig::new()
                .with_base_url(base_url)
                .with_api_key(key)
                .connect(transport);
            client
                .list_models()
                .await
                .map(|models| (models_of(models), true))
                .map_err(|error| brief(error.to_string()))
        }
        Protocol::DifyChat => crate::dify::probe(base_url)
            .await
            .map(|models| (models, true))
            .map_err(brief),
    }
}

fn models_of(models: ModelList) -> Vec<String> {
    let mut names: Vec<String> = models
        .into_iter()
        .map(|model| model.id.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect();
    names.sort();
    names.dedup();
    names.truncate(MAX_MODELS);
    names
}

fn brief(mut message: String) -> String {
    if message.len() > 240 {
        message.truncate(240);
        message.push('…');
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    struct Server {
        base: String,
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<()>>,
        paths: Arc<Mutex<Vec<String>>>,
    }

    impl Server {
        fn new(handler: impl Fn(&str) -> String + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let paths = Arc::new(Mutex::new(Vec::new()));
            let seen = paths.clone();
            let handler = Arc::new(handler);
            let worker = thread::spawn(move || {
                while !stopping.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let mut buf = [0u8; 8192];
                            let n = stream.read(&mut buf).unwrap_or(0);
                            let text = String::from_utf8_lossy(&buf[..n]).to_string();
                            let path = text
                                .lines()
                                .next()
                                .and_then(|line| line.split_whitespace().nth(1))
                                .unwrap_or("/")
                                .to_string();
                            seen.lock().unwrap().push(path.clone());
                            let body = handler(&path);
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes());
                        }
                        Err(_) => thread::sleep(Duration::from_millis(2)),
                    }
                }
            });
            Self {
                base,
                stop,
                thread: Some(worker),
                paths,
            }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(worker) = self.thread.take() {
                let _ = worker.join();
            }
        }
    }

    #[tokio::test]
    async fn detects_openai_compatible_models() {
        let server = Server::new(|_| {
            r#"{"object":"list","data":[{"id":"model-b","object":"model"},{"id":"model-a","object":"model"}]}"#
                .into()
        });
        let view = detect_with_key(&server.base, "key", Some(Protocol::OpenAiCompatible))
            .await
            .unwrap();
        assert_eq!(view.protocol, Protocol::OpenAiCompatible);
        assert_eq!(view.vendor, Vendor::CustomEndpoint);
        assert_eq!(
            view.models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["model-a", "model-b"]
        );
        assert!(server
            .paths
            .lock()
            .unwrap()
            .iter()
            .any(|path| path.ends_with("/models")));
    }

    #[tokio::test]
    async fn detects_ollama_tags_without_key() {
        let server = Server::new(|_| {
            r#"{"models":[{"model":"qwen3:8b","name":"qwen3:8b"},{"model":"llama3:8b","name":"llama3:8b"}]}"#
                .into()
        });
        let view = detect_with_key(&server.base, "", Some(Protocol::Ollama))
            .await
            .unwrap();
        assert_eq!(view.protocol, Protocol::Ollama);
        assert_eq!(view.vendor, Vendor::Ollama);
        assert_eq!(
            view.models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            vec!["llama3:8b", "qwen3:8b"]
        );
        assert!(view.models[0].tool_calling);
        assert!(!view.models[0].vision);
        assert!(server
            .paths
            .lock()
            .unwrap()
            .iter()
            .any(|path| path == "/api/tags"));
    }

    #[tokio::test]
    async fn detects_dify_public_chat() {
        let server = Server::new(|path| {
            if path.starts_with("/api/passport") {
                r#"{"access_token":"passport-token"}"#.into()
            } else {
                r#"{"app_id":"app-id","site":{"title":"UOOC"}}"#.into()
            }
        });
        let view = detect_with_key(&format!("{}/chatbot/abc123", server.base), "", None)
            .await
            .unwrap();
        assert_eq!(view.protocol, Protocol::DifyChat);
        assert_eq!(view.vendor, Vendor::Dify);
        assert_eq!(view.models.len(), 1);
        assert_eq!(view.models[0].id, "chatbot");
        assert_eq!(view.models[0].name, "dify-chatbot-abc123");
        assert!(!view.models[0].tool_calling);
        assert!(!view.models[0].vision);
        assert_eq!(view.models[0].url, format!("{}/chatbot/abc123", server.base));
        assert!(view.verified);
        assert_eq!(view.base_url, format!("{}/chatbot/abc123", server.base));
        let paths = server.paths.lock().unwrap();
        assert!(paths
            .iter()
            .any(|path| path.starts_with("/api/passport?user_id=")));
        assert!(paths.iter().any(|path| path == "/api/site"));
    }

    #[tokio::test]
    async fn reports_failure_for_unreachable_endpoint() {
        let error = detect_with_key(
            "http://127.0.0.1:1",
            "key",
            Some(Protocol::OpenAiCompatible),
        )
        .await
        .unwrap_err();
        assert!(error.contains("无法自动识别协议"));
    }
}
