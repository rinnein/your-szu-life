use rig_core::message::ToolName;
use rig_core::tool::{DynamicTool, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct ToolEvent {
    pub text: String,
}

impl ToolEvent {
    fn line(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

pub fn web_search_tool(progress: mpsc::UnboundedSender<ToolEvent>) -> DynamicTool {
    DynamicTool::new(
        ToolName::new("web_search").expect("static tool name"),
        "进行网页实时搜索以检索权威事实、百科知识或学术解析；遇到冷门考点或不确定的选项时使用。",
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "搜索关键词或短语" }
            },
            "required": ["query"]
        }),
        move |args| {
            let progress = progress.clone();
            Box::pin(async move {
                let query = args["query"].as_str().unwrap_or("").trim().to_string();
                let _ = progress.send(ToolEvent::line(format!(
                    "[工具调用: web_search(\"{query}\")]\n"
                )));
                let result = execute_bing_search(&query).await;
                let _ = progress.send(ToolEvent::line(
                    "[工具返回: web_search 已获取检索结果，提交模型推导...]\n",
                ));
                Ok(ToolOutput::text(result))
            })
        },
    )
}

pub fn web_fetch_tool(progress: mpsc::UnboundedSender<ToolEvent>) -> DynamicTool {
    DynamicTool::new(
        ToolName::new("web_fetch").expect("static tool name"),
        "深度抓取指定网页的正文文本内容，自动提取主要文字段落，用于精读权威解析或学术文献。",
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "待抓取的完整网页 URL（HTTP 或 HTTPS）" }
            },
            "required": ["url"]
        }),
        move |args| {
            let progress = progress.clone();
            Box::pin(async move {
                let url = args["url"].as_str().unwrap_or("").trim().to_string();
                let _ = progress.send(ToolEvent::line(format!(
                    "[工具调用: web_fetch(\"{url}\")]\n"
                )));
                let result = execute_web_fetch(&url).await;
                let _ = progress.send(ToolEvent::line(
                    "[工具返回: web_fetch 已抓取并清洗网页正文，精读考点...]\n",
                ));
                Ok(ToolOutput::text(result))
            })
        },
    )
}

fn search_http_client() -> Result<reqwest::Client, String> {
    szu_core::netproxy::client_builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| "无法初始化搜索客户端".into())
}

/// 执行网页检索（优先采用 Bing 搜索国内直连，支持多节点回退与 DuckDuckGo 兜底）
pub async fn execute_bing_search(query: &str) -> String {
    let clean_query = query.trim();
    if clean_query.is_empty() {
        return "搜索查询为空".into();
    }

    let search_one = |url_base: &'static str| async move {
        let client = search_http_client().ok()?;
        let mut url = reqwest::Url::parse(url_base).ok()?;
        url.query_pairs_mut().append_pair("q", clean_query);
        let resp = client
            .get(url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
            )
            .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let html_text = resp.text().await.ok()?;
        let items = parse_bing_snippets_from_html(&html_text);
        if items.is_empty() {
            return None;
        }
        Some(
            items
                .into_iter()
                .enumerate()
                .map(|(i, (title, link, snippet))| {
                    if link.is_empty() {
                        format!("{}. [{}] {}", i + 1, title, snippet)
                    } else {
                        format!("{}. [{}] {}\n   链接: {}", i + 1, title, snippet, link)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };

    if let Some(res) = search_one("https://cn.bing.com/search").await {
        return res;
    }
    if let Some(res) = search_one("https://www.bing.com/search").await {
        return res;
    }
    execute_web_search(clean_query).await
}

pub fn parse_bing_snippets_from_html(html_text: &str) -> Vec<(String, String, String)> {
    let doc = scraper::Html::parse_document(html_text);
    let algo_sel = scraper::Selector::parse("li.b_algo").ok();
    let title_sel = scraper::Selector::parse("h2 a").ok();
    let snippet_sel = scraper::Selector::parse(".b_caption p, p.b_algoSlug, p").ok();

    let mut results = Vec::new();
    if let (Some(algo_s), Some(title_s), Some(snip_s)) = (algo_sel, title_sel, snippet_sel) {
        for algo in doc.select(&algo_s).take(6) {
            let (title, link) = if let Some(a) = algo.select(&title_s).next() {
                let t = a.text().collect::<Vec<_>>().join(" ").trim().to_string();
                let l = a.value().attr("href").unwrap_or("").trim().to_string();
                (t, l)
            } else {
                (String::new(), String::new())
            };
            let snippet = if let Some(p) = algo.select(&snip_s).next() {
                p.text().collect::<Vec<_>>().join(" ").trim().to_string()
            } else {
                String::new()
            };
            if !title.is_empty() || !snippet.is_empty() {
                results.push((title, link, snippet));
            }
        }
    }
    results
}

fn is_private_or_local_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ipv4) => {
            ipv4.is_loopback()
                || ipv4.is_private()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || ipv4.is_unspecified()
        }
        std::net::IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unspecified()
                || (ipv6.segments()[0] & 0xfe00) == 0xfc00
                || (ipv6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

pub async fn execute_web_fetch(raw_url: &str) -> String {
    let raw_url = raw_url.trim();
    let Ok(parsed) = reqwest::Url::parse(raw_url) else {
        return "网页地址无效，仅支持以 http:// 或 https:// 开头的有效网址".into();
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return "网页协议不受支持，仅支持 HTTP 或 HTTPS".into();
    }
    let Some(host) = parsed.host_str() else {
        return "网页地址缺少主机名".into();
    };
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_private_or_local_ip(ip) {
            return "出于安全考量，禁止访问本地或私有局域网地址".into();
        }
    } else {
        let port = parsed.port_or_known_default().unwrap_or(80);
        let Ok(mut addrs) = tokio::net::lookup_host((host, port)).await else {
            return format!("无法解析域名「{host}」");
        };
        if addrs.any(|addr| is_private_or_local_ip(addr.ip())) {
            return "出于安全考量，该域名解析至私有或本地回环地址，已被拦截".into();
        }
    }

    let client = match search_http_client() {
        Ok(client) => client,
        Err(error) => return format!("无法初始化 HTTP 客户端: {error}"),
    };
    let resp = match client
        .get(parsed)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
        )
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(error) => return format!("网页抓取失败: {error}"),
    };
    if !resp.status().is_success() {
        return format!("网页请求返回状态码: {}", resp.status());
    }
    if let Some(len) = resp.content_length() {
        if len > 2 * 1024 * 1024 {
            return "网页内容过大（超过 2MB），已跳过抓取".into();
        }
    }
    let html_text = match resp.text().await {
        Ok(text) => text,
        Err(error) => return format!("读取网页内容失败: {error}"),
    };
    let cleaned = extract_clean_article_text(&html_text);
    if cleaned.is_empty() {
        return "网页未包含可读取的文本正文".into();
    }
    if cleaned.chars().count() > 5000 {
        let truncated: String = cleaned.chars().take(5000).collect();
        format!("{truncated}\n\n[注：网页正文较长，已截取前 5000 字符]")
    } else {
        cleaned
    }
}

pub fn extract_clean_article_text(html_text: &str) -> String {
    let doc = scraper::Html::parse_document(html_text);
    let container_sel =
        scraper::Selector::parse("article, main, .content, .main-content, #content, body").ok();
    let p_sel = scraper::Selector::parse("h1, h2, h3, h4, p, li").ok();
    if let (Some(cs), Some(ps)) = (container_sel, p_sel) {
        if let Some(container) = doc.select(&cs).next() {
            let mut paragraphs = Vec::new();
            for elem in container.select(&ps) {
                let tag = elem.value().name();
                let text = elem.text().collect::<Vec<_>>().join(" ").trim().to_string();
                if text.len() >= 2 {
                    if tag.starts_with('h') {
                        paragraphs.push(format!("\n### {text}"));
                    } else {
                        paragraphs.push(text);
                    }
                }
            }
            if !paragraphs.is_empty() {
                return paragraphs.join("\n");
            }
        }
    }
    if let Some(body_sel) = scraper::Selector::parse("body").ok() {
        if let Some(body) = doc.select(&body_sel).next() {
            let raw_text = body.text().collect::<Vec<_>>().join(" ");
            let lines: Vec<&str> = raw_text.split_whitespace().collect();
            return lines.join(" ");
        }
    }
    String::new()
}

pub async fn execute_web_search(query: &str) -> String {
    let clean_query = query.trim();
    if clean_query.is_empty() {
        return "搜索查询为空".into();
    }

    let ddg_lite = async {
        let client = search_http_client().ok()?;
        let resp = client
            .post("https://lite.duckduckgo.com/lite/")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .form(&[("q", clean_query)])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let html_text = resp.text().await.ok()?;
        let snippets = parse_search_snippets_from_html(&html_text);
        (!snippets.is_empty()).then(|| snippets.join("\n\n"))
    }
    .await;
    if let Some(res) = ddg_lite {
        return res;
    }

    let ddg_html = async {
        let client = search_http_client().ok()?;
        let mut url = reqwest::Url::parse("https://html.duckduckgo.com/html/").ok()?;
        url.query_pairs_mut().append_pair("q", clean_query);
        let resp = client
            .get(url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let html_text = resp.text().await.ok()?;
        let snippets = parse_search_snippets_from_html(&html_text);
        (!snippets.is_empty()).then(|| snippets.join("\n\n"))
    }
    .await;
    if let Some(res) = ddg_html {
        return res;
    }
    format!(
        "已检索网络关键词「{}」，未获得有效外部网页摘要，请根据已知知识库与题目逻辑推导作答。",
        clean_query
    )
}

pub fn parse_search_snippets_from_html(html_text: &str) -> Vec<String> {
    let doc = scraper::Html::parse_document(html_text);
    let snippet_sel =
        scraper::Selector::parse(".result-snippet, .result__snippet, td.result-snippet").ok();
    let title_sel =
        scraper::Selector::parse(".result-link, .result__title, a.result-link, a.result__url").ok();
    let mut snippets = Vec::new();
    let titles: Vec<String> = if let Some(ts) = title_sel {
        doc.select(&ts)
            .map(|e| e.text().collect::<Vec<_>>().join(" ").trim().to_string())
            .collect()
    } else {
        Vec::new()
    };
    if let Some(ss) = snippet_sel {
        for (i, elem) in doc.select(&ss).enumerate().take(5) {
            let title = titles.get(i).cloned().unwrap_or_default();
            let snip = elem.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !snip.is_empty() {
                if title.is_empty() {
                    snippets.push(format!("{}. {}", i + 1, snip));
                } else {
                    snippets.push(format!("{}. [{}] {}", i + 1, title, snip));
                }
            }
        }
    }
    snippets
}

pub fn looks_like_vision_unsupported(message: &str) -> bool {
    let lower = message.to_lowercase();
    let image_hint = lower.contains("image")
        || lower.contains("vision")
        || lower.contains("multimodal")
        || lower.contains("图片")
        || lower.contains("图像");
    let reject_hint = lower.contains("unsupported")
        || lower.contains("not support")
        || lower.contains("does not support")
        || lower.contains("only text")
        || lower.contains("text only")
        || lower.contains("invalid content")
        || lower.contains("不支持")
        || lower.contains("仅支持文本");
    image_hint && reject_hint
}

pub fn looks_like_tool_unsupported(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("tool")
        && (lower.contains("unsupported")
            || lower.contains("not support")
            || lower.contains("does not support")
            || lower.contains("invalid")
            || lower.contains("不支持"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_snippets_parsing_from_html() {
        let sample_html = r#"
            <table>
                <tr><td><a class="result-link" href="https://example.com/1">汪元量 - 百度百科</a></td></tr>
                <tr><td class="result-snippet">汪元量，字大有，宋末元初著名琴师，曾出仕南宋宫廷。</td></tr>
                <tr><td><a class="result-link" href="https://example.com/2">送琴师毛敏仲北行</a></td></tr>
                <tr><td class="result-snippet">汪元量所作诗作，抒发亡国遗民之痛。</td></tr>
            </table>
        "#;
        let snippets = parse_search_snippets_from_html(sample_html);
        assert_eq!(snippets.len(), 2);
        assert!(snippets[0].contains("汪元量 - 百度百科"));
        assert!(snippets[0].contains("宋末元初著名琴师"));
        assert!(snippets[1].contains("送琴师毛敏仲北行"));
    }

    #[test]
    fn bing_snippets_parsing_from_html() {
        let sample = r#"
            <ul>
                <li class="b_algo">
                    <h2><a href="https://baike.baidu.com/item/1">深圳大学简介 - 百度百科</a></h2>
                    <div class="b_caption"><p>深圳大学（Shenzhen University），简称“深大”。</p></div>
                </li>
                <li class="b_algo">
                    <h2><a href="https://www.szu.edu.cn">深圳大学官方网站</a></h2>
                    <p class="b_algoSlug">深圳大学办学历史、师资队伍、学科建设。</p>
                </li>
            </ul>
        "#;
        let results = parse_bing_snippets_from_html(sample);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "深圳大学简介 - 百度百科");
        assert_eq!(results[0].1, "https://baike.baidu.com/item/1");
        assert!(results[0].2.contains("Shenzhen University"));
        assert_eq!(results[1].0, "深圳大学官方网站");
    }

    #[tokio::test]
    async fn web_fetch_rejects_ssrf_and_invalid_urls() {
        let res = execute_web_fetch("http://127.0.0.1:8080/secret").await;
        assert!(res.contains("禁止访问本地或私有局域网地址"));
        let res = execute_web_fetch("http://192.168.1.1/admin").await;
        assert!(res.contains("禁止访问本地或私有局域网地址"));
        let res = execute_web_fetch("file:///etc/passwd").await;
        assert!(res.contains("仅支持 HTTP 或 HTTPS"));
        let res = execute_web_fetch("not-a-valid-url").await;
        assert!(res.contains("网页地址无效"));
    }

    #[test]
    fn extract_clean_article_text_from_html() {
        let sample = r#"
            <html><head><style>body { color: red; }</style></head>
            <body>
                <nav><a href="/">首页</a></nav>
                <main>
                    <h1>深度学习基础</h1>
                    <p>反向传播算法是多层神经网络的核心训练机制。</p>
                    <p>梯度下降用于更新权重以最小化损失函数。</p>
                </main>
                <footer>版权所有</footer>
            </body></html>
        "#;
        let text = extract_clean_article_text(sample);
        assert!(text.contains("深度学习基础"));
        assert!(text.contains("反向传播算法"));
        assert!(text.contains("梯度下降用于更新权重"));
    }
}
