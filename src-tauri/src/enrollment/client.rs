use super::{
    model::*,
    protocol::{self, Error, Failure, Result},
    session,
};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::{json, Value};
use std::sync::Arc;
#[derive(Clone)]
pub struct Client {
    pub jar: Arc<CookieStoreMutex>,
    pub http: reqwest::blocking::Client,
    pub mode: Mode,
    pub token: String,
    pub student: String,
    pub generation: u64,
}
impl Client {
    pub fn new(secrets: &session::Secrets, mode: Mode, fresh: bool) -> Result<Self> {
        let jar = session::school_jar(secrets, mode, fresh)?;
        Ok(Self {
            http: session::client(jar.clone())?,
            jar,
            mode,
            student: secrets.student.clone(),
            generation: 0,
            token: if fresh {
                String::new()
            } else {
                match mode {
                    Mode::Campus => secrets.campus_token.clone(),
                    Mode::Webvpn => secrets.webvpn_token.clone(),
                }
            },
        })
    }
    fn response(
        &self,
        path: &str,
        form: &[(String, String)],
    ) -> Result<reqwest::blocking::Response> {
        let response = self
            .http
            .post(format!("{}/xsxkapp/sys/xsxkapp/{path}", self.mode.origin()))
            .header("token", &self.token)
            .header("Origin", self.mode.origin())
            .header(
                "Referer",
                format!(
                    "{}/xsxkapp/sys/xsxkapp/*default/index.do",
                    self.mode.origin()
                ),
            )
            .form(form)
            .send()
            .map_err(|_| Error::new(Failure::Network, "请求超时或网络中断，等待学校响应"))?;
        Ok(response)
    }
    pub fn quiet_post(&self, path: &str, form: &[(String, String)], seconds: u64) -> Result<Value> {
        let r = self
            .http
            .post(format!(
                "{}/xsxkapp/sys/xsxkapp/{path}?timestamp={}",
                self.mode.origin(),
                chrono::Utc::now().timestamp_millis()
            ))
            .header("token", &self.token)
            .header("Origin", self.mode.origin())
            .form(form)
            .timeout(std::time::Duration::from_secs(seconds))
            .send()
            .map_err(|_| Error::new(Failure::Network, "学校信息暂时无法获取"))?;
        decode(r)
    }
    pub fn keepalive(&self, selected: bool) -> Result<()> {
        if selected {
            self.quiet_post(
                "elective/courseResult.do",
                &[("studentCode".into(), self.student.clone())],
                5,
            )?;
        } else {
            // This endpoint alone interprets code 2 as admission restriction.
            let path = format!("student/{}.do", self.student);
            let r = self
                .http
                .post(format!("{}/xsxkapp/sys/xsxkapp/{path}", self.mode.origin()))
                .header("token", &self.token)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .map_err(|_| Error::new(Failure::Network, "保活请求暂时不可用"))?;
            let status = r.status().as_u16();
            let location = r
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned();
            let body = r
                .text()
                .map_err(|_| Error::new(Failure::Network, "保活响应读取失败"))?;
            protocol::classify_student(status, &location, &body)?;
        }
        Ok(())
    }
    pub fn post(&self, path: &str, form: &[(String, String)]) -> Result<Value> {
        decode(self.response(path, form)?)
    }
    pub fn from_jar(
        mode: Mode,
        token: String,
        jar: Arc<CookieStoreMutex>,
        student: String,
    ) -> Result<Self> {
        Ok(Self {
            http: session::client(jar.clone())?,
            jar,
            mode,
            token,
            student,
            generation: 0,
        })
    }
    pub fn student_info(&self, student: &str) -> Result<Value> {
        if self.token.is_empty() {
            return Err(Error::new(Failure::Session, "尚未登录选课系统"));
        }
        let response = self.response(&format!("student/{student}.do"), &[])?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let body = response
            .text()
            .map_err(|_| Error::new(Failure::Network, "学生信息响应读取失败"))?;
        let value = protocol::classify_student(status, &location, &body)?;
        let data = value["data"].clone();
        let id = protocol::field(&data, &["code", "number"]);
        if id != student {
            return Err(Error::new(Failure::Permanent, "选课账户与当前账户不一致"));
        }
        Ok(data)
    }
    pub fn context(&self, student: &str) -> Result<Context> {
        let value = self.student_info(student)?;
        let data = &value;
        let batch = &data["electiveBatch"];
        let batch_id = protocol::text(&batch["code"]);
        if batch_id.is_empty() {
            return Err(Error::new(
                Failure::Initializing,
                "当前无可用选课批次，可能尚未开放或初始化中",
            ));
        }
        let id = protocol::field(data, &["code", "number"]);
        if !id.is_empty() && id != student {
            return Err(Error::new(Failure::Permanent, "选课账户与当前账户不一致"));
        }
        Ok(Context {
            student: student.into(),
            batch: batch_id,
            term: protocol::field(batch, &["schoolTerm", "schoolTermCode"]),
            campus: protocol::field(data, &["campus"]),
        })
    }
    pub fn options(&self) -> Result<Value> {
        let batches = self.post("elective/batch.do", &[])?;
        let dictionaries = self.post("publicinfo/dictionary.do", &[])?;
        let batches = batches["dataList"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "批次列表缺失"))?;
        let campuses = dictionaries["data"]["dictionaryList"]["XQ"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "校区列表缺失"))?;
        Ok(json!({"batches":batches,"campuses":campuses}))
    }
    pub fn catalog(&self, c: &Context, q: &Query) -> Result<(Vec<Course>, bool)> {
        if !["TJKC", "FANKC", "FAWKC", "XGXK", "TYKC", "FXKC", "MOOC"]
            .contains(&q.category.as_str())
        {
            return Err(Error::new(Failure::Protocol, "课程类别无效"));
        }
        let setting = json!({"data":{"studentCode":c.student,"campus":c.campus,"electiveBatchCode":c.batch,"isMajor":"1","teachingClassType":q.category,"checkConflict":"2","checkCapacity":"2","queryContent":q.keyword},"pageSize":"10","pageNumber":q.page,"order":"","orderBy":"courseNumber"});
        let v = self.post(
            if q.category == "TJKC" {
                "elective/recommendedCourse.do"
            } else {
                "elective/programCourse.do"
            },
            &[("querySetting".into(), setting.to_string())],
        )?;
        let rows = v["dataList"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "课程目录格式变化，保留缓存"))?;
        let mut courses = vec![];
        for row in rows {
            if let Some(classes) = row["tcList"].as_array() {
                for class in classes {
                    let mut merged = row.clone();
                    if let (Some(m), Some(c)) = (merged.as_object_mut(), class.as_object()) {
                        m.extend(c.clone());
                    }
                    if let Some(course) = protocol::course(&merged, &q.category) {
                        courses.push(course);
                    }
                }
            } else if let Some(c) = protocol::course(row, &q.category) {
                courses.push(c);
            }
        }
        Ok((courses, rows.len() < 10))
    }
    pub fn selected(&self, c: &Context) -> Result<Vec<Course>> {
        let v = self.post(
            "elective/courseResult.do",
            &[
                ("studentCode".into(), c.student.clone()),
                (
                    "timestamp".into(),
                    chrono::Utc::now().timestamp_millis().to_string(),
                ),
            ],
        )?;
        let rows = v["dataList"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "已选列表数据不完整"))?;
        rows.iter()
            .map(|v| {
                protocol::course(v, "")
                    .ok_or_else(|| Error::new(Failure::Protocol, "已选列表缺少教学班标识"))
            })
            .collect()
    }
    pub fn capacity(&self, c: &Context, course: &Course) -> Result<i64> {
        let v = self.post(
            "elective/teachingclass/capacity.do",
            &[
                ("teachingClassId".into(), course.id.clone()),
                ("batchCode".into(), c.batch.clone()),
            ],
        )?;
        let data = &v["data"];
        let number = |key: &str| {
            data[key]
                .as_i64()
                .or_else(|| data[key].as_str()?.parse().ok())
        };
        if let Some(n) = number("remainingCapacity") {
            return Ok(n.max(0));
        }
        let (capacity, enrolled) = match course.main_select {
            Some(true) => ("mainClassCapacity", "mainElectiveNumber"),
            Some(false) => ("nonMainClassCapacity", "nonMainElectiveNumber"),
            None => return Err(Error::new(Failure::Protocol, "未确认主选范围，不执行退课")),
        };
        number(capacity)
            .zip(number(enrolled))
            .map(|(a, b)| (a - b).max(0))
            .ok_or_else(|| Error::new(Failure::Protocol, "余量响应无法核实，不执行退课"))
    }
    pub fn labs(&self, c: &Context, course: &Course) -> Result<Vec<Vec<Course>>> {
        let v = self.post(
            "elective/testCourse.do",
            &[
                ("jxbid".into(), course.id.clone()),
                ("electiveBatchCode".into(), c.batch.clone()),
                ("studentCode".into(), c.student.clone()),
                ("isMajor".into(), "1".into()),
                ("teachingClassType".into(), course.category.clone()),
                ("campus".into(), c.campus.clone()),
                ("checkCapacity".into(), "0".into()),
                ("checkConflict".into(), "0".into()),
            ],
        )?;
        let rows = v["dataList"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "实验班数据不完整"))?;
        rows.iter()
            .map(|r| {
                let classes = r["tcList"]
                    .as_array()
                    .ok_or_else(|| Error::new(Failure::Protocol, "实验班列表缺失"))?;
                classes
                    .iter()
                    .map(|v| {
                        let mut merged = r.clone();
                        merged
                            .as_object_mut()
                            .ok_or_else(|| Error::new(Failure::Protocol, "实验课程格式无效"))?
                            .extend(
                                v.as_object()
                                    .ok_or_else(|| {
                                        Error::new(Failure::Protocol, "实验教学班格式无效")
                                    })?
                                    .clone(),
                            );
                        protocol::course(&merged, &course.category)
                            .ok_or_else(|| Error::new(Failure::Protocol, "实验教学班标识缺失"))
                    })
                    .collect()
            })
            .collect()
    }
    pub fn process(&self) -> Result<Value> {
        let contract = super::contract::current();
        let path = contract.process_path.ok_or_else(|| {
            Error::new(
                Failure::Protocol,
                "官方异步结果接口尚未识别，仅核验已选列表",
            )
        })?;
        self.post(&path, &[])
    }
    pub fn submit(&self, c: &Context, course: &Course, dropping: bool) -> Result<Value> {
        if !dropping && course.has_test && course.test_ids.is_empty() {
            return Err(Error::new(Failure::Permanent, "请先选择关联实验教学班"));
        }
        let contract = super::contract::current();
        self.post(
            if dropping {
                contract
                    .drop_path
                    .as_deref()
                    .unwrap_or("elective/deleteVolunteer.do")
            } else {
                "elective/volunteer.do"
            },
            &[(
                if dropping { "deleteParam" } else { "addParam" }.into(),
                protocol::selection(c, course, dropping).to_string(),
            )],
        )
    }
    pub fn captcha(&self) -> Result<(String, Vec<u8>)> {
        let v = self.post("student/4/vcode.do", &[])?;
        let token = protocol::field(&v["data"], &["token", "vtoken"]);
        let token = if token.is_empty() {
            protocol::field(&v, &["token", "vtoken"])
        } else {
            token
        };
        if token.is_empty() || token.len() > 512 {
            return Err(Error::new(Failure::Protocol, "验证码令牌缺失"));
        }
        let r = self
            .http
            .get(format!(
                "{}/xsxkapp/sys/xsxkapp/student/vcode/image.do",
                self.mode.origin()
            ))
            .query(&[("vtoken", &token)])
            .send()
            .map_err(|_| Error::new(Failure::Network, "验证码请求失败"))?;
        if !r.status().is_success() {
            return Err(Error::new(Failure::Network, "验证码暂不可用"));
        }
        let image_url = r.url().clone();
        if !self
            .jar
            .lock()
            .unwrap()
            .get_request_values(&image_url)
            .any(|(name, _)| matches!(name, "route" | "insert_cookie"))
        {
            return Err(Error::new(Failure::Protocol, "验证码响应缺少会话Cookie"));
        }
        let bytes = r
            .bytes()
            .map_err(|_| Error::new(Failure::Network, "验证码图片读取失败"))?;
        if bytes.len() > 1024 * 1024 {
            return Err(Error::new(Failure::Protocol, "验证码图片过大"));
        }
        Ok((token, bytes.to_vec()))
    }
    pub fn login(
        &mut self,
        student: &str,
        password: &str,
        token: &str,
        points: &[[u32; 2]],
    ) -> Result<()> {
        let v = self.post(
            "student/check/login.do",
            &[
                ("loginName".into(), student.into()),
                ("loginPwd".into(), super::captcha::password(password)),
                ("vtoken".into(), token.into()),
                (
                    "verifyCode".into(),
                    super::captcha::coordinates(points).map_err(Error::from)?,
                ),
            ],
        )?;
        let id = protocol::field(&v["data"], &["number", "code"]);
        if id != student {
            return Err(Error::new(Failure::Permanent, "登录账户不匹配"));
        }
        self.student = student.to_owned();
        self.token = protocol::text(&v["data"]["token"]);
        if self.token.is_empty() {
            return Err(Error::new(Failure::Protocol, "登录响应缺少令牌"));
        }
        Ok(())
    }
}
pub fn decode(response: reqwest::blocking::Response) -> Result<Value> {
    let status = response.status().as_u16();
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = response
        .text()
        .map_err(|_| Error::new(Failure::Network, "响应读取中断"))?;
    protocol::classify(status, &location, &body)
}
