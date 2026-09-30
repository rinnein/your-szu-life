use super::model::{courses_page, id, number, text, Course, User, Video};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::Value;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) const ORIGIN: &str = "https://www.uooc.net.cn";
pub(super) const LOGIN: &str = "https://www.uooc.net.cn/user/login";
pub(super) const LIST: &str = "/home/course/list";
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Error {
    Cancelled,
    Unauthorized,
    Gate,
    Network,
    Invalid,
    Server(i64),
}
impl Error {
    pub fn message(&self) -> String {
        match self {
            Self::Cancelled => "任务已取消".into(),
            Self::Unauthorized => "UOOC 登录已失效，请重新登录".into(),
            Self::Gate => "需要先完成前置任务或测验".into(),
            Self::Network => "UOOC 请求失败，请检查网络后重试".into(),
            Self::Invalid => "UOOC 返回数据格式无效".into(),
            Self::Server(code) => format!("UOOC 拒绝请求（状态码 {code}）"),
        }
    }
}
pub(super) struct Client {
    http: reqwest::Client,
    pub jar: Arc<CookieStoreMutex>,
    base: String,
}
impl Client {
    pub fn new(jar: Arc<CookieStoreMutex>) -> Result<Self, String> {
        Self::with_base(jar, ORIGIN)
    }
    pub fn with_base(jar: Arc<CookieStoreMutex>, base: &str) -> Result<Self, String> {
        #[cfg(target_os = "android")]
        crate::platform_tls::initialize_platform_verifier()?;
        let http = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent("your-szu-life/0.2 UOOC")
            .build()
            .map_err(|_| "无法初始化 UOOC 网络服务")?;
        Ok(Self {
            http,
            jar,
            base: base.into(),
        })
    }
    async fn request(
        &self,
        path: &str,
        fields: &[(String, String)],
        post: bool,
        cancel: &CancellationToken,
    ) -> Result<Value, Error> {
        let attempts = if post { 1 } else { 3 };
        for attempt in 0..attempts {
            let request = if post {
                self.http.post(format!("{}{path}", self.base)).form(fields)
            } else {
                self.http.get(format!("{}{path}", self.base)).query(fields)
            }
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", format!("{}/home", self.base))
            .header("XSRF", "");
            let result = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                result = async {
                    let mut response = request.send().await.map_err(|_| Error::Network)?;
                    if response.status().is_redirection() || matches!(response.status().as_u16(), 401 | 403) { return Err(Error::Unauthorized); }
                    if !response.status().is_success() { return Err(Error::Network); }
                    let mut bytes = Vec::new();
                    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Network)? {
                        if bytes.len() + chunk.len() > 4 * 1024 * 1024 { return Err(Error::Invalid); }
                        bytes.extend_from_slice(&chunk);
                    }
                    let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Invalid)?;
                    let code = text(&value["code"]).parse::<i64>().map_err(|_| Error::Invalid)?;
                    match code { 1 => Ok(value["data"].clone()), 401 => Err(Error::Unauthorized), 600 => Err(Error::Gate), other => Err(Error::Server(other)) }
                } => result,
            };
            if result != Err(Error::Network) || attempt + 1 == attempts {
                return result;
            }
            tokio::select! { biased; _ = cancel.cancelled() => return Err(Error::Cancelled), _ = tokio::time::sleep(Duration::from_secs(1 << attempt)) => {} }
        }
        Err(Error::Network)
    }
    pub async fn user(&self, cancel: &CancellationToken) -> Result<User, Error> {
        let value = self
            .request("/home/member/user", &[], false, cancel)
            .await?;
        Ok(User {
            id: id(&value["id"]).ok_or(Error::Unauthorized)?,
            name: ["name", "nick"]
                .iter()
                .map(|k| text(&value[*k]))
                .find(|v| !v.is_empty())
                .unwrap_or_else(|| "UOOC 用户".into()),
        })
    }
    pub async fn courses(&self, cancel: &CancellationToken) -> Result<Vec<Course>, Error> {
        let mut courses = vec![];
        let mut seen = HashSet::new();
        let mut page = 1;
        loop {
            let data = self
                .request(
                    LIST,
                    &[
                        ("keyword".into(), "".into()),
                        ("page".into(), page.to_string()),
                        ("type".into(), "learn".into()),
                    ],
                    false,
                    cancel,
                )
                .await?;
            let (rows, pages) = courses_page(&data).map_err(|_| Error::Invalid)?;
            courses.extend(rows.into_iter().filter(|c| seen.insert(c.id.clone())));
            if page >= pages {
                break;
            }
            page += 1;
        }
        Ok(courses)
    }
    pub async fn progress(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<super::progress::CourseProgress, Error> {
        let value = self
            .request(
                "/home/course/progress",
                &[("cid".into(), cid.into())],
                false,
                cancel,
            )
            .await?;
        if !value.is_object() {
            return Err(Error::Invalid);
        }
        Ok(super::progress::CourseProgress::parse(
            &value,
            super::progress::today(),
        ))
    }
    pub async fn catalog(
        &self,
        cid: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        self.request(
            "/home/learn/getCatalogList",
            &[
                ("cid".into(), cid.into()),
                ("hidemsg_".into(), "true".into()),
            ],
            false,
            cancel,
        )
        .await?
        .as_array()
        .cloned()
        .ok_or(Error::Invalid)
    }
    pub async fn unit(
        &self,
        video: &Video,
        cancel: &CancellationToken,
    ) -> Result<Vec<Value>, Error> {
        let mut fields = vec![
            ("cid".into(), video.cid.clone()),
            ("chapter_id".into(), video.chapter.clone()),
            ("section_id".into(), video.section.clone()),
            (
                "catalog_id".into(),
                video.subsection.as_ref().unwrap_or(&video.section).clone(),
            ),
            ("hidemsg_".into(), "true".into()),
            ("show".into(), "".into()),
        ];
        if let Some(sub) = &video.subsection {
            fields.push(("subsection_id".into(), sub.clone()));
        }
        self.request("/home/learn/getUnitLearn", &fields, false, cancel)
            .await?
            .as_array()
            .cloned()
            .ok_or(Error::Invalid)
    }
    pub async fn mark(
        &self,
        video: &Video,
        duration: f64,
        position: f64,
        cancel: &CancellationToken,
    ) -> Result<bool, Error> {
        let mut fields = vec![
            ("cid".into(), video.cid.clone()),
            ("chapter_id".into(), video.chapter.clone()),
            ("section_id".into(), video.section.clone()),
            ("resource_id".into(), video.resource.clone()),
            ("hidemsg_".into(), "true".into()),
            ("network".into(), "2".into()),
            ("source".into(), "1".into()),
            ("video_length".into(), format!("{duration:.2}")),
            ("video_pos".into(), format!("{position:.2}")),
        ];
        if let Some(sub) = &video.subsection {
            fields.push(("subsection_id".into(), sub.clone()));
        }
        let result = self
            .request("/home/learn/markVideoLearn", &fields, true, cancel)
            .await?;
        if result.get("finished").is_none() {
            return Err(Error::Invalid);
        }
        Ok(number(&result["finished"]) != 0.0)
    }
}
