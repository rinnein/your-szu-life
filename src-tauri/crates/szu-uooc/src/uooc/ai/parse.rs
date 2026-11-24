use super::super::quiz::model::{Answers, Kind, Question};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub answers: Answers,
    pub reasons: BTreeMap<String, String>,
}

/// 模型可能返回标准 `{answers, reasons}`、数组或单题对象；这里保持与旧实现
/// 相同的容错范围，并把选项文本映射回精确的选项 id。
pub fn parse_answers(raw: &str, questions: &[Question]) -> Result<Parsed, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("AI 返回了空答案".into());
    }
    let raw_clean = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .unwrap_or(raw)
        .trim()
        .trim_end_matches("```")
        .trim();

    #[derive(Deserialize, Default)]
    struct BatchAnswerWithReasons {
        #[serde(default)]
        answers: BTreeMap<String, Vec<String>>,
        #[serde(default)]
        reasons: BTreeMap<String, String>,
        #[serde(default)]
        analyses: BTreeMap<String, String>,
    }

    let parsed: BatchAnswerWithReasons = serde_json::from_str(raw_clean)
        .or_else(
            |_: serde_json::Error| -> Result<BatchAnswerWithReasons, serde_json::Error> {
                #[derive(Deserialize)]
                struct ItemAnswer {
                    id: Option<String>,
                    question_id: Option<String>,
                    answers: Vec<String>,
                    reason: Option<String>,
                    analysis: Option<String>,
                }
                let list = serde_json::from_str::<Vec<ItemAnswer>>(raw_clean);
                if let Ok(items) = list {
                    let mut ans_map = BTreeMap::new();
                    let mut rsn_map = BTreeMap::new();
                    for item in items {
                        let qid = item.id.or(item.question_id);
                        if let Some(id) = qid {
                            ans_map.insert(id.clone(), item.answers);
                            if let Some(r) = item.reason.or(item.analysis) {
                                rsn_map.insert(id, r);
                            }
                        }
                    }
                    return Ok(BatchAnswerWithReasons {
                        answers: ans_map,
                        reasons: rsn_map,
                        analyses: BTreeMap::new(),
                    });
                }

                #[derive(Deserialize)]
                struct SingleAnswer {
                    answers: Vec<String>,
                    reason: Option<String>,
                }
                let single = serde_json::from_str::<SingleAnswer>(raw_clean)?;
                let mut ans_map = BTreeMap::new();
                let mut rsn_map = BTreeMap::new();
                if let Some(first_q) = questions.first() {
                    ans_map.insert(first_q.id.clone(), single.answers);
                    if let Some(r) = single.reason {
                        rsn_map.insert(first_q.id.clone(), r);
                    }
                }
                Ok(BatchAnswerWithReasons {
                    answers: ans_map,
                    reasons: rsn_map,
                    analyses: BTreeMap::new(),
                })
            },
        )
        .map_err(|_: serde_json::Error| {
            format!(
                "AI 答案格式无效: {}",
                raw_clean.chars().take(200).collect::<String>()
            )
        })?;

    let mut final_answers = BTreeMap::new();
    let mut final_reasons = parsed.reasons;
    for (k, v) in parsed.analyses {
        final_reasons.entry(k).or_insert(v);
    }

    for q in questions {
        if let Some(ans_list) = parsed.answers.get(&q.id) {
            let mut valid_ans = Vec::new();
            for ans in ans_list {
                let ans_trim = ans.trim();
                if let Some(opt) = q
                    .options
                    .iter()
                    .find(|o| super::super::quiz::qa::is_option_match(o, ans_trim))
                {
                    if !valid_ans.contains(&opt.id) {
                        valid_ans.push(opt.id.clone());
                    }
                } else if (q.kind == Kind::Text || q.kind == Kind::Subjective)
                    && !ans_trim.is_empty()
                {
                    valid_ans.push(ans_trim.to_string());
                } else if !valid_ans.contains(&ans_trim.to_string()) {
                    valid_ans.push(ans_trim.to_string());
                }
            }
            if !valid_ans.is_empty() {
                final_answers.insert(q.id.clone(), valid_ans);
            }
        }
    }

    Ok(Parsed {
        answers: final_answers,
        reasons: final_reasons,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: &str) -> Question {
        Question {
            id: id.into(),
            text: "题".into(),
            kind: Kind::Single,
            options: vec![
                super::super::super::quiz::model::OptionItem {
                    id: "A".into(),
                    text: "选项 A".into(),
                },
                super::super::super::quiz::model::OptionItem {
                    id: "B".into(),
                    text: "选项 B".into(),
                },
            ],
            blanks: 0,
            images: vec![],
        }
    }

    #[test]
    fn parses_standard_object_and_maps_text_answers_to_ids() {
        let q = question("q1");
        let parsed = parse_answers(
            r#"{"answers":{"q1":["选项 B"]},"reasons":{"q1":"因为"}}"#,
            &[q],
        )
        .unwrap();
        assert_eq!(parsed.answers.get("q1").unwrap(), &vec!["B"]);
        assert_eq!(parsed.reasons.get("q1").unwrap(), "因为");
    }

    #[test]
    fn parses_array_and_single_forms() {
        let q = question("q1");
        let array = parse_answers(
            r#"[{"id":"q1","answers":["A"],"reason":"r"}]"#,
            std::slice::from_ref(&q),
        )
        .unwrap();
        assert_eq!(array.answers.get("q1").unwrap(), &vec!["A"]);
        let single = parse_answers(r#"{"answers":["B"],"reason":"s"}"#, &[q]).unwrap();
        assert_eq!(single.answers.get("q1").unwrap(), &vec!["B"]);
    }

    #[test]
    fn strips_markdown_fence_and_rejects_garbage() {
        let q = question("q1");
        let parsed = parse_answers(
            "```json\n{\"answers\":{\"q1\":[\"A\"]}}\n```",
            std::slice::from_ref(&q),
        )
        .unwrap();
        assert_eq!(parsed.answers.get("q1").unwrap(), &vec!["A"]);
        assert!(parse_answers("not json", &[q]).is_err());
    }
}
