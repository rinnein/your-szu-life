use super::model::*;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::HashSet;

pub struct Paper {
    pub questions: Vec<Question>,
    pub signature: String,
    pub fields: Vec<(String, String)>,
    pub bindings: Vec<Vec<String>>,
    pub action: Option<reqwest::Url>,
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).expect("static quiz selector")
}
fn text(e: ElementRef<'_>) -> String {
    fn walk(e: ElementRef<'_>, out: &mut String) {
        if matches!(e.value().name(), "script" | "style" | "noscript") {
            return;
        }
        for node in e.children() {
            if let Some(child) = ElementRef::wrap(node) {
                walk(child, out);
            } else if let scraper::Node::Text(t) = node.value() {
                out.push_str(t);
                out.push(' ');
            }
        }
    }
    let mut out = String::new();
    walk(e, &mut out);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 按 DOM 顺序把元素内容转换成富文本：`<img>` 原位转成 Markdown 绝对地址，
/// 其余标签丢弃。返回文本与按出现顺序排列的图片 URL。
fn rich_text(e: ElementRef<'_>, base: &reqwest::Url) -> (String, Vec<String>) {
    fn walk(
        e: ElementRef<'_>,
        base: &reqwest::Url,
        out: &mut String,
        images: &mut Vec<String>,
    ) {
        if matches!(e.value().name(), "script" | "style" | "noscript") {
            return;
        }
        for node in e.children() {
            if let Some(child) = ElementRef::wrap(node) {
                let name = child.value().name();
                if name == "img" {
                    let Some(src) = child.value().attr("src") else {
                        continue;
                    };
                    let Ok(url) = base.join(src) else {
                        continue;
                    };
                    let alt = child
                        .value()
                        .attr("alt")
                        .map(|a| a.replace(['[', ']'], " "))
                        .map(|a| a.trim().to_string())
                        .filter(|a| !a.is_empty())
                        .unwrap_or_else(|| "题目图片".into());
                    out.push_str(&super::richtext::markdown_image(&alt, url.as_str()));
                    out.push(' ');
                    images.push(url.to_string());
                } else if name == "br" {
                    out.push(' ');
                } else {
                    let is_block = matches!(
                        name,
                        "p" | "div" | "li" | "ul" | "ol" | "tr" | "td" | "th" | "table"
                            | "section" | "article" | "header" | "footer" | "blockquote"
                            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                    );
                    if is_block {
                        out.push(' ');
                    }
                    walk(child, base, out, images);
                    if is_block {
                        out.push(' ');
                    }
                }
            } else if let scraper::Node::Text(t) = node.value() {
                out.push_str(t);
                out.push(' ');
            }
        }
    }
    let mut out = String::new();
    let mut images = Vec::new();
    walk(e, base, &mut out, &mut images);
    (super::richtext::clean_rich_text(&out), images)
}
pub fn parse(html: &str, url: &reqwest::Url, task_id: &str) -> Result<Paper, String> {
    let doc = Html::parse_document(html);
    let mut questions = vec![];
    let mut bindings = vec![];
    let mut seen = HashSet::new();
    let mut question_form = None;
    for (index, element) in doc.select(&selector(".queContainer")).enumerate() {
        let inputs: Vec<_> = element
            .select(&selector(
                "input[type=radio],input[type=checkbox],textarea,input[type=text]",
            ))
            .filter(|e| e.value().attr("name").is_some() && e.value().attr("disabled").is_none())
            .collect();
        if inputs.is_empty() {
            return Err("此题型暂不能在应用内编辑，请使用官方试卷".into());
        }
        let id = element
            .value()
            .attr("id")
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("q{index}:{}", inputs[0].value().attr("name").unwrap()));
        if !seen.insert(id.clone()) {
            return Err("试卷题目编号重复".into());
        }
        let choice = inputs[0].value().name() == "input"
            && matches!(inputs[0].value().attr("type"), Some("radio" | "checkbox"));
        let kind = if !choice {
            Kind::Text
        } else if inputs[0].value().attr("type") == Some("checkbox") {
            Kind::Multiple
        } else {
            Kind::Single
        };
        let names: Vec<_> = inputs
            .iter()
            .map(|e| e.value().attr("name").unwrap().to_owned())
            .collect();
        let mut options = vec![];
        let mut option_images: Vec<String> = vec![];
        if choice {
            let mut ids = HashSet::new();
            for e in &inputs {
                let value = e
                    .value()
                    .attr("value")
                    .filter(|v| !v.is_empty())
                    .ok_or("选项缺少提交编号")?
                    .to_owned();
                if !ids.insert(value.clone()) || e.value().attr("name") != Some(names[0].as_str()) {
                    return Err("选择题字段结构暂不支持".into());
                }
                let label = e.ancestors().filter_map(ElementRef::wrap).find(|p| {
                    p.value().classes().any(|c| c == "ti-a") || p.value().name() == "label"
                });
                let (mut content, images) = label
                    .map(|label| rich_text(label, url))
                    .unwrap_or_default();
                if content.is_empty()
                    && label.is_some_and(|e| e.select(&selector("img[src]")).next().is_some())
                {
                    content = format!("图片选项 {}", options.len() + 1);
                }
                if content.is_empty() {
                    return Err("无法完整读取选项内容".into());
                }
                option_images.extend(images);
                options.push(OptionItem {
                    id: value,
                    text: content,
                });
            }
        }
        let (mut question_text, mut images) = element
            .select(&selector(".ti-q,.que-title,.question-title"))
            .next()
            .map(|e| rich_text(e, url))
            .filter(|(text, _)| !text.is_empty())
            .unwrap_or_else(|| (text(element), Vec::new()));
        if question_text.is_empty() && element.select(&selector("img[src]")).next().is_some() {
            question_text = "请根据题目图片作答。".into();
        }
        if question_text.is_empty() || question_text.len() > 64000 {
            return Err("题目内容为空或过长".into());
        }
        // 与 Question.images 的顺序保持一致：题干图片 → 各选项图片。
        images.extend(option_images);
        if images.len() > 12 {
            return Err("单题图片数量超过支持范围".into());
        }
        questions.push(Question {
            id,
            text: question_text,
            kind,
            options,
            blanks: if choice { 0 } else { names.len() },
            images: vec![],
        });
        bindings.push(names);
        let form = element
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().name() == "form")
            .map(|e| e.id());
        if index == 0 {
            question_form = form;
        } else if question_form != form {
            return Err("题目分布在不同表单，需使用官网".into());
        }
    }
    if questions.is_empty() || questions.len() > 200 {
        return Err("无法读取试卷题目，可能需要验证或试卷尚未开放".into());
    }
    let form = doc
        .select(&selector("form"))
        .find(|e| Some(e.id()) == question_form);
    let mut fields = vec![];
    let mut action = None;
    if let Some(form) = form {
        // Only an explicit, same-origin native POST form is a verified protocol.
        // AJAX handlers, missing actions, or verification widgets require the official page.
        let native = form
            .value()
            .attr("method")
            .is_some_and(|m| m.eq_ignore_ascii_case("post"))
            && form.value().attr("onsubmit").is_none()
            && form
                .select(&selector(
                    "[onclick],input[type=file],[data-captcha],input[name*=captcha]",
                ))
                .next()
                .is_none();
        for e in form.select(&selector("input[type=hidden][name]")) {
            fields.push((
                e.value().attr("name").unwrap().to_owned(),
                e.value().attr("value").unwrap_or_default().to_owned(),
            ));
        }
        let bound_task = fields
            .iter()
            .any(|(k, v)| matches!(k.as_str(), "task_id" | "taskId") && v == task_id);
        if native && bound_task {
            if let Some(target) = form
                .value()
                .attr("action")
                .filter(|v| !v.is_empty())
                .and_then(|s| url.join(s).ok())
            {
                if target.origin() == url.origin()
                    && (target.path() == "/exam/commit"
                        || target.path() == format!("/exam/{task_id}"))
                    && target.username().is_empty()
                    && target.password().is_none()
                {
                    action = Some(target);
                }
            }
        }
    }
    let identity =
        serde_json::to_vec(&(task_id, &questions, &bindings)).map_err(|_| "试卷格式无效")?;
    let signature = format!("{:x}", Sha1::digest(identity));
    Ok(Paper {
        questions,
        signature,
        fields,
        bindings,
        action,
    })
}
impl Paper {
    pub fn answer_fields(&self, answers: &Answers) -> Result<Vec<(String, String)>, String> {
        validate_answers(&self.questions, answers, true)?;
        let mut fields = self.fields.clone();
        for (q, names) in self.questions.iter().zip(&self.bindings) {
            let a = &answers[&q.id];
            if q.kind == Kind::Text {
                fields.extend(names.iter().cloned().zip(a.iter().cloned()));
            } else {
                fields.extend(a.iter().map(|a| (names[0].clone(), a.clone())));
            }
        }
        Ok(fields)
    }
}
pub fn resource_id(v: &Value) -> Option<String> {
    super::super::model::id(&v["res_id"])
}
pub fn verdict(bytes: &[u8]) -> Option<bool> {
    fn message(text: &str) -> Option<bool> {
        if text.contains("请重新提交测验") || text.contains("测验未通过") {
            Some(false)
        } else if text.contains("测验通过") || text.contains("测验已通过") {
            Some(true)
        } else {
            None
        }
    }
    if let Ok(value) = serde_json::from_slice::<Value>(bytes) {
        let verdict = value["msg"].as_str().and_then(message);
        return if verdict == Some(true) && super::super::model::text(&value["code"]) != "1" {
            None
        } else {
            verdict
        };
    }
    let doc = Html::parse_document(std::str::from_utf8(bytes).ok()?);
    doc.select(&selector(".layui-layer-content"))
        .filter(|e| {
            !e.ancestors()
                .filter_map(ElementRef::wrap)
                .any(|p| p.value().classes().any(|c| c == "queContainer"))
        })
        .find_map(|e| message(&text(e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_become_inline_markdown_in_stem_and_options() {
        let html = r#"<form method="post" action="/exam/123">
<input type="hidden" name="task_id" value="123">
<div class="queContainer" id="q1">
  <div class="ti-q">如图<img src="/a.png" alt="图A"/>求面积</div>
  <label class="ti-a"><input type="radio" name="answers[1]" value="A"/>选项A<img src="//cdn.example.com/x.png"/></label>
  <label class="ti-a"><input type="radio" name="answers[1]" value="B"/>选项B</label>
</div>
</form>"#;
        let url = reqwest::Url::parse("https://www.uooc.net.cn/exam/123").unwrap();
        let paper = parse(html, &url, "123").unwrap();
        let question = &paper.questions[0];

        assert_eq!(
            question.text,
            "如图 ![图A](https://www.uooc.net.cn/a.png) 求面积"
        );
        assert!(!question.text.contains("附图"));
        assert_eq!(
            question.options[0].text,
            "选项A ![题目图片](https://cdn.example.com/x.png)"
        );
        assert_eq!(question.options[1].text, "选项B");
        // 题干图片在前，选项图片在后，与 hydrate_question_images 的顺序约定一致。
        let stem_images = super::super::richtext::find_markdown_images(&question.text);
        let sources: Vec<&str> = stem_images.iter().map(|image| image.src.as_str()).collect();
        assert_eq!(sources, vec!["https://www.uooc.net.cn/a.png"]);
        let option_images =
            super::super::richtext::find_markdown_images(&question.options[0].text);
        assert_eq!(
            option_images[0].src,
            "https://cdn.example.com/x.png"
        );
    }
}
