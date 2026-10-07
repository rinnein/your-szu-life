use super::model::{Context, Course};
use serde_json::{json, Value};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    Needs2fa,
    AdmissionRestricted,
    Session,
    Forbidden,
    Initializing,
    Throttled,
    Network,
    Full,
    Permanent,
    Business,
    Protocol,
}
#[derive(Clone, Debug)]
pub struct Error {
    pub kind: Failure,
    pub message: String,
}
impl Error {
    pub fn new(kind: Failure, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::new(Failure::Protocol, s)
    }
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn text(v: &Value) -> String {
    v.as_str().map(str::to_owned).unwrap_or_else(|| {
        if v.is_number() {
            v.to_string()
        } else {
            String::new()
        }
    })
}
pub fn field(v: &Value, names: &[&str]) -> String {
    names
        .iter()
        .map(|n| text(&v[*n]))
        .find(|s| !s.is_empty())
        .unwrap_or_default()
}
pub fn classify(status: u16, location: &str, body: &str) -> Result<Value> {
    if status >= 500 {
        return Err(Error::new(Failure::Network, "学校系统暂时不可用，等待恢复"));
    }
    if status == 429 || body.contains("请求频繁") || body.contains("访问频繁") {
        return Err(Error::new(Failure::Throttled, "请求受限，稍后重试"));
    }
    if body.contains("初始化") {
        return Err(Error::new(Failure::Initializing, "系统初始化中"));
    }
    if location.contains("webvpn.szu.edu.cn/users/")
        || location.contains("authserver-443.webvpn")
        || (body.contains("webvpn")
            && (body.contains("type=\"password\"") || body.contains("统一认证")))
    {
        return Err(Error::new(Failure::Needs2fa, "需2FA验证"));
    }
    let payload: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let code = text(&payload["code"]);
    if status == 401
        || matches!(code.as_str(), "302" | "401")
        || location.contains("index.do")
        || location.contains("authserver")
        || body.contains("登录已过期")
        || body.contains("登录失效")
        || (body.contains("vtoken") && body.contains("loginPwd"))
    {
        return Err(Error::new(Failure::Session, "选课会话失效，请重新登录"));
    }
    if status == 403 || code == "403" {
        return Err(Error::new(Failure::Forbidden, "访问被拒绝，需要核验会话"));
    }
    if status >= 400 {
        return Err(Error::new(
            Failure::Protocol,
            format!("学校接口返回 HTTP {status}"),
        ));
    }
    if payload.is_null() {
        return Err(Error::new(
            Failure::Protocol,
            "学校返回了无法识别的数据，保留缓存",
        ));
    }
    if !code.is_empty() && !matches!(code.as_str(), "1" | "0" | "200") {
        let message = field(&payload, &["msg", "message"]);
        let kind = if ["已满", "满员", "容量已满", "课余量不足"]
            .iter()
            .any(|w| message.contains(w))
        {
            Failure::Full
        } else if ["冲突", "不符合", "不可选", "已结束", "已截止", "学分上限"]
            .iter()
            .any(|w| message.contains(w))
        {
            Failure::Permanent
        } else {
            Failure::Business
        };
        return Err(Error::new(
            kind,
            if message.is_empty() {
                "学校拒绝请求".into()
            } else {
                message.chars().take(300).collect::<String>()
            },
        ));
    }
    Ok(payload)
}
pub fn course(v: &Value, category: &str) -> Option<Course> {
    let id = field(v, &["teachingClassID", "teachingClassId", "JXBID"]);
    if id.is_empty() {
        return None;
    }
    let number = |names: &[&str]| {
        names
            .iter()
            .find_map(|k| v[*k].as_f64().or_else(|| v[*k].as_str()?.parse().ok()))
    };
    Some(Course {
        id,
        code: field(v, &["courseNumber", "courseCode", "KCH"]),
        name: field(v, &["courseName", "KCM"]),
        teacher: field(v, &["teacherName", "SKJS"]),
        category: field(v, &["teachingClassType"]).chars().collect::<String>(),
        credits: number(&["credit", "credits", "XF"]),
        schedule: field(v, &["teachingPlace", "teachingTime", "schedule"]),
        capacity: number(&["classCapacity", "capacity"]).map(|n| n as i64),
        remaining: number(&["remainingCapacity", "remaining"]).map(|n| n as i64),
        campus: field(v, &["campus", "campusCode"]),
        main_select: match text(&v["isMainSelectObject"]).as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        },
        has_test: text(&v["hasTest"]) == "1",
        test_ids: vec![],
    })
    .map(|mut c| {
        if c.category.is_empty() {
            c.category = category.into();
        }
        c
    })
}
pub fn selection(context: &Context, c: &Course, dropping: bool) -> Value {
    let mut data = json!({"operationType":if dropping {"2"} else {"1"},"studentCode":context.student,"electiveBatchCode":context.batch,"teachingClassId":c.id,"isMajor":"1"});
    if !dropping {
        data["campus"] = json!(context.campus);
        data["teachingClassType"] = json!(c.category);
        data["chooseVolunteer"] = json!("1");
        if !c.test_ids.is_empty() {
            data["testTeachingClassID"] = json!(c.test_ids.join(","));
        }
    }
    json!({"data":data})
}

pub fn classify_student(status: u16, location: &str, body: &str) -> Result<Value> {
    if (200..300).contains(&status) {
        if let Ok(value) = serde_json::from_str::<Value>(body) {
            if text(&value["code"]) == "2" && value.get("data").is_some_and(Value::is_null) {
                return Err(Error::new(
                    Failure::AdmissionRestricted,
                    field(&value, &["msg", "message"]),
                ));
            }
        }
    }
    classify(status, location, body)
}
