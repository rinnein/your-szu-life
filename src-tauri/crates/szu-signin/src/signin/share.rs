//! 签到码共享房间（szu-mesh 临时 EasyTier 房间之上的业务层）。
//!
//! 房间按**课程**隔离：`your-szu-life:study/signin/lms:<course_id>`，密钥取房间名
//! 的 SHA-256（本质是广播，无保密需求）。一门课会先后出现数字签到与二维码签到，
//! 因此同一条消息通道同时承载两种码：
//!
//! ```json
//! { "course_id": 42, "rollcall_id": 123,
//!   "code": { "kind": "number", "rollcall_id": 123, "code": "0837" },
//!   "shared_at_ms": 1700000000000 }
//! ```
//!
//! 有效期规则：
//! - 二维码：自带时间（`create_time`）优先，否则用共享时间，加 15s；轮换后每次
//!   都是新 `data`，因此每份码各自计时。
//! - 数字码：**不做时间过期**，只随点名结束失效——房间由探测循环按「时间窗 ∩
//!   可答点名」维护，窗口结束即销毁。`seen` 只保证同一份码不重复提交。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use sha2::{Digest, Sha256};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Service;

/// 二维码有效期（毫秒）：教师端二维码轮换周期，过期即失效。
const QR_TTL_MS: i64 = 15_000;

/// 二维码 payload：QR 内容是 JSON `{rollcallId, courseId, data, ...}`
/// （H5 scanner 页 4853 确认），提交时只取 `data` 原样字段；create_time/hash
/// 由服务端从 `data` 校验，前端不感知。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct QrFields {
    pub rollcall_id: i64,
    #[serde(default)]
    pub course_id: Option<i64>,
    pub data: String,
    /// 部分 QR JSON 可能自带生成时间（秒/毫秒）；缺省时用共享时间兜底。
    #[serde(default)]
    pub create_time: i64,
}

/// 房间内可共享的签到码。同一条通道承载两种签到方式，由 `kind` 区分。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SharedCode {
    /// 数字签到码（无时间有效期，随点名结束失效）。
    Number { rollcall_id: i64, code: String },
    /// 二维码原始 data（15s 有效期，教师端轮换）。
    Qr {
        rollcall_id: i64,
        data: String,
        #[serde(default)]
        create_time: i64,
    },
}

impl SharedCode {
    pub fn rollcall_id(&self) -> i64 {
        match self {
            SharedCode::Number { rollcall_id, .. } | SharedCode::Qr { rollcall_id, .. } => {
                *rollcall_id
            }
        }
    }

    /// 前端事件里的种类标记（`number` / `qr`）。
    pub fn kind(&self) -> &'static str {
        match self {
            SharedCode::Number { .. } => "number",
            SharedCode::Qr { .. } => "qr",
        }
    }

    /// 截止时刻：数字码不过期，二维码沿用 `create_time`/共享时间 + 15s。
    pub fn valid_until_ms(&self, shared_at_ms: i64) -> i64 {
        match self {
            SharedCode::Number { .. } => i64::MAX,
            SharedCode::Qr { create_time, .. } => {
                let start = if *create_time > 1_000_000_000_000 {
                    *create_time
                } else if *create_time > 1_000_000_000 {
                    // 秒级时间戳（畅课 create_time）。
                    *create_time * 1000
                } else {
                    shared_at_ms
                };
                start + QR_TTL_MS
            }
        }
    }

    pub fn is_expired(&self, shared_at_ms: i64, now_ms: i64) -> bool {
        self.valid_until_ms(shared_at_ms) <= now_ms
    }

    /// 去重键：种类 + 点名 + 码值。数字码与二维码 data 不共用前缀，避免碰撞。
    pub fn message_id(&self) -> String {
        let mut hasher = Sha256::new();
        match self {
            SharedCode::Number { rollcall_id, code } => {
                hasher.update(format!("number:{rollcall_id}:{code}").as_bytes());
            }
            SharedCode::Qr {
                rollcall_id, data, ..
            } => {
                hasher.update(format!("qr:{rollcall_id}:{data}").as_bytes());
            }
        }
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn from_qr_fields(fields: &QrFields) -> Self {
        SharedCode::Qr {
            rollcall_id: fields.rollcall_id,
            data: fields.data.clone(),
            create_time: fields.create_time,
        }
    }
}

impl From<&QrFields> for SharedCode {
    fn from(fields: &QrFields) -> Self {
        Self::from_qr_fields(fields)
    }
}

/// 房间消息的业务负载。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SharedPayload {
    /// 房间所属课程；接收方必须校验与自身课程一致。
    pub course_id: i64,
    /// 发起共享时房间指向的点名（可能为空：手动开共享时未绑定点名）。
    #[serde(default)]
    pub rollcall_id: Option<i64>,
    pub code: SharedCode,
    pub shared_at_ms: i64,
}

impl SharedPayload {
    /// 本房间是否应当处理这条消息（按课程隔离）。
    pub fn accepts(&self, course_id: i64) -> bool {
        self.course_id == course_id
    }
}

/// 推送给前端的事件（Channel）。
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum ShareEvent {
    RoomJoined { course_id: i64 },
    RoomClosed { course_id: i64 },
    /// 收到并尝试提交了一份共享码（失败不阻断后续匹配）。
    Matched {
        course_id: i64,
        ok: bool,
        message: String,
        /// `number` | `qr`，供前端区分提示文案。
        kind: String,
    },
    /// 共享了一份码。
    Shared {
        course_id: i64,
        ok: bool,
        message: String,
        kind: String,
    },
    Error { message: String },
}

/// 从扫码得到的原始字符串解析二维码字段。
/// 支持 H5 的三种形态（scanner 页 4853 的分发逻辑）：
/// 1. 裸 JSON `{"rollcallId":..,"courseId":..,"data":..}`；
/// 2. 中转 URL `https://host/j?p=<urlencoded JSON>` 或 `?_p=<JSON>`
///    （scanner-jumper），先解包成 JSON 再判别；
/// 3. 直接含 `"rollcallId":` 的 JSON 字符串。
pub fn parse_qr_fields(raw: &str) -> Option<QrFields> {
    let normalized = normalize_scanned(raw)?;
    let value: Value = serde_json::from_str(&normalized).ok()?;
    qr_fields_from_value(&value)
}

/// URL 中转解包：`/j`、`/scanner-jumper` 路径时取 `_p` / `p` 参数。
fn normalize_scanned(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.contains(r#""rollcallId":"#) {
        return Some(raw.to_string());
    }
    let url = reqwest::Url::parse(raw).ok()?;
    let path = url.path();
    if path != "/j" && path != "/scanner-jumper" {
        return None;
    }
    for (key, value) in url.query_pairs() {
        if key == "_p" || key == "p" {
            return Some(value.into_owned());
        }
    }
    None
}

fn qr_fields_from_value(value: &Value) -> Option<QrFields> {
    let data = value
        .get("data")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if data.is_empty() {
        return None;
    }
    let rollcall_id = value
        .get("rollcallId")
        .or_else(|| value.get("rollcall_id"))
        .and_then(Value::as_i64)
        .or_else(|| {
            value
                .get("rollcallId")
                .or_else(|| value.get("rollcall_id"))
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
        })?;
    let course_id = value
        .get("courseId")
        .or_else(|| value.get("course_id"))
        .and_then(Value::as_i64);
    let create_time = value
        .get("create_time")
        .or_else(|| value.get("createTime"))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0);
    Some(QrFields {
        rollcall_id,
        course_id,
        data,
        create_time,
    })
}

/// 二维码是否仍在有效期内（`create_time` 优先，缺省用共享时间）。
pub fn qr_valid_until_ms(fields: &QrFields, shared_at_ms: i64) -> i64 {
    SharedCode::from_qr_fields(fields).valid_until_ms(shared_at_ms)
}

/// 房间名与密钥（广播房间，无保密需求：密钥即房间名的 hash）。
pub fn room_identity(course_id: i64) -> (String, String) {
    let name = format!("your-szu-life:study/signin/lms:{course_id}");
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    let secret: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (name, secret)
}

/// 一个活跃的共享房间会话。
pub struct RoomSession {
    pub course_id: i64,
    pub rollcall_id: Option<i64>,
    /// true = 用户在 UI 里显式开启（不受时间窗自动回收）；false = 探测循环
    /// 按课表窗口自动维护。
    pub manual: bool,
    handle: Arc<szu_mesh::mesh::share_room::RoomHandle>,
    /// 已处理过的消息 id（提交与共享共用一个去重集合）。
    seen: Arc<Mutex<HashSet<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl RoomSession {
    /// 加入房间并启动「收到即提交」的匹配循环。
    pub async fn join(
        service: Arc<Service>,
        course_id: i64,
        rollcall_id: Option<i64>,
    ) -> Result<Self, String> {
        let (name, secret) = room_identity(course_id);
        let device_id = service.device_id()?;
        let hostname = szu_core::device::system_name().unwrap_or_else(|| "SZU-Life".into());
        let handle =
            szu_mesh::mesh::share_room::RoomHandle::join(&name, &secret, &device_id, &hostname)
                .await?;
        let _ = service
            .share_events
            .broadcast(&ShareEvent::RoomJoined { course_id });

        let seen: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let mut rx = handle.subscribe();
        let seen_for_task = seen.clone();
        let weak = Arc::downgrade(&service);
        let device_for_task = device_id.clone();
        let task = tokio::spawn(async move {
            let device_id = device_for_task;
            loop {
                match rx.recv().await {
                    Ok(message) => {
                        let Some(service) = weak.upgrade() else { break };
                        let payload: Result<SharedPayload, _> =
                            serde_json::from_value(message.payload.clone());
                        let Ok(payload) = payload else { continue };
                        if !payload.accepts(course_id) {
                            // 别的课程的码不在这里提交，也不计入去重。
                            continue;
                        }
                        if !seen_for_task.lock().unwrap().insert(message.id) {
                            continue;
                        }
                        let kind = payload.code.kind();
                        let now = chrono::Utc::now().timestamp_millis();
                        if payload.code.is_expired(payload.shared_at_ms, now) {
                            let _ = service.share_events.broadcast(&ShareEvent::Matched {
                                course_id: payload.course_id,
                                ok: false,
                                message: "二维码已过期，等待下一份".into(),
                                kind: kind.into(),
                            });
                            continue;
                        }
                        let result = match &payload.code {
                            SharedCode::Number { rollcall_id, code } => {
                                service.submit_lms_number(*rollcall_id, course_id, code).await
                            }
                            SharedCode::Qr {
                                rollcall_id, data, ..
                            } => service.submit_lms_qr(*rollcall_id, data, &device_id).await,
                        };
                        let _ = service.share_events.broadcast(&ShareEvent::Matched {
                            course_id: payload.course_id,
                            ok: result.is_ok(),
                            message: result.err().unwrap_or_else(|| match kind {
                                "number" => "数字码匹配成功，签到已提交".into(),
                                _ => "二维码匹配成功，签到已提交".into(),
                            }),
                            kind: kind.into(),
                        });
                        // 失败不阻断：继续等下一份共享码。
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(Self {
            course_id,
            rollcall_id,
            manual: false,
            handle: Arc::new(handle),
            seen,
            task,
        })
    }

    /// 发送侧句柄：宿主可在释放房间互斥锁之后再发起共享。
    pub fn share_handle(&self) -> ShareHandle {
        ShareHandle {
            course_id: self.course_id,
            rollcall_id: self.rollcall_id,
            seen: self.seen.clone(),
            handle: self.handle.clone(),
        }
    }

    /// 共享一份码（共享模式不自动提交，共享者自己已经通过手动入口签到）。
    pub async fn share(&self, code: &SharedCode) -> Result<bool, String> {
        let id = code.message_id();
        self.share_handle().share(&id, code).await
    }

    pub async fn leave(self) {
        self.task.abort();
        // 仍有临时句柄时 `Arc` 不会归零；届时实例销毁延后到最后一个句柄释放。
        if let Ok(handle) = Arc::try_unwrap(self.handle) {
            handle.leave().await;
        }
    }
}

/// 共享房间的发送侧句柄：[`Service`] 在释放 `room` 互斥锁之后再调用
/// [`ShareHandle::share`]，避免跨 `await` 持有 `std::sync::MutexGuard`。
#[derive(Clone)]
pub struct ShareHandle {
    pub course_id: i64,
    pub rollcall_id: Option<i64>,
    seen: Arc<Mutex<HashSet<String>>>,
    handle: Arc<szu_mesh::mesh::share_room::RoomHandle>,
}

impl std::fmt::Debug for ShareHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShareHandle")
            .field("course_id", &self.course_id)
            .field("rollcall_id", &self.rollcall_id)
            .finish_non_exhaustive()
    }
}

impl ShareHandle {
    /// 共享一份码：去重 → 广播房间。返回是否为新码。
    pub async fn share(&self, id: &str, code: &SharedCode) -> Result<bool, String> {
        if !self.seen.lock().unwrap().insert(id.to_string()) {
            return Ok(false);
        }
        let now = chrono::Utc::now().timestamp_millis();
        let payload = SharedPayload {
            course_id: self.course_id,
            rollcall_id: self.rollcall_id,
            code: code.clone(),
            shared_at_ms: now,
        };
        self.handle
            .broadcast(szu_mesh::mesh::share_room::RoomMessage {
                id: id.to_string(),
                payload: serde_json::to_value(&payload).unwrap_or(Value::Null),
                sent_at_ms: now,
            })
            .await;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qr_fields(rollcall_id: i64, data: &str) -> QrFields {
        QrFields {
            rollcall_id,
            course_id: None,
            data: data.into(),
            create_time: 0,
        }
    }

    #[test]
    fn parse_qr_fields_accepts_json_and_jumper_url() {
        let json = parse_qr_fields(r#"{"rollcallId":123,"courseId":9,"data":"abc"}"#)
            .expect("裸 JSON 应可解析");
        assert_eq!(json.rollcall_id, 123);
        assert_eq!(json.course_id, Some(9));
        assert_eq!(json.data, "abc");
        let jumper = parse_qr_fields(
            "https://lms.szu.edu.cn/j?p=%7B%22rollcallId%22%3A456%2C%22data%22%3A%22xyz%22%7D",
        )
        .expect("scanner-jumper URL 应可解析");
        assert_eq!(jumper.rollcall_id, 456);
        assert_eq!(jumper.data, "xyz");
        assert!(parse_qr_fields("不是二维码内容").is_none());
        assert!(parse_qr_fields("").is_none());
        assert!(
            parse_qr_fields(r#"{"data":"x"}"#).is_none(),
            "缺 rollcallId 应拒绝"
        );
        assert!(
            parse_qr_fields(r#"{"rollcallId":1}"#).is_none(),
            "缺 data 应拒绝"
        );
    }

    #[test]
    fn validity_uses_qr_create_time_over_shared_time() {
        let fields = QrFields {
            rollcall_id: 1,
            course_id: None,
            data: "d".into(),
            create_time: 1_700_000_000,
        };
        let shared_late = 1_700_000_100_000;
        assert_eq!(
            qr_valid_until_ms(&fields, shared_late),
            1_700_000_000_000 + QR_TTL_MS
        );
        let no_time = QrFields {
            rollcall_id: 1,
            course_id: None,
            data: "d".into(),
            create_time: 0,
        };
        assert_eq!(
            qr_valid_until_ms(&no_time, 5_000_000),
            5_000_000 + QR_TTL_MS
        );
    }

    #[test]
    fn number_code_never_expires_by_time() {
        let code = SharedCode::Number {
            rollcall_id: 7,
            code: "0837".into(),
        };
        assert_eq!(code.valid_until_ms(1_000), i64::MAX);
        assert!(!code.is_expired(0, i64::MAX - 1));
        assert_eq!(code.kind(), "number");
        assert_eq!(code.rollcall_id(), 7);
    }

    #[test]
    fn number_and_qr_message_ids_do_not_collide() {
        let number = SharedCode::Number {
            rollcall_id: 1,
            code: "1234".into(),
        };
        let qr_code = SharedCode::Qr {
            rollcall_id: 1,
            data: "1234".into(),
            create_time: 0,
        };
        assert_ne!(number.message_id(), qr_code.message_id());
        // 同一码重复共享 → 同一 id（去重生效）。
        assert_eq!(
            number.message_id(),
            SharedCode::Number {
                rollcall_id: 1,
                code: "1234".into()
            }
            .message_id()
        );
        // 数字变化 / 点名不同 → 新码。
        assert_ne!(
            number.message_id(),
            SharedCode::Number {
                rollcall_id: 1,
                code: "5678".into()
            }
            .message_id()
        );
        assert_ne!(
            number.message_id(),
            SharedCode::Number {
                rollcall_id: 2,
                code: "1234".into()
            }
            .message_id()
        );
    }

    #[test]
    fn payload_roundtrips_and_is_course_scoped() {
        let payload = SharedPayload {
            course_id: 42,
            rollcall_id: Some(7),
            code: SharedCode::Number {
                rollcall_id: 7,
                code: "0837".into(),
            },
            shared_at_ms: 1_000,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains(r#""kind":"number""#), "{json}");
        let parsed: SharedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, payload.code);
        assert!(parsed.accepts(42));
        assert!(!parsed.accepts(43));

        let qr_payload = SharedPayload {
            code: SharedCode::from(&qr_fields(9, "abc")),
            ..payload
        };
        let json = serde_json::to_string(&qr_payload).unwrap();
        assert!(json.contains(r#""kind":"qr""#), "{json}");
        let parsed: SharedPayload = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.code,
            SharedCode::Qr {
                rollcall_id: 9,
                data: "abc".into(),
                create_time: 0
            }
        );
    }

    #[test]
    fn room_identity_is_course_scoped() {
        let (name_a, secret_a) = room_identity(42);
        let (name_b, secret_b) = room_identity(43);
        assert_eq!(name_a, "your-szu-life:study/signin/lms:42");
        assert_ne!(name_a, name_b);
        assert_ne!(secret_a, secret_b);
        assert_eq!(secret_a.len(), 64);
        assert_eq!(room_identity(42), (name_a, secret_a));
    }

    #[test]
    fn message_id_is_rollcall_scoped_data_hash() {
        let base = message_id(&qr_fields(1, "same"));
        assert_eq!(base.len(), 64);
        assert_eq!(base, message_id(&qr_fields(1, "same")));
        // 不同点名或轮换后的 data 都是新码。
        assert_ne!(base, message_id(&qr_fields(2, "same")));
        assert_ne!(base, message_id(&qr_fields(1, "rotated")));
    }
}

/// 二维码去重键（旧调用点与测试沿用的入口）。
pub fn message_id(fields: &QrFields) -> String {
    SharedCode::from_qr_fields(fields).message_id()
}
