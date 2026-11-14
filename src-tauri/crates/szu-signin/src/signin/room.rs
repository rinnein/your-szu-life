//! 签到码共享房间（szu-mesh 临时 EasyTier 房间之上的业务层）。
//!
//! 房间名 `your-szu-life:study/signin/lms/qr:<course_id>`，密钥取房间名的
//! SHA-256（本质是广播，无保密需求）。消息 payload 由本模块定义：
//!
//! ```json
//! { "kind": "lms_qr", "course_id": 42, "rollcall_id": 123,
//!   "qr": { "data": "...", "create_time": 1700000000, "hash": "..." },
//!   "shared_at_ms": 1700000000000 }
//! ```
//!
//! 过期检测：二维码自带时间（`create_time`）优先，否则用共享时间；两者都
//! 加 15s 有效期，过期后不再发起提交尝试（避免无效请求刷接口）。

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

/// 房间消息的业务负载。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QrSharePayload {
    /// 消息种类，目前只有 `lms_qr`。
    pub kind: String,
    pub course_id: i64,
    pub rollcall_id: Option<i64>,
    pub qr: QrFields,
    pub shared_at_ms: i64,
}

/// 推送给前端的事件（Channel）。
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum QrEvent {
    RoomJoined { course_id: i64 },
    RoomClosed { course_id: i64 },
    /// 收到并尝试提交了一份共享码（失败不阻断后续匹配）。
    Matched { course_id: i64, ok: bool, message: String },
    /// 共享了一份码。
    Shared { course_id: i64, ok: bool, message: String },
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

/// 二维码是否仍在有效期内：`create_time` 优先，缺省用共享时间。
pub fn qr_valid_until_ms(fields: &QrFields, shared_at_ms: i64) -> i64 {
    let start = if fields.create_time > 1_000_000_000_000 {
        fields.create_time
    } else if fields.create_time > 1_000_000_000 {
        // 秒级时间戳（畅课 create_time）。
        fields.create_time * 1000
    } else {
        shared_at_ms
    };
    start + QR_TTL_MS
}

/// 房间名与密钥（广播房间，无保密需求：密钥即房间名的 hash）。
pub fn room_identity(course_id: i64) -> (String, String) {
    let name = format!("your-szu-life:study/signin/lms/qr:{course_id}");
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    let secret: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (name, secret)
}

/// 消息去重键：对 rollcallId+data 做 hash（二维码轮换后 data 变化）。
pub fn message_id(fields: &QrFields) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", fields.rollcall_id, fields.data).as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 一个活跃的共享房间会话。
pub struct RoomSession {
    pub course_id: i64,
    pub rollcall_id: Option<i64>,
    /// true = 用户在 UI 里显式开启（不受时间窗自动回收）；false = 探测循环
    /// 按课表窗口自动维护。
    pub manual: bool,
    handle: szu_mesh::mesh::qr_room::RoomHandle,
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
            szu_mesh::mesh::qr_room::RoomHandle::join(&name, &secret, &device_id, &hostname)
                .await?;
        let _ = service
            .qr_events
            .broadcast(&QrEvent::RoomJoined { course_id });

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
                        let payload: Result<QrSharePayload, _> =
                            serde_json::from_value(message.payload.clone());
                        let Ok(payload) = payload else { continue };
                        if payload.kind != "lms_qr" {
                            continue;
                        }
                        if !seen_for_task.lock().unwrap().insert(message.id) {
                            continue;
                        }
                        let now = chrono::Utc::now().timestamp_millis();
                        if qr_valid_until_ms(&payload.qr, payload.shared_at_ms) <= now {
                            let _ = service.qr_events.broadcast(&QrEvent::Matched {
                                course_id: payload.course_id,
                                ok: false,
                                message: "签到码已过期，等待下一份".into(),
                            });
                            continue;
                        }
                        let result = service
                            .submit_lms_qr(
                                payload.qr.rollcall_id,
                                &payload.qr.data,
                                &device_id,
                            )
                            .await;
                        let _ = service.qr_events.broadcast(&QrEvent::Matched {
                            course_id: payload.course_id,
                            ok: result.is_ok(),
                            message: result
                                .err()
                                .unwrap_or_else(|| "匹配成功，签到已提交".into()),
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
            handle,
            seen,
            task,
        })
    }

    /// 共享一份扫码结果：去重 → 广播房间。返回是否为新码。
    /// 共享模式不自动提交（共享者自己已通过「扫码」入口签到）。
    pub fn share(&self, fields: &QrFields) -> Result<bool, String> {
        let id = message_id(fields);
        if !self.seen.lock().unwrap().insert(id.clone()) {
            return Ok(false);
        }
        let _ = fields.course_id; // course_id 仅用于展示/路由
        let now = chrono::Utc::now().timestamp_millis();
        let payload = QrSharePayload {
            kind: "lms_qr".into(),
            course_id: self.course_id,
            rollcall_id: self.rollcall_id,
            qr: fields.clone(),
            shared_at_ms: now,
        };
        self.handle.broadcast(szu_mesh::mesh::qr_room::RoomMessage {
            id,
            payload: serde_json::to_value(&payload).unwrap_or(Value::Null),
            sent_at_ms: now,
        });
        Ok(true)
    }

    pub async fn leave(self) {
        self.task.abort();
        self.handle.leave().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_qr_fields_accepts_json_and_jumper_url() {
        let json = parse_qr_fields(
            r#"{"rollcallId":123,"courseId":9,"data":"abc"}"#,
        )
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
        assert!(parse_qr_fields(r#"{"data":"x"}"#).is_none(), "缺 rollcallId 应拒绝");
                assert!(parse_qr_fields(r#"{"rollcallId":1}"#).is_none(), "缺 data 应拒绝");
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
        assert_eq!(qr_valid_until_ms(&no_time, 5_000_000), 5_000_000 + QR_TTL_MS);
    }

    #[test]
    fn room_identity_is_deterministic_and_course_scoped() {
        let (name_a, secret_a) = room_identity(42);
        let (name_b, secret_b) = room_identity(43);
        assert_eq!(name_a, "your-szu-life:study/signin/lms/qr:42");
        assert_ne!(name_a, name_b);
        assert_ne!(secret_a, secret_b);
        assert_eq!(secret_a.len(), 64);
        assert_eq!(room_identity(42), (name_a, secret_a));
    }

    #[test]
    fn message_id_is_rollcall_scoped_data_hash() {
        let base = message_id(&QrFields {
            rollcall_id: 1,
            course_id: None,
            data: "same".into(),
            create_time: 0,
        });
        assert_eq!(base.len(), 64);
        assert_eq!(
            base,
            message_id(&QrFields {
                rollcall_id: 1,
                course_id: None,
                data: "same".into(),
                create_time: 0,
            })
        );
        // 不同点名或轮换后的 data 都是新码。
        assert_ne!(
            base,
            message_id(&QrFields {
                rollcall_id: 2,
                course_id: None,
                data: "same".into(),
                create_time: 0,
            })
        );
        assert_ne!(
            base,
            message_id(&QrFields {
                rollcall_id: 1,
                course_id: None,
                data: "rotated".into(),
                create_time: 0,
            })
        );
    }
}
