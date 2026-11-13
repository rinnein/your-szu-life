//! 题干/选项富文本约定（对齐 uooc-qa-share 的 `src/lib/rich-text.ts` 与 `src/lib/stem.ts`）。
//!
//! - 只允许纯文本 + Markdown 图片语法 `![说明](...)`；
//! - 图片引用使用 `#<pic_id>`：未上传时是本地的 `temp_pic_id`，上传成功后是服务端正式 pic_id；
//! - 不允许任何 HTML 标签（包括 `<img>`），抓取端需先转换成 Markdown；
//! - 普通图片必须是完整的 http(s) 绝对地址；base64 图片允许 `data:image/...;base64,...`。

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

/// 与上游 `MARKDOWN_IMAGE_SOURCE` 完全一致：支持 alt、`<带空格 URL>` 与可选 title。
static MARKDOWN_IMAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"!\[([^\]\n]*)\]\(\s*(?:<([^>\n]+)>|([^()\s]+))(?:\s+(?:"[^"\n]*"|'[^'\n]*'|\([^)\n]*\)))?\s*\)"#,
    )
    .unwrap()
});

/// 与上游 `HTML_TAG_PATTERN` 一致：只匹配真正的标签形态，不会误伤 `x < y`。
static HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)<(?:[a-zA-Z/][a-zA-Z0-9:-]*\b[^>]*|!--[\s\S]*?--)>").unwrap()
});
static ANY_HTML_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<[^>]*>").unwrap());
static NAMED_ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&[a-z]+;").unwrap());
static WHITESPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static PIC_REFERENCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#<([^>]+)>$").unwrap());
static DATA_IMAGE_MIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^data:(image/(?:png|jpeg|jpg|gif|webp|bmp|avif));base64,").unwrap()
});
static IMG_TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<img\b[^>]*>").unwrap());
static SRC_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap()
});
static ALT_ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\balt\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap()
});

pub const MAX_UPLOAD_IMAGES: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownImage {
    pub raw: String,
    pub alt: String,
    pub src: String,
    pub start: usize,
    pub end: usize,
}

/// 题库图片缓存：`pic_id` 既可能是未上传的本地 `temp_pic_id`（"1"、"2"…），
/// 也可能是上传成功后的服务端 pic_id。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankImage {
    pub pic_id: String,
    pub url: String,
}

/// 上传 payload 的顶层图片项，字段名与服务端保持一致。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UploadImage {
    pub temp_pic_id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pic_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64_pic_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// 一次上传请求内的图片分配器：保证 `temp_pic_id` 全局唯一并按来源去重。
#[derive(Default)]
pub struct UploadContext {
    next_temp_id: u32,
    images: Vec<UploadImage>,
    seen: HashMap<String, u32>,
}

impl UploadContext {
    pub fn new() -> Self {
        Self {
            next_temp_id: 1,
            images: Vec::new(),
            seen: HashMap::new(),
        }
    }

    pub fn into_images(mut self) -> Option<Vec<UploadImage>> {
        if self.images.is_empty() {
            None
        } else {
            Some(std::mem::take(&mut self.images))
        }
    }

    fn assign(&mut self, source: &str, data_uri: Option<&str>) -> Option<u32> {
        if let Some(existing) = self.seen.get(source) {
            return Some(*existing);
        }
        if self.images.len() >= MAX_UPLOAD_IMAGES {
            return None;
        }
        let temp_pic_id = self.next_temp_id;
        self.next_temp_id = self.next_temp_id.saturating_add(1);
        self.seen.insert(source.to_string(), temp_pic_id);
        match data_uri {
            Some(uri) => {
                let mime = DATA_IMAGE_MIME_RE
                    .captures(uri)
                    .map(|caps| caps[1].to_ascii_lowercase())
                    .map(|m| if m == "image/jpg" { "image/jpeg".into() } else { m });
                self.images.push(UploadImage {
                    temp_pic_id,
                    pic_url: None,
                    base64_pic_data: Some(uri.to_string()),
                    mime_type: mime,
                });
            }
            None => {
                self.images.push(UploadImage {
                    temp_pic_id,
                    pic_url: Some(source.to_string()),
                    base64_pic_data: None,
                    mime_type: None,
                });
            }
        }
        Some(temp_pic_id)
    }
}

/// 查找文本中的所有 Markdown 图片（不做安全过滤）。
pub fn find_markdown_images(text: &str) -> Vec<MarkdownImage> {
    MARKDOWN_IMAGE_RE
        .captures_iter(text)
        .filter_map(|caps| {
            let whole = caps.get(0)?;
            Some(MarkdownImage {
                raw: whole.as_str().to_string(),
                alt: caps.get(1).map(|m| m.as_str()).unwrap_or_default().to_string(),
                src: caps
                    .get(2)
                    .or_else(|| caps.get(3))
                    .map(|m| m.as_str().trim())
                    .unwrap_or_default()
                    .to_string(),
                start: whole.start(),
                end: whole.end(),
            })
        })
        .collect()
}

/// 移除 Markdown 图片语法；`keep_alt` 为 true 时保留 alt 文本。
pub fn strip_markdown_images(text: &str, keep_alt: bool) -> String {
    MARKDOWN_IMAGE_RE
        .replace_all(text, |caps: &regex::Captures<'_>| {
            if keep_alt {
                caps.get(1).map(|m| m.as_str()).unwrap_or_default().to_string()
            } else {
                String::new()
            }
        })
        .into_owned()
}

/// 检测文本是否包含 HTML 标签；Markdown 图片（包括 `#<pic_id>`）不算 HTML。
pub fn has_html_tags(raw: &str) -> bool {
    HTML_TAG_RE.is_match(&strip_markdown_images(raw, false))
}

/// 把文本中的 `#<pic_id>` 解析成 pic_id。
pub fn parse_pic_reference(src: &str) -> Option<&str> {
    PIC_REFERENCE_RE
        .captures(src.trim())
        .and_then(|caps| caps.get(1).map(|m| m.as_str()))
}

/// 只对图片之外的文本片段做变换，避免破坏 Markdown 图片语法。
fn map_text_segments<F>(text: &str, mut map: F) -> String
where
    F: FnMut(&str) -> String,
{
    let mut out = String::new();
    let mut last = 0;
    for image in find_markdown_images(text) {
        out.push_str(&map(&text[last..image.start]));
        out.push_str(&image.raw);
        last = image.end;
    }
    out.push_str(&map(&text[last..]));
    out
}

fn decode_entities(raw: &str) -> String {
    raw.replace("&nbsp;", " ")
        .replace("&ensp;", " ")
        .replace("&emsp;", " ")
        .replace("&thinsp;", " ")
        .replace("&#160;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// 清洗题干/选项：去掉 HTML 标签与实体、压缩空白，但保留 Markdown 图片。
///
/// 如果清洗后仍留有标签形态（例如原文里的 `a<b>c`），再次替换为空格，
/// 避免服务端 `hasHtmlTags` 把整批上传拒绝。
pub fn clean_rich_text(raw: &str) -> String {
    let without_tags = HTML_TAG_RE.replace_all(raw, " ");
    let decoded = decode_entities(&without_tags);
    let collapsed = map_text_segments(&decoded, |segment| {
        WHITESPACE_RE.replace_all(segment, " ").into_owned()
    });
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        String::new()
    } else if has_html_tags(trimmed) {
        HTML_TAG_RE.replace_all(trimmed, " ").trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// 与上游 `normalizeStem` 完全一致：去图片（不保留 alt）→ 去 HTML → 命名实体替换为空格
/// → 压缩空白 → trim → lowercase。
pub fn normalize_stem(raw: &str) -> String {
    let without_images = strip_markdown_images(raw, false);
    let without_html = ANY_HTML_TAG_RE.replace_all(&without_images, "");
    let without_entities = NAMED_ENTITY_RE.replace_all(&without_html, " ");
    without_entities
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn extract_attr(tag: &str, regex: &Regex) -> Option<String> {
    regex
        .captures(tag)
        .and_then(|caps| caps.get(1).or_else(|| caps.get(2)).or_else(|| caps.get(3)))
        .map(|value| value.as_str().to_string())
}

fn resolve_html_image_src(src: &str, base: Option<&str>) -> Option<String> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    let lower = src.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || DATA_IMAGE_MIME_RE.is_match(src)
    {
        return Some(src.to_string());
    }
    let base = reqwest::Url::parse(base?).ok()?;
    if src.starts_with("//") {
        return reqwest::Url::parse(&format!("{}:{src}", base.scheme()))
            .ok()
            .map(|url| url.to_string());
    }
    base.join(src).ok().map(|url| url.to_string())
}

/// 把原始 HTML 中的 `<img>` 转换为 Markdown，用于 `/exam/view`、`getTaskPaper`
/// 等返回富 HTML 的来源；`base` 用于解析相对地址（通常是 UOOC 站点 origin）。
pub fn html_images_to_markdown(raw: &str, base: Option<&str>) -> String {
    let replaced = IMG_TAG_RE.replace_all(raw, |caps: &regex::Captures<'_>| {
        let tag = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
        let alt = extract_attr(tag, &ALT_ATTR_RE)
            .map(|value| value.replace(['[', ']'], " "))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "题目图片".into());
        let src = extract_attr(tag, &SRC_ATTR_RE).unwrap_or_default();
        match resolve_html_image_src(&src, base) {
            Some(url) => format!("{} ", markdown_image(&alt, &url)),
            None => String::new(),
        }
    });
    clean_rich_text(&replaced)
}

/// 判断两个富文本是否在「忽略图片」的语义下相同。
pub fn rich_text_equivalent(a: &str, b: &str) -> bool {
    normalize_stem(a) == normalize_stem(b)
}

fn is_supported_source(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || DATA_IMAGE_MIME_RE.is_match(src)
}

/// 一道题内的本地图片规范化器：题干与各选项共享同一组 `temp_pic_id`，
/// 避免分别调用时产生重复编号。
#[derive(Default)]
pub struct LocalTextCanonicalizer {
    next_temp_id: u32,
    images: Vec<BankImage>,
    by_url: HashMap<String, String>,
}

impl LocalTextCanonicalizer {
    pub fn new() -> Self {
        Self {
            next_temp_id: 1,
            images: Vec::new(),
            by_url: HashMap::new(),
        }
    }

    pub fn canonicalize(&mut self, raw: &str) -> String {
        let cleaned = clean_rich_text(raw);
        let mut out = String::new();
        let mut last = 0;

        for image in find_markdown_images(&cleaned) {
            out.push_str(&cleaned[last..image.start]);
            let src = image.src.trim();
            if parse_pic_reference(src).is_some() {
                out.push_str(&image.raw);
            } else if is_supported_source(src) {
                let pic_id = if let Some(existing) = self.by_url.get(src) {
                    existing.clone()
                } else {
                    let id = self.next_temp_id.to_string();
                    self.next_temp_id = self.next_temp_id.saturating_add(1);
                    self.by_url.insert(src.to_string(), id.clone());
                    self.images.push(BankImage {
                        pic_id: id.clone(),
                        url: src.to_string(),
                    });
                    id
                };
                out.push_str(&markdown_image(&image.alt, &format!("#<{pic_id}>")));
            } else {
                out.push_str(&image.alt);
            }
            last = image.end;
        }
        out.push_str(&cleaned[last..]);
        out.trim().to_string()
    }

    pub fn into_images(self) -> Vec<BankImage> {
        self.images
    }
}

/// 本地入库前的规范化：把普通图片改写成题内自增的 `temp_pic_id` 引用，
/// 并返回 `temp_pic_id → URL` 缓存。已存在的 `#<pic_id>` 引用原样保留。
pub fn canonicalize_local_text(raw: &str) -> (String, Vec<BankImage>) {
    let mut canonicalizer = LocalTextCanonicalizer::new();
    let text = canonicalizer.canonicalize(raw);
    (text, canonicalizer.into_images())
}

/// 发出上传前的规范化：把文本里的每个 `#<ref>` 都重新映射成本次请求的全局
/// `temp_pic_id`，并把 URL / base64 写入 payload 的 `images`。
///
/// 查不到的引用会降级为 alt 文本；超过 50 张的图片同样降级，避免整批被拒绝。
pub fn prepare_rich_text_for_upload(
    raw: &str,
    images: &[BankImage],
    ctx: &mut UploadContext,
) -> String {
    let cleaned = clean_rich_text(raw);
    let by_pic: HashMap<&str, &str> = images
        .iter()
        .map(|image| (image.pic_id.as_str(), image.url.as_str()))
        .collect();
    let mut out = String::new();
    let mut last = 0;

    for image in find_markdown_images(&cleaned) {
        out.push_str(&cleaned[last..image.start]);
        let src = image.src.trim();
        let resolved = parse_pic_reference(src)
            .and_then(|pic_id| by_pic.get(pic_id).copied())
            .or_else(|| is_supported_source(src).then_some(src));

        let assigned = resolved.and_then(|url| {
            if DATA_IMAGE_MIME_RE.is_match(url) {
                ctx.assign(url, Some(url))
            } else if url.to_ascii_lowercase().starts_with("http://")
                || url.to_ascii_lowercase().starts_with("https://")
            {
                ctx.assign(url, None)
            } else {
                None
            }
        });

        match assigned {
            Some(temp_pic_id) => {
                out.push_str(&markdown_image(&image.alt, &format!("#<{temp_pic_id}>")))
            }
            None => out.push_str(&image.alt),
        }
        last = image.end;
    }
    out.push_str(&cleaned[last..]);
    out.trim().to_string()
}

/// 生成 Markdown 图片语法：URL 含空格或括号时使用 `<...>` 形式。
pub fn markdown_image(alt: &str, src: &str) -> String {
    let alt = alt.replace(['[', ']'], " ");
    let alt = alt.trim();
    let src = src.trim();
    let destination = if src.contains(['(', ')', ' ']) {
        format!("<{src}>")
    } else {
        src.to_string()
    };
    format!("![{alt}]({destination})")
}

/// 提取富文本中所有 `#<pic_id>` 引用（按出现顺序去重）。
pub fn referenced_pic_ids(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    find_markdown_images(text)
        .into_iter()
        .filter_map(|image| {
            parse_pic_reference(&image.src)
                .map(|id| id.to_string())
                .filter(|id| seen.insert(id.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_images_are_parsed_and_stripped() {
        let text = "如图 ![曲线图](https://www.uooc.net.cn/a.png) 与 ![b](#<2>) 求面积";
        let images = find_markdown_images(text);
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].alt, "曲线图");
        assert_eq!(images[0].src, "https://www.uooc.net.cn/a.png");
        assert_eq!(images[1].src, "#<2>");

        assert_eq!(
            strip_markdown_images(text, false),
            "如图  与  求面积"
        );
        assert_eq!(
            strip_markdown_images(text, true),
            "如图 曲线图 与 b 求面积"
        );
    }

    #[test]
    fn markdown_image_wraps_urls_with_spaces_or_parentheses() {
        assert_eq!(
            markdown_image("图", "https://a.com/x(1).png"),
            "![图](<https://a.com/x(1).png>)"
        );
        assert_eq!(
            markdown_image("图", "https://a.com/x y.png"),
            "![图](<https://a.com/x y.png>)"
        );
        assert_eq!(
            markdown_image("a]b", "https://a.com/x.png"),
            "![a b](https://a.com/x.png)"
        );
    }

    #[test]
    fn markdown_images_support_angles_titles_and_data_uris() {
        let angle = find_markdown_images("![图](<https://a.com/x y.png> \"title\")");
        assert_eq!(angle[0].src, "https://a.com/x y.png");
        let data = find_markdown_images("![图](data:image/png;base64,AAAA)");
        assert_eq!(data[0].src, "data:image/png;base64,AAAA");
    }

    #[test]
    fn html_detection_ignores_markdown_and_math() {
        assert!(!has_html_tags("若 x < y 且 z > 0，则？"));
        assert!(!has_html_tags("![图](#<1>)"));
        assert!(has_html_tags("题干 <img src=\"a.png\">"));
    }

    #[test]
    fn stem_hash_is_independent_of_image_urls() {
        let a = normalize_stem("如图 ![图](https://a.com/1.png) 求面积");
        let b = normalize_stem("如图 ![图](#<pic_123>) 求面积");
        assert_eq!(a, b);
        assert_eq!(a, "如图 求面积");
    }

    #[test]
    fn clean_rich_text_strips_tags_but_keeps_images() {
        assert_eq!(
            clean_rich_text("<p>题干 <img src=\"x\"> 内容</p>"),
            "题干 内容"
        );
        assert_eq!(
            clean_rich_text("<span>![图](https://a.com/1.png)</span>"),
            "![图](https://a.com/1.png)"
        );
        assert_eq!(clean_rich_text("a<b>c"), "a c");
    }

    #[test]
    fn canonicalize_assigns_temp_ids_and_dedupes() {
        let (text, images) = canonicalize_local_text(
            "第一张 ![甲](https://a.com/1.png)，第二张 ![](https://a.com/2.png)，重复 ![甲](https://a.com/1.png)",
        );
        assert_eq!(
            text,
            "第一张 ![甲](#<1>)，第二张 ![](#<2>)，重复 ![甲](#<1>)"
        );
        assert_eq!(
            images,
            vec![
                BankImage { pic_id: "1".into(), url: "https://a.com/1.png".into() },
                BankImage { pic_id: "2".into(), url: "https://a.com/2.png".into() },
            ]
        );
    }

    #[test]
    fn html_images_are_converted_with_resolved_urls() {
        let html = r#"<p>如图 <img src="/a.png" alt="图A"/> 求面积 <img src="//cdn.example.com/b.png"></p>"#;
        let text = html_images_to_markdown(html, Some("https://www.uooc.net.cn/exam/1"));
        assert_eq!(
            text,
            "如图 ![图A](https://www.uooc.net.cn/a.png) 求面积 ![题目图片](https://cdn.example.com/b.png)"
        );

        let absolute = html_images_to_markdown(
            r#"<img src="https://x.com/b.png">"#,
            Some("https://www.uooc.net.cn/"),
        );
        assert_eq!(absolute, "![题目图片](https://x.com/b.png)");
    }

    #[test]
    fn canonicalize_keeps_existing_pic_refs_and_drops_relative() {
        let (text, images) = canonicalize_local_text("![a](#<pic_uuid>) ![b](/relative.png)");
        assert_eq!(text, "![a](#<pic_uuid>) b");
        assert!(images.is_empty());
    }

    #[test]
    fn upload_preparation_remaps_refs_and_builds_payload() {
        let images = vec![
            BankImage { pic_id: "1".into(), url: "https://a.com/1.png".into() },
            BankImage {
                pic_id: "2".into(),
                url: "data:image/png;base64,AAAA".into(),
            },
        ];
        let mut ctx = UploadContext::new();
        let text = prepare_rich_text_for_upload("![甲](#<1>) 与 ![乙](#<2>)", &images, &mut ctx);
        assert_eq!(text, "![甲](#<1>) 与 ![乙](#<2>)");
        let payload = ctx.into_images().unwrap();
        assert_eq!(payload.len(), 2);
        assert_eq!(payload[0].temp_pic_id, 1);
        assert_eq!(payload[0].pic_url.as_deref(), Some("https://a.com/1.png"));
        assert_eq!(payload[1].temp_pic_id, 2);
        assert_eq!(payload[1].mime_type.as_deref(), Some("image/png"));
        assert_eq!(payload[1].base64_pic_data.as_deref(), Some("data:image/png;base64,AAAA"));
    }

    #[test]
    fn upload_preparation_drops_unknown_refs_and_honours_limit() {
        let mut ctx = UploadContext::new();
        let text = prepare_rich_text_for_upload("![未知](#<nope>)", &[], &mut ctx);
        assert_eq!(text, "未知");
        assert!(ctx.into_images().is_none());

        let images: Vec<BankImage> = (1..=60)
            .map(|i| BankImage {
                pic_id: i.to_string(),
                url: format!("https://a.com/{i}.png"),
            })
            .collect();
        let mut refs = String::new();
        for i in 1..=60 {
            refs.push_str(&format!("![{i}](#<{i}>) "));
        }
        let mut ctx = UploadContext::new();
        let prepared = prepare_rich_text_for_upload(&refs, &images, &mut ctx);
        assert_eq!(ctx.into_images().unwrap().len(), MAX_UPLOAD_IMAGES);
        // 超出上限的图片降级为 alt，不再带 #<> 引用。
        assert!(!prepared.contains("#<51>"));
        assert!(prepared.contains("60"));
    }

    #[test]
    fn referenced_pic_ids_are_deduped() {
        assert_eq!(
            referenced_pic_ids("![a](#<x>) ![b](#<x>) ![c](#<y>)"),
            vec!["x".to_string(), "y".to_string()]
        );
    }
}
