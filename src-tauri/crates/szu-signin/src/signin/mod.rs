//! 签到/打卡模块：畅课（TronClass）与 SZU 考勤（qw.szu.edu.cn）两类签到、
//! 课表缓存、签到码共享房间与课表驱动的计划探测。
//!
//! 所有 Tauri command 集中在本 crate（`signin_` 前缀），跨 crate 复用
//! szu-lms 的畅课客户端与 szu-mesh 的临时房间；命令注册四处校验见
//! `src-tauri/src/tests.rs`。

pub mod automation;
pub mod buildings;
pub mod commands;
pub mod db;
pub mod geo;
pub mod matching;
pub mod migrations;
pub mod model;
pub mod photo;
pub mod qw;
pub mod share;
pub mod scheduler;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tauri::{Emitter, Manager};

use automation::{AutoRuntime, SigninAutomationSettings};
use szu_core::storage::SharedCredentialStore;

// 跨 crate 消费（szu-todos 聚合、MCP 资源、课表签到标签）。
pub use model::{CachedSzuCheckin, CheckinTodoItem, CheckinView, SzuCheckinState};
use model::{LastPoint, QwSessionState, QwStatusView, ResolvedPoint, TimetableCache};

/// 课表缓存的内存有效期。
const TIMETABLE_TTL_MS: i64 = 6 * 60 * 60 * 1000;
/// LMS 雷达提交的偏移上限（服务端围栏 200m，留安全余量）。
const LMS_JITTER_CAP_M: f64 = 150.0;
/// 共享房间的时间窗外沿：课次开始/结束各放宽 10 分钟。
pub const ROOM_WINDOW_MARGIN_MS: i64 = 10 * 60 * 1000;

/// 本机在点名接口里的稳定设备标识（随机 UUID，首次使用时生成）。
pub const SIGNIN_DEVICE_ID: &str = "signin.device_id";

/// Tauri managed state。
pub struct SigninStore {
    credentials: SharedCredentialStore,
    service: OnceLock<Arc<Service>>,
}

impl SigninStore {
    pub fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            service: OnceLock::new(),
        }
    }
}

pub struct Service {
    app: Option<tauri::AppHandle>,
    credentials: SharedCredentialStore,
    path: PathBuf,
    /// 考勤与统一认证的站点基址；生产固定为常量，测试注入 mock 地址。
    qw_base: String,
    auth_base: String,
    /// 课表缓存（内存副本，落盘走 db）。
    timetable: Mutex<TimetableCache>,
    /// 最近一次待签视图（探测循环与 szu-todos 聚合读取）。
    pending: Mutex<CheckinView>,
    /// 按课次的 SZU 考勤状态；只由课表窗口驱动的 probe 刷新。
    szu_checkins: Mutex<HashMap<String, CachedSzuCheckin>>,
    /// 签到码共享房间（同一时刻至多一个活跃房间）。
    room: Mutex<Option<share::RoomSession>>,
    /// 网关会话状态（与 lms/jxxk 的会话管理同构）。
    qw_session: Mutex<QwSessionState>,
    qw_message: Mutex<String>,
    /// cas/status 带回的账号信息（student_id = username）。
    qw_student_id: Mutex<Option<String>>,
    qw_user_name: Mutex<Option<String>>,
    /// 用户主动停止匹配的课程：在时间窗结束前不再自动建房。
    room_suspended: Mutex<HashSet<i64>>,
    /// 上次自动建房尝试时间（失败冷却，避免窗口内每轮探测都重试组网）。
    room_last_attempt: Mutex<Option<std::time::Instant>>,
    /// 自动签到开关（设备本地，由命令写入并即时生效）。
    automation: Mutex<SigninAutomationSettings>,
    /// 自动提交尝试账本（内存；成功/失败次数与最近失败原因）。
    auto_runtime: Mutex<AutoRuntime>,
    /// 探测串行锁：scheduler 与 `probe_if_due` 可能并发触发，避免重复自动提交。
    probe_lock: tokio::sync::Mutex<()>,
    /// 自引用，用于派生后台任务。
    self_weak: Mutex<Weak<Service>>,
    /// 事件订阅池：房间状态 / 匹配结果 / 共享计数。
    pub share_events: szu_core::ipc_channel::SubscriberPool<share::ShareEvent>,
}

impl Service {
    fn new(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        path: PathBuf,
    ) -> Result<Arc<Self>, String> {
        Self::with_bases(
            app,
            credentials,
            path,
            qw::QW_ORIGIN.to_string(),
            qw::QW_CAS_LOGIN.to_string(),
        )
    }

    /// 测试注入 mock 站点基址用。
    fn with_bases(
        app: Option<tauri::AppHandle>,
        credentials: SharedCredentialStore,
        path: PathBuf,
        qw_base: String,
        auth_base: String,
    ) -> Result<Arc<Self>, String> {
        let timetable = szu_core::migrations::db::run_sync(async {
            let database = db::SigninDatabase::open(&path).await?;
            database.load_timetable().await
        })
        .map_err(|e| format!("无法读取签到本地数据: {e}"))?;
        Ok(Arc::new(Self {
            app,
            credentials,
            path,
            qw_base,
            auth_base,
            timetable: Mutex::new(timetable),
            pending: Mutex::new(CheckinView::default()),
            szu_checkins: Mutex::new(HashMap::new()),
            room: Mutex::new(None),
            qw_session: Mutex::new(QwSessionState::NeedsAuth),
            qw_message: Mutex::new(String::new()),
            qw_student_id: Mutex::new(None),
            qw_user_name: Mutex::new(None),
            room_suspended: Mutex::new(HashSet::new()),
            room_last_attempt: Mutex::new(None),
            automation: Mutex::new(SigninAutomationSettings::default()),
            auto_runtime: Mutex::new(AutoRuntime::default()),
            probe_lock: tokio::sync::Mutex::new(()),
            self_weak: Mutex::new(Weak::new()),
            share_events: szu_core::ipc_channel::SubscriberPool::new(),
        }))
    }

    fn database(&self) -> Result<db::SigninDatabase, String> {
        let path = self.path.clone();
        szu_core::migrations::db::run_sync(async move { db::SigninDatabase::open(&path).await })
    }

    /// 本机点名设备标识（持久 UUID）。
    pub fn device_id(&self) -> Result<String, String> {
        let existing = szu_core::preferences::load::<String>(SIGNIN_DEVICE_ID)?;
        if let Some(id) = existing.filter(|id| !id.trim().is_empty()) {
            return Ok(id);
        }
        let id = uuid::Uuid::new_v4().to_string();
        szu_core::preferences::save(SIGNIN_DEVICE_ID, &id)?;
        Ok(id)
    }

    /// 当前待签视图（szu-todos 聚合读取）。
    pub fn checkin_view(&self) -> CheckinView {
        self.pending.lock().unwrap().clone()
    }

    /// 当前自动签到开关。
    pub fn automation_settings(&self) -> SigninAutomationSettings {
        self.automation.lock().unwrap().clone()
    }

    /// 保存自动签到开关：先落盘再更新内存缓存，保存失败不改变运行时状态。
    ///
    /// SZU 总开关切换会改变首页/课表里 SZU 签到状态的可见性，因此清掉旧缓存并
    /// 广播一次状态变化，让课表模块立即按新设置重算快照。
    pub fn set_automation_settings(
        &self,
        settings: SigninAutomationSettings,
    ) -> Result<SigninAutomationSettings, String> {
        let previous = self.automation_settings();
        let saved = settings.save()?;
        *self.automation.lock().unwrap() = saved.clone();
        if previous.szu_enabled != saved.szu_enabled {
            if !saved.szu_enabled {
                self.szu_checkins.lock().unwrap().clear();
            }
            if let Some(app) = &self.app {
                let _ = app.emit("signin-checkin-status-changed", ());
            }
        }
        Ok(saved)
    }

    /// 任一自动签到能力开启（调度器据此收紧课中探测节拍）。
    pub fn auto_checkin_active(&self) -> bool {
        self.automation.lock().unwrap().active()
    }

    /// 只读某个课次的 SZU 考勤状态。
    ///
    /// 状态只由 `probe()` 在课表窗口内刷新；课表模块不得自行轮询。
    /// SZU（实验性）总开关关闭时视为未知：首页与课表只显示畅课签到状态，
    /// 也不应拿旧缓存继续展示。
    pub fn cached_szu_checkin(&self, session_key: &str) -> SzuCheckinState {
        if !self.automation.lock().unwrap().szu_enabled {
            return SzuCheckinState::Unknown;
        }
        self.szu_checkins
            .lock()
            .unwrap()
            .get(session_key)
            .map(|cached| cached.state)
            .unwrap_or_default()
    }

    /// 最近一次 LMS 点名列表（含已签到条目），供课表标签使用；无缓存时为空。
    pub fn cached_rollcalls(&self) -> Vec<szu_lms::lms::rollcall::RollcallItem> {
        self.lms_service()
            .ok()
            .and_then(|lms| lms.rollcall_cache.get())
            .map(|(items, _)| items)
            .unwrap_or_default()
    }

    /// 课表缓存视图。
    pub fn timetable_snapshot(&self) -> TimetableCache {
        self.timetable.lock().unwrap().clone()
    }

    /// 拉取课表（带 TTL 缓存）。`force` 强制重拉。
    pub async fn refresh_timetable(&self, force: bool) -> Result<TimetableCache, String> {
        let fetched_at = self.timetable.lock().unwrap().fetched_at_ms;
        let stale = fetched_at <= 0
            || chrono::Utc::now().timestamp_millis() - fetched_at > TIMETABLE_TTL_MS;
        if !force && !stale {
            return Ok(self.timetable_snapshot());
        }
        self.ensure_qw(false).await;
        if *self.qw_session.lock().unwrap() != QwSessionState::Ready {
            // 统一认证 SSO 已失效（登录链停在 authserver）：如实提示重新认证，
            // 不要继续发请求报出误导性的「会话已失效」。
            return Err(self.qw_message.lock().unwrap().clone());
        }
        let client = self.build_qw_client()?;
        let sessions = match qw::student_schedule(&client).await {
            Ok(sessions) => sessions,
            Err(error) if error.session_expired => {
                // 会话中途失效：force 重bootstrap（必要时重走登录链）再拉一次。
                self.ensure_qw(true).await;
                if *self.qw_session.lock().unwrap() != QwSessionState::Ready {
                    return Err(self.qw_message.lock().unwrap().clone());
                }
                let client = self.build_qw_client()?;
                qw::student_schedule(&client)
                    .await
                    .map_err(|error| error.message)?
            }
            Err(error) => return Err(error.message),
        };
        let cache = TimetableCache {
            fetched_at_ms: chrono::Utc::now().timestamp_millis(),
            semester: sessions.first().and_then(|s| s.semester.clone()),
            sessions,
        };
        let database = self.database()?;
        let payload = cache.clone();
        szu_core::migrations::db::run_sync(async move { database.save_timetable(&payload).await })?;
        *self.timetable.lock().unwrap() = cache.clone();
        Ok(cache)
    }

    /// 指定时刻（默认现在）的本地课次 (start_ms, end_ms) 列表。
    pub fn today_sessions(&self, now_ms: i64) -> Vec<(i64, i64)> {
        let date = local_date(now_ms);
        self.timetable
            .lock()
            .unwrap()
            .sessions_on(&date)
            .into_iter()
            .map(|session| (session.start_ms, session.end_ms))
            .collect()
    }

    /// 用 vault 会话 + 统一认证 SSO 构建网关客户端。
    fn build_qw_client(&self) -> Result<qw::QwClient, String> {
        let cookies = self
            .credentials
            .load_qw_session()
            .map_err(|error| log::warn!("读取考勤会话失败: {error}"))
            .ok()
            .flatten();
        let sso = match &self.app {
            Some(app) => Some(szu_core::auth::reusable_sso_jar(app)?),
            None => None,
        };
        qw::QwClient::new(
            &self.qw_base,
            &self.auth_base,
            cookies.as_deref(),
            sso.as_deref(),
        )
    }

    /// 确保网关会话可用（与 szu-lms 的 ensure 同构）：vault 会话或 SSO 有效时
    /// 建立/恢复会话并落盘 cookie；失败只更新状态不清缓存。
    pub async fn ensure_qw(&self, force: bool) {
        {
            let state = *self.qw_session.lock().unwrap();
            if !force && state == QwSessionState::Ready {
                return;
            }
        }
        let client = match self.build_qw_client() {
            Ok(client) => client,
            Err(message) => {
                self.set_qw_state(QwSessionState::NeedsAuth, &message);
                return;
            }
        };
        match client.bootstrap().await {
            Ok(qw::Bootstrap::Established { student_id, name }) => match client.cookies() {
                Ok(cookies) => {
                    if let Err(error) = self.credentials.save_qw_session(&cookies) {
                        log::warn!("保存考勤会话失败: {error}");
                        // 不能置 Ready：refresh_timetable 之后会从 vault 重建
                        // 客户端，没落盘的会话会让接口全部 401，表现成误导性
                        // 的「会话已失效」。如实报不可用，让用户重试。
                        self.set_qw_state(QwSessionState::Unavailable, "无法保存考勤会话，请重试");
                        return;
                    }
                    *self.qw_student_id.lock().unwrap() =
                        (!student_id.is_empty()).then_some(student_id);
                    *self.qw_user_name.lock().unwrap() =
                        (!name.is_empty()).then_some(name);
                    self.set_qw_state(QwSessionState::Ready, "");
                }
                Err(message) => self.set_qw_state(QwSessionState::Unavailable, &message),
            },
            Ok(qw::Bootstrap::NeedsAuth) => {
                self.set_qw_state(QwSessionState::NeedsAuth, "请先完成应用统一认证，再使用 SZU 考勤签到")
            }
            Err(message) => self.set_qw_state(QwSessionState::Unavailable, &message),
        }
    }

    fn set_qw_state(&self, state: QwSessionState, message: &str) {
        *self.qw_session.lock().unwrap() = state;
        *self.qw_message.lock().unwrap() = message.to_owned();
    }

    /// 网关会话状态视图（`force` 时先重探一次）。
    pub async fn qw_status(&self, force: bool) -> QwStatusView {
        self.ensure_qw(force).await;
        QwStatusView {
            state: *self.qw_session.lock().unwrap(),
            message: self.qw_message.lock().unwrap().clone(),
        }
    }

    /// 计划探测一轮：刷新 LMS 待签列表与 SZU 考勤状态，并运行已开启的自动签到。
    /// LMS 侧列表自带 30s 缓存，本方法被探测循环与 szu-todos 复用时不会刷接口。
    pub async fn probe(&self) {
        // scheduler 与 szu-todos 的刷新可能并发触发；只允许一轮，避免重复提交。
        let Ok(_probe_guard) = self.probe_lock.try_lock() else {
            return;
        };
        let now = chrono::Utc::now().timestamp_millis();
        let automation = self.automation_settings();
        let previous_items = self.pending.lock().unwrap().items.clone();

        let lms_result = match self.lms_service() {
            Ok(lms) => lms.rollcall_list(false).await,
            Err(message) => Err(message),
        };

        let (mut items, complete) = match lms_result {
            Ok(mut rollcalls) => {
                if automation.lms_auto_radar {
                    let signed = self.auto_lms_radar(&rollcalls, now).await;
                    if !signed.is_empty() {
                        // 服务端已记录签到：强制刷新缓存，让课表与列表立即反映。
                        if let Ok(lms) = self.lms_service() {
                            if let Ok(refreshed) = lms.rollcall_list(true).await {
                                rollcalls = refreshed;
                            }
                        }
                        rollcalls.retain(|item| !signed.contains(&item.rollcall_id));
                    }
                }
                // 只清理当前仍可信的候选，保留失败退避账本。
                let keep = self.auto_keep_keys(&rollcalls);
                self.auto_runtime.lock().unwrap().retain(&keep);
                let runtime = self.auto_runtime.lock().unwrap();
                (automation::lms_items(&rollcalls, &runtime, now), true)
            }
            Err(_) => {
                // 探测失败：保留旧 LMS 条目但标记不完整，提醒层不据此关闭。
                (
                    previous_items
                        .iter()
                        .filter(|item| item.id.starts_with("checkin:lms:"))
                        .cloned()
                        .collect(),
                    false,
                )
            }
        };

        let mut status_changed = self.refresh_szu_checkins(now).await;
        if automation.szu_enabled {
            status_changed |= self.auto_szu(now).await;
            let sessions: Vec<model::TimetableSession> = self
                .timetable
                .lock()
                .unwrap()
                .sessions_on(&local_date(now))
                .into_iter()
                .cloned()
                .collect();
            let refs: Vec<&model::TimetableSession> = sessions.iter().collect();
            let states = self.szu_checkins.lock().unwrap().clone();
            let runtime = self.auto_runtime.lock().unwrap();
            items.extend(automation::szu_items(&refs, &states, &runtime, now));
        }

        let view = CheckinView { items, complete };
        let items_changed = previous_items != view.items;
        *self.pending.lock().unwrap() = view;
        if items_changed || status_changed {
            if let Some(app) = &self.app {
                let _ = app.emit("signin-checkin-status-changed", ());
            }
        }
    }

    /// 自动提交账本的保留键：当前待签 LMS 点名 + 当天全部 SZU 课次。
    fn auto_keep_keys(
        &self,
        rollcalls: &[szu_lms::lms::rollcall::RollcallItem],
    ) -> HashSet<String> {
        let mut keep: HashSet<String> = rollcalls
            .iter()
            .filter(|item| item.need_answer())
            .map(|item| AutoRuntime::lms_key(item.rollcall_id))
            .collect();
        let cache = self.timetable.lock().unwrap().clone();
        for session in cache.sessions_on(&local_date(chrono::Utc::now().timestamp_millis())) {
            keep.insert(AutoRuntime::szu_key(session));
        }
        keep
    }

    /// 畅课雷达自动签到：只处理雷达类型；返回本轮提交成功的点名 ID。
    async fn auto_lms_radar(
        &self,
        rollcalls: &[szu_lms::lms::rollcall::RollcallItem],
        now: i64,
    ) -> HashSet<i64> {
        let plans: Vec<(i64, i64, String)> = {
            let mut runtime = self.auto_runtime.lock().unwrap();
            rollcalls
                .iter()
                .filter(|item| {
                    item.need_answer()
                        && item.method() == szu_lms::lms::rollcall::RollcallMethod::Radar
                })
                .filter(|item| runtime.begin(&AutoRuntime::lms_key(item.rollcall_id), now))
                .map(|item| {
                    (
                        item.rollcall_id,
                        item.course_id,
                        item.course_title.clone(),
                    )
                })
                .collect()
        };
        let mut signed = HashSet::new();
        for (rollcall_id, course_id, course_title) in plans {
            let key = AutoRuntime::lms_key(rollcall_id);
            match self
                .submit_lms_radar(rollcall_id, course_id, &course_title, None, None)
                .await
            {
                Ok(_) => {
                    self.auto_runtime.lock().unwrap().succeed(&key);
                    signed.insert(rollcall_id);
                    log::info!("已自动完成畅课雷达签到（点名 {rollcall_id}）");
                }
                Err(message) => {
                    log::warn!("自动雷达签到失败（畅课点名 {rollcall_id}）: {message}");
                    self.auto_runtime
                        .lock()
                        .unwrap()
                        .fail(&key, format!("自动签到失败：{message}"));
                }
            }
        }
        signed
    }

    /// SZU 自动签到：对当前窗口内 `Pending` 的课次提交。返回状态是否有变化。
    async fn auto_szu(&self, now: i64) -> bool {
        let sessions: Vec<model::TimetableSession> = self
            .timetable
            .lock()
            .unwrap()
            .sessions_on(&local_date(now))
            .into_iter()
            .cloned()
            .collect();
        let candidates: Vec<(model::TimetableSession, String, String)> = {
            let states = self.szu_checkins.lock().unwrap();
            sessions
                .into_iter()
                .filter(|session| {
                    now >= session.start_ms - scheduler::WINDOW_HALF_MS
                        && now <= session.end_ms + scheduler::WINDOW_HALF_MS
                })
                .filter(|session| {
                    states
                        .get(&automation::szu_session_key(session))
                        .is_some_and(|cached| cached.state == model::SzuCheckinState::Pending)
                })
                .map(|session| {
                    let state_key = automation::szu_session_key(&session);
                    let runtime_key = AutoRuntime::szu_key(&session);
                    (session, state_key, runtime_key)
                })
                .collect()
        };
        if candidates.is_empty() {
            return false;
        }
        // 不在探测里 bootstrap：会话未就绪时留给课表慢刷新/手动刷新。
        if *self.qw_session.lock().unwrap() != QwSessionState::Ready {
            return false;
        }

        let mut changed = false;
        for (session, state_key, runtime_key) in candidates {
            if self.auto_runtime.lock().unwrap().is_completed(&runtime_key) {
                continue;
            }
            let has_photo = match self.course_has_photo(&session.course_name).await {
                Ok(has_photo) => has_photo,
                Err(message) => {
                    self.auto_runtime
                        .lock()
                        .unwrap()
                        .skip(&runtime_key, format!("读取签到照片失败：{message}"));
                    continue;
                }
            };
            if !has_photo {
                self.auto_runtime
                    .lock()
                    .unwrap()
                    .skip(&runtime_key, automation::NO_PHOTO_MESSAGE);
                continue;
            }
            if !self.auto_runtime.lock().unwrap().begin(&runtime_key, now) {
                continue;
            }
            match self.auto_submit_szu_session(&session).await {
                Ok(()) => {
                    self.auto_runtime.lock().unwrap().succeed(&runtime_key);
                    self.szu_checkins.lock().unwrap().insert(
                        state_key,
                        model::CachedSzuCheckin {
                            state: model::SzuCheckinState::Signed,
                            checked_at_ms: now,
                        },
                    );
                    changed = true;
                    log::info!("已自动完成 SZU 签到（{}）", session.course_name);
                }
                Err(message) if message.contains("已经签到") => {
                    // 服务端已签：按成功处理，避免继续重试。
                    self.auto_runtime.lock().unwrap().succeed(&runtime_key);
                    self.szu_checkins.lock().unwrap().insert(
                        state_key,
                        model::CachedSzuCheckin {
                            state: model::SzuCheckinState::Signed,
                            checked_at_ms: now,
                        },
                    );
                    changed = true;
                }
                Err(message) => {
                    log::warn!("自动 SZU 签到失败（{}）: {message}", session.course_name);
                    self.auto_runtime
                        .lock()
                        .unwrap()
                        .fail(&runtime_key, format!("自动签到失败：{message}"));
                }
            }
        }
        changed
    }

    async fn auto_submit_szu_session(&self, session: &model::TimetableSession) -> Result<(), String> {
        let resolved = self
            .resolve_radar_point(
                &szu_course_key(&session.course_name),
                &session.course_name,
            )
            .await?;
        let Some((latitude, longitude, label, radius)) =
            automation::szu_auto_point(&resolved, session)
        else {
            return Err("无法解析该课次的楼栋位置".into());
        };
        let source = if resolved.source == "last_success" {
            "geolocation"
        } else {
            "building"
        };
        // SZU 服务端按楼栋围栏强校验；抖动上限优先用本课次匹配到的楼栋半径。
        let cap_m = matching::match_building(
            buildings::builtin(),
            session.building_name.as_deref(),
            session.building_name_en.as_deref(),
            session.classroom.as_deref(),
        )
        .ok()
        .map(|(building, _)| building.radius_m as f64)
        .unwrap_or(radius as f64);
        self.submit_szu_at(
            &session.course_name,
            (latitude, longitude),
            cap_m,
            label,
            source,
            None,
            session.classroom.as_deref(),
        )
        .await
    }

    /// 课程是否已有可复用的签到照片（提前保存或上次提交留下的）。
    async fn course_has_photo(&self, course_name: &str) -> Result<bool, String> {
        let database = self.database()?;
        let key = szu_course_key(course_name);
        let memory =
            szu_core::migrations::db::run_sync(async move { database.load_memory(&key).await })?;
        Ok(memory.has_photo())
    }

    /// 课表窗口内的 SZU 状态刷新。频率完全由 `scheduler::run` 的
    /// 30s/5min 动态节拍决定，课表模块不另行轮询。返回状态是否变化。
    async fn refresh_szu_checkins(&self, now: i64) -> bool {
        // 总开关关闭时不做任何 SZU 考勤探测（不刷 QW 接口），并丢弃旧状态：
        // 首页与课表只保留畅课签到状态。
        if !self.automation.lock().unwrap().szu_enabled {
            self.szu_checkins.lock().unwrap().clear();
            return false;
        }
        let sessions: Vec<model::TimetableSession> = self
            .timetable
            .lock()
            .unwrap()
            .sessions_on(&local_date(now))
            .into_iter()
            .cloned()
            .collect();
        let mut targets = Vec::new();
        for session in sessions {
            if now < session.start_ms - scheduler::WINDOW_HALF_MS
                || now > session.end_ms + scheduler::WINDOW_HALF_MS
            {
                continue;
            }
            let key = automation::szu_session_key(&session);
            let fresh = self
                .szu_checkins
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|cached| now - cached.checked_at_ms < 60_000);
            if !fresh {
                targets.push((key, session));
            }
        }
        if targets.is_empty() {
            return false;
        }

        // 不在探测里 bootstrap：会话未就绪时由 timetable 的慢刷新或用户手动刷新负责，
        // 避免每个 30s 窗口重复触发认证。
        if *self.qw_session.lock().unwrap() != QwSessionState::Ready {
            return false;
        }
        let Ok(client) = self.build_qw_client() else {
            return false;
        };
        let Some(student_id) = self.qw_student_id.lock().unwrap().clone() else {
            return false;
        };

        let mut changed = false;
        for (key, session) in targets {
            let point = matching::match_building(
                buildings::builtin(),
                session.building_name.as_deref(),
                session.building_name_en.as_deref(),
                session.classroom.as_deref(),
            )
            .ok()
            .map(|(building, _)| (building.latitude_gcj02, building.longitude_gcj02));
            let state = match point {
                Some((latitude, longitude)) => {
                    match qw::check_signin_status(
                        &client,
                        latitude,
                        longitude,
                        &student_id,
                        None,
                        None,
                    )
                    .await
                    {
                        Ok(status)
                            if classroom_matches(
                                session.classroom.as_deref(),
                                status.classroom.as_deref(),
                            ) =>
                        {
                            if status.already_signed {
                                SzuCheckinState::Signed
                            } else if status.valid {
                                SzuCheckinState::Pending
                            } else {
                                SzuCheckinState::Unknown
                            }
                        }
                        _ => SzuCheckinState::Unknown,
                    }
                }
                None => SzuCheckinState::Unknown,
            };
            let previous = {
                let mut map = self.szu_checkins.lock().unwrap();
                let previous = map.get(&key).map(|cached| cached.state).unwrap_or_default();
                map.insert(
                    key,
                    model::CachedSzuCheckin {
                        state,
                        checked_at_ms: now,
                    },
                );
                previous
            };
            if previous != state {
                changed = true;
            }
        }
        changed
    }

    /// szu-todos 的 refresh 钩子。自动刷新（首页订阅 / 后台轮询）只在
    /// 课表探测窗口内进行——窗外不动网络，避免全时刷新；用户主动刷新
    /// （`force`，下拉刷新）不受限，始终拉一次。
    pub async fn probe_if_due(&self, force: bool) {
        if !force {
            let now = chrono::Utc::now().timestamp_millis();
            if !scheduler::probe_window_active(&self.today_sessions(now), now) {
                return;
            }
        }
        self.probe().await;
    }

    pub fn lms_service(&self) -> Result<Arc<szu_lms::lms::Service>, String> {
        let app = self.app.as_ref().ok_or("应用句柄不可用")?;
        szu_lms::lms::service(app)
    }

    /// 雷达默认点解析：上次成功定位 → 课表课程位置 → 兜底致理楼。
    pub async fn resolve_radar_point(
        &self,
        course_key: &str,
        course_title: &str,
    ) -> Result<ResolvedPoint, String> {
        let database = self.database()?;
        let key = course_key.to_string();
        let memory = szu_core::migrations::db::run_sync(async move {
            database.load_memory(&key).await
        })?;
        if let Some(point) = &memory.last_point {
            return Ok(ResolvedPoint {
                latitude: point.latitude,
                longitude: point.longitude,
                label: point.label.clone(),
                source: "last_success".into(),
                fence_radius_m: None,
            });
        }
        if let Some(point) = self.timetable_point(course_title) {
            return Ok(point);
        }
        let fallback = buildings::fallback().ok_or("内置地点列表缺少兜底楼栋")?;
        Ok(ResolvedPoint {
            latitude: fallback.latitude_gcj02,
            longitude: fallback.longitude_gcj02,
            label: fallback.name.clone(),
            source: "fallback".into(),
            fence_radius_m: Some(fallback.radius_m),
        })
    }

    /// 内置楼栋 → GCJ-02 坐标点。
    fn building_point(&self, building_id: i64) -> Result<ResolvedPoint, String> {
        let building = buildings::builtin()
            .iter()
            .find(|b| b.id == building_id)
            .ok_or("所选地点不在内置列表中")?;
        Ok(ResolvedPoint {
            latitude: building.latitude_gcj02,
            longitude: building.longitude_gcj02,
            label: building.name.clone(),
            source: "timetable".into(),
            fence_radius_m: Some(building.radius_m),
        })
    }

    /// 课表 → 楼栋坐标。先取当天该课程的最近未结束课次，否则取最近一场。
    fn timetable_point(&self, course_title: &str) -> Option<ResolvedPoint> {
        let title = matching::normalize(course_title);
        let now = chrono::Utc::now().timestamp_millis();
        let cache = self.timetable.lock().unwrap().clone();
        let mut candidates: Vec<&model::TimetableSession> = cache
            .sessions
            .iter()
            .filter(|session| matching::normalize(&session.course_name) == title)
            .collect();
        candidates.sort_by_key(|session| {
            if session.end_ms >= now {
                0 // 今天的/未结束的优先
            } else {
                1
            }
        });
        let session = candidates.first()?;
        let (building, _) = matching::match_building(
            buildings::builtin(),
            session.building_name.as_deref(),
            session.building_name_en.as_deref(),
            session.classroom.as_deref(),
        )
        .ok()?;
        Some(ResolvedPoint {
            latitude: building.latitude_gcj02,
            longitude: building.longitude_gcj02,
            label: building.name.clone(),
            source: "timetable".into(),
            fence_radius_m: Some(building.radius_m),
        })
    }

    /// 畅课数字签到。成功后把数字码共享到该课程的房间（房间不在时静默跳过）：
    /// 数字码在本次点名结束前一直有效，同课程的其他设备可直接复用。
    pub async fn submit_lms_number(
        &self,
        rollcall_id: i64,
        course_id: i64,
        code: &str,
    ) -> Result<(), String> {
        let device_id = self.device_id()?;
        self.lms_service()?
            .submit_number(rollcall_id, code, &device_id)
            .await?;
        self.share_number_code(course_id, rollcall_id, code)
            .await
            .map_err(|error| log::warn!("共享数字签到码失败: {error}"))
            .ok();
        Ok(())
    }

    /// 畅课雷达签到。定位来源三选一：`custom`（浏览器 WGS-84 读数或已转换的
    /// GCJ-02 点，按 `custom_is_wgs84` 区分）、`building_id`（内置楼栋 GCJ-02），
    /// 或全空走默认点解析链（上次成功 → 课表 → 兜底）。成功后把「用户选择的
    /// 点」写入课程记忆。返回实际提交用的点（未加抖动）。
    pub async fn submit_lms_radar(
        &self,
        rollcall_id: i64,
        course_id: i64,
        course_title: &str,
        custom: Option<(f64, f64, bool)>,
        building_id: Option<i64>,
    ) -> Result<ResolvedPoint, String> {
        let resolved = match (building_id, custom) {
            (Some(id), _) => self.building_point(id)?,
            (None, Some((lat, lng, is_wgs84))) => {
                let (lat, lng) = if is_wgs84 {
                    geo::wgs84_to_gcj02(lat, lng)
                } else {
                    (lat, lng)
                };
                ResolvedPoint {
                    latitude: lat,
                    longitude: lng,
                    label: "自定义定位".into(),
                    source: "last_success".into(),
                    fence_radius_m: None,
                }
            }
            (None, None) => {
                self.resolve_radar_point(&course_id.to_string(), course_title)
                    .await?
            }
        };
        let (jittered_lat, jittered_lng) = {
            let mut rng = rand::rng();
            geo::jitter_point(&mut rng, resolved.latitude, resolved.longitude, LMS_JITTER_CAP_M)
        };
        let reading = szu_lms::lms::rollcall::RadarReading {
            latitude: jittered_lat,
            longitude: jittered_lng,
            ..Default::default()
        };
        let device_id = self.device_id()?;
        // 深大关闭蓝牙：纯 GPS 模式，radarSignal 省略（H5 源码确认 beacon-only）。
        self.lms_service()?
            .submit_radar(rollcall_id, &reading, &device_id, None)
            .await?;
        // 记住「用户选择的点」（未加抖动），下次默认恢复。
        self.save_last_point(
            &course_id.to_string(),
            &LastPoint {
                latitude: resolved.latitude,
                longitude: resolved.longitude,
                label: resolved.label.clone(),
                source: if resolved.source == "last_success" {
                    "geolocation".into()
                } else {
                    "building".into()
                },
            },
        )
        .await
        .map_err(|error| log::warn!("保存上次定位失败: {error}"))
        .ok();
        Ok(resolved)
    }

    /// 畅课二维码签到：`data` 是 QR JSON 里的原样字段，服务端校验其内部的
    /// create_time/hash；成功判定（200 + data.status=="on_call"）在 szu-lms 内。
    pub async fn submit_lms_qr(
        &self,
        rollcall_id: i64,
        data: &str,
        device_id: &str,
    ) -> Result<(), String> {
        self.lms_service()?.submit_qr(rollcall_id, data, device_id).await
    }

    /// SZU 考勤签到（雷达点 + 照片）。定位来源同 [`Service::submit_lms_radar`]，
    /// 但偏移上限收敛到楼栋围栏半径；`photo_b64` 缺省用该课程上次照片。
    pub async fn submit_szu(
        &self,
        course_name: &str,
        building_id: Option<i64>,
        custom: Option<(f64, f64, bool)>,
        photo_b64: Option<&str>,
    ) -> Result<(), String> {
        // 定位点：自定义（WGS-84 读数或 GCJ-02 点）或内置楼栋（GCJ-02）。
        // 保存记忆用的「用户选择的点」与提交用的「抖动点」分开。
        let (selected, cap_m, label, source) = match (building_id, custom) {
            (Some(id), _) => {
                let point = self.building_point(id)?;
                let fence = point.fence_radius_m.unwrap_or(75) as f64;
                ((point.latitude, point.longitude), fence, point.label, "building")
            }
            (None, Some((lat, lng, is_wgs84))) => {
                let (lat, lng) = if is_wgs84 {
                    geo::wgs84_to_gcj02(lat, lng)
                } else {
                    (lat, lng)
                };
                ((lat, lng), LMS_JITTER_CAP_M, "自定义定位".to_string(), "geolocation")
            }
            (None, None) => return Err("请选择签到地点".into()),
        };
        self.submit_szu_at(
            course_name,
            selected,
            cap_m,
            label,
            source,
            photo_b64,
            None,
        )
        .await
    }

    /// SZU 签到的共同实现：`expected_classroom` 非空时校验服务端按该坐标匹配到
    /// 的课次教室，避免自动签到拿兜底/旧定位签错课次。
    async fn submit_szu_at(
        &self,
        course_name: &str,
        selected: (f64, f64),
        cap_m: f64,
        label: String,
        source: &str,
        photo_b64: Option<&str>,
        expected_classroom: Option<&str>,
    ) -> Result<(), String> {
        let (submitted_lat, submitted_lng) = {
            let mut rng = rand::rng();
            geo::jitter_point(&mut rng, selected.0, selected.1, cap_m)
        };
        // 2. 照片：新照片归一化后入记忆；缺省用上次照片。
        let (jpeg, mime) = match photo_b64 {
            Some(raw) => {
                let bytes = photo::base64_decode(raw)?;
                let (jpeg, mime) = photo::normalize_photo(&bytes)?;
                self.save_last_photo(
                    &szu_course_key(course_name),
                    &photo::base64_encode(&jpeg),
                    mime,
                )
                .await
                .map_err(|error| log::warn!("保存上次照片失败: {error}"))
                .ok();
                (jpeg, mime)
            }
            None => {
                let database = self.database()?;
                let key = szu_course_key(course_name);
                let memory =
                    szu_core::migrations::db::run_sync(async move { database.load_memory(&key).await })?;
                if !memory.has_photo() {
                    return Err("请先拍照或选择照片".into());
                }
                let bytes = photo::base64_decode(&memory.last_photo_b64)?;
                photo::normalize_photo(&bytes)?
            }
        };
        // 3. 会话 + 可签课次核对 + 上传 + 提交（H5 时序：check-signin-status →
        //    photo → signin）。会话中途失效时 force 重bootstrap 重试一次。
        let mut attempt = 0;
        loop {
            attempt += 1;
            self.ensure_qw(false).await;
            if *self.qw_session.lock().unwrap() != QwSessionState::Ready {
                return Err("请先完成应用统一认证，再进行 SZU 签到".into());
            }
            let client = self.build_qw_client()?;
            let student_id = self
                .qw_student_id
                .lock()
                .unwrap()
                .clone()
                .ok_or("考勤账号信息缺失，请刷新后重试")?;
            let user_name = self.qw_user_name.lock().unwrap().clone().unwrap_or_default();
            let status = match qw::check_signin_status(
                &client,
                submitted_lat,
                submitted_lng,
                &student_id,
                None,
                None,
            )
            .await
            {
                Ok(status) => status,
                Err(error) if error.session_expired && attempt == 1 => {
                    self.set_qw_state(QwSessionState::NeedsAuth, &error.message);
                    continue;
                }
                Err(error) => return Err(error.message),
            };
            if status.already_signed {
                return Err("该课次已经签到，无需重复提交".into());
            }
            if !status.valid {
                return Err(status
                    .message
                    .unwrap_or_else(|| "当前不满足签到条件（不在签到时间或范围内）".into()));
            }
            if let Some(classroom) = expected_classroom {
                if !classroom_matches(Some(classroom), status.classroom.as_deref()) {
                    return Err("当前定位与课次教室不一致，已取消自动签到，请手动签到".into());
                }
            }
            // 4. 上传照片（multipart 字段 photo）拿 photo_path，再随 JSON 提交。
            let uploaded = match client
                .upload_photo(
                    "attendance/api/v1/attendance/photo",
                    jpeg.clone(),
                    mime,
                    "attendance.jpg",
                )
                .await
            {
                Ok(uploaded) => uploaded,
                Err(error) if error.session_expired && attempt == 1 => {
                    self.set_qw_state(QwSessionState::NeedsAuth, &error.message);
                    continue;
                }
                Err(error) => return Err(error.message),
            };
            let photo_path = uploaded
                .get("data")
                .and_then(|data| data.get("photo_path"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let payload = qw::SigninPayload {
                student_id,
                name: user_name,
                course_name: course_name.to_string(),
                student_course_id: status.student_course_id,
                session_id: status.session_id,
                classroom: status.classroom.unwrap_or_default(),
                latitude: submitted_lat,
                longitude: submitted_lng,
                location_address: label.clone(),
                timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                language: "zh-CN".into(),
                photo: photo_path,
            };
            match client
                .post_json("attendance/signin", qw::signin_body(&payload))
                .await
            {
                Ok(_) => break,
                Err(error) if error.session_expired && attempt == 1 => {
                    self.set_qw_state(QwSessionState::NeedsAuth, &error.message);
                    continue;
                }
                Err(error) => return Err(error.message),
            }
        }
        // 4. 记住这次成功的定位点（用户选择的点，未加抖动）。
        self.save_last_point(
            &szu_course_key(course_name),
            &LastPoint {
                latitude: selected.0,
                longitude: selected.1,
                label,
                source: source.to_string(),
            },
        )
        .await
        .map_err(|error| log::warn!("保存上次定位失败: {error}"))
        .ok();
        Ok(())
    }

    /// 某课程上次签到的照片缩略图。
    pub async fn last_photo(&self, course_name: &str) -> Result<model::LastPhotoView, String> {
        let database = self.database()?;
        let key = szu_course_key(course_name);
        let memory =
            szu_core::migrations::db::run_sync(async move { database.load_memory(&key).await })?;
        Ok(model::LastPhotoView {
            has_photo: memory.has_photo(),
            thumbnail: if memory.has_photo() {
                photo::base64_decode(&memory.last_photo_b64)
                    .ok()
                    .and_then(|bytes| photo::photo_thumbnail_data_uri(&bytes).ok())
            } else {
                None
            },
        })
    }

    /// 提前保存某课程的签到照片：不要求提交成功，供后续手动/自动签到复用。
    pub async fn save_szu_photo(
        &self,
        course_name: &str,
        photo_b64: &str,
    ) -> Result<model::LastPhotoView, String> {
        let bytes = photo::base64_decode(photo_b64)?;
        let (jpeg, mime) = photo::normalize_photo(&bytes)?;
        self.save_last_photo(
            &szu_course_key(course_name),
            &photo::base64_encode(&jpeg),
            mime,
        )
        .await?;
        Ok(model::LastPhotoView {
            has_photo: true,
            thumbnail: photo::photo_thumbnail_data_uri(&jpeg).ok(),
        })
    }

    async fn save_last_point(&self, course_key: &str, point: &LastPoint) -> Result<(), String> {
        let database = self.database()?;
        let key = course_key.to_string();
        let point = point.clone();
        szu_core::migrations::db::run_sync(async move {
            database.save_last_point(&key, &point).await
        })
    }

    async fn save_last_photo(
        &self,
        course_key: &str,
        photo_b64: &str,
        mime: &str,
    ) -> Result<(), String> {
        let database = self.database()?;
        let key = course_key.to_string();
        let photo_b64 = photo_b64.to_string();
        let mime = mime.to_string();
        szu_core::migrations::db::run_sync(async move {
            database.save_last_photo(&key, &photo_b64, &mime).await
        })
    }

    /// 加入（或切换到）某课程的签到码共享房间（用户显式操作）。
    /// 同课程幂等：已活跃时标记为手动房间即可。
    pub async fn share_room_join(
        self: &Arc<Self>,
        course_id: i64,
        rollcall_id: Option<i64>,
    ) -> Result<(), String> {
        self.share_room_join_inner(course_id, rollcall_id, true).await
    }

    async fn share_room_join_inner(
        self: &Arc<Self>,
        course_id: i64,
        rollcall_id: Option<i64>,
        manual: bool,
    ) -> Result<(), String> {
        {
            let guard = self.room.lock().unwrap();
            if guard.as_ref().map(|session| session.course_id) == Some(course_id) {
                drop(guard);
                if manual {
                    if let Some(session) = self.room.lock().unwrap().as_mut() {
                        session.manual = true;
                    }
                    // 手动加入视为重新愿意共享，解除暂停。
                    self.room_suspended.lock().unwrap().remove(&course_id);
                }
                return Ok(());
            }
        }
        self.teardown_room(false).await;
        let mut session = share::RoomSession::join(self.clone(), course_id, rollcall_id).await?;
        session.manual = manual;
        *self.room.lock().unwrap() = Some(session);
        Ok(())
    }

    /// 关闭当前房间。`suspend` 为 true 表示用户主动停止：在时间窗结束前
    /// 不再自动为该课程建房。
    async fn teardown_room(&self, suspend: bool) {
        let existing = self.room.lock().unwrap().take();
        if let Some(session) = existing {
            let course_id = session.course_id;
            session.leave().await;
            if suspend {
                self.room_suspended.lock().unwrap().insert(course_id);
            }
            let _ = self.share_events.broadcast(&share::ShareEvent::RoomClosed { course_id });
        }
    }

    pub async fn share_room_leave(&self) {
        self.teardown_room(true).await;
    }

    /// 课程时间窗内的共享目标（纯函数见 [`window_targets`]）。
    fn room_targets(&self, now_ms: i64) -> Vec<(i64, i64)> {
        let date = local_date(now_ms);
        let cache = self.timetable.lock().unwrap().clone();
        let rollcalls = self
            .lms_service()
            .ok()
            .and_then(|lms| lms.rollcall_cache.get().map(|(items, _)| items))
            .unwrap_or_default();
        window_targets(&cache.sessions_on(&date), &rollcalls, now_ms)
    }

    /// 共享房间的自动启停（探测循环每轮调用）：
    /// - 手动房间不动，由用户自行关闭；
    /// - 自动房间在窗口结束或点名消失后销毁；
    /// - 无房间且窗口内有待签点名时自动建房（用户停止过的课程除外，
    ///   失败后 60s 冷却）。
    pub async fn update_rooms(self: &Arc<Self>, now_ms: i64) {
        let targets = self.room_targets(now_ms);
        let in_window: HashSet<i64> = targets.iter().map(|(course, _)| *course).collect();
        // 窗口已结束的课程解除暂停。
        self.room_suspended
            .lock()
            .unwrap()
            .retain(|course| in_window.contains(course));

        let current = self
            .room
            .lock()
            .unwrap()
            .as_ref()
            .map(|session| (session.course_id, session.manual));
        match current {
            Some((course_id, true)) => {
                let _ = course_id; // 手动房间交还给用户
            }
            Some((course_id, false)) => {
                if !in_window.contains(&course_id) {
                    self.teardown_room(false).await;
                }
            }
            None => {
                let suspended = self.room_suspended.lock().unwrap().clone();
                let Some(target) = targets
                    .iter()
                    .find(|(course, _)| !suspended.contains(course))
                    .copied()
                else {
                    return;
                };
                {
                    let mut last = self.room_last_attempt.lock().unwrap();
                    if last.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(60)) {
                        return;
                    }
                    *last = Some(std::time::Instant::now());
                }
                if let Err(error) = self.share_room_join_inner(target.0, Some(target.1), false).await {
                    log::warn!("自动加入签到码房间失败（课程 {}）: {error}", target.0);
                } else {
                    // 成功后清掉冷却，让下一次状态变化能立即响应。
                    *self.room_last_attempt.lock().unwrap() = None;
                }
            }
        }
    }

    /// 共享一份签到码到当前房间（不自动提交）。返回是否为新码。
    ///
    /// 只有房间课程与 `course_id` 一致时才广播：一门课的房间不应收到别的课程的码。
    async fn share_code(&self, course_id: i64, code: &share::SharedCode) -> Result<bool, String> {
        // 先取出发送侧句柄并释放 std Mutex：跨 await 持有 Guard 会让 future 不是 Send。
        let id = code.message_id();
        let sender = {
            let guard = self.room.lock().unwrap();
            match guard.as_ref() {
                Some(session) if session.course_id == course_id => Some(session.share_handle()),
                _ => None,
            }
        };
        let Some(sender) = sender else {
            return Ok(false);
        };
        sender.share(&id, code).await
    }

    /// 共享一条数字签到码（数字签到成功后自动调用；无房间时静默跳过）。
    pub async fn share_number_code(
        &self,
        course_id: i64,
        rollcall_id: i64,
        code: &str,
    ) -> Result<bool, String> {
        if !self.share_room_active() {
            return Ok(false);
        }
        self.share_code(
            course_id,
            &share::SharedCode::Number {
                rollcall_id,
                code: code.to_string(),
            },
        )
        .await
    }

    /// 共享一份二维码 data（扫码提交成功后自动调用；无房间时静默跳过）。
    pub async fn share_qr_code(&self, fields: &share::QrFields) -> Result<bool, String> {
        let course_id = match fields.course_id {
            Some(course_id) => course_id,
            None => match self.room.lock().unwrap().as_ref() {
                Some(session) => session.course_id,
                None => return Ok(false),
            },
        };
        self.share_code(course_id, &share::SharedCode::from_qr_fields(fields))
            .await
    }

    pub fn share_room_active(&self) -> bool {
        self.room.lock().unwrap().is_some()
    }
}

/// 状态接口返回的教室与课表教室是否指同一处。
/// 两侧都为空或任一侧缺失时不否决；都非空时要求相等或互相包含。
fn classroom_matches(session: Option<&str>, status: Option<&str>) -> bool {
    match (session, status) {
        (Some(session), Some(status)) => {
            let session = matching::normalize(session);
            let status = matching::normalize(status);
            session == status || session.contains(&status) || status.contains(&session)
        }
        _ => true,
    }
}

/// SZU 侧的课程键（按课程名隔离记忆）。
pub fn szu_course_key(course_name: &str) -> String {
    format!("szu:{}", matching::normalize(course_name))
}

/// 共享房间的自动目标：课表课次（开始/结束各放宽 [`ROOM_WINDOW_MARGIN_MS`]）
/// ∩ 待签点名（课程名归一化匹配出 LMS course_id）。纯函数便于单测窗口规则。
pub fn window_targets(
    sessions: &[&model::TimetableSession],
    rollcalls: &[szu_lms::lms::rollcall::RollcallItem],
    now_ms: i64,
) -> Vec<(i64, i64)> {
    let mut targets = Vec::new();
    for session in sessions {
        if now_ms < session.start_ms - ROOM_WINDOW_MARGIN_MS
            || now_ms > session.end_ms + ROOM_WINDOW_MARGIN_MS
        {
            continue;
        }
        let title = matching::normalize(&session.course_name);
        for item in rollcalls {
            if !item.need_answer() || matching::normalize(&item.course_title) != title {
                continue;
            }
            targets.push((item.course_id, item.rollcall_id));
            break;
        }
    }
    targets
}

fn local_date(now_ms: i64) -> String {
    use chrono::TimeZone as _;
    chrono::Local
        .timestamp_millis_opt(now_ms)
        .single()
        .map(|time| time.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

pub fn initialize(app: &tauri::AppHandle) -> Result<(), String> {
    let store = app.state::<SigninStore>();
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "应用数据目录不可用".to_string())?
        .join("signin/state.sqlite");
    let service = Service::new(Some(app.clone()), store.credentials.clone(), path)?;
    // 自动签到开关是设备本地偏好；读取失败按关闭处理，不影响签到主流程。
    *service.automation.lock().unwrap() = SigninAutomationSettings::load();
    *service.self_weak.lock().unwrap() = Arc::downgrade(&service);
    store
        .service
        .set(service)
        .map_err(|_| "签到服务已初始化".to_string())?;
    // 探测循环由 app 启动流程在服务就绪后派发（见 app.rs 的 start_probe_loop），
    // 避免 initialize 的同步上下文里再起异步任务。
    Ok(())
}

pub fn service(app: &tauri::AppHandle) -> Result<Arc<Service>, String> {
    app.state::<SigninStore>()
        .service
        .get()
        .cloned()
        .ok_or_else(|| "签到服务尚未初始化".into())
}

/// 启动课表驱动的探测循环（app 初始化完成后调用）。
pub fn start_probe_loop(app: &tauri::AppHandle) {
    let Ok(service) = service(app) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        scheduler::run(service).await;
    });
}
