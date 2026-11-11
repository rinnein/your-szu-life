use super::{
    client::decode,
    protocol::{self, Error, Failure, Result},
    session,
};
use serde_json::{json, Value};
pub struct Academic {
    client: reqwest::blocking::Client,
    context: Option<Value>,
}
impl Academic {
    pub fn new(raw: &str) -> Result<Self> {
        Ok(Self {
            client: session::client(session::jar(raw)?)?,
            context: None,
        })
    }
    pub fn with_context(mut self, context: Option<Value>) -> Self {
        self.context = context;
        self
    }
    fn context(&self) -> Result<Value> {
        if let Some(context) = &self.context {
            return Ok(context.clone());
        }
        let value = self.post(
            "cxxsscfa",
            &[
                ("SCLBDM".into(), "04".into()),
                ("*order".into(), "-CZSJ".into()),
            ],
        )?;
        rows(&value, "cxxsscfa")?
            .first()
            .cloned()
            .ok_or_else(|| Error::new(Failure::Protocol, "请先在官方学业页面加载当前方案"))
    }
    pub fn post(&self, name: &str, form: &[(String, String)]) -> Result<Value> {
        let r = self
            .client
            .post(format!(
                "https://ehall.szu.edu.cn/jwapp/sys/xywccx/modules/xywccx/{name}.do"
            ))
            .form(form)
            .send()
            .map_err(|_| Error::new(Failure::Network, "学业系统暂时无法访问"))?;
        decode(r)
    }
    pub fn verify(&self, student: &str) -> Result<()> {
        let row = self.context()?;
        if protocol::text(&row["XH"]) != student {
            return Err(Error::new(Failure::Session, "学业查询账户不匹配"));
        }
        let form: Vec<_> = ["XH", "PYFADM", "BYNJDM", "SCLBDM"]
            .iter()
            .map(|k| (k.to_string(), protocol::text(&row[*k])))
            .collect();
        let result = self.post("cxscfa", &form)?;
        if rows(&result, "cxscfa")?.is_empty() {
            return Err(Error::new(Failure::Session, "尚未验证学业方案"));
        }
        Ok(())
    }
    pub fn load(&self, student: &str) -> Result<Value> {
        let row = self.context()?;
        if protocol::text(&row["XH"]) != student {
            return Err(Error::new(Failure::Permanent, "学业查询账户不匹配"));
        }
        let form: Vec<_> = ["XH", "PYFADM", "BYNJDM", "SCLBDM"]
            .iter()
            .map(|k| (k.to_string(), protocol::text(&row[*k])))
            .collect();
        let summary = self.post("cxscfa", &form)?;
        let modules = self.post("cxscfakz", &form)?;
        let mut output = vec![];
        for module in rows(&modules, "cxscfakz")? {
            let mut course_form = form.clone();
            for k in ["BY1", "KZH"] {
                course_form.push((k.into(), protocol::text(&module[k])));
            }
            course_form.extend([
                ("ISHIDE".into(), "0".into()),
                ("pageSize".into(), "100".into()),
                ("*order".into(), "+KCLBDM,+KCH".into()),
            ]);
            let mut courses = vec![];
            for page in 1..=100 {
                let mut f = course_form.clone();
                f.push(("pageNumber".into(), page.to_string()));
                let v = self.post("cxscfakzkc", &f)?;
                let r = rows(&v, "cxscfakzkc")?;
                courses.extend(r.clone());
                if r.len() < 100 {
                    break;
                }
                if page == 100 {
                    return Err(Error::new(Failure::Protocol, "学业课程分页超出上限"));
                }
            }
            output.push(json!({"module":module,"courses":courses}));
        }
        Ok(
            json!({"summary":rows(&summary,"cxscfa")?,"modules":output,"calculatedAt":row["CZSJ"],"updatedAt":chrono::Utc::now().timestamp_millis()}),
        )
    }
    pub fn recalculate(&self, student: &str) -> Result<Value> {
        let row = self.context()?;
        if protocol::text(&row["XH"]) != student {
            return Err(Error::new(Failure::Permanent, "学业账户不匹配"));
        }
        let form: Vec<_> = ["PYFADM", "BYNJDM", "SCLBDM"]
            .iter()
            .map(|k| (k.to_string(), protocol::text(&row[*k])))
            .collect();
        if form.iter().any(|(_, v)| v.is_empty()) {
            return Err(Error::new(Failure::Protocol, "学业方案参数缺失"));
        }
        let result = self.post("bysc", &form)?;
        if protocol::text(&result["datas"]["bysc"]["code"]) != "0" {
            return Err(Error::new(Failure::Business, "学校未确认重算请求"));
        }
        Ok(result)
    }
    pub fn progress(&self, student: &str) -> Result<Value> {
        self.post("byscjd", &[("ZXJDKEY".into(), format!("BYSC_{student}"))])
    }
}
fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v["datas"][key]["rows"]
        .as_array()
        .ok_or_else(|| Error::new(Failure::Protocol, "学业接口格式变化，保留原数据"))
}
