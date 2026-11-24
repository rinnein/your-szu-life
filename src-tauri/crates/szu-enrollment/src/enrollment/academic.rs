use super::{
    ehall::Ehall,
    protocol::{self, Error, Failure, Result},
};
use serde_json::{json, Value};

pub struct Academic {
    ehall: Ehall,
}
/// eHall「我的课表」微应用。
const WDKB_ENTRY: &str = "https://ehall.szu.edu.cn/jwapp/sys/wdkb/*default/index.do";
const WDKB_APP_ID: &str = "4770397878132218";
/// eHall「学业完成查询」微应用（appId 来自 `serviceCenterData.json`）。
const XYWCX_ENTRY: &str = "https://ehall.szu.edu.cn/jwapp/sys/xywccx/*default/index.do";
const XYWCX_APP_ID: &str = "4980269146247992";

impl Academic {
    /// 用 authserver 的统一认证 SSO 自动建立 eHall 会话。
    pub fn from_app(app: &tauri::AppHandle) -> Result<Self> {
        Ok(Self {
            ehall: Ehall::from_app(app)?,
        })
    }

    /// 学业方案上下文（学号 + 培养方案 + 年级 + 学生类别）。
    ///
    /// 直接读 `cxscfa` 当前方案行：`cxxsscfa`（方案列表）在新版 eHall 会返回
    /// 403，且手动认证窗口捕获的那份上下文可能已随重算变化，不再使用。
    fn context(&self, student: &str) -> Result<Value> {
        let value = self.post("cxscfa", &[("XH".into(), student.into())])?;
        rows(&value, "cxscfa")?
            .first()
            .cloned()
            .ok_or_else(|| Error::new(Failure::Protocol, "当前账号还没有可用的学业完成方案"))
    }
    pub fn post(&self, name: &str, form: &[(String, String)]) -> Result<Value> {
        self.ehall.post(
            &format!("/jwapp/sys/xywccx/modules/xywccx/{name}.do"),
            form,
            XYWCX_ENTRY,
        )
    }

    /// 上课期间的学期固定课表（eHall `wdkb/xskcb`）。
    ///
    /// 与 sleepy 的 SZU 导入一致：先 GET `wdkb` 入口初始化微应用会话，
    /// 再 POST `dqxnxq.do` 取当前学期，最后 POST `xskcb.do`。
    pub fn semester_schedule(
        &self,
        preferred_term: Option<&str>,
    ) -> Result<super::model::SemesterSchedule> {
        self.ehall
            .bootstrap(WDKB_ENTRY, WDKB_APP_ID, "我的课表")?;

        let term_list = self.ehall.post(
            "/jwapp/sys/wdkb/modules/jshkcb/dqxnxq.do",
            &[],
            WDKB_ENTRY,
        )?;
        let rows = term_list["datas"]["dqxnxq"]["rows"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "教务学期列表格式变化，请稍后重试"))?;
        let term = select_term(rows, preferred_term)?;

        let value = self.ehall.post(
            "/jwapp/sys/wdkb/modules/xskcb/xskcb.do",
            &[("XNXQDM".into(), term.clone())],
            WDKB_ENTRY,
        )?;
        let rows = value["datas"]["xskcb"]["rows"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "教务课表格式变化，请稍后重试"))?;
        let mut courses = Vec::new();
        for row in rows {
            let name = protocol::text(&row["KCM"]).trim().to_string();
            let weekday = protocol::text(&row["SKXQ"]).parse::<u8>().unwrap_or(0);
            let start_period = protocol::text(&row["KSJC"]).parse::<u8>().unwrap_or(0);
            let end_period = protocol::text(&row["JSJC"]).parse::<u8>().unwrap_or(0);
            if name.is_empty()
                || !(1..=7).contains(&weekday)
                || start_period == 0
                || end_period < start_period
                || end_period > 14
            {
                continue;
            }
            let raw_weeks = protocol::text(&row["ZCMC"]).trim().to_string();
            let weeks = parse_weeks(&protocol::text(&row["SKZC"]), &raw_weeks);
            if weeks.is_empty() {
                continue;
            }
            courses.push(super::model::SemesterCourse {
                name,
                teacher: protocol::text(&row["SKJS"]).trim().to_string(),
                classroom: protocol::text(&row["JASMC"]).trim().to_string(),
                weekday,
                start_period,
                end_period,
                weeks,
                raw_weeks,
            });
        }
        Ok(super::model::SemesterSchedule { term, courses })
    }
    pub fn load(&self, student: &str) -> Result<Value> {
        self.ehall.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID, "学业完成查询")?;
        let row = self.context(student)?;
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
        self.ehall.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID, "学业完成查询")?;
        let row = self.context(student)?;
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
        self.ehall.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID, "学业完成查询")?;
        self.post("byscjd", &[("ZXJDKEY".into(), format!("BYSC_{student}"))])
    }
}
/// 从 `dqxnxq.rows` 选当前学期。
///
/// 优先使用调用方给的学期（应用内 qw 课表的当前学期）；否则找 SFDQ/CURRENT/SFSY
/// 标记；最后才在唯一学期时取 rows[0]。禁止无条件取 rows[0]。
fn select_term(rows: &[Value], preferred: Option<&str>) -> Result<String> {
    let dm = |row: &Value| protocol::text(&row["DM"]).trim().to_string();
    if let Some(preferred) = preferred.map(str::trim).filter(|term| !term.is_empty()) {
        if rows.iter().any(|row| dm(row) == preferred) {
            return Ok(preferred.to_string());
        }
    }
    for row in rows {
        let current = ["SFDQ", "CURRENT", "SFSY"].iter().any(|key| {
            row.get(*key).is_some_and(|value| {
                value.as_str() == Some("1")
                    || value.as_i64() == Some(1)
                    || value.as_bool() == Some(true)
            })
        });
        if current {
            let term = dm(row);
            if !term.is_empty() {
                return Ok(term);
            }
        }
    }
    if rows.len() == 1 {
        let term = dm(&rows[0]);
        if !term.is_empty() {
            return Ok(term);
        }
    }
    Err(Error::new(
        Failure::Protocol,
        "无法识别当前教务学期，请先在教务页面选择学期后重试",
    ))
}

fn parse_weeks(mask: &str, raw: &str) -> Vec<u8> {
    let mut weeks = Vec::new();
    for (index, ch) in mask.trim().chars().enumerate() {
        if ch == '1' && index < 30 {
            weeks.push((index + 1) as u8);
        }
    }
    if !weeks.is_empty() {
        return weeks;
    }
    parse_raw_weeks(raw)
}

/// `SKZC` 缺失时的回退：解析 `1-16周`、`1-16周(单)`、`1-8,10-16周` 等文本。
fn parse_raw_weeks(raw: &str) -> Vec<u8> {
    let mut weeks = Vec::new();
    for token in raw.split(|ch: char| matches!(ch, ',' | '，' | '、' | ';' | '；' | ' ')) {
        if token.trim().is_empty() {
            continue;
        }
        let odd = token.contains('单');
        let even = token.contains('双');
        let numbers: Vec<u32> = token
            .split(|ch: char| !ch.is_ascii_digit())
            .filter_map(|part| part.parse::<u32>().ok())
            .collect();
        let (first, last) = match numbers.as_slice() {
            [] => continue,
            [value] => (*value, *value),
            [first, last, ..] => (*first, *last),
        };
        for week in first..=last.min(30) {
            if week == 0 || week > 30 {
                continue;
            }
            if (odd && week % 2 == 0) || (even && week % 2 == 1) {
                continue;
            }
            let week = week as u8;
            if !weeks.contains(&week) {
                weeks.push(week);
            }
        }
    }
    weeks.sort_unstable();
    weeks
}

fn rows<'a>(v: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    v["datas"][key]["rows"]
        .as_array()
        .ok_or_else(|| Error::new(Failure::Protocol, "学业接口格式变化，保留原数据"))
}

#[cfg(test)]
mod tests {
    use super::{parse_raw_weeks, parse_weeks};

    #[test]
    fn term_selection_prefers_known_term_then_current_marker() {
        use serde_json::json;
        let rows = vec![
            json!({"DM":"2025-2026-1","SFDQ":"0"}),
            json!({"DM":"2026-2027-1","SFDQ":"1"}),
        ];
        assert_eq!(
            super::select_term(&rows, Some("2025-2026-1")).unwrap(),
            "2025-2026-1"
        );
        assert_eq!(super::select_term(&rows, None).unwrap(), "2026-2027-1");
        assert!(super::select_term(&rows, Some("2024-2025-1")).is_ok());
        let ambiguous = vec![json!({"DM":"2025-2026-1"}), json!({"DM":"2026-2027-1"})];
        assert!(super::select_term(&ambiguous, None).is_err());
        assert_eq!(
            super::select_term(&[json!({"DM":"2026-2027-2"})], None).unwrap(),
            "2026-2027-2"
        );
    }

    #[test]
    fn week_mask_wins_over_text() {
        assert_eq!(parse_weeks("1101", "1-4周"), vec![1, 2, 4]);
        assert_eq!(parse_weeks("", "1-3周"), vec![1, 2, 3]);
    }

    #[test]
    fn raw_weeks_support_ranges_odd_even_and_segments() {
        assert_eq!(parse_raw_weeks("1-4周(单)"), vec![1, 3]);
        assert_eq!(parse_raw_weeks("1-4周(双)"), vec![2, 4]);
        assert_eq!(parse_raw_weeks("1-3,5周"), vec![1, 2, 3, 5]);
        assert_eq!(parse_raw_weeks("第2周"), vec![2]);
        assert!(parse_raw_weeks("待定").is_empty());
    }

    #[test]
    fn weeks_are_capped_at_thirty() {
        assert_eq!(parse_raw_weeks("1-40周").last(), Some(&30));
    }
}
