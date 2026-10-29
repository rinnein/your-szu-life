use super::model::{courses_page, id, learned_courses_page, number, text, Course, User, Video};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ThreadSummary {
    pub tid: String,
}

fn collect_thread_ids(value: &Value, output: &mut Vec<String>, depth: usize) {
    if depth > 5 {
        return;
    }
    match value {
        Value::String(serialized) => {
            if let Ok(nested) = serde_json::from_str::<Value>(serialized) {
                collect_thread_ids(&nested, output, depth + 1);
            }
        }
        Value::Array(rows) => rows
            .iter()
            .for_each(|row| collect_thread_ids(row, output, depth + 1)),
        Value::Object(map) => {
            if let Some(tid) = [
                "tid",
                "thread_id",
                "threadId",
                "threadid",
                "topic_id",
                "topicId",
                "topicid",
                "id",
            ]
                .iter()
                .find_map(|key| id(map.get(*key).unwrap_or(&Value::Null)))
            {
                if !output.contains(&tid) {
                    output.push(tid);
                }
            }
            for key in [
                "data",
                "list",
                "rows",
                "items",
                "threads",
                "topics",
                "result",
                "ask_list",
                "thread_list",
                "threadList",
            ] {
                if let Some(child) = map.get(key) {
                    collect_thread_ids(child, output, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn find_thread_content(value: &Value, depth: usize) -> Option<String> {
    if depth > 5 {
        return None;
    }
    match value {
        Value::String(content) if !content.trim().is_empty() => {
            if let Ok(nested) = serde_json::from_str::<Value>(content) {
                find_thread_content(&nested, depth + 1).or_else(|| Some(content.clone()))
            } else {
                Some(content.clone())
            }
        }
        Value::Object(map) => {
            for key in ["content", "body", "text", "ask_content", "thread_content"] {
                if let Some(content) = map.get(key).map(text).filter(|v| !v.trim().is_empty()) {
                    return Some(content);
                }
            }
            for key in ["data", "detail", "thread", "topic", "post", "result"] {
                if let Some(child) = map.get(key) {
                    if let Some(content) = find_thread_content(child, depth + 1) {
                        return Some(content);
                    }
                }
            }
            map.values()
                .find_map(|child| find_thread_content(child, depth + 1))
        }
        Value::Array(rows) => rows
            .iter()
            .find_map(|row| find_thread_content(row, depth + 1)),
        _ => None,
    }
}

fn thread_rows(value: &Value) -> Option<Vec<&Value>> {
    match value {
        Value::Array(rows) => Some(rows.iter().collect()),
        Value::Object(map) => [
            "list",
            "rows",
            "items",
            "threads",
            "topics",
            "ask_list",
            "thread_list",
            "threadList",
        ]
        .iter()
        .find_map(|key| map.get(*key).and_then(thread_rows))
            .or_else(|| map.get("data").and_then(thread_rows)),
        _ => None,
    }
}

pub(super) const ORIGIN: &str = "https://www.uooc.net.cn";
pub(super) const LOGIN: &str = "https://www.uooc.net.cn/user/login";
pub(super) const LIST: &str = "/home/course/list";
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum CourseSignin {
    NotRequired,
    Signed,
    AlreadySigned,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Error {
    Cancelled,
    Unauthorized,
    Forbidden,
    Gate,
    Network,
    Invalid,
    UnexpectedRedirect,
    Server(i64, String, Vec<String>),
    Quiz(String),
}
impl Error {
    pub fn message(&self) -> String {
        match self {
            Self::Cancelled => "任务已取消".into(),
            Self::Unauthorized => "UOOC 登录已失效，请重新登录".into(),
            Self::Forbidden => "UOOC 拒绝访问（HTTP 403，可能不允许此请求方法）".into(),
            Self::Gate => "需要先完成前置任务或测验".into(),
            Self::Network => "UOOC 请求失败，请检查网络后重试".into(),
            Self::Invalid => "UOOC 返回数据格式无效".into(),
            Self::UnexpectedRedirect => "UOOC 返回了非登录重定向，请检查访问方式或网络".into(),
            Self::Server(601, msg, details) => {
                let detail_str = details.join("；");
                if detail_str.contains("评价") || msg.contains("评价") {
                    let text = if !detail_str.is_empty() { &detail_str } else { msg };
                    format!("请先完成课程评价：{text}（UOOC 业务码 601）。可在官网打开课程完成评价后再提交。")
                } else if !detail_str.is_empty() {
                    format!("服务器验证错误：{detail_str}（{msg}，业务码 601）")
                } else if !msg.is_empty() {
                    format!("{msg}（UOOC 服务器验证错误 601）")
                } else {
                    "请先完成课程评价或检查必填项（UOOC 业务码 601）".into()
                }
            }
            Self::Server(code, msg, details) => {
                let detail_str = details.join("；");
                if !detail_str.is_empty() && !msg.is_empty() {
                    format!("{msg}：{detail_str}（业务码 {code}）")
                } else if !detail_str.is_empty() {
                    format!("{detail_str}（业务码 {code}）")
                } else if !msg.is_empty() {
                    format!("{msg}（UOOC 业务码 {code}）")
                } else {
                    format!("UOOC 拒绝请求（业务码 {code}）")
                }
            }
            Self::Quiz(message) => message.clone(),
        }
    }
}
pub(super) struct Client {
    http: reqwest::Client,
    pub jar: Arc<CookieStoreMutex>,
    base: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn discussion_payload_helpers_accept_nested_realistic_shapes() {
        let value = json!({
            "page": {"pages": 1},
            "data": {"data": {"rows": [{"threadId": "17"}]}}
        });
        let mut tids = Vec::new();
        collect_thread_ids(&value, &mut tids, 0);
        assert_eq!(tids, vec!["17"]);
        assert_eq!(
            find_thread_content(&json!({"data": {"post": [{"body": "<p>hello</p>"}]}}), 0),
            Some("<p>hello</p>".into())
        );
    }

    #[test]
    fn discussion_payload_helpers_accept_json_string_layers() {
        let value = json!({"data": "{\"rows\":[{\"tid\":\"18\"}]}"});
        let mut tids = Vec::new();
        collect_thread_ids(&value, &mut tids, 0);
        assert_eq!(tids, vec!["18"]);
        assert_eq!(thread_rows(&json!({"data": {"rows": []}})).unwrap().len(), 0);
    }
}
impl Client {
    pub(super) fn origin(&self) -> &str {
        &self.base
    }
    pub(super) async fn quiz_list(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let mut all = vec![];
        let mut seen = HashSet::new();
        for page in 1..=1000 {
            let data = self
                .request(
                    "/home/task/testList",
                    &[
                        ("cid".into(), cid.into()),
                        ("page".into(), page.to_string()),
                        ("pagesize".into(), "20".into()),
                    ],
                    false,
                    cancel,
                )
                .await?;
            if let Some(arr) = data.as_array() {
                for row in arr {
                    if let Some(rid) = id(&row["res_id"]) {
                        if seen.insert(rid) {
                            all.push(row.clone());
                        }
                    }
                }
                return Ok(all);
            }
            let rows = if let Some(arr) = data["data"].as_array() {
                arr
            } else if let Some(arr) = data["list"].as_array() {
                arr
            } else if let Some(arr) = data["rows"].as_array() {
                arr
            } else {
                return Err(Error::Invalid);
            };
            for row in rows {
                if let Some(rid) = id(&row["res_id"]) {
                    if seen.insert(rid) {
                        all.push(row.clone());
                    }
                }
            }
            let pages = text(&data["page"]["pages"])
                .parse::<u64>()
                .map_err(|_| Error::Invalid)?;
            if page >= pages {
                return Ok(all);
            }
        }
        Err(Error::Invalid)
    }
    pub(super) async fn exam_list(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let mut all = vec![];
        for page in 1..=100 {
            let data = self
                .request(
                    "/home/task/examList",
                    &[
                        ("cid".into(), cid.into()),
                        ("page".into(), page.to_string()),
                        ("pagesize".into(), "20".into()),
                    ],
                    false,
                    cancel,
                )
                .await?;
            if let Some(arr) = data.as_array() {
                all.extend(arr.iter().cloned());
                return Ok(all);
            }
            let rows = if let Some(arr) = data["data"].as_array() {
                arr
            } else if let Some(arr) = data["list"].as_array() {
                arr
            } else if let Some(arr) = data["rows"].as_array() {
                arr
            } else {
                return Err(Error::Invalid);
            };
            for row in rows {
                all.push(row.clone());
            }
            let pages = text(&data["page"]["pages"])
                .parse::<u64>()
                .unwrap_or(1);
            if page >= pages {
                return Ok(all);
            }
        }
        Ok(all)
    }
    pub(super) async fn homework_list(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let mut all = vec![];
        for page in 1..=100 {
            let data = self
                .request(
                    "/home/task/homeworkList",
                    &[
                        ("cid".into(), cid.into()),
                        ("page".into(), page.to_string()),
                        ("pagesize".into(), "20".into()),
                    ],
                    false,
                    cancel,
                )
                .await?;
            if let Some(arr) = data.as_array() {
                all.extend(arr.iter().cloned());
                return Ok(all);
            }
            let rows = if let Some(arr) = data["data"].as_array() {
                arr
            } else if let Some(arr) = data["list"].as_array() {
                arr
            } else if let Some(arr) = data["rows"].as_array() {
                arr
            } else {
                return Err(Error::Invalid);
            };
            for row in rows {
                all.push(row.clone());
            }
            let pages = text(&data["page"]["pages"])
                .parse::<u64>()
                .unwrap_or(1);
            if page >= pages {
                return Ok(all);
            }
        }
        Ok(all)
    }
    pub(super) async fn exist_eval(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<bool, Error> {
        let val = self
            .request(
                "/home/task/existEval",
                &[("cid".into(), cid.into())],
                false,
                cancel,
            )
            .await?;
        if let Some(b) = val["data"].as_bool() {
            return Ok(b);
        }
        if let Some(n) = val["data"].as_i64() {
            return Ok(n > 0);
        }
        if let Some(arr) = val["data"].as_array() {
            return Ok(!arr.is_empty());
        }
        Ok(!val["data"].is_null())
    }
    pub(super) async fn eval_list(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let val = self
            .request(
                "/home/task/evalList",
                &[("cid".into(), cid.into())],
                false,
                cancel,
            )
            .await?;
        if let Some(arr) = val["data"].as_array() {
            return Ok(arr.clone());
        }
        if let Some(arr) = val.as_array() {
            return Ok(arr.clone());
        }
        Ok(vec![])
    }
    pub(super) async fn submit_eval_score(
        &self,
        cid: &str,
        tid: &str,
        eval_id: &str,
        score: f64,
        comments: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request(
            "/home/task/submitEvalScore",
            &[
                ("cid".into(), cid.into()),
                ("tid".into(), tid.into()),
                ("eval_id".into(), eval_id.into()),
                ("score".into(), format!("{score}")),
                ("comments".into(), comments.into()),
            ],
            true,
            cancel,
        )
        .await
    }
    pub(super) async fn exam_task_info(
        &self,
        tid: &str,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request(
            "/exam/taskInfo",
            &[("tid".into(), tid.into()), ("cid".into(), cid.into())],
            false,
            cancel,
        )
        .await
    }
    pub(super) async fn course_comment(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request(
            "/home/course/comment",
            &[("cid".into(), cid.into())],
            false,
            cancel,
        )
        .await
    }
    pub(super) async fn add_course_comment(
        &self,
        cid: &str,
        score: u32,
        content: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request(
            "/home/course/addcomment",
            &[
                ("cid".into(), cid.into()),
                ("score".into(), score.to_string()),
                ("content".into(), content.into()),
            ],
            true,
            cancel,
        )
        .await
    }
    pub(super) async fn set_know_review(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request(
            "/home/course/setKnowReview",
            &[("cid".into(), cid.into())],
            true,
            cancel,
        )
        .await
    }
    pub(super) async fn exam_save(
        &self,
        cid: Option<&str>,
        tid: &str,
        fields: &[(String, String)],
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        let mut params = vec![("tid".into(), tid.into())];
        if let Some(c) = cid {
            if !c.is_empty() {
                params.push(("cid".into(), c.into()));
            }
        }
        params.extend_from_slice(fields);
        self.request("/exam/save", &params, true, cancel).await
    }
    pub(super) async fn exam_commit(
        &self,
        cid: Option<&str>,
        tid: &str,
        fields: &[(String, String)],
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        let mut params = vec![("tid".into(), tid.into())];
        if let Some(c) = cid {
            if !c.is_empty() {
                params.push(("cid".into(), c.into()));
            }
        }
        params.extend_from_slice(fields);
        self.request("/exam/commit", &params, true, cancel).await
    }
    pub(super) async fn exam_heartbeat(
        &self,
        cid: &str,
        tid: &str,
        cancel: &CancellationToken,
    ) -> Result<bool, Error> {
        let res = self
            .request(
                "/home/cheatrecord/getAlert",
                &[
                    ("course_id".into(), cid.into()),
                    ("task_type".into(), "1".into()),
                    ("relation_id".into(), tid.into()),
                ],
                false,
                cancel,
            )
            .await;
        if let Ok(val) = res {
            if let Some(list) = val["data"].as_array() {
                for item in list {
                    if let Some(record_id) = id(&item["id"]) {
                        let _ = self
                            .request(
                                "/home/cheatrecord/alertRecode",
                                &[("id".into(), record_id)],
                                true,
                                cancel,
                            )
                            .await;
                    }
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub(super) async fn quiz_document(
        &self,
        task: &str,
        cancel: &CancellationToken,
    ) -> Result<String, Error> {
        let bytes = self
            .quiz_http(&format!("{}/exam/{task}", self.base), None, cancel)
            .await?;
        String::from_utf8(bytes).map_err(|_| Error::Invalid)
    }
    pub(super) async fn quiz_http(
        &self,
        url: &str,
        fields: Option<&[(String, String)]>,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>, Error> {
        let target = reqwest::Url::parse(url).map_err(|_| Error::Invalid)?;
        let base = reqwest::Url::parse(&self.base).map_err(|_| Error::Invalid)?;
        if target.origin() != base.origin()
            || !target.path().starts_with("/exam/")
            || !target.username().is_empty()
            || target.password().is_some()
        {
            return Err(Error::Invalid);
        }
        let request = match fields {
            Some(fields) => self.http.post(target).form(fields),
            None => self.http.get(target),
        }
        .header("Accept", "text/html,application/json")
        .header("Referer", format!("{}/home", self.base));
        tokio::select! { biased; _=cancel.cancelled()=>Err(Error::Cancelled),r=async {
            let mut response=request.send().await.map_err(|_|Error::Network)?;
            if response.status().is_redirection() {
                return Err(if response.headers().get(reqwest::header::LOCATION).and_then(|v|v.to_str().ok()).is_some_and(|s|super::user_agent::is_login_redirect(response.url(),s)){Error::Unauthorized}else{Error::UnexpectedRedirect});
            }
            match response.status().as_u16(){401=>return Err(Error::Unauthorized),403=>return Err(Error::Forbidden),200..=299=>{},_=>return Err(Error::Network)}
            let mut bytes=vec![];while let Some(chunk)=response.chunk().await.map_err(|_|Error::Network)? {if bytes.len()+chunk.len()>4*1024*1024{return Err(Error::Invalid);}bytes.extend_from_slice(&chunk);}
            if serde_json::from_slice::<Value>(&bytes).ok().is_some_and(|v|text(&v["code"])=="401"){return Err(Error::Unauthorized);}
            Ok(bytes)
        }=>r}
    }
    pub(super) async fn quiz_image(
        &self,
        url: &str,
        cancel: &CancellationToken,
    ) -> Result<String, Error> {
        use base64::Engine;
        if url.starts_with("data:image/") {
            let (header, data) = url.split_once(',').ok_or(Error::Invalid)?;
            if !header.ends_with(";base64") || data.len() > 1400000 {
                return Err(Error::Invalid);
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| Error::Invalid)?;
            let format = image::guess_format(&bytes).map_err(|_| Error::Invalid)?;
            if !matches!(
                format,
                image::ImageFormat::Png
                    | image::ImageFormat::Jpeg
                    | image::ImageFormat::WebP
                    | image::ImageFormat::Gif
            ) {
                return Err(Error::Invalid);
            }
            return Ok(format!(
                "data:{};base64,{}",
                format.to_mime_type(),
                base64::engine::general_purpose::STANDARD.encode(bytes)
            ));
        }
        let target = reqwest::Url::parse(url).map_err(|_| Error::Invalid)?;
        if !matches!(target.scheme(), "http" | "https")
            || !matches!(
                target.host_str(),
                Some("www.uooc.net.cn" | "assets.uooconline.com")
            )
            || !target.username().is_empty()
            || target.password().is_some()
        {
            return Err(Error::Invalid);
        }
        tokio::select! {biased;_=cancel.cancelled()=>Err(Error::Cancelled),r=async{
            let mut response=self.http.get(target).send().await.map_err(|_|Error::Network)?;
            if !response.status().is_success(){return Err(Error::Network);}
            let mut bytes=vec![];while let Some(chunk)=response.chunk().await.map_err(|_|Error::Network)?{if bytes.len()+chunk.len()>1024*1024{return Err(Error::Invalid);}bytes.extend_from_slice(&chunk);}
            let format=image::guess_format(&bytes).map_err(|_|Error::Invalid)?;
            if !matches!(format,image::ImageFormat::Png|image::ImageFormat::Jpeg|image::ImageFormat::WebP|image::ImageFormat::Gif){return Err(Error::Invalid);}
            Ok(format!("data:{};base64,{}",format.to_mime_type(),base64::engine::general_purpose::STANDARD.encode(bytes)))
        }=>r}
    }
    pub fn new(jar: Arc<CookieStoreMutex>) -> Result<Self, String> {
        Self::with_base(jar, ORIGIN)
    }
    pub fn with_base(jar: Arc<CookieStoreMutex>, base: &str) -> Result<Self, String> {
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()?;
        let http = crate::netproxy::client_builder()
            .cookie_provider(jar.clone())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(super::user_agent::DESKTOP)
            .build()
            .map_err(|_| "无法初始化 UOOC 网络服务")?;
        Ok(Self {
            http,
            jar,
            base: base.into(),
        })
    }
    async fn request(
        &self,
        path: &str,
        fields: &[(String, String)],
        post: bool,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        self.request_with_referer(path, fields, post, None, cancel)
            .await
    }
    async fn request_with_referer(
        &self,
        path: &str,
        fields: &[(String, String)],
        post: bool,
        referer: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        // Course info is a GET with a sign-in side effect. Verify uncertain results,
        // rather than automatically repeating the operation like an ordinary read.
        let attempts = if post || path == "/home/course/info" {
            1
        } else {
            3
        };
        for attempt in 0..attempts {
            let request = if post {
                self.http.post(format!("{}{path}", self.base)).form(fields)
            } else {
                self.http.get(format!("{}{path}", self.base)).query(fields)
            }
            .header("Accept", "application/json, text/plain, */*")
            .header(
                "Referer",
                referer
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{}/home", self.base)),
            )
            .header("XSRF", "");
            let result = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                result = async {
                    let mut response = request.send().await.map_err(|_| Error::Network)?;
                    if response.status().is_redirection() {
                        return Err(if response.headers().get(reqwest::header::LOCATION).and_then(|value| value.to_str().ok())
                            .is_some_and(|location| super::user_agent::is_login_redirect(response.url(), location)) { Error::Unauthorized } else { Error::UnexpectedRedirect });
                    }
                    if response.status().as_u16() == 401 { return Err(Error::Unauthorized); }
                    if response.status().as_u16() == 403 { return Err(Error::Forbidden); }
                    if !response.status().is_success() { return Err(Error::Network); }
                    let mut bytes = Vec::new();
                    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
                        if bytes.len() + chunk.len() > 4 * 1024 * 1024 { return Err(Error::Invalid); }
                        bytes.extend_from_slice(&chunk);
                    }
                    let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
                    let code = text(&value["code"])
                        .parse::<i64>()
                        .map_err(|_| Error::Invalid)?;
                    match code {
                        1 => Ok(value["data"].clone()),
                        401 => Err(Error::Unauthorized),
                        600 => Err(Error::Gate),
                        other => {
                            let msg = text(&value["msg"]);
                            let details: Vec<String> = if let Some(arr) = value["data"].as_array() {
                                arr.iter().map(text).filter(|s| !s.is_empty()).collect()
                            } else if let Some(obj) = value["data"].as_object() {
                                obj.values().map(text).filter(|s| !s.is_empty()).collect()
                            } else {
                                let d = text(&value["data"]);
                                if !d.is_empty() { vec![d] } else { vec![] }
                            };
                            Err(Error::Server(other, msg, details))
                        }
                    }
                } => result,
            };
            if result != Err(Error::Network) || attempt + 1 == attempts {
                return result;
            }
            tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(Duration::from_secs(1 << attempt)) => {} }
        }
        Err(Error::Network)
    }
    pub async fn signin(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<CourseSignin, Error> {
        // Official course/router.js enters via course/info. Live-account observations:
        // data.signin: 0 = not required, 1 = newly signed, 2 = already signed.
        // This marker is distinct from the assessment weight in course/progress.
        let data = self
            .request_with_referer(
                "/home/course/info",
                &[("cid".into(), cid.into())],
                false,
                Some(&format!("{}/home/course/{cid}", self.base)),
                cancel,
            )
            .await?;
        match text(&data["signin"]).as_str() {
            "0" => Ok(CourseSignin::NotRequired),
            "1" => Ok(CourseSignin::Signed),
            "2" => Ok(CourseSignin::AlreadySigned),
            _ => Err(Error::Invalid),
        }
    }
    pub async fn user(&self, cancel: &CancellationToken) -> Result<User, Error> {
        let value = self
            .request("/home/member/user", &[], false, cancel)
            .await?;
        Ok(User {
            id: id(&value["id"]).ok_or(Error::Unauthorized)?,
            name: ["name", "nick"]
                .iter()
                .map(|k| text(&value[*k]))
                .find(|v| !v.is_empty())
                .unwrap_or_else(|| "UOOC 用户".into()),
        })
    }
    pub async fn courses(&self, cancel: &CancellationToken) -> Result<Vec<Course>, Error> {
        self.course_pages(cancel, false).await
    }
    /// 全部已学课程，包含已结课/已退课等非开课中状态。
    pub async fn learned_courses(&self, cancel: &CancellationToken) -> Result<Vec<Course>, Error> {
        self.course_pages(cancel, true).await
    }
    async fn course_pages(
        &self,
        cancel: &CancellationToken,
        include_all_statuses: bool,
    ) -> Result<Vec<Course>, Error> {
        let mut courses = vec![];
        let mut seen = HashSet::new();
        let mut page = 1;
        loop {
            let data = self
                .request(
                    LIST,
                    &[
                        ("keyword".into(), "".into()),
                        ("page".into(), page.to_string()),
                        ("type".into(), "learn".into()),
                    ],
                    false,
                    cancel,
                )
                .await?;
            let (rows, pages) = if include_all_statuses {
                learned_courses_page(&data)
            } else {
                courses_page(&data)
            }
            .map_err(|_| Error::Invalid)?;
            courses.extend(rows.into_iter().filter(|c| seen.insert(c.id.clone())));
            if page >= pages {
                break;
            }
            page += 1;
        }
        Ok(courses)
    }
    pub async fn progress(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<super::progress::CourseProgress, Error> {
        let value = self
            .request(
                "/home/course/progress",
                &[("cid".into(), cid.into())],
                false,
                cancel,
            )
            .await?;
        if !value.is_object() {
            return Err(Error::Invalid);
        }
        Ok(super::progress::CourseProgress::parse(
            &value,
            super::progress::today(),
        ))
    }
    pub async fn thread_list(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<ThreadSummary>, Error> {
        let value = self
            .request(
                "/Home/Threads/list",
                &[
                    ("cid".into(), cid.into()),
                    ("onlyteacher".into(), "0".into()),
                    ("page".into(), "1".into()),
                    ("pagesize".into(), "12".into()),
                    ("type".into(), "30".into()),
                ],
                false,
                cancel,
            )
            .await?;
        let mut tids = Vec::new();
        if let Some(rows) = thread_rows(&value) {
            for row in rows {
                if let Some(tid) = [
                    "tid",
                    "thread_id",
                    "threadId",
                    "threadid",
                    "topic_id",
                    "topicId",
                    "topicid",
                    "id",
                ]
                .iter()
                .find_map(|key| id(&row[*key]))
                {
                    tids.push(tid);
                }
            }
        }
        if tids.is_empty() {
            collect_thread_ids(&value, &mut tids, 0);
        }
        Ok(tids
            .into_iter()
            .map(|tid| ThreadSummary { tid })
            .collect())
    }
    pub async fn thread_details(
        &self,
        cid: &str,
        tid: &str,
        cancel: &CancellationToken,
    ) -> Result<String, Error> {
        let value = self
            .request(
                "/Home/Threads/details",
                &[
                    ("ask_page".into(), "1".into()),
                    ("ask_pagesize".into(), "10".into()),
                    ("cid".into(), cid.into()),
                    ("page".into(), "1".into()),
                    ("pagesize".into(), "20".into()),
                    ("tid".into(), tid.into()),
                ],
                false,
                cancel,
            )
            .await?;
        find_thread_content(&value, 0).ok_or(Error::Invalid)
    }
    pub async fn reply_thread(
        &self,
        cid: &str,
        tid: &str,
        content: &str,
        cancel: &CancellationToken,
    ) -> Result<(), Error> {
        self.request(
            "/Home/Threads/reply",
            &[
                ("cid".into(), cid.into()),
                ("content".into(), content.into()),
                ("tid".into(), tid.into()),
            ],
            true,
            cancel,
        )
        .await
        .map(|_| ())
    }
    pub async fn post_thread(
        &self,
        cid: &str,
        content: &str,
        subject: &str,
        cancel: &CancellationToken,
    ) -> Result<(), Error> {
        self.request(
            "/Home/Threads/post",
            &[
                ("cid".into(), cid.into()),
                ("content".into(), content.into()),
                ("subject".into(), subject.into()),
                ("type".into(), "30".into()),
            ],
            true,
            cancel,
        )
        .await
        .map(|_| ())
    }
    pub async fn catalog(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        self.request(
            "/home/learn/getCatalogList",
            &[
                ("cid".into(), cid.into()),
                ("hidemsg_".into(), "true".into()),
            ],
            false,
            cancel,
        )
        .await?
        .as_array()
        .cloned()
        .ok_or(Error::Invalid)
    }
    pub async fn unit(
        &self,
        video: &Video,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let mut fields = vec![
            ("cid".into(), video.cid.clone()),
            ("chapter_id".into(), video.chapter.clone()),
            ("section_id".into(), video.section.clone()),
            (
                "catalog_id".into(),
                video.subsection.as_ref().unwrap_or(&video.section).clone(),
            ),
            ("hidemsg_".into(), "true".into()),
            ("show".into(), "".into()),
        ];
        if let Some(sub) = &video.subsection {
            fields.push(("subsection_id".into(), sub.clone()));
        }
        self.request("/home/learn/getUnitLearn", &fields, false, cancel)
            .await?
            .as_array()
            .cloned()
            .ok_or(Error::Invalid)
    }
    pub async fn mark(
        &self,
        video: &Video,
        duration: f64,
        position: f64,
        cancel: &CancellationToken,
    ) -> Result<bool, Error> {
        let mut fields = vec![
            ("cid".into(), video.cid.clone()),
            ("chapter_id".into(), video.chapter.clone()),
            ("section_id".into(), video.section.clone()),
            ("resource_id".into(), video.resource.clone()),
            ("hidemsg_".into(), "true".into()),
            ("network".into(), "2".into()),
            ("source".into(), "1".into()),
            ("video_length".into(), format!("{duration:.2}")),
            ("video_pos".into(), format!("{position:.2}")),
        ];
        if let Some(sub) = &video.subsection {
            fields.push(("subsection_id".into(), sub.clone()));
        }
        let result = self
            .request("/home/learn/markVideoLearn", &fields, true, cancel)
            .await?;
        if result.get("finished").is_none() {
            return Err(Error::Invalid);
        }
        Ok(number(&result["finished"]) != 0.0)
    }
}
