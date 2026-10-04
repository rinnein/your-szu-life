use super::{client::Error, model::*};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub(super) fn id(v: &Value) -> Result<String, Error> {
    match v {
        Value::Number(n) if n.as_u64().is_some() => Ok(n.to_string()),
        Value::String(s)
            if !s.is_empty() && s.len() < 64 && s.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Ok(s.clone())
        }
        _ => Err(Error::local("OJ 编号格式无效")),
    }
}
pub(super) fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}
pub(super) fn timestamp(v: &Value) -> Result<i64, Error> {
    let s = v.as_str().ok_or_else(|| Error::local("测验时间缺失"))?;
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.timestamp_millis())
        .map_err(|_| Error::local("测验时间格式无效"))
}
pub(super) fn group(v: &Value) -> Result<Group, Error> {
    Ok(Group {
        id: id(&v["id"])?,
        name: text(&v["name"]),
    })
}
pub(super) fn contest(v: &Value) -> Result<Contest, Error> {
    Ok(Contest {
        id: id(&v["id"])?,
        group_id: id(if v["group"].is_object() {
            &v["group"]["id"]
        } else {
            &v["group"]
        })?,
        title: text(&v["title"]),
        description: text(&v["description"]),
        begin: timestamp(&v["begin_time"])?,
        end: timestamp(&v["end_time"])?,
        now: timestamp(&v["now_time"])?,
        is_exam: v["is_exam"].as_bool().unwrap_or(false),
    })
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(text).collect())
        .unwrap_or_default()
}
fn number(v: &Value) -> f64 {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0.)
}
pub(super) fn display_label(mut index: usize) -> String {
    let mut bytes = Vec::new();
    loop {
        bytes.push(b'A' + (index % 26) as u8);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    bytes.reverse();
    String::from_utf8(bytes).unwrap()
}
pub(super) fn problems(v: &Value) -> Result<Vec<Problem>, Error> {
    let map = v["problem_json"]
        .as_object()
        .ok_or_else(|| Error::local("测验题型数据缺失"))?;
    let mut result = vec![];
    for (kind, rows) in map {
        let kind = kind.parse::<u8>().map_err(|_| Error::local("未知题型"))?;
        if kind > 4 {
            return Err(Error::local("此测验包含暂不兼容的题型"));
        }
        let mut ordinal = 0;
        for p in rows
            .as_array()
            .ok_or_else(|| Error::local("题列表格式无效"))?
        {
            if p["deleted"].as_bool() == Some(true) {
                continue;
            }
            let pid = id(&p["id"])?;
            let inputs = strings(&p["sample_input"]);
            let outputs = strings(&p["sample_output"]);
            if inputs.len() != outputs.len() {
                return Err(Error::local("题目样例输入输出数量不一致"));
            }
            let samples = inputs
                .into_iter()
                .zip(outputs)
                .map(|(input, output)| Sample { input, output })
                .collect();
            let languages = if let Some(a) = p["languages"].as_array() {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            } else {
                ["c", "cpp", "java", "py", "go", "js"]
                    .into_iter()
                    .filter(|l| {
                        p["languages"][*l].as_bool() == Some(true)
                            || p["languages"]["all"].as_bool() == Some(true)
                    })
                    .map(str::to_owned)
                    .collect()
            };
            let mut templates = BTreeMap::new();
            for field in ["template", "code_template", "code_filling_structure"] {
                if let Some(t) = p[field].as_object() {
                    for (k, value) in t {
                        if let Some(value) = value.as_str() {
                            templates.insert(k.clone(), value.into());
                        }
                    }
                }
            }
            let description = text(&p["description"]);
            result.push(Problem {
                id: pid.clone(),
                display_label: display_label(ordinal),
                kind,
                title: if text(&p["title"]).is_empty() {
                    format!("第 {} 题", result.len() + 1)
                } else {
                    text(&p["title"])
                },
                blanks: description.matches("[=]").count(),
                description,
                input: text(&p["input_description"]),
                output: text(&p["output_description"]),
                hint: text(&p["hint"]),
                time_limit: number(&p["time_limit"]),
                memory_limit: number(&p["memory_limit"]) as u64,
                samples,
                choices: strings(&p["choice"]),
                multiple: p["is_multiple"]
                    .as_bool()
                    .or(p["multiple"].as_bool())
                    .unwrap_or(false),
                languages,
                templates,
                score: v["problem_score"][kind.to_string()][&pid].as_f64(),
                io_mode: p["io_mode"].as_str().unwrap_or("stdio").into(),
                input_file: p["input_file"].as_str().map(str::to_owned),
                output_file: p["output_file"].as_str().map(str::to_owned),
            });
            ordinal += 1;
        }
    }
    Ok(result)
}
pub(super) fn noncode_body(
    contest: &str,
    problems: &[Problem],
    drafts: &BTreeMap<String, Draft>,
) -> Result<Value, Error> {
    let mut body = json!({"contest":contest.parse::<u64>().map_err(|_|Error::local("测验编号无效"))?,"choice_submission":{},"fill_in_submission":{},"qa_submission_list":[]});
    for p in problems {
        let d = drafts.get(&problem_key(p));
        match p.kind {
        1|2=>{let answer=d.map(|d|d.answer.clone()).unwrap_or(json!([]));let a=answer.as_array().ok_or_else(||Error::local("选择或填空答案格式无效"))?;
            if p.kind==1 && a.iter().any(|v|v.as_u64().is_none_or(|i|i as usize>=p.choices.len())){return Err(Error::local("选择题选项无效"))}
            if p.kind==2&&(a.len()>p.blanks||a.iter().any(|v|!v.is_string())){return Err(Error::local("填空题答案无效"))}
            body[if p.kind==1{"choice_submission"}else{"fill_in_submission"}][&p.id]=answer;},
        3=>body["qa_submission_list"].as_array_mut().unwrap().push(json!({"qa_problem_id":p.id.parse::<u64>().unwrap(),"change":d.is_some(),"answer":d.and_then(|d|d.answer.as_str()).unwrap_or("")})),_=>{}
    }
    }
    Ok(body)
}
pub(super) fn source_url(base: &str, contest: &str, problem: Option<&Problem>) -> String {
    format!(
        "{base}#/contest/{contest}/{}",
        problem
            .map(|p| format!("?problem={}&type={}", p.id, p.kind))
            .unwrap_or_default()
    )
}
pub(super) fn companion(
    site: &Site,
    contest: &Contest,
    p: &Problem,
    batch: &str,
    size: usize,
    _bulk: bool,
) -> Result<Value, Error> {
    if !matches!(p.kind, 0 | 4) || p.time_limit <= 0. || p.memory_limit == 0 {
        return Err(Error::local("仅支持具有时间与内存限制的编程题"));
    }
    fn line(s: &str) -> String {
        if s.ends_with('\n') {
            s.into()
        } else {
            format!("{s}\n")
        }
    }
    let (input, output) = if p.io_mode == "file" {
        (
            json!({"type":"file","fileName":p.input_file.as_ref().ok_or_else(||Error::local("输入文件名缺失"))?}),
            json!({"type":"file","fileName":p.output_file.as_ref().ok_or_else(||Error::local("输出文件名缺失"))?}),
        )
    } else {
        (json!({"type":"stdin"}), json!({"type":"stdout"}))
    };
    Ok(
        json!({"name":p.display_label,"group":format!("{} - {}",site.name,contest.title),"url":source_url(&site.url,&contest.id,Some(p)),"interactive":false,"timeLimit":(p.time_limit*1000.).round() as u64,"memoryLimit":p.memory_limit,"tests":p.samples.iter().map(|s|json!({"input":line(&s.input),"output":line(&s.output)})).collect::<Vec<_>>(),"testType":"single","input":input,"output":output,"languages":{"java":{"mainClass":"Main","taskClass":""}},"batch":{"id":batch,"size":size}}),
    )
}
