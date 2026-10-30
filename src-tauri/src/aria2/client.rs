use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Serialize)]
struct JsonRpcRequest<T: Serialize> {
    jsonrpc: &'static str,
    id: String,
    method: &'static str,
    params: T,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct JsonRpcEnvelope {
    id: Option<String>,
    result: Option<serde_json::Value>,
    error: Option<JsonRpcError>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aria2Status {
    pub status: String,
    #[serde(default)]
    pub total_length: String,
    #[serde(default)]
    pub completed_length: String,
    #[serde(default)]
    pub download_speed: String,
    pub error_message: Option<String>,
    #[serde(default)]
    pub piece_length: String,
    #[serde(default)]
    pub num_pieces: String,
    pub bitfield: Option<String>,
    #[serde(default)]
    pub connections: String,
}

impl Aria2Status {
    pub fn completed_bytes(&self) -> u64 {
        self.completed_length.parse().unwrap_or(0)
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_length.parse().unwrap_or(0)
    }

    pub fn speed_bytes_per_sec(&self) -> u64 {
        self.download_speed.parse().unwrap_or(0)
    }

    pub fn piece_length(&self) -> u64 {
        self.piece_length.parse().unwrap_or(0)
    }

    pub fn num_pieces(&self) -> u32 {
        self.num_pieces.parse().unwrap_or(0)
    }

    pub fn connections(&self) -> u32 {
        self.connections.parse().unwrap_or(0)
    }
}

pub struct Aria2WebSocketClient {
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    token: String,
}

impl Aria2WebSocketClient {
    pub async fn connect(url: &str, secret: &str) -> Result<Self, String> {
        let mut last_error = String::new();
        for _ in 0..20 {
            match connect_async(url).await {
                Ok((ws_stream, _)) => {
                    return Ok(Self {
                        ws_stream,
                        token: format!("token:{secret}"),
                    });
                }
                Err(e) => {
                    last_error = e.to_string();
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
        Err(format!("连接 aria2c WebSocket 失败 ({url}): {last_error}"))
    }

    pub async fn call_raw<P: Serialize>(
        &mut self,
        method: &'static str,
        params: P,
    ) -> Result<serde_json::Value, String> {
        let id = format!("req-{}", REQUEST_COUNTER.fetch_add(1, Ordering::SeqCst));
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: id.clone(),
            method,
            params,
        };

        let json_text = serde_json::to_string(&request)
            .map_err(|e| format!("序列化 JSON-RPC 请求失败: {e}"))?;

        self.ws_stream
            .send(Message::Text(json_text.into()))
            .await
            .map_err(|e| format!("发送 WebSocket 消息失败: {e}"))?;

        let timeout = Duration::from_secs(10);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!("等待 aria2c 响应超时 ({method})"));
            }

            match tokio::time::timeout(remaining, self.ws_stream.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    if let Ok(envelope) = serde_json::from_str::<JsonRpcEnvelope>(&text) {
                        if envelope.id.as_deref() == Some(&id) {
                            if let Some(err) = envelope.error {
                                return Err(format!("aria2c 错误 ({}): {}", err.code, err.message));
                            }
                            return Ok(envelope.result.unwrap_or(serde_json::Value::Null));
                        }
                    }
                }
                Ok(Some(Ok(Message::Close(_)))) => {
                    return Err("aria2c WebSocket 连接已关闭".to_string());
                }
                Ok(Some(Err(e))) => {
                    return Err(format!("读取 aria2c WebSocket 错误: {e}"));
                }
                Ok(None) => {
                    return Err("aria2c WebSocket 意外结束".to_string());
                }
                Err(_) => {
                    return Err(format!("等待 aria2c 响应超时 ({method})"));
                }
                _ => {}
            }
        }
    }

    pub async fn add_uri(
        &mut self,
        uris: &[String],
        dir: &Path,
        filename: &str,
    ) -> Result<String, String> {
        let mut options = HashMap::new();
        options.insert("dir".to_string(), dir.to_string_lossy().to_string());
        options.insert("out".to_string(), filename.to_string());
        options.insert("continue".to_string(), "true".to_string());

        let params = (self.token.clone(), uris, options);
        let result = self.call_raw("aria2.addUri", params).await?;

        let gid = result
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| "addUri 返回值中未包含有效的 GID".to_string())?;

        Ok(gid)
    }

    pub async fn add_metalink(
        &mut self,
        metalink_base64: &str,
        dir: &Path,
    ) -> Result<String, String> {
        let mut options = HashMap::new();
        options.insert("dir".to_string(), dir.to_string_lossy().to_string());
        options.insert("continue".to_string(), "true".to_string());

        let params = (self.token.clone(), metalink_base64, options);
        let result = self.call_raw("aria2.addMetalink", params).await?;

        let gid = result
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "addMetalink 返回值中未包含有效的 GID".to_string())?;

        Ok(gid)
    }

    pub async fn tell_status(&mut self, gid: &str) -> Result<Aria2Status, String> {
        let keys = vec![
            "status",
            "totalLength",
            "completedLength",
            "downloadSpeed",
            "errorMessage",
            "errorCode",
            "pieceLength",
            "numPieces",
            "bitfield",
            "connections",
        ];
        let params = (self.token.clone(), gid, keys);
        let result = self.call_raw("aria2.tellStatus", params).await?;
        serde_json::from_value::<Aria2Status>(result)
            .map_err(|e| format!("解析 aria2c 任务状态失败: {e}"))
    }

    pub async fn remove(&mut self, gid: &str) -> Result<(), String> {
        let params = (self.token.clone(), gid);
        let _ = self.call_raw("aria2.remove", params).await;
        Ok(())
    }
}
