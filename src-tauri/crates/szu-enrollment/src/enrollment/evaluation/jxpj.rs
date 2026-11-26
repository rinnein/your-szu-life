//! 教学质量综合评价平台（jxpj.szu.edu.cn）客户端。
//!
//! 登录链与 eHall / 畅课同一套机制：从 vault 投影 authserver cookie，访问
//! `sso/szulogin` 后手动跟随 CAS 跳转（该入口无 ticket 时返回一段 JS 跳转，
//! 不是 HTTP 302，因此必须解析 HTML）。CASTGC 有效时自动完成 service ticket
//! 登录，失效时返回 [`Failure::Session`]。
//!
//! `base` / `auth` 由构造参数注入，生产环境用 [`JXPJ_BASE`] 与 [`AUTH_BASE`]，
//! 测试注入本地 mock 服务。

use super::super::protocol::{Error, Failure, Result};
use super::model::{PlannedChoice, PlannedComment};
use reqwest::{blocking::Client, Method, StatusCode};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub const JXPJ_BASE: &str = "https://jxpj.szu.edu.cn/education-jxcp-service-web";
pub const AUTH_BASE: &str = "https://authserver.szu.edu.cn";

/// 登录链最多跟随的跳转次数（真实链路 7 跳，留足余量）。
const MAX_HOPS: usize = 16;

pub struct JxpjClient {
    http: Client,
    base: String,
    auth: String,
    /// 服务端若下发 `x-csrf-token`，后续 POST 原样回带。
    csrf: Mutex<Option<String>>,
}

/// 登录后拿到的账号上下文。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub term_id: i64,
    pub term_name: String,
}

/// `studentEval/query` 的一行。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetRow {
    pub id: String,
    pub target_name: String,
    pub term_name: String,
    pub pending: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub sc_id: i64,
    pub course_name: String,
    pub course_no: String,
    pub employee_name: String,
    pub class_name: String,
    pub current_times: i64,
    pub limited_times: i64,
    pub expired_date: Option<String>,
    pub participate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub value_low: String,
    pub value_high: String,
    pub value_normal: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub sub_id: i64,
    pub sub_name: String,
    pub kind: String,
    pub choices: Vec<Choice>,
}

/// `studentEval/querySearchTargetById/{id}` 的详情。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub target_id: i64,
    pub target_name: String,
    pub score_type: i64,
    pub comment_required: i64,
    pub stay_seconds: i64,
    pub records: Vec<Record>,
    pub questions: Vec<Question>,
}

impl JxpjClient {
    pub fn new(
        jxpj_base: &str,
        auth_base: &str,
        sso: Arc<reqwest_cookie_store::CookieStoreMutex>,
    ) -> Result<Self> {
        Ok(Self {
            http: super::super::session::client(sso)?,
            base: jxpj_base.trim_end_matches('/').to_string(),
            auth: auth_base.trim_end_matches('/').to_string(),
            csrf: Mutex::new(None),
        })
    }

    /// 用统一认证会话建立评教会话并返回账号上下文。
    pub fn bootstrap(&self) -> Result<Account> {
        let mut url = format!("{}/sso/szulogin", self.base);
        for _ in 0..MAX_HOPS {
            let response = self
                .http
                .get(&url)
                .send()
                .map_err(|_| Error::new(Failure::Network, "评教平台暂时无法访问"))?;
            self.capture_csrf(&response);
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| Error::new(Failure::Protocol, "评教登录跳转缺少目标地址"))?;
                url = resolve_url(&url, location)?;
                continue;
            }
            if !status.is_success() {
                return Err(session_error());
            }
            let body = response
                .text()
                .map_err(|_| Error::new(Failure::Network, "评教登录响应读取中断"))?;
            if let Some(next) = js_redirect(&body) {
                if !next.starts_with(&self.auth) {
                    return Err(session_error());
                }
                url = next;
                continue;
            }
            // 落到统一认证登录页而不是自动回跳，说明 CASTGC 已失效。
            if url.starts_with(&self.auth) {
                return Err(session_error());
            }
            return self.account();
        }
        Err(session_error())
    }

    /// `sso/login`：确认登录态、取当前学期与学生评教角色。
    fn account(&self) -> Result<Account> {
        let value = self.request(
            Method::GET,
            &format!("{}/sso/login", self.base),
            None,
            "#/sso/login",
        )?;
        if value["code"].as_i64() != Some(200) {
            return Err(session_error());
        }
        let data = &value["payload"]["data"];
        if text(&data["loginName"]).is_empty() {
            return Err(session_error());
        }
        let roles = data["roleModuleList"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let has_evaluation = roles
            .iter()
            .any(|role| text(&role["url"]).contains("/studenteval"));
        if !has_evaluation {
            return Err(Error::new(
                Failure::Forbidden,
                "当前账号没有学生评教权限",
            ));
        }
        Ok(Account {
            term_id: i64_of(&data["termId"]),
            term_name: text(&data["termName"]),
        })
    }

    /// 当前学期的待评/已评清单。
    pub fn list_targets(&self, term_id: i64) -> Result<Vec<TargetRow>> {
        let body = json!({
            "bgTermId": term_id,
            "endTermId": term_id,
            "pageNum": 1,
            "pageSize": 100,
        });
        let value = self.request(
            Method::POST,
            &format!("{}/studentEval/query?pageNum=1&pageSize=100", self.base),
            Some(body),
            "#/studenteval/index",
        )?;
        ensure_ok(&value)?;
        Ok(value["payload"]["data"]["dataList"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(parse_row)
            .collect())
    }

    /// 目标的明细；已评完时返回 `None`（服务端返回空数组）。
    pub fn target_detail(&self, id: &str) -> Result<Option<Detail>> {
        let value = self.request(
            Method::GET,
            &format!("{}/studentEval/querySearchTargetById/{}", self.base, id),
            None,
            "#/studenteval/index",
        )?;
        ensure_ok(&value)?;
        let data = value["payload"]["data"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(data.first().map(parse_detail))
    }

    /// 提交一个目标的全部选择与评语。成功返回 `Ok(())`。
    pub fn save_score(
        &self,
        target_id: i64,
        choices: &[PlannedChoice],
        comments: &[PlannedComment],
    ) -> Result<()> {
        let body = json!({ "choiceList": choices, "commentList": comments });
        let value = self.request(
            Method::POST,
            &format!("{}/studentEval/saveScore/{}", self.base, target_id),
            Some(body),
            "#/studenteval/eval",
        )?;
        ensure_ok(&value)
    }

    fn request(
        &self,
        method: Method,
        url: &str,
        body: Option<Value>,
        route: &str,
    ) -> Result<Value> {
        let mut builder = self
            .http
            .request(method, url)
            .header("schoolCode", "public")
            .header("routeUrl", route);
        if let Some(token) = self.csrf.lock().ok().and_then(|token| token.clone()) {
            builder = builder.header("x-csrf-token", token);
        }
        if let Some(body) = body {
            builder = builder.json(&body);
        }
        let response = builder
            .send()
            .map_err(|_| Error::new(Failure::Network, "评教平台暂时无法访问"))?;
        self.capture_csrf(&response);
        let status = response.status();
        let body = response
            .text()
            .map_err(|_| Error::new(Failure::Network, "评教接口响应读取中断"))?;
        if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
            return Err(session_error());
        }
        serde_json::from_str(&body)
            .map_err(|_| Error::new(Failure::Protocol, "评教接口响应格式变化，请稍后重试"))
    }

    fn capture_csrf(&self, response: &reqwest::blocking::Response) {
        if let Some(token) = response
            .headers()
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
        {
            if !token.is_empty() {
                if let Ok(mut slot) = self.csrf.lock() {
                    *slot = Some(token);
                }
            }
        }
    }
}

fn session_error() -> Error {
    Error::new(
        Failure::Session,
        "统一认证会话已失效，请重新登录应用",
    )
}

fn ensure_ok(value: &Value) -> Result<()> {
    if value["code"].as_i64() == Some(200) {
        return Ok(());
    }
    let message = text(&value["message"]);
    Err(Error::new(
        Failure::Business,
        if message.is_empty() {
            "评教请求失败".to_string()
        } else {
            message
        },
    ))
}

fn resolve_url(current: &str, location: &str) -> Result<String> {
    let current = reqwest::Url::parse(current)
        .map_err(|_| Error::new(Failure::Protocol, "评教登录地址无效"))?;
    Ok(current
        .join(location)
        .map_err(|_| Error::new(Failure::Protocol, "评教登录跳转地址无效"))?
        .to_string())
}

/// 入口页用一段 JS 跳统一认证，不是 HTTP 302。
fn js_redirect(body: &str) -> Option<String> {
    let at = body.find("window.location.href")?;
    let rest = &body[at..];
    let rest = &rest[rest.find('=')? + 1..];
    let quote = rest.chars().find(|c| *c == '\'' || *c == '"')?;
    let start = rest.find(quote)? + 1;
    let end = rest[start..].find(quote)?;
    let url = rest[start..start + end].trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

fn parse_row(row: &Value) -> Option<TargetRow> {
    let id = text(&row["id"]);
    if id.is_empty() {
        return None;
    }
    Some(TargetRow {
        id,
        target_name: text(&row["targetName"]),
        term_name: text(&row["termName"]),
        pending: i64_of(&row["noEvalCount"]),
    })
}

fn parse_detail(value: &Value) -> Detail {
    Detail {
        target_id: i64_of(&value["targetId"]),
        target_name: text(&value["targetName"]),
        score_type: i64_of(&value["scoreType"]),
        comment_required: i64_of(&value["commentRequired"]),
        stay_seconds: i64_of(&value["staySeconds"]),
        records: array(value, "studentCourseData")
            .iter()
            .map(parse_record)
            .collect(),
        questions: array(value, "subItemList")
            .iter()
            .map(parse_question)
            .collect(),
    }
}

fn parse_record(record: &Value) -> Record {
    Record {
        sc_id: i64_of(&record["scId"]),
        course_name: text(&record["courseName"]),
        course_no: text(&record["courseNo"]),
        employee_name: text(&record["employeeName"]),
        class_name: text(&record["classesName"]),
        current_times: i64_of(&record["currentTimes"]),
        limited_times: i64_of(&record["limitedTimes"]),
        expired_date: optional(&record["expiredDate"]),
        participate: flag(&record["participate"], true),
    }
}

fn parse_question(question: &Value) -> Question {
    Question {
        sub_id: i64_of(&question["id"]),
        sub_name: text(&question["subName"]),
        kind: text(&question["subitemType"]),
        choices: array(question, "choices")
            .iter()
            .filter_map(|choice| {
                let label = text(&choice["label"]);
                (!label.is_empty()).then(|| Choice {
                    label,
                    value_low: text(&choice["valueLow"]),
                    value_high: text(&choice["valueHigh"]),
                    value_normal: text(&choice["valueNormal"]),
                })
            })
            .collect(),
    }
}

fn array(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}

fn text(value: &Value) -> String {
    match value {
        Value::String(value) => value.trim().to_string(),
        Value::Number(value) => value.to_string(),
        _ => String::new(),
    }
}

fn optional(value: &Value) -> Option<String> {
    let text = text(value);
    (!text.is_empty()).then_some(text)
}

fn i64_of(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|value| value as i64))
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| value.trim().parse::<f64>().ok())
                .map(|value| value as i64)
        })
        .unwrap_or(0)
}

fn flag(value: &Value, fallback: bool) -> bool {
    match value {
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_i64().map(|value| value != 0).unwrap_or(fallback),
        Value::String(value) => match value.trim() {
            "true" => true,
            "false" => false,
            "1" => true,
            "0" => false,
            _ => fallback,
        },
        _ => fallback,
    }
}
