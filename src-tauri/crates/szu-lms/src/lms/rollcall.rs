//! 畅课点名（签到）接口。
//!
//! 契约来自 2026-11 的 H5 前端源码分析 + 登录会话实测（answer_chunk 6150、
//! scanner 页 4853、API 表 3559/65655）：
//! - 数字：`PUT /api/rollcall/{id}/answer_number_rollcall` `{deviceId, numberCode}`
//! - 雷达：`PUT /api/rollcall/{id}/answer?api_version=1.76`
//!   `{deviceId, latitude, longitude, speed, accuracy, altitude, altitudeAccuracy, heading}`；
//!   `radarSignal = md5(message+deviceId+uid+timestamp),timestamp` **仅在蓝牙信标
//!   模式存在**（message/timestamp 来自信标广播 payload；深大关闭蓝牙 → 纯 GPS
//!   请求不带该字段）
//! - 二维码：QR 内容是 JSON `{rollcallId, courseId, data, ...}`（或 `/j?p=` /
//!   `?_p=` URL 包裹）；提交体只取 **`{data, deviceId}`**，`PUT /api/rollcall/
//!   {QR.rollcallId}/answer_qr_rollcall`，成功 = HTTP 200 且 `data.status == "on_call"`
//!
//! 已知限制：同一场点名里服务端把 deviceId 当不透明指纹，一设备替多人签到
//! 会被拒绝（`rollcall_device_already_in_use`）；数字签到每账号每场限 10 次。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use md5::{Digest, Md5};

use super::{BootstrapFailure, Service};

/// 待签列表的内存缓存有效期。探测循环与页面共享同一份，避免每 30s 重复拉。
const LIST_CACHE_TTL: Duration = Duration::from_secs(30);

/// 畅课待签项（`GET /api/radar/rollcalls?api_version=1.1.0` 的行）。
/// 字段名与类型以抓包为准，解析保持宽容。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RollcallItem {
    pub rollcall_id: i64,
    #[serde(default)]
    pub is_number: bool,
    #[serde(default)]
    pub is_radar: bool,
    #[serde(default)]
    pub course_id: i64,
    #[serde(default)]
    pub course_title: String,
    #[serde(default)]
    pub student_rollcall_id: i64,
    /// 畅课原始 `type` 字段。
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

impl RollcallItem {
    /// 已完成签到（`on_call_fine`）的条目不再视为待签。
    pub fn need_answer(&self) -> bool {
        self.status.as_deref() != Some("on_call_fine")
    }

    /// 签到方式判定：雷达 > 数字 > type 字段（与畅课 H5 跳转优先级一致）。
    pub fn method(&self) -> RollcallMethod {
        if self.is_radar {
            RollcallMethod::Radar
        } else if self.is_number {
            RollcallMethod::Number
        } else {
            match self.kind.as_deref().unwrap_or_default() {
                "qr" | "qr_rollcall" | "qrcode" => RollcallMethod::Qr,
                "number" | "number_rollcall" => RollcallMethod::Number,
                "radar" | "gps" => RollcallMethod::Radar,
                _ => RollcallMethod::Unknown,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RollcallMethod {
    Radar,
    Number,
    Qr,
    /// 未识别的类型（拍照/自主登记等）。
    Unknown,
}

/// 雷达签到提交的定位读数（浏览器/H5 的 getCurrentPosition 结果）。
/// 坐标必须是 GCJ-02（与教师端围栏同坐标系）。
#[derive(Clone, Debug)]
pub struct RadarReading {
    pub latitude: f64,
    pub longitude: f64,
    pub speed: f64,
    pub accuracy: f64,
    pub altitude: f64,
    pub altitude_accuracy: f64,
    pub heading: f64,
}

impl Default for RadarReading {
    fn default() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
            speed: 0.0,
            accuracy: 65.0,
            altitude: 0.0,
            altitude_accuracy: 10.0,
            heading: 0.0,
        }
    }
}

/// 雷达签名的信号串：`md5(message + deviceId + uid + timestamp),timestamp`。
/// `message` 的来源待真机校准；时间戳为秒级（与 H5 行为一致）。
pub fn radar_signal(message: &str, device_id: &str, uid: i64, timestamp_secs: i64) -> String {
    let raw = format!("{message}{device_id}{uid}{timestamp_secs}");
    let mut hasher = Md5::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("{hex},{timestamp_secs}")
}

/// 列表缓存（探测循环与页面共享）。
#[derive(Default)]
pub struct RollcallCache {
    inner: Mutex<Option<CachedList>>,
}

#[derive(Clone)]
struct CachedList {
    items: Vec<RollcallItem>,
    fetched_at: Instant,
}

impl RollcallCache {
    pub fn get(&self) -> Option<(Vec<RollcallItem>, bool)> {
        let guard = self.inner.lock().unwrap();
        let cached = guard.as_ref()?;
        let fresh = cached.fetched_at.elapsed() < LIST_CACHE_TTL;
        Some((cached.items.clone(), fresh))
    }

    pub fn store(&self, items: &[RollcallItem]) {
        *self.inner.lock().unwrap() = Some(CachedList {
            items: items.to_vec(),
            fetched_at: Instant::now(),
        });
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = None;
    }
}

impl Service {
    /// 当前待签列表。30s 内复用缓存；`force` 强制重拉。
    pub async fn rollcall_list(&self, force: bool) -> Result<Vec<RollcallItem>, String> {
        if !force {
            if let Some((items, fresh)) = self.rollcall_cache.get() {
                if fresh {
                    return Ok(items);
                }
            }
        }
        let client = self.rollcall_client().await?;
        let mut retried = false;
        loop {
            match client
                .get_json("api/radar/rollcalls", &[("api_version", "1.1.0".into())])
                .await
            {
                Ok(value) => {
                    let items = parse_rollcall_list(&value);
                    self.rollcall_cache.store(&items);
                    return Ok(items);
                }
                Err(error) if error.session_expired && !retried => {
                    retried = true;
                    if let Err(BootstrapFailure::NeedsAuth(message)) =
                        self.establish(&client).await
                    {
                        return Err(message);
                    }
                }
                Err(error) => return Err(error.message),
            }
        }
    }

    /// 单场点名详情（radarSignal 的 `message` 候选来源之一，真机校准用）。
    pub async fn rollcall_detail(&self, rollcall_id: i64) -> Result<Value, String> {
        let client = self.rollcall_client().await?;
        let mut retried = false;
        loop {
            match client
                .get_json(&format!("api/rollcall/{rollcall_id}"), &[])
                .await
            {
                Ok(value) => return Ok(value),
                Err(error) if error.session_expired && !retried => {
                    retried = true;
                    if let Err(BootstrapFailure::NeedsAuth(message)) =
                        self.establish(&client).await
                    {
                        return Err(message);
                    }
                }
                Err(error) => return Err(error.message),
            }
        }
    }

    /// 数字签到。
    pub async fn submit_number(
        &self,
        rollcall_id: i64,
        number_code: &str,
        device_id: &str,
    ) -> Result<(), String> {
        let body = json!({ "deviceId": device_id, "numberCode": number_code });
        self.put_with_retry(
            &[],
            &body,
            format!("api/rollcall/{rollcall_id}/answer_number_rollcall"),
        )
        .await
    }

    /// 雷达（GPS）签到。`beacon` 是蓝牙信标 payload（message, timestamp, uid），
    /// 存在时计算并携带 radarSignal；深大为纯 GPS 模式，恒传 None（H5 源码
    /// 确认：无信标时 radarSignal 为 undefined，请求体省略该字段）。
    pub async fn submit_radar(
        &self,
        rollcall_id: i64,
        reading: &RadarReading,
        device_id: &str,
        beacon: Option<(&str, i64, i64)>,
    ) -> Result<(), String> {
        let mut body = json!({
            "deviceId": device_id,
            "latitude": reading.latitude,
            "longitude": reading.longitude,
            "speed": reading.speed,
            "accuracy": reading.accuracy,
            "altitude": reading.altitude,
            "altitudeAccuracy": reading.altitude_accuracy,
            "heading": reading.heading,
        });
        if let Some((message, timestamp, uid)) = beacon {
            body["radarSignal"] = Value::String(radar_signal(message, device_id, uid, timestamp));
        }
        self.put_with_retry(
            &[("api_version", "1.76".into())],
            &body,
            format!("api/rollcall/{rollcall_id}/answer"),
        )
        .await
    }

    /// 二维码签到。`data` 是 QR JSON 里的原样 `data` 字段（create_time/hash 由
    /// 服务端校验，不随请求提交）；成功判定为 HTTP 200 且 `data.status ==
    /// "on_call"`（H5 scanner 页 4853 的 ee 函数）。
    pub async fn submit_qr(&self, rollcall_id: i64, data: &str, device_id: &str) -> Result<(), String> {
        let body = json!({ "data": data, "deviceId": device_id });
        let response = self
            .put_with_retry_value(
                &[],
                &body,
                format!("api/rollcall/{rollcall_id}/answer_qr_rollcall"),
            )
            .await?;
        match response
            .get("data")
            .and_then(|data| data.get("status"))
            .and_then(Value::as_str)
        {
            Some("on_call") => Ok(()),
            Some(other) => Err(format!("签到未完成（状态 {other}），请重新扫码")),
            // 响应缺 status 字段时按 HTTP 成功处理（兼容旧服务端）。
            None => Ok(()),
        }
    }

    /// PUT 应答统一走「会话失效 → 重新引导一次 → 重试」。
    async fn put_with_retry(
        &self,
        query: &[(&str, String)],
        body: &Value,
        path: String,
    ) -> Result<(), String> {
        self.put_with_retry_value(query, body, path).await.map(|_| ())
    }

    /// 同 [`Service::put_with_retry`]，但返回响应体供成功判定。
    async fn put_with_retry_value(
        &self,
        query: &[(&str, String)],
        body: &Value,
        path: String,
    ) -> Result<Value, String> {
        let client = self.rollcall_client().await?;
        let mut retried = false;
        loop {
            match client.put_json(&path, query, body).await {
                Ok(value) => return Ok(value),
                Err(error) if error.session_expired && !retried => {
                    retried = true;
                    if let Err(BootstrapFailure::NeedsAuth(message)) =
                        self.establish(&client).await
                    {
                        return Err(message);
                    }
                }
                Err(error) => return Err(error.message),
            }
        }
    }

    /// 确保会话可用后取客户端。会话失效由各请求的 session_expired
    /// 重试路径处理；这里只保证缓存数据可用（无缓存时阻塞首拉一次）。
    async fn rollcall_client(&self) -> Result<super::client::Client, String> {
        self.ensure(false).await;
        self.build_client()
            .ok_or_else(|| "无法建立畅课连接".to_string())
    }
}

/// 从 `{"rollcalls": [...]}` 解析待签列表；条目解析失败跳过而非整单失败。
pub fn parse_rollcall_list(value: &Value) -> Vec<RollcallItem> {
    let entries = value
        .get("rollcalls")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    entries.iter().filter_map(parse_rollcall).collect()
}

fn parse_rollcall(value: &Value) -> Option<RollcallItem> {
    let rollcall_id = as_i64(value.get("rollcall_id"))?;
    Some(RollcallItem {
        rollcall_id,
        is_number: as_bool(value.get("is_number")),
        is_radar: as_bool(value.get("is_radar")),
        course_id: as_i64(value.get("course_id")).unwrap_or_default(),
        course_title: value
            .get("course_title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        student_rollcall_id: as_i64(value.get("student_rollcall_id")).unwrap_or_default(),
        kind: value
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn as_i64(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(|value| value.as_i64().or_else(|| value.as_str()?.parse().ok()))
}

fn as_bool(value: Option<&Value>) -> bool {
    value.map_or(false, |value| {
        value.as_bool().unwrap_or_else(|| value.as_i64().unwrap_or(0) != 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radar_signal_is_md5_of_concatenation() {
        let signal = radar_signal("msg", "device-1", 42, 1_700_000_000);
        let (hex, ts) = signal.rsplit_once(',').expect("信号应是 hex,timestamp");
        assert_eq!(ts, "1700000000");
        assert_eq!(hex.len(), 32);
        // 独立重算一次对照。
        let mut hasher = Md5::new();
        hasher.update(format!("msgdevice-142{}", 1_700_000_000).as_bytes());
        let expected: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(hex, expected);
    }

    #[test]
    fn parse_rollcall_list_tolerates_numeric_and_string_booleans() {
        let value = json!({
            "rollcalls": [
                {
                    "rollcall_id": 12345,
                    "is_number": 1,
                    "is_radar": true,
                    "course_id": 999,
                    "course_title": "概率论与数理统计",
                    "student_rollcall_id": "777",
                    "type": "radar",
                    "status": "on_call"
                },
                { "rollcall_id": "67890", "type": "qr", "status": "on_call_fine" }
            ]
        });
        let items = parse_rollcall_list(&value);
        assert_eq!(items.len(), 2);
        assert!(items[0].is_number && items[0].is_radar);
        assert_eq!(items[0].student_rollcall_id, 777);
        assert_eq!(items[0].method(), RollcallMethod::Radar);
        assert!(items[0].need_answer());
        // 已完成的条目不再算待签。
        assert!(!items[1].need_answer());
        assert_eq!(items[1].method(), RollcallMethod::Qr);
    }

    #[test]
    fn method_priority_is_radar_then_number() {
        let item = RollcallItem {
            rollcall_id: 1,
            is_number: true,
            is_radar: true,
            course_id: 1,
            course_title: String::new(),
            student_rollcall_id: 1,
            kind: Some("number".into()),
            status: None,
        };
        assert_eq!(item.method(), RollcallMethod::Radar);
    }

    #[test]
    fn radar_signal_only_used_in_beacon_mode() {
        // 深大纯 GPS：beacon 为 None，body 不含 radarSignal（前端 H5 同构）。
        let reading = RadarReading::default();
        let body = json!({
            "deviceId": "d1",
            "latitude": reading.latitude,
            "longitude": reading.longitude,
        });
        assert!(body.get("radarSignal").is_none());
        // 信标模式：radarSignal = hash(message+deviceId+uid+timestamp),timestamp。
        let signal = radar_signal("beacon-msg", "d1", 7, 1_700_000_000);
        let (hex, ts) = signal.rsplit_once(',').unwrap();
        assert_eq!(ts, "1700000000");
        assert_eq!(hex.len(), 32);
    }
}
