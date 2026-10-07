//! Read endpoint declarations from the deployed school's own script bundle.
//! Never execute remote JavaScript, and never reuse another school's /add API.
use super::{
    client::Client,
    protocol::{Error, Failure, Result},
};
use std::sync::{Mutex, OnceLock};
#[derive(Clone, Default)]
pub struct Contract {
    pub drop_path: Option<String>,
    pub process_path: Option<String>,
}
static CONTRACT: OnceLock<Mutex<Contract>> = OnceLock::new();
pub fn current() -> Contract {
    CONTRACT
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .clone()
}
pub fn parse(source: &str) -> Contract {
    let functions = regex::Regex::new(r"function\s+([A-Za-z_$][\w$]*)\s*\(").unwrap();
    let path = regex::Regex::new(r#"/sys/xsxkapp/([A-Za-z0-9/_]+\.do)"#).unwrap();
    let mut result = Contract::default();
    for (i, m) in functions.captures_iter(source).enumerate() {
        let _ = i;
        let name = m[1].to_lowercase();
        if name != "deletevolunteerresult" && !name.contains("process") {
            continue;
        }
        let end = m.get(0).unwrap().end();
        let segment = &source[end..];
        let segment = segment.split("function ").next().unwrap_or(segment);
        let segment = &segment[..segment
            .char_indices()
            .nth(3000)
            .map(|(i, _)| i)
            .unwrap_or(segment.len())];
        if let Some(p) = path.captures(segment) {
            let p = p[1].to_owned();
            if !p.starts_with("elective/") {
                continue;
            }
            if name == "deletevolunteerresult" {
                result.drop_path = Some(p);
            } else if name != "initprocessinterval" {
                result.process_path = Some(p);
            }
        }
    }
    result
}
pub fn discover(c: &Client) -> Result<()> {
    if current().process_path.is_some() {
        return Ok(());
    }
    let entry = format!(
        "{}/xsxkapp/sys/xsxkapp/*default/grablessons.do",
        c.mode.origin()
    );
    let response = c
        .http
        .get(&entry)
        .query(&[("token", &c.token)])
        .send()
        .map_err(|_| Error::new(Failure::Network, "无法读取官方选课页面"))?;
    if !response.status().is_success() {
        return Ok(());
    }
    let html = response
        .text()
        .map_err(|_| Error::new(Failure::Network, "官方页面读取失败"))?;
    let scripts = regex::Regex::new(r#"<script[^>]+src=["']([^"']+)["']"#).unwrap();
    for capture in scripts.captures_iter(&html).take(40) {
        let Ok(url) = reqwest::Url::parse(&entry).unwrap().join(&capture[1]) else {
            continue;
        };
        if !matches!(
            url.host_str(),
            Some("xkres.szu.edu.cn" | "bkxk.szu.edu.cn" | "bkxk.webvpn.szu.edu.cn")
        ) || !url.path().ends_with(".js")
        {
            continue;
        }
        let Ok(response) = c.http.get(url).send() else {
            continue;
        };
        if !response.status().is_success()
            || response.content_length().is_some_and(|n| n > 2_000_000)
        {
            continue;
        }
        let Ok(source) = response.text() else {
            continue;
        };
        let found = parse(&source);
        let mut contract = CONTRACT.get_or_init(Default::default).lock().unwrap();
        if found.drop_path.is_some() {
            contract.drop_path = found.drop_path;
        }
        if found.process_path.is_some() {
            contract.process_path = found.process_path;
        }
    }
    Ok(())
}
