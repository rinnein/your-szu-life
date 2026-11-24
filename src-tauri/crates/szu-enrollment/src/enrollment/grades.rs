use super::{
    ehall::Ehall,
    model::{GradeItem, GradeReport, GradeTerm},
    protocol::{self, Error, Failure, Result},
};
use serde_json::Value;
use std::collections::BTreeMap;

/// eHall「成绩查询」微应用。
const CJCX_ENTRY: &str = "https://ehall.szu.edu.cn/jwapp/sys/cjcx/*default/index.do";
/// wisedu 标准成绩查询 appId；与学校当前部署不一致时会降级为只走 CAS 入口。
const CJCX_APP_ID: &str = "4768574631264620";
const CJCX_MODULE: &str = "/jwapp/sys/cjcx/modules/cjcx";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 50;

pub(super) struct Grades {
    ehall: Ehall,
}

impl Grades {
    pub fn from_app(app: &tauri::AppHandle) -> Result<Self> {
        Ok(Self {
            ehall: Ehall::from_app(app)?,
        })
    }

    /// 拉取全部课程成绩并计算学期/总 GPA。
    ///
    /// 只读网络，不修改任何教务状态。统一认证失效时返回 `Failure::Session`，
    /// 前端会提示重新登录应用。
    pub fn load(&self) -> Result<GradeReport> {
        if let Err(error) = self.ehall.bootstrap(CJCX_ENTRY, CJCX_APP_ID, "成绩查询") {
            // appId 与学校当前部署不一致时，模块 API 仍可能只认 CAS 会话；
            // 入口也建立不了才返回原始错误（通常是统一认证失效）。
            if self.ehall.bootstrap_entry(CJCX_ENTRY).is_err() {
                return Err(error);
            }
        }
        let rows = match self.rows() {
            Ok(rows) => rows,
            Err(primary) => {
                // 角色入口可能指向了别的微应用：清掉 Referer 后再用入口本身试一次。
                if self.ehall.bootstrap_entry(CJCX_ENTRY).is_err() {
                    return Err(primary);
                }
                match self.rows() {
                    Ok(rows) => rows,
                    Err(_) => return Err(primary),
                }
            }
        };
        let items = normalize(&rows);
        if items.is_empty() {
            return Err(Error::new(
                Failure::Protocol,
                "教务系统没有返回任何成绩，请确认当前账号已有成绩记录",
            ));
        }
        Ok(build_report(items))
    }

    /// 新版统计接口与旧版 `xscjcx` 之间的兜底。
    fn rows(&self) -> Result<Vec<Value>> {
        match self.paged_rows() {
            Ok(rows) if !rows.is_empty() => Ok(rows),
            Ok(_) => self.legacy_rows(),
            Err(primary) => match self.legacy_rows() {
                Ok(rows) if !rows.is_empty() => Ok(rows),
                _ => Err(primary),
            },
        }
    }

    fn paged_rows(&self) -> Result<Vec<Value>> {
        let mut rows = Vec::new();
        let mut previous: Option<String> = None;
        for page in 1..=MAX_PAGES {
            let form = vec![
                ("pageSize".into(), PAGE_SIZE.to_string()),
                ("pageNumber".into(), page.to_string()),
                ("*order".into(), "-XNXQDM,-KCH,-KXH".into()),
            ];
            let value =
                self.ehall
                    .post(&format!("{CJCX_MODULE}/xscjcxtjgl.do"), &form, CJCX_ENTRY)?;
            let Some(list) = value["datas"]["xscjcxtjgl"]["rows"].as_array() else {
                return Err(Error::new(
                    Failure::Protocol,
                    "成绩查询接口格式变化，正在尝试旧接口",
                ));
            };
            // 服务器忽略 pageNumber 时会反复返回同一页，直接收手避免无限重复。
            let signature = serde_json::to_string(list).unwrap_or_default();
            if previous.as_deref() == Some(signature.as_str()) {
                break;
            }
            previous = Some(signature);
            let total =
                number(&value["datas"]["xscjcxtjgl"]["totalSize"]).map(|total| total as u64);
            rows.extend(list.iter().cloned());
            if page_finished(list.len(), rows.len(), total) {
                return Ok(rows);
            }
            if page == MAX_PAGES {
                return Err(Error::new(Failure::Protocol, "成绩分页超出上限"));
            }
        }
        Ok(rows)
    }

    fn legacy_rows(&self) -> Result<Vec<Value>> {
        let value = self
            .ehall
            .get(&format!("{CJCX_MODULE}/xscjcx.do"), CJCX_ENTRY)?;
        value["datas"]["xscjcx"]["rows"]
            .as_array()
            .cloned()
            .ok_or_else(|| Error::new(Failure::Protocol, "成绩接口格式变化，请稍后重试"))
    }
}

/// 只在空页或达到 `totalSize` 时结束，避免学校把 `pageSize` 压到更小时漏页。
fn page_finished(page_len: usize, accumulated: usize, total: Option<u64>) -> bool {
    page_len == 0 || total.is_some_and(|total| accumulated as u64 >= total)
}

fn normalize(rows: &[Value]) -> Vec<GradeItem> {
    let mut items: Vec<GradeItem> = rows
        .iter()
        .filter_map(|row| {
            let course_name = protocol::text(&row["KCM"]).trim().to_string();
            if course_name.is_empty() {
                return None;
            }
            let term_id = protocol::text(&row["XNXQDM"]).trim().to_string();
            let display = protocol::text(&row["XNXQDM_DISPLAY"]).trim().to_string();
            let term_name = if display.is_empty() {
                term_id.clone()
            } else {
                display
            };
            let credit = number(&row["XF"]);
            let grade_point = number(&row["XFJD"]);
            let credit_grade_point = number(&row["JDF"]).or_else(|| match (credit, grade_point) {
                (Some(credit), Some(point)) => Some(credit * point),
                _ => None,
            });
            let grade = protocol::text(&row["DJCJMC"]).trim().to_string();
            let score = {
                let value = protocol::field(row, &["ZCJ", "CJ"]).trim().to_string();
                if value.is_empty() {
                    if grade.is_empty() {
                        "—".to_string()
                    } else {
                        grade.clone()
                    }
                } else {
                    value
                }
            };
            let nature = {
                let value = protocol::field(row, &["KCXZDM_DISPLAY", "KCLBDM_DISPLAY"])
                    .trim()
                    .to_string();
                if value.is_empty() {
                    "未知".to_string()
                } else {
                    value
                }
            };
            let is_valid = valid(&row["SFYX"]);
            // 旧接口可能没有 `QDXF`：只有明确及格的课程才把学分计入取得学分。
            let earned_credit = number(&row["QDXF"]).or_else(|| {
                let credit = credit?;
                (is_valid && score_passed(&score)).then_some(credit)
            });
            Some(GradeItem {
                term_id,
                term_name,
                course_code: protocol::text(&row["KCH"]).trim().to_string(),
                course_name,
                nature,
                credit,
                earned_credit,
                score,
                grade,
                grade_point,
                credit_grade_point,
                section: protocol::text(&row["KXH"]).trim().to_string(),
                exam_date: protocol::text(&row["KSSJ"]).trim().to_string(),
                valid: is_valid,
            })
        })
        .collect();
    items.sort_by(|a, b| {
        b.term_id
            .cmp(&a.term_id)
            .then(a.course_code.cmp(&b.course_code))
            .then(a.section.cmp(&b.section))
    });
    items
}

fn build_report(items: Vec<GradeItem>) -> GradeReport {
    let mut groups: BTreeMap<String, Vec<&GradeItem>> = BTreeMap::new();
    for item in &items {
        groups.entry(item.term_id.clone()).or_default().push(item);
    }

    let mut terms = Vec::with_capacity(groups.len());
    for (term_id, group) in groups.iter().rev() {
        let term_name = group
            .iter()
            .map(|item| item.term_name.as_str())
            .find(|name| !name.is_empty() && name != term_id)
            .unwrap_or(term_id)
            .to_string();
        let selected: f64 = group.iter().filter_map(|item| item.credit).sum();
        let earned: f64 = group.iter().filter_map(|item| item.earned_credit).sum();
        let (points, credits) = weighted(group.iter().copied());
        terms.push(GradeTerm {
            term_id: term_id.clone(),
            term_name,
            selected_credits: round2(selected),
            earned_credits: round2(earned),
            average_grade_point: gpa(points, credits),
            percent: if selected > 0.0 {
                Some(round2(earned / selected * 100.0))
            } else {
                None
            },
            item_count: group.len(),
        });
    }

    let selected: f64 = items.iter().filter_map(|item| item.credit).sum();
    let earned: f64 = items.iter().filter_map(|item| item.earned_credit).sum();
    let (points, credits) = weighted(items.iter());
    GradeReport {
        gpa: gpa(points, credits),
        selected_credits: round2(selected),
        earned_credits: round2(earned),
        item_count: items.len(),
        terms,
        items,
        updated_at: chrono::Utc::now().timestamp_millis(),
    }
}

/// 学分加权绩点：只统计有效、有学分且有课程绩点的课程。
fn weighted<'a>(items: impl Iterator<Item = &'a GradeItem>) -> (f64, f64) {
    let mut points = 0.0;
    let mut credits = 0.0;
    for item in items {
        if !item.valid {
            continue;
        }
        let (Some(credit), Some(point)) = (item.credit, item.grade_point) else {
            continue;
        };
        if credit <= 0.0 {
            continue;
        }
        points += credit * point;
        credits += credit;
    }
    (points, credits)
}

fn gpa(points: f64, credits: f64) -> Option<f64> {
    (credits > 0.0).then(|| round2(points / credits))
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// 旧接口没有 `QDXF` 时用成绩判断是否取得学分（60 分或等级制及格以上）。
fn score_passed(score: &str) -> bool {
    if let Ok(value) = score.trim().parse::<f64>() {
        return value >= 60.0;
    }
    matches!(
        score.trim(),
        "优秀"
            | "良好"
            | "中等"
            | "及格"
            | "合格"
            | "通过"
            | "A+"
            | "A"
            | "B+"
            | "B"
            | "C+"
            | "C"
            | "D"
    )
}

/// `SFYX` 在深大接口里可能是数字、字符串或布尔；缺省视为有效。
fn valid(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map_or(true, |number| number != 0.0),
        Value::String(text) => {
            let text = text.trim();
            text.is_empty() || text == "1" || text == "是" || text.eq_ignore_ascii_case("true")
        }
        _ => true,
    }
}

fn number(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::{build_report, normalize, page_finished, PAGE_SIZE};
    use serde_json::json;

    #[test]
    fn groups_terms_desc_and_weights_gpa() {
        let rows = vec![
            json!({"XNXQDM":"2025-2026-1","XNXQDM_DISPLAY":"2025-2026学年第一学期","KCH":"A1","KCM":"高等数学","XF":"4","QDXF":"4","XFJD":"4.0","JDF":"16","DJCJMC":"A","SFYX":"1"}),
            json!({"XNXQDM":"2025-2026-1","KCH":"A2","KCM":"大学英语","XF":2,"QDXF":2,"XFJD":"3.0","DJCJMC":"B+","SFYX":1}),
            json!({"XNXQDM":"2024-2025-2","XNXQDM_DISPLAY":"2024-2025学年第二学期","KCH":"B1","KCM":"程序设计","XF":"3","QDXF":"3","XFJD":"4.5","JDF":"13.5","ZCJ":"96","SFYX":"1"}),
        ];
        let report = build_report(normalize(&rows));
        assert_eq!(
            report
                .terms
                .iter()
                .map(|t| t.term_id.as_str())
                .collect::<Vec<_>>(),
            vec!["2025-2026-1", "2024-2025-2"]
        );
        // 学期一：16 + 2*3 = 22 / 6 = 3.67；全部：22 + 3*4.5 = 35.5 / 9 = 3.94
        assert_eq!(report.gpa, Some(3.94));
        let first = &report.terms[0];
        assert_eq!(first.selected_credits, 6.0);
        assert_eq!(first.earned_credits, 6.0);
        assert_eq!(first.average_grade_point, Some(3.67));
        assert_eq!(first.percent, Some(100.0));
        assert_eq!(report.terms[1].average_grade_point, Some(4.5));
        assert_eq!(report.item_count, 3);
        assert!(report.updated_at > 0);
    }

    #[test]
    fn invalid_rows_stay_visible_but_do_not_count() {
        let rows = vec![
            json!({"XNXQDM":"2025-2026-1","KCM":"有效课","KCH":"A","XF":"3","QDXF":"3","XFJD":"4.0","JDF":"12","SFYX":"1"}),
            json!({"XNXQDM":"2025-2026-1","KCM":"无效课","KCH":"B","XF":"2","QDXF":"0","XFJD":"4.5","JDF":"9","SFYX":"0","DJCJMC":"无效"}),
        ];
        let items = normalize(&rows);
        assert_eq!(items.len(), 2);
        assert!(
            !items
                .iter()
                .find(|item| item.course_name == "无效课")
                .unwrap()
                .valid
        );
        let report = build_report(items);
        assert_eq!(report.gpa, Some(4.0));
        assert_eq!(report.terms[0].average_grade_point, Some(4.0));
    }

    #[test]
    fn credit_grade_point_falls_back_to_credit_times_point() {
        let rows = vec![
            json!({"XNXQDM":"2025-2026-1","KCM":"无 JDF","KCH":"A","XF":"2.5","XFJD":"3.2","SFYX":"1"}),
        ];
        let items = normalize(&rows);
        assert_eq!(items[0].credit_grade_point, Some(8.0));
        assert_eq!(items[0].nature, "未知");
        assert_eq!(items[0].score, "—");
    }

    #[test]
    fn empty_course_names_are_skipped_and_terms_fallback_to_id() {
        let rows = vec![
            json!({"XNXQDM":"2025-2026-1","KCM":"","KCH":"A"}),
            json!({"XNXQDM":"2025-2026-1","KCM":"有名字","KCH":"B","XF":"1","XFJD":"4"}),
        ];
        let report = build_report(normalize(&rows));
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.terms[0].term_name, "2025-2026-1");
        assert_eq!(report.terms[0].average_grade_point, Some(4.0));
    }

    #[test]
    fn level_grade_is_kept_next_to_numeric_score() {
        let rows = vec![
            json!({"XNXQDM":"2025-2026-1","KCM":"百分制","KCH":"A","ZCJ":"95","DJCJMC":"A","XF":"3","XFJD":"4.0"}),
            json!({"XNXQDM":"2025-2026-1","KCM":"等级制","KCH":"B","ZCJ":"","DJCJMC":"优秀","XF":"2","XFJD":"4.5"}),
        ];
        let items = normalize(&rows);
        assert_eq!(items[0].score, "95");
        assert_eq!(items[0].grade, "A");
        assert_eq!(items[0].earned_credit, Some(3.0));
        assert_eq!(items[1].score, "优秀");
        assert_eq!(items[1].grade, "优秀");
        assert_eq!(items[1].earned_credit, Some(2.0));
    }

    #[test]
    fn pagination_stops_only_on_empty_page_or_total() {
        assert!(page_finished(0, 0, None));
        assert!(!page_finished(PAGE_SIZE - 1, PAGE_SIZE - 1, None));
        assert!(!page_finished(PAGE_SIZE, PAGE_SIZE, None));
        assert!(page_finished(PAGE_SIZE, PAGE_SIZE, Some(100)));
        assert!(!page_finished(PAGE_SIZE, PAGE_SIZE, Some(200)));
        assert!(page_finished(0, PAGE_SIZE, Some(200)));
    }

    #[test]
    fn numeric_strings_and_missing_values_are_tolerated() {
        let rows = vec![
            json!({"XNXQDM":2025,"KCM":"字符串学分","XF":"abc","QDXF":null,"XFJD":"3.5","KSSJ":20260101}),
        ];
        let items = normalize(&rows);
        assert_eq!(items[0].term_id, "2025");
        assert_eq!(items[0].credit, None);
        assert_eq!(items[0].earned_credit, None);
        assert_eq!(items[0].grade_point, Some(3.5));
        assert_eq!(items[0].exam_date, "20260101");
    }
}
