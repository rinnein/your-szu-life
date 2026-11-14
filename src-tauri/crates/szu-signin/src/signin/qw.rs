//! qw.szu.edu.cn 考勤系统客户端。
//!
//! qw.szu.edu.cn 是学校企业微信应用的统一网关，会话由统一认证 CAS 派生：
//! 首次访问经服务端重定向建立，成功后 cookie 持久化到 vault（`runtime.qw.session`），
//! 之后恢复复用，与畅课/巡课的会话管理同一套路。楼栋列表接口免登录，
//! 课表/签到接口需要 CAS 会话。

use reqwest::{Client as Http, Method, StatusCode, Url};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

use super::model::TimetableSession;

/// 考勤站点根地址。
pub const QW_ORIGIN: &str = "https://qw.szu.edu.cn/";
/// 统一认证 CAS 登录入口。
pub const QW_CAS_LOGIN: &str = "https://authserver.szu.edu.cn/authserver/login";

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// 登录链最大跳数（与 szu-lms 一致）。
const MAX_REDIRECTS: usize = 10;

/// 课表拉取窗口（天）。`student-schedule` 返回日期区间内的扁平课次数组。
const SCHEDULE_DAYS: i64 = 60;

/// 会话探测结果：建立成功时带回账号信息（student_id = username，name）。
#[derive(Debug)]
pub enum Bootstrap {
    Established { student_id: String, name: String },
    NeedsAuth,
}

#[derive(Debug)]
pub struct ApiError {
    pub message: String,
    pub session_expired: bool,
}

impl ApiError {
    fn local(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            session_expired: false,
        }
    }
}

pub struct QwClient {
    http: Http,
    jar: Arc<CookieStoreMutex>,
    qw: Url,
    auth: Url,
}

impl QwClient {
    /// `cookies` 是之前保存的网关会话 jar（JSON，仅含 qw 域 cookie）；
    /// `sso` 保留统一认证的原生 Cookie 身份，只投影当前 HTTP 上下文。
    pub fn new(
        qw_base: &str,
        auth_base: &str,
        cookies: Option<&str>,
        sso: Option<&CookieStoreMutex>,
    ) -> Result<Self, String> {
        let qw = normalize_base(qw_base, "考勤地址无效")?;
        let auth = normalize_base(auth_base, "统一认证地址无效")?;
        let mut store = if cookies.map(|c| !c.is_empty()).unwrap_or(false) {
            cookie_store::serde::json::load_all(std::io::Cursor::new(cookies.unwrap()))
                .map_err(|_| "考勤会话数据损坏，将重新建立会话".to_string())
                .unwrap_or_default()
        } else {
            cookie_store::CookieStore::default()
        };
        if let Some(sso) = sso {
            for cookie in sso
                .lock()
                .map_err(|_| "统一认证会话锁失败".to_string())?
                .iter_unexpired()
            {
                let _ = store.insert(cookie.clone(), &auth);
            }
        }
        let jar = Arc::new(CookieStoreMutex::new(store));
        if let Some(sso) = sso {
            szu_core::webview_cookies::copy_snapshot(sso, &jar);
        }
        let builder = szu_core::netproxy::client_builder()
            .cookie_provider(szu_core::webview_cookies::http_provider(jar.clone()))
            // CAS 跳转由模块自己逐跳跟随并限定目标域。
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("SZULife/0.3 Attendance");
        #[cfg(target_os = "android")]
        szu_core::platform_tls::initialize_platform_verifier()?;
        Ok(Self {
            http: builder
                .build()
                .map_err(|_| "无法建立考勤连接".to_string())?,
            jar,
            qw,
            auth,
        })
    }

    /// 保存 jar 中属于网关域的 cookie（不含 authserver 会话，避免重复
    /// 保存主凭据）。这里只能按**域**匹配：真实服务端的会话 cookie 是
    /// `cas_session=…; Path=/attendance`，而站点基址的 path 是 `/`，
    /// `Cookie::matches` 会连 path 一起比较并把会话丢掉（下次重建客户端
    /// 后所有接口 401）。也正因为服务端 cookie 可能带父域，不能用 host
    /// 精确比较。
    pub fn cookies(&self) -> Result<String, String> {
        let jar = self.jar.lock().map_err(|_| "考勤会话不可用".to_string())?;
        let mut filtered = cookie_store::CookieStore::default();
        for cookie in jar.iter_any() {
            if !cookie.is_expired() && cookie.domain.matches(&self.qw) {
                let _ = filtered.insert_raw(&cookie.clone().into(), &self.qw);
            }
        }
        let mut bytes = vec![];
        cookie_store::serde::json::save_incl_expired_and_nonpersistent(&filtered, &mut bytes)
            .map_err(|_| "无法保存考勤会话".to_string())?;
        String::from_utf8(bytes).map_err(|_| "会话编码无效".into())
    }

    fn is_trusted_redirect(&self, url: &Url) -> bool {
        let Some(host) = url.host_str().map(|h| h.trim_end_matches('.')) else {
            return false;
        };
        host == self.qw.host_str().unwrap_or_default()
            || host == self.auth.host_str().unwrap_or_default()
            || host == "szu.edu.cn"
            || host.ends_with(".szu.edu.cn")
    }

    /// 探测 CAS 会话：`GET /attendance/cas/status` 返回 200 且 `success` 为真
    /// 即视为已登录（响应 `user.username` 即学号）。未登录（`success:false`）
    /// 返回 `None`，由调用方走建立链；HTTP 层错误上抛。
    async fn probe_cas_status(&self) -> Result<Option<Bootstrap>, String> {
        let url = self.qw.join("attendance/cas/status").unwrap();
        let response = self
            .http
            .get(url)
            .header("Referer", self.qw.as_str())
            .send()
            .await
            .map_err(|_| "无法连接考勤系统，请检查校园网或代理设置".to_string())?;
        let status = response.status();
        if status.is_redirection() || status == StatusCode::UNAUTHORIZED {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("考勤服务返回 HTTP {}", status.as_u16()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "考勤响应读取中断".to_string())?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "考勤会话状态响应格式不兼容".to_string())?;
        // 实测（真实服务器，2026-11-14）：未登录时 cas/status 也返回
        // `{"success": true, "logged_in": false}`——success 指请求成功，
        // 登录态只看 `logged_in` 与 user.username。只判 success 会把
        // 未登录误判为已建立（这正是课表报「会话已失效」的根因）。
        let success = value
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let logged_in = value
            .get("logged_in")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !success || !logged_in {
            return Ok(None);
        }
        let user = value.get("user");
        let student_id = user
            .and_then(|user| user.get("username"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = user
            .and_then(|user| user.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if student_id.is_empty() {
            return Ok(None);
        }
        Ok(Some(Bootstrap::Established { student_id, name }))
    }

    fn is_qw(&self, url: &Url) -> bool {
        url.host_str() == self.qw.host_str()
    }

    /// 建立或恢复考勤会话。与 jxxk/畅课同一套路：先探测会话状态，未登录时
    /// 访问 CAS 保护的考勤入口并**逐跳跟随重定向**（authserver SSO 有效时
    /// 自动带票回跳 `attendance/cas/callback`，服务端 Set-Cookie 由 cookie
    /// jar 吸收）建立 qw 会话，落地后复探状态确认。会话与票据都失效时停在
    /// 统一认证登录页，返回 [`Bootstrap::NeedsAuth`]。
    pub async fn bootstrap(&self) -> Result<Bootstrap, String> {
        // 快路径：vault 恢复的会话仍有效时不再走登录链。
        if let Some(established) = self.probe_cas_status().await? {
            return Ok(established);
        }
        let mut url = self
            .qw
            .join("attendance/index.html")
            .map_err(|_| "考勤入口地址无效".to_string())?;
        for hop in 0..=MAX_REDIRECTS {
            let response = self
                .http
                .get(url.clone())
                .header("Referer", self.qw.as_str())
                .send()
                .await
                .map_err(|_| "无法连接考勤系统，请检查校园网或代理设置".to_string())?;
            let status = response.status();
            log::info!(
                "[qw] bootstrap hop {hop}: GET {url} -> HTTP {}",
                status.as_u16()
            );
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default()
                    .to_string();
                let next = url
                    .join(&location)
                    .map_err(|_| "考勤登录跳转地址无效".to_string())?;
                if !self.is_trusted_redirect(&next) {
                    return Err(format!(
                        "考勤登录跳转目标不受信任: {}",
                        next.host_str().unwrap_or_default()
                    ));
                }
                url = next;
                continue;
            }
            if !status.is_success() {
                return Err(format!("考勤服务返回 HTTP {}", status.as_u16()));
            }
            let _ = response.bytes().await;
            if !self.is_qw(&url) {
                // 停在统一认证登录页：authserver SSO 也已失效。
                return Ok(Bootstrap::NeedsAuth);
            }
            // 已回到考勤域（链建立完成）：复探状态确认会话。
            return self
                .probe_cas_status()
                .await
                .map(|established| established.unwrap_or(Bootstrap::NeedsAuth));
        }
        Err("考勤登录跳转次数过多".into())
    }

    pub async fn get_json(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ApiError> {
        let response = self
            .request(Method::GET, path, query)?
            .send()
            .await
            .map_err(|_| ApiError::local("无法连接考勤系统，请检查校园网或代理设置"))?;
        self.parse_response(response).await
    }

    pub async fn post_json(&self, path: &str, body: Value) -> Result<Value, ApiError> {
        let response = self
            .request(Method::POST, path, &[])?
            .json(&body)
            .send()
            .await
            .map_err(|_| ApiError::local("无法连接考勤系统，请检查校园网或代理设置"))?;
        self.parse_response(response).await
    }

    /// 上传签到照片（multipart，字段名 `photo`，H5 用 attendance.jpg），
    /// 返回服务端响应（应含 `data.photo_path`）。
    pub async fn upload_photo(
        &self,
        path: &str,
        bytes: Vec<u8>,
        mime: &str,
        filename: &str,
    ) -> Result<Value, ApiError> {
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|_| ApiError::local("照片类型无效"))?;
        let form = reqwest::multipart::Form::new().part("photo", part);
        let response = self
            .request(Method::POST, path, &[])?
            .multipart(form)
            .send()
            .await
            .map_err(|_| ApiError::local("无法上传签到照片"))?;
        self.parse_response(response).await
    }

    fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<reqwest::RequestBuilder, ApiError> {
        let url = self
            .qw
            .join(path.trim_start_matches('/'))
            .map_err(|_| ApiError::local("考勤接口地址无效"))?;
        Ok(self
            .http
            .request(method, url)
            .query(query)
            .header("Referer", self.qw.as_str())
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Accept", "application/json"))
    }

    async fn parse_response(&self, response: reqwest::Response) -> Result<Value, ApiError> {
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| ApiError::local("考勤响应读取中断"))?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(ApiError::local("考勤响应过大"));
        }
        if status.is_redirection() || status == StatusCode::UNAUTHORIZED {
            return Err(ApiError {
                message: "考勤会话已失效，请重新完成统一认证".into(),
                session_expired: true,
            });
        }
        if !status.is_success() {
            let text = String::from_utf8_lossy(&bytes);
            return Err(ApiError::local(format!(
                "考勤服务返回 HTTP {}: {}",
                status.as_u16(),
                text.trim()
            )));
        }
        serde_json::from_slice(&bytes).map_err(|_| ApiError::local("考勤接口响应格式不兼容"))
    }
}

/// 拉取并解析课表。`days` 取最近的排课窗口；会话失效（`session_expired`）
/// 由调用方决定是否重bootstrap重试，其余错误直接上抛。
pub async fn student_schedule(client: &QwClient) -> Result<Vec<TimetableSession>, ApiError> {
    let value = client
        .get_json(
            "attendance/api/v1/student-schedule",
            &[
                ("days", SCHEDULE_DAYS.to_string()),
                ("week_type", "1".into()),
            ],
        )
        .await?;
    parse_schedule(&value).map_err(|message| ApiError::local(message))
}

/// 解析 `data.schedule[]`；时间字段解析失败的行跳过并记日志。
pub fn parse_schedule(value: &Value) -> Result<Vec<TimetableSession>, String> {
    let rows = value
        .get("data")
        .and_then(|data| data.get("schedule"))
        .and_then(Value::as_array)
        .ok_or_else(|| "考勤课表响应格式不兼容".to_string())?;
    let semester = value
        .get("data")
        .and_then(|data| data.get("semester"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut sessions = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(session) = parse_session(row, semester.as_deref()) else {
            log::warn!("跳过无法解析的课表行: {row}");
            continue;
        };
        sessions.push(session);
    }
    Ok(sessions)
}

fn parse_session(row: &Value, semester: Option<&str>) -> Option<TimetableSession> {
    let date = row.get("date")?.as_str()?.to_string();
    let course_name = row
        .get("course_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let start_ms = parse_time_field(&date, row.get("start_time"))?;
    let end_ms = parse_time_field(&date, row.get("end_time"))?;
    Some(TimetableSession {
        date,
        course_name,
        building_name: text(row.get("building_name")),
        building_name_en: text(row.get("building_name_en")),
        classroom: text(row.get("classroom")),
        start_ms,
        end_ms,
        semester: semester.map(str::to_string),
    })
}

fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 解析课表时间字段。服务端可能给出纪元数值、ISO 日期时间或「HH:MM」，
/// 这里统一转成 epoch 毫秒（本地时区）。
pub fn parse_time_field(date: &str, value: Option<&Value>) -> Option<i64> {
    use chrono::{NaiveDate, NaiveTime, TimeZone};
    let base = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    let timestamp = |datetime: chrono::NaiveDateTime| -> Option<i64> {
        chrono::Local
            .from_local_datetime(&datetime)
            .single()
            .map(|time| time.timestamp_millis())
    };
    match value? {
        Value::Number(number) => {
            let raw = number.as_f64()?;
            if raw > 1e12 {
                Some(raw as i64)
            } else if raw > 1e9 {
                Some((raw * 1000.0) as i64)
            } else {
                None
            }
        }
        Value::String(raw) => {
            let raw = raw.trim();
            if let Ok(seconds) = raw.parse::<f64>() {
                return if seconds > 1e12 {
                    Some(seconds as i64)
                } else if seconds > 1e9 {
                    Some((seconds * 1000.0) as i64)
                } else {
                    None
                };
            }
            if let Ok(datetime) = chrono::DateTime::parse_from_rfc3339(raw) {
                return Some(datetime.timestamp_millis());
            }
            if let Ok(datetime) =
                chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
            {
                return timestamp(datetime);
            }
            if let Ok(time) = NaiveTime::parse_from_str(raw, "%H:%M:%S") {
                return timestamp(base.and_time(time));
            }
            if let Ok(time) = NaiveTime::parse_from_str(raw, "%H:%M") {
                return timestamp(base.and_time(time));
            }
            None
        }
        _ => None,
    }
}

fn normalize_base(raw: &str, message: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw.trim()).map_err(|_| message.to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(message.into());
    }
    let path = format!("{}/", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url)
}

/// 签到提交体（H5 index.js 的 `context.payload` + 上传所得 `photo_path`）。
/// `student_course_id`/`session_id` 来自 check-signin-status 的
/// `course_instance`；手动模式（无实例）时留空并填写 course_name/classroom。
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct SigninPayload {
    pub student_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub course_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub student_course_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub classroom: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub location_address: String,
    /// ISO 8601（H5 用 `new Date().toISOString()`）。
    pub timestamp: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub language: String,
    /// 上传接口返回的 `photo_path`（不是照片本体）。
    pub photo: String,
}

pub fn signin_body(payload: &SigninPayload) -> Value {
    serde_json::to_value(payload).unwrap_or(Value::Null)
}

/// check-signin-status 的关键响应字段（H5 index.js @~68308）。
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct SigninStatus {
    pub valid: bool,
    pub already_signed: bool,
    pub signed_at: Option<String>,
    pub message: Option<String>,
    pub status: Option<String>,
    pub session_id: Option<i64>,
    pub student_course_id: Option<i64>,
    pub classroom: Option<String>,
}

/// `POST /attendance/api/v1/check-signin-status`：按坐标查询当前可签课次，
/// 返回归一化后的状态。`student_course_id`/`session_id` 缺省时由服务端按
/// 位置与课表匹配。
pub async fn check_signin_status(
    client: &QwClient,
    latitude: f64,
    longitude: f64,
    student_id: &str,
    student_course_id: Option<i64>,
    session_id: Option<i64>,
) -> Result<SigninStatus, ApiError> {
    let body = json!({
        "longitude": longitude,
        "latitude": latitude,
        "student_id": student_id,
        "student_course_id": student_course_id,
        "session_id": session_id,
    });
    let value = client
        .post_json("attendance/api/v1/check-signin-status", body)
        .await?;
    let data = value.get("data").cloned().unwrap_or(Value::Null);
    let success = value.get("success").and_then(Value::as_bool).unwrap_or(false);
    let instance = data.get("course_instance");
    let mut status = SigninStatus {
        valid: success
            && data.get("can_signin").and_then(Value::as_bool).unwrap_or(false),
        already_signed: data
            .get("already_signed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        signed_at: data
            .get("signed_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        message: data
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string),
        status: data.get("status").and_then(Value::as_str).map(str::to_string),
        session_id: instance
            .and_then(|instance| instance.get("session_id"))
            .and_then(Value::as_i64),
        student_course_id: instance
            .and_then(|instance| {
                instance
                    .get("student_course_id")
                    .or_else(|| instance.get("enrollment_id"))
                    .or_else(|| instance.get("id"))
            })
            .and_then(Value::as_i64),
        classroom: instance
            .and_then(|instance| instance.get("classroom"))
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    if status.message.is_none() {
        status.message = value
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string);
    }
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实服务端的会话 cookie 是 `cas_session=…; Path=/attendance`，而站点
    /// 基址的 path 是 `/`。落盘过滤必须只看域，不能按请求 URL 做 path 匹配，
    /// 否则会话 cookie 会被丢掉，重开客户端后 API 一律 401。
    #[test]
    fn cookies_keeps_path_scoped_session_cookie() {
        let client = QwClient::new(
            "https://qw.szu.edu.cn/",
            "https://authserver.szu.edu.cn/",
            None,
            None,
        )
        .unwrap();
        let raw =
            cookie_store::RawCookie::parse("cas_session=abc; Path=/attendance; Secure; HttpOnly")
                .unwrap();
        client
            .jar
            .lock()
            .unwrap()
            .insert_raw(&raw, &client.qw)
            .unwrap();
        let saved = client.cookies().unwrap();
        assert!(
            saved.contains("cas_session"),
            "path 限定在 /attendance 的会话 cookie 必须落盘: {saved}"
        );
        // 重建客户端（refresh_timetable 每次都重新 build）后仍要能匹配到 API。
        let restored = QwClient::new(
            "https://qw.szu.edu.cn/",
            "https://authserver.szu.edu.cn/",
            Some(&saved),
            None,
        )
        .unwrap();
        let api = restored
            .qw
            .join("attendance/api/v1/student-schedule")
            .unwrap();
        let jar = restored.jar.lock().unwrap();
        assert!(
            jar.iter_any()
                .any(|cookie| cookie.name() == "cas_session" && cookie.matches(&api)),
            "恢复后的 jar 必须把会话 cookie 带到 /attendance 下的接口"
        );
    }

    #[test]
    fn parse_schedule_handles_documented_shape() {
        let value = json!({
            "success": true,
            "data": {
                "semester": "2026-2027-1",
                "schedule": [
                    {
                        "date": "2026-11-09",
                        "course_name": "概率论与数理统计",
                        "building_name": "致理楼",
                        "building_name_en": "Zhili Building",
                        "classroom": "致理楼L1-203",
                        "start_time": "08:30",
                        "end_time": "10:10",
                        "mapping_status": "native_v2"
                    },
                    {
                        "date": "2026-11-10",
                        "course_name": "数据库系统",
                        "building_name": "致腾楼",
                        "classroom": "致腾楼328",
                        "start_time": 1_700_000_000i64,
                        "end_time": "2026-11-10 14:00:00"
                    },
                    { "date": "bad", "course_name": "坏行" }
                ]
            }
        });
        let sessions = parse_schedule(&value).unwrap();
        assert_eq!(sessions.len(), 2, "无法解析的行应跳过");
        assert_eq!(sessions[0].course_name, "概率论与数理统计");
        assert!(sessions[0].start_ms > 0);
        assert!(sessions[0].end_ms > sessions[0].start_ms);
        assert_eq!(sessions[1].semester.as_deref(), Some("2026-2027-1"));
    }

    #[test]
    fn parse_time_field_accepts_epoch_and_iso_and_clock() {
        let base = parse_time_field("2026-11-13", Some(&json!("08:30"))).unwrap();
        let seconds = parse_time_field("2026-11-13", Some(&json!(1_700_000_000i64))).unwrap();
        let millis = parse_time_field("2026-11-13", Some(&json!(1_700_000_000_000i64))).unwrap();
        assert_eq!(millis, 1_700_000_000_000);
        assert!((seconds - 1_700_000_000_000).abs() < 1000);
        let iso = parse_time_field(
            "2026-11-13",
            Some(&json!("2026-11-13T08:30:00+08:00")),
        )
        .unwrap();
        assert!(base > 0 && iso > 0);
        assert!(parse_time_field("bad-date", Some(&json!("08:30"))).is_none());
        assert!(parse_time_field("2026-11-13", Some(&json!("nope"))).is_none());
    }

    #[test]
    fn signin_body_serializes_full_payload_and_skips_empty() {
        let payload = SigninPayload {
            student_id: "2025150041".into(),
            name: "测试".into(),
            course_name: "".into(),
            student_course_id: Some(130350),
            session_id: Some(42),
            classroom: "".into(),
            latitude: 22.123456789,
            longitude: 113.987654321,
            location_address: "致理楼".into(),
            timestamp: "2026-11-14T08:30:00Z".into(),
            language: "zh-CN".into(),
            photo: "uploads/abc.jpg".into(),
        };
        let body = signin_body(&payload);
        // 坐标按 H5 原样提交（AMap GCJ-02 读数，不做 round）。
        assert_eq!(body["latitude"], 22.123456789);
        assert_eq!(body["longitude"], 113.987654321);
        assert_eq!(body["student_course_id"], 130350);
        assert_eq!(body["session_id"], 42);
        assert_eq!(body["photo"], "uploads/abc.jpg");
        assert_eq!(body["timestamp"], "2026-11-14T08:30:00Z");
        // 空字符串字段省略（JSON.stringify 对 undefined 的行为）。
        assert!(body.get("course_name").is_none());
        assert!(body.get("classroom").is_none());
        let minimal = signin_body(&SigninPayload::default());
        assert!(minimal.get("student_course_id").is_none());
        assert!(minimal.get("name").is_none());
    }
}
