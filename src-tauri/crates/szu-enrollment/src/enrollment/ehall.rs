use super::{
    client::decode,
    protocol::{self, Error, Failure, Result},
    session,
};
use serde_json::Value;
use std::sync::Mutex;

pub(super) const EHALL_BASE: &str = "https://ehall.szu.edu.cn";
const MAX_REDIRECTS: usize = 15;

/// eHall 微应用共用的会话与请求外壳。
///
/// 我的课表、学业完成查询与成绩查询都需要同一条认证链：入口 CAS →
/// `appShow` → 角色列表 → `gid_` 角色入口，随后模块 API 必须带上
/// `X-Requested-With` 与角色入口 Referer。这里把这条链收敛成一份实现，
/// 各业务模块只保留自己的入口、appId 与请求路径。
pub(super) struct Ehall {
    client: reqwest::blocking::Client,
    /// 当前应用的角色入口（带 `gid_`）；模块 API 必须用它作为 Referer。
    referer: Mutex<Option<String>>,
}

impl Ehall {
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

    /// 建立指定 eHall 应用会话并记住角色入口。
    ///
    /// 顺序与真实浏览器一致：
    /// 1. GET 应用入口并跟随 CAS，拿到 `MOD_AUTH_CAS`（最终 403 也视为 CAS 成功，
    ///    因为应用还需要角色）；
    /// 2. GET `appShow?appId=...` 初始化应用上下文；
    /// 3. GET `appMultiGroupEntranceList` 取带 `gid_` 的角色入口；
    /// 4. GET 角色入口（`gid_`）建立角色会话，并把它作为后续模块 API 的 Referer。
    pub fn bootstrap(&self, entry: &str, app_id: &str, label: &str) -> Result<()> {
        self.enter(entry)?;

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
        let groups = value["data"]["groupList"].as_array().ok_or_else(|| {
            Error::new(
                Failure::Session,
                format!("当前账号没有可用的「{label}」角色"),
            )
        })?;
        let target = groups
            .iter()
            .find(|group| {
                let name = protocol::text(&group["groupName"]);
                name.contains("学生") || name.contains("本科")
            })
            .or_else(|| groups.first())
            .map(|group| protocol::text(&group["targetUrl"]).replace("&amp;", "&"))
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| {
                Error::new(
                    Failure::Session,
                    format!("当前账号没有可用的「{label}」角色"),
                )
            })?;

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
            return Err(Error::new(
                Failure::Session,
                "教务角色跳转次数过多，请重新认证",
            ));
        }
        *self.referer.lock().unwrap() = Some(target);
        Ok(())
    }

    /// 只完成入口 CAS，不解析角色入口。
    ///
    /// 用于已知 appId 与学校当前部署不一致时降级：部分微应用的模块 API
    /// 只校验 CAS 会话，也能用入口本身作为 Referer。调用后清掉可能错误的
    /// 角色 Referer，让后续请求回退到各模块自己的入口地址。
    pub fn bootstrap_entry(&self, entry: &str) -> Result<()> {
        *self.referer.lock().unwrap() = None;
        self.enter(entry)
    }

    fn enter(&self, entry: &str) -> Result<()> {
        let mut url = reqwest::Url::parse(entry)
            .map_err(|_| Error::new(Failure::Protocol, "教务入口地址无效"))?;
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
            // 微应用入口在 CAS 成功后仍可能 403（缺应用角色），此时 CAS 会话已建立。
            return Ok(());
        }
        Err(Error::new(
            Failure::Session,
            "教务登录跳转次数过多，请重新认证",
        ))
    }

    /// 微应用模块 API 统一要求 `X-Requested-With` 与角色入口 Referer。
    pub fn post(
        &self,
        path: &str,
        form: &[(String, String)],
        fallback_referer: &str,
    ) -> Result<Value> {
        let response = self
            .client
            .post(format!("{EHALL_BASE}{path}"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", self.referer(fallback_referer))
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
                "教务接口拒绝访问，请重新登录应用后再试",
            ));
        }
        decode(response)
    }

    /// GET 版模块 API（部分旧接口只接受 GET）。
    pub fn get(&self, path: &str, fallback_referer: &str) -> Result<Value> {
        let response = self
            .client
            .get(format!("{EHALL_BASE}{path}"))
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", self.referer(fallback_referer))
            .send()
            .map_err(|_| Error::new(Failure::Network, "教务系统暂时无法访问"))?;
        let status = response.status();
        if status.as_u16() >= 500 {
            return Err(Error::new(Failure::Network, "教务系统暂时不可用"));
        }
        if status.is_redirection() || !status.is_success() {
            return Err(Error::new(
                Failure::Session,
                "教务接口拒绝访问，请重新登录应用后再试",
            ));
        }
        decode(response)
    }

    fn referer(&self, fallback: &str) -> String {
        self.referer
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| fallback.to_string())
    }
}

fn trusted_host(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str().map(|host| host.trim_end_matches('.')) else {
        return false;
    };
    host == "szu.edu.cn" || host.ends_with(".szu.edu.cn")
}
