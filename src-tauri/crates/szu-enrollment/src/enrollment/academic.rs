use super::{
    client::decode,
    protocol::{self, Error, Failure, Result},
    session,
};
use serde_json::{json, Value};
use std::sync::Mutex;

pub struct Academic {
    client: reqwest::blocking::Client,
    /// 当前应用的角色入口（带 `gid_`）；模块 API 必须用它作为 Referer。
    referer: Mutex<Option<String>>,
}
const EHALL_BASE: &str = "https://ehall.szu.edu.cn";
/// eHall「我的课表」微应用。
const WDKB_ENTRY: &str = "https://ehall.szu.edu.cn/jwapp/sys/wdkb/*default/index.do";
const WDKB_APP_ID: &str = "4770397878132218";
/// eHall「学业完成查询」微应用（appId 来自 `serviceCenterData.json`）。
const XYWCX_ENTRY: &str = "https://ehall.szu.edu.cn/jwapp/sys/xywccx/*default/index.do";
const XYWCX_APP_ID: &str = "4980269146247992";
const MAX_REDIRECTS: usize = 15;

impl Academic {
    /// 用 authserver 的统一认证 SSO 自动建立 eHall 会话。
    ///
    /// 与 lms/jxxk/qw 同一套机制：从 vault 投影 authserver cookie，访问
    /// eHall 时手动跟随 CAS 跳转；CASTGC 有效时自动完成 service ticket
    /// 登录，失效时返回 NeedsAuth 类错误。
    pub fn from_app(app: &tauri::AppHandle) -> Result<Self> {
        let sso = szu_core::auth::reusable_sso_jar(app)
            .map_err(|message| Error::new(Failure::Session, message))?;
        Ok(Self {
            client: session::client(sso)?,
            referer: Mutex::new(None),
        })
    }

    /// 建立指定 eHall 应用会话。
    ///
    /// 顺序与真实浏览器一致：
    /// 1. GET 应用入口并跟随 CAS，拿到 `MOD_AUTH_CAS`（最终 403 也视为 CAS 成功，
    ///    因为应用还需要角色）；
    /// 2. GET `appShow?appId=...` 初始化应用上下文；
    /// 3. GET `appMultiGroupEntranceList` 取带 `gid_` 的角色入口；
    /// 4. GET 角色入口（`gid_`）建立角色会话，并把它作为后续模块 API 的 Referer。
    fn bootstrap(&self, entry: &str, app_id: &str) -> Result<()> {
        let mut url = reqwest::Url::parse(entry)
            .map_err(|_| Error::new(Failure::Protocol, "教务入口地址无效"))?;
        let mut cas_ok = false;
        for _ in 0..=MAX_REDIRECTS {
            let response = self
                .client
                .get(url.clone())
                .header("Referer", "https://ehall.szu.edu.cn/")
                .send()
                .map_err(|_| Error::new(Failure::Network, "教务系统暂时无法访问"))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| Error::new(Failure::Protocol, "教务登录跳转缺少目标地址"))?;
                let next = url
                    .join(location)
                    .map_err(|_| Error::new(Failure::Protocol, "教务登录跳转地址无效"))?;
                if !trusted_host(&next) {
                    return Err(Error::new(
                        Failure::Session,
                        "教务登录跳转到了不受信任的地址，请重新认证",
                    ));
                }
                url = next;
                continue;
            }
            if url.host_str() != Some("ehall.szu.edu.cn") || url.path().starts_with("/login") {
                return Err(Error::new(
                    Failure::Session,
                    "统一认证会话已失效，请重新登录应用",
                ));
            }
            // wdkb 入口在 CAS 成功后仍可能 403（缺应用角色），此时 CAS 会话已建立。
            cas_ok = true;
            break;
        }
        if !cas_ok {
            return Err(Error::new(Failure::Session, "教务登录跳转次数过多，请重新认证"));
        }

        let app_show = self
            .client
            .get(format!("{EHALL_BASE}/appShow?appId={app_id}"))
            .header("Referer", format!("{EHALL_BASE}/new/index.html"))
            .send()
            .map_err(|_| Error::new(Failure::Network, "教务应用入口暂时无法访问"))?;
        let app_show_location = app_show
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if app_show_location.is_none() && !app_show.status().is_success() {
            return Err(Error::new(
                Failure::Session,
                "教务应用入口拒绝访问，请重新登录应用",
            ));
        }

        let roles = self
            .client
            .get(format!(
                "{EHALL_BASE}/appMultiGroupEntranceList?appId={app_id}&param=&r_t={}",
                chrono::Utc::now().timestamp_millis()
            ))
            .header("X-Requested-With", "XMLHttpRequest")
            .header(
                "Referer",
                format!("{EHALL_BASE}/portal/html/select_role.html?appId={app_id}"),
            )
            .send()
            .map_err(|_| Error::new(Failure::Network, "教务角色列表暂时无法访问"))?;
        if !roles.status().is_success() {
            return Err(Error::new(
                Failure::Session,
                "教务角色列表拒绝访问，请重新登录应用",
            ));
        }
        let value: Value = roles
            .json()
            .map_err(|_| Error::new(Failure::Protocol, "教务角色列表格式变化"))?;
        let groups = value["data"]["groupList"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Session, "当前账号没有可用的「我的课表」角色"))?;
        let target = groups
            .iter()
            .find(|group| {
                let name = protocol::text(&group["groupName"]);
                name.contains("学生") || name.contains("本科")
            })
            .or_else(|| groups.first())
            .map(|group| protocol::text(&group["targetUrl"]).replace("&amp;", "&"))
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| Error::new(Failure::Session, "当前账号没有可用的「我的课表」角色"))?;

        let target_url = reqwest::Url::parse(&target)
            .map_err(|_| Error::new(Failure::Protocol, "教务角色入口地址无效"))?;
        let mut url = target_url.clone();
        let mut role_ok = false;
        for _ in 0..=MAX_REDIRECTS {
            let response = self
                .client
                .get(url.clone())
                .header("Referer", format!("{EHALL_BASE}/new/index.html"))
                .send()
                .map_err(|_| Error::new(Failure::Network, "教务角色入口暂时无法访问"))?;
            let status = response.status();
            if status.is_redirection() {
                let location = response
                    .headers()
                    .get(reqwest::header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| Error::new(Failure::Protocol, "教务角色跳转缺少目标地址"))?;
                url = url
                    .join(location)
                    .map_err(|_| Error::new(Failure::Protocol, "教务角色跳转地址无效"))?;
                continue;
            }
            if !status.is_success() || url.host_str() != Some("ehall.szu.edu.cn") {
                return Err(Error::new(
                    Failure::Session,
                    "教务角色入口拒绝访问，请重新登录应用",
                ));
            }
            role_ok = true;
            break;
        }
        if !role_ok {
            return Err(Error::new(Failure::Session, "教务角色跳转次数过多，请重新认证"));
        }
        *self.referer.lock().unwrap() = Some(target);
        Ok(())
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
        self.post_path(&format!("/jwapp/sys/xywccx/modules/xywccx/{name}.do"), form)
    }

    fn referer(&self, fallback: &str) -> String {
        self.referer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| fallback.to_string())
    }

    /// xywccx 微应用要求 X-Requested-With 与角色入口 Referer。
    fn post_path(&self, path: &str, form: &[(String, String)]) -> Result<Value> {
        let response = self
            .client
            .post(format!("https://ehall.szu.edu.cn{path}"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", self.referer(XYWCX_ENTRY))
            .form(form)
            .send()
            .map_err(|_| Error::new(Failure::Network, "教务系统暂时无法访问"))?;
        let status = response.status();
        if status.as_u16() >= 500 {
            return Err(Error::new(Failure::Network, "教务系统暂时不可用"));
        }
        if status.is_redirection() || !status.is_success() {
            return Err(Error::new(
                Failure::Session,
                "学业接口拒绝访问，请重新登录应用后再试",
            ));
        }
        decode(response)
    }

    /// 金智 wdkb 微应用要求 X-Requested-With 与 Referer。
    fn post_wisedu(&self, path: &str, form: &[(String, String)]) -> Result<Value> {
        let response = self
            .client
            .post(format!("https://ehall.szu.edu.cn{path}"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", self.referer(WDKB_ENTRY))
            .form(form)
            .send()
            .map_err(|_| Error::new(Failure::Network, "教务课表暂时无法访问"))?;
        let status = response.status();
        if status.is_redirection() {
            return Err(Error::new(
                Failure::Session,
                "教务会话已失效，请重新登录应用后再试",
            ));
        }
        if !status.is_success() {
            return Err(Error::new(
                Failure::Session,
                format!("教务接口返回 HTTP {}，请重新登录应用后再试", status.as_u16()),
            ));
        }
        decode(response)
    }

    /// 上课期间的学期固定课表（eHall `wdkb/xskcb`）。
    ///
    /// 与 sleepy 的 SZU 导入一致：先 GET `wdkb` 入口初始化微应用会话，
    /// 再 POST `dqxnxq.do` 取当前学期，最后 POST `xskcb.do`。
    pub fn semester_schedule(
        &self,
        preferred_term: Option<&str>,
    ) -> Result<super::model::SemesterSchedule> {
        self.bootstrap(WDKB_ENTRY, WDKB_APP_ID)?;

        let term_list = self.post_wisedu("/jwapp/sys/wdkb/modules/jshkcb/dqxnxq.do", &[])?;
        let rows = term_list["datas"]["dqxnxq"]["rows"]
            .as_array()
            .ok_or_else(|| Error::new(Failure::Protocol, "教务学期列表格式变化，请稍后重试"))?;
        let term = select_term(rows, preferred_term)?;

        let value = self.post_wisedu(
            "/jwapp/sys/wdkb/modules/xskcb/xskcb.do",
            &[("XNXQDM".into(), term.clone())],
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
        self.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID)?;
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
        self.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID)?;
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
        self.bootstrap(XYWCX_ENTRY, XYWCX_APP_ID)?;
        self.post("byscjd", &[("ZXJDKEY".into(), format!("BYSC_{student}"))])
    }
}
fn trusted_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str().map(|host| host.trim_end_matches('.')) else {
        return false;
    };
    host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")
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
