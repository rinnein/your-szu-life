mod protocol;
mod session;
pub(super) mod settings;

use crate::campus_network::{model::*, network::NetworkSnapshot, staff::StaffPortalConfig};
use std::sync::{atomic::AtomicBool, Mutex};

pub(super) type CredentialsProvider<'a> = dyn Fn() -> Result<(String, String), String> + Sync + 'a;

pub(super) struct PendingConfirmation {
    token: String,
    expires_at_ms: u64,
    snapshot: NetworkSnapshot,
    config: StaffPortalConfig,
    username: String,
    mac: Option<String>,
    revision: u64,
    credentials: String,
}
#[derive(Clone)]
pub(super) struct StaffNetworkHint {
    fingerprint: String,
    ip: String,
    acid: String,
}
pub(super) struct StaffRuntime {
    pub(super) settings: Mutex<Option<StaffNetworkSettings>>,
    pub(super) status: Mutex<StaffLoginStatusView>,
    pub(super) suspended: AtomicBool,
    pub(super) configured: AtomicBool,
    pub(super) needs_apply: AtomicBool,
    confirmation: Mutex<Option<PendingConfirmation>>,
    hint: Mutex<Option<StaffNetworkHint>>,
    last_fingerprint: Mutex<Option<String>>,
}
impl Default for StaffRuntime {
    fn default() -> Self {
        Self {
            settings: Mutex::new(None),
            status: Mutex::new(StaffLoginStatusView::default()),
            suspended: AtomicBool::new(false),
            configured: AtomicBool::new(false),
            needs_apply: AtomicBool::new(true),
            confirmation: Mutex::new(None),
            hint: Mutex::new(None),
            last_fingerprint: Mutex::new(None),
        }
    }
}

use super::{
    http::{CheckedHttp, HttpError, PortalHttp},
    login::backoff_delay,
    logout::{disconnect_target, LogoutTarget},
    network::{NetworkEnvironment, NetworkObservation, SystemNetworkEnvironment},
    operations::LoginGuard,
    portal::{decode_portal_html, meta_refresh_target, parse_jsonp_callback},
    staff::{
        apply_staff_info, parse_staff_config, probe_staff_portal, staff_info_url, staff_redirect,
        staff_url, StaffPortalProbe,
    },
    state::{AppNetworkEvents, NetworkEvents},
    CampusNetworkStore,
};
use protocol::{credential_stamp, login_url, remember_value};
use reqwest::Url;
use serde_json::Value;
use session::{StaffSession, StaffSessionClient};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::Manager;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    Transport,
    Business,
    Manual,
    Cancelled,
    Ineligible,
}
struct Failure {
    kind: FailureKind,
    message: &'static str,
}
impl Failure {
    fn transport(message: &'static str) -> Self {
        Self {
            kind: FailureKind::Transport,
            message,
        }
    }
    fn manual(message: &'static str) -> Self {
        Self {
            kind: FailureKind::Manual,
            message,
        }
    }
    fn cancelled() -> Self {
        Self {
            kind: FailureKind::Cancelled,
            message: "操作已取消，或网络、账户、设置发生变化",
        }
    }
}
impl From<HttpError> for Failure {
    fn from(error: HttpError) -> Self {
        if error == HttpError::Canceled {
            Self::cancelled()
        } else {
            Self::transport("教工区门户请求失败或超时，尚未确认认证结果")
        }
    }
}
fn result(outcome: StaffApplyOutcome, message: &str) -> StaffApplyResultView {
    StaffApplyResultView {
        outcome,
        message: message.into(),
        confirmation_token: None,
    }
}
fn json_response(
    http: &dyn PortalHttp,
    url: &Url,
    callback: Option<&str>,
) -> Result<Value, Failure> {
    let response = http.get(url)?;
    if !(200..300).contains(&response.status) {
        return Err(Failure::transport("教工区门户返回 HTTP 错误"));
    }
    match callback {
        Some(callback) => parse_jsonp_callback(&String::from_utf8_lossy(&response.body), callback),
        None => serde_json::from_slice(&response.body).map_err(|_| "响应不是 JSON".into()),
    }
    .map_err(|_| Failure::transport("教工区门户响应格式无效"))
}
fn refresh_online(
    http: &dyn PortalHttp,
    base: &StaffPortalProbe,
    now: u64,
) -> Result<StaffPortalProbe, Failure> {
    let config = base
        .config
        .as_ref()
        .ok_or_else(|| Failure::transport("门户缺少终端配置"))?;
    let (url, callback) = staff_info_url(config, now);
    let value = json_response(http, &url, Some(&callback))?;
    let mut probe = base.clone();
    apply_staff_info(&mut probe, &value);
    if !matches!(
        probe.state,
        PortalOnlineState::Online | PortalOnlineState::Offline
    ) {
        return Err(Failure::transport("无法确认当前教工区会话"));
    }
    Ok(probe)
}
fn online_as(probe: &StaffPortalProbe, username: &str) -> bool {
    probe.state == PortalOnlineState::Online
        && probe
            .session
            .as_ref()
            .is_some_and(|session| session.username == username)
        && probe.domain.is_empty()
}
fn verify_online(
    http: &dyn PortalHttp,
    base: &StaffPortalProbe,
    username: &str,
    environment: &dyn NetworkEnvironment,
) -> Result<StaffPortalProbe, Failure> {
    for attempt in 0..3 {
        if attempt > 0 {
            environment.sleep(Duration::from_secs(1));
        }
        let probe = refresh_online(http, base, environment.now_ms())?;
        if online_as(&probe, username) {
            return Ok(probe);
        }
        if probe.state == PortalOnlineState::Online {
            return Err(Failure::manual(
                "在线账号与所选账号不一致，请手动确认当前会话",
            ));
        }
    }
    Err(Failure::transport("门户尚未确认账号在线，请稍后检查"))
}
fn success_page(http: &dyn PortalHttp, probe: &StaffPortalProbe) -> Result<(), Failure> {
    let page = probe
        .page_url
        .as_ref()
        .ok_or_else(|| Failure::manual("门户未提供成功页上下文"))?;
    let config = probe
        .config
        .as_ref()
        .ok_or_else(|| Failure::manual("门户未提供终端配置"))?;
    let mut url = staff_redirect(page, "./srun_portal_success")
        .ok_or_else(|| Failure::manual("成功页地址不受信任"))?;
    url.set_query(page.query());
    let mut visited = std::collections::HashSet::new();
    for _ in 0..=4 {
        if !visited.insert(url.clone()) {
            return Err(Failure::transport("成功页重定向循环"));
        }
        let response = http.get(&url)?;
        if matches!(response.status, 301 | 302 | 303 | 307 | 308) {
            url = response
                .location
                .as_deref()
                .and_then(|target| staff_redirect(&url, target))
                .ok_or_else(|| Failure::manual("成功页跳转不受信任"))?;
            continue;
        }
        if !(200..300).contains(&response.status) {
            return Err(Failure::transport("认证成功页访问失败，模式尚未确认应用"));
        }
        let html = decode_portal_html(&response.body, response.content_type.as_deref());
        if let Some(target) = meta_refresh_target(&html) {
            url = staff_redirect(&url, &target)
                .ok_or_else(|| Failure::manual("成功页跳转不受信任"))?;
            continue;
        }
        if let Some(actual) = parse_staff_config(&html) {
            if actual.ip != config.ip || actual.acid != config.acid {
                return Err(Failure::cancelled());
            }
        }
        return Ok(());
    }
    Err(Failure::transport("成功页重定向过多"))
}
fn publish(
    store: &CampusNetworkStore,
    guard: &LoginGuard,
    snapshot: &NetworkSnapshot,
    probe: &StaffPortalProbe,
    events: &dyn NetworkEvents,
) {
    let reachable = store
        .network_overview
        .lock()
        .map(|v| v.portal_reachable)
        .unwrap_or(false);
    let observation = NetworkObservation {
        snapshot: snapshot.clone(),
        staff: probe.clone(),
        dorm: super::network::DormPortalProbe {
            reachable,
            ..Default::default()
        },
    };
    store.apply_observation(guard.token(), &observation, events);
}
fn phase(
    store: &CampusNetworkStore,
    guard: &LoginGuard,
    state: DormLoginState,
    message: &str,
    events: &dyn NetworkEvents,
) {
    guard.commit(|| {
        store.change_staff_status(
            |status| {
                status.state = state;
                status.message = Some(message.into());
            },
            events,
        )
    });
}
fn mark_pending(store: &CampusNetworkStore, events: &dyn NetworkEvents) -> Result<(), Failure> {
    store.staff.suspended.store(true, Ordering::Release);
    store.staff.needs_apply.store(true, Ordering::Release);
    let mut settings =
        settings::load(store).map_err(|_| Failure::manual("无法读取教工区设置，未继续认证"))?;
    settings.applied_revision = None;
    // Keep the desired enabled flag and last applied mode; do not turn an apply into public logout.
    *store
        .staff
        .settings
        .lock()
        .map_err(|_| Failure::manual("教工区设置锁失败"))? = Some(settings.clone());
    settings::persist(store, &settings)
        .map_err(|_| Failure::manual("无法保存待应用状态，未继续认证"))?;
    events.staff_settings_changed(
        &settings::cached_view(store, &settings)
            .map_err(|_| Failure::manual("无法读取教工区凭据状态"))?,
    );
    Ok(())
}
fn confirmation(
    store: &CampusNetworkStore,
    supplied: Option<&str>,
    snapshot: &NetworkSnapshot,
    probe: &StaffPortalProbe,
    settings: &StaffNetworkSettings,
    stamp: &str,
    now: u64,
) -> Result<Option<StaffApplyResultView>, Failure> {
    let config = probe
        .config
        .as_ref()
        .ok_or_else(|| Failure::transport("门户未提供终端配置"))?;
    let session = probe.session.as_ref();
    let mut pending = store
        .staff
        .confirmation
        .lock()
        .map_err(|_| Failure::manual("教工区确认锁失败"))?;
    if let Some(token) = supplied {
        let previous = pending
            .take()
            .ok_or_else(|| Failure::manual("重新登录确认已失效，请重新点击应用"))?;
        if previous.token != token
            || now > previous.expires_at_ms
            || previous.snapshot != *snapshot
            || previous.config != *config
            || previous.revision != settings.revision
            || previous.credentials != stamp
            || session.is_none_or(|session| {
                session.username != previous.username || session.user_mac != previous.mac
            })
        {
            return Err(Failure::manual(
                "网络、在线会话或配置已变化，请重新确认后应用",
            ));
        }
        return Ok(None);
    }
    *pending = None;
    if let Some(session) = session.filter(|_| probe.state == PortalOnlineState::Online) {
        let token = uuid::Uuid::new_v4().to_string();
        *pending = Some(PendingConfirmation {
            token: token.clone(),
            expires_at_ms: now.saturating_add(120_000),
            snapshot: snapshot.clone(),
            config: config.clone(),
            username: session.username.clone(),
            mac: session.user_mac.clone(),
            revision: settings.revision,
            credentials: stamp.into(),
        });
        return Ok(Some(StaffApplyResultView { outcome: StaffApplyOutcome::RequiresConfirmation, message: "当前设备已在线。应用此模式需要先下线并解绑本机，再使用所选凭据重新登录；其他设备不受操作影响。".into(), confirmation_token: Some(token) }));
    }
    Ok(None)
}

fn run_login(
    store: &CampusNetworkStore,
    guard: &LoginGuard,
    session: &dyn StaffSession,
    environment: &dyn NetworkEnvironment,
    current: &CredentialsProvider<'_>,
    events: &dyn NetworkEvents,
    supplied: Option<&str>,
    automatic_target: Option<&StaffNetworkHint>,
) -> Result<StaffApplyResultView, Failure> {
    let automatic = automatic_target.is_some();
    let settings = settings::load(store).map_err(|_| Failure::manual("无法读取教工区设置"))?;
    let (username, password) = settings::credentials(store, &settings, current)
        .map_err(|_| Failure::manual("教工区凭据不可用，请检查账户或单独配置"))?;
    let stamp = credential_stamp(&username, &password);
    if automatic
        && (settings.mode != StaffLoginMode::AppAuto
            || !settings.enabled
            || !settings::applied(&settings, &stamp)
            || store.staff.suspended.load(Ordering::Acquire))
    {
        return Err(Failure {
            kind: FailureKind::Ineligible,
            message: "等待手动应用已保存的教工区配置",
        });
    }
    let cookie = if settings.mode == StaffLoginMode::MacAuth {
        Some(remember_value(&username, &password).map_err(Failure::manual)?)
    } else {
        None
    };
    let snapshot = environment.snapshot();
    if automatic_target.is_some_and(|target| target.fingerprint != snapshot.fingerprint()) {
        return Err(Failure::cancelled());
    }
    let context_current = || {
        environment.snapshot() == snapshot
            && settings::load(store).is_ok_and(|now| {
                now.revision == settings.revision
                    && now.mode == settings.mode
                    && (!automatic
                        || (now.enabled && !store.staff.suspended.load(Ordering::Acquire)))
            })
            && settings::credentials(store, &settings, current)
                .is_ok_and(|(card, pass)| credential_stamp(&card, &pass) == stamp)
    };
    let allowed = || guard.current() && context_current();
    let http = CheckedHttp {
        inner: session,
        allowed: &allowed,
    };
    phase(
        store,
        guard,
        DormLoginState::Probing,
        "正在确认教工区门户和当前会话",
        events,
    );
    let mut probe = probe_staff_portal(&http, environment.now_ms());
    if !allowed() {
        return Err(Failure::cancelled());
    }
    let config = probe
        .config
        .clone()
        .ok_or_else(|| Failure::transport("未取得有效教工区门户配置，未发送登录请求"))?;
    if !matches!(
        probe.state,
        PortalOnlineState::Online | PortalOnlineState::Offline
    ) {
        return Err(Failure::transport("无法确认教工区是否在线，未发送登录请求"));
    }
    if automatic_target.is_some_and(|target| target.ip != config.ip || target.acid != config.acid) {
        return Err(Failure::cancelled());
    }
    if settings.mode == StaffLoginMode::MacAuth && !config.mac_auth {
        return Err(Failure::manual(
            "当前门户未启用校园网无感认证，未自动切换登录模式",
        ));
    }
    if automatic {
        if !trusted_network(store, &snapshot, &probe) {
            return Err(Failure {
                kind: FailureKind::Ineligible,
                message: "等待已确认的教工区网络，不进行试错认证",
            });
        }
        if probe.state == PortalOnlineState::Online {
            publish(store, guard, &snapshot, &probe, events);
            let message = if online_as(&probe, &username) {
                "当前账号已在线，未重复认证"
            } else {
                "当前设备已有其他账号在线，需要更换时请手动应用"
            };
            phase(store, guard, DormLoginState::Online, message, events);
            return Ok(result(StaffApplyOutcome::AlreadyOnline, message));
        }
    } else {
        if let Some(result) = confirmation(
            store,
            supplied,
            &snapshot,
            &probe,
            &settings,
            &stamp,
            environment.now_ms(),
        )? {
            phase(
                store,
                guard,
                DormLoginState::Online,
                "等待确认重新登录，当前网络尚未改变",
                events,
            );
            return Ok(result);
        }
        guard
            .commit(|| mark_pending(store, events))
            .ok_or_else(Failure::cancelled)??;
        if probe.state == PortalOnlineState::Online {
            phase(
                store,
                guard,
                DormLoginState::LoggingIn,
                "正在下线并解绑当前设备",
                events,
            );
            let mut observation = NetworkObservation {
                snapshot: snapshot.clone(),
                staff: probe.clone(),
                ..Default::default()
            };
            let target = LogoutTarget::from_observation(&NetworkRegion::Staff, &observation)
                .ok_or_else(|| Failure::manual("当前会话缺少完整注销信息"))?;
            let report = disconnect_target(
                &target,
                &http,
                environment,
                &mut observation,
                &allowed,
                |observation| {
                    publish(store, guard, &snapshot, &observation.staff, events);
                },
            );
            if report.changed || !allowed() {
                return Err(Failure::cancelled());
            }
            if report.state != PortalOnlineState::Offline {
                return Err(Failure::transport("尚未确认旧会话下线，未提交新的登录请求"));
            }
            probe = observation.staff;
        }
    }
    let mut captcha_url = staff_url("/v2/srun_portal_captcha_image_info");
    captcha_url
        .query_pairs_mut()
        .append_pair("user_name", &username)
        .append_pair("ip", &config.ip);
    let captcha = http.get(&captcha_url)?;
    if captcha.status != 404 {
        if !(200..300).contains(&captcha.status) {
            return Err(Failure::transport("验证码检查失败，未发送登录请求"));
        }
        let value: Value = serde_json::from_slice(&captcha.body)
            .map_err(|_| Failure::transport("验证码检查响应无效"))?;
        if value.get("code").and_then(Value::as_i64) == Some(0)
            && value
                .get("data")
                .is_some_and(|v| v.as_str() == Some("1") || v.as_i64() == Some(1))
        {
            return Err(Failure {
                kind: FailureKind::Business,
                message: "门户要求人工验证码，已暂停自动认证，请在门户手动处理后重试",
            });
        }
    }
    phase(
        store,
        guard,
        DormLoginState::LoggingIn,
        "正在获取认证挑战并登录",
        events,
    );
    let callback = format!("szuLifeChallenge_{}", environment.now_ms());
    let mut url = staff_url("/cgi-bin/get_challenge");
    url.query_pairs_mut()
        .append_pair("callback", &callback)
        .append_pair("username", &username)
        .append_pair("ip", &config.ip)
        .append_pair("_", &environment.now_ms().to_string());
    let challenge = json_response(&http, &url, Some(&callback))?;
    if challenge.get("error").and_then(Value::as_str) != Some("ok") {
        return Err(Failure::transport("认证挑战获取失败"));
    }
    let token = challenge
        .get("challenge")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && v.len() <= 256 && v.is_ascii())
        .ok_or_else(|| Failure::transport("认证挑战无效"))?;
    let (url, callback) = login_url(
        &username,
        &password,
        &config,
        &probe.nas_ip,
        token,
        environment.now_ms(),
    );
    let response = json_response(&http, &url, Some(&callback))?;
    if response.get("error").and_then(Value::as_str) != Some("ok") {
        return Err(Failure {
            kind: FailureKind::Business,
            message: "教工区账号认证失败，请检查凭据或登录门户",
        });
    }
    let online = verify_online(&http, &probe, &username, environment)?;
    guard
        .commit(|| {
            if !context_current() {
                return Err(Failure::cancelled());
            }
            session.remember(cookie.as_deref()).map_err(Failure::from)
        })
        .ok_or_else(Failure::cancelled)??;
    phase(
        store,
        guard,
        DormLoginState::LoggingIn,
        "正在完成所选记住密码流程",
        events,
    );
    success_page(&http, &online)?;
    let online = verify_online(&http, &online, &username, environment)?;
    if !allowed() {
        return Err(Failure::cancelled());
    }
    publish(store, guard, &snapshot, &online, events);
    guard
        .commit(|| {
            if !context_current() {
                return Err(Failure::cancelled());
            }
            let mut saved =
                settings::load(store).map_err(|_| Failure::manual("无法读取模式应用结果"))?;
            saved.applied_revision = Some(saved.revision);
            saved.applied_mode = Some(saved.mode);
            saved.applied_credentials = Some(stamp.clone());
            saved.pending_failure_notice = None;
            settings::persist(store, &saved)
                .map_err(|_| Failure::manual("已认证，但保存模式结果失败，请重新应用"))?;
            store.staff.suspended.store(false, Ordering::Release);
            store.staff.needs_apply.store(false, Ordering::Release);
            *store
                .staff
                .hint
                .lock()
                .map_err(|_| Failure::manual("网络识别状态锁失败"))? = Some(StaffNetworkHint {
                fingerprint: snapshot.fingerprint(),
                ip: config.ip.clone(),
                acid: config.acid.clone(),
            });
            store.change_staff_status(
                |status| {
                    status.state = DormLoginState::Online;
                    status.consecutive_failures = 0;
                    status.next_probe_at_ms = None;
                    status.message = Some("已登录并完成所选模式的门户流程".into());
                },
                events,
            );
            events.staff_settings_changed(
                &settings::cached_view(store, &saved)
                    .map_err(|_| Failure::manual("无法读取教工区凭据状态"))?,
            );
            events.log(&DormLoginLogView {
                region: Some(NetworkRegion::Staff),
                timestamp_ms: environment.now_ms(),
                stage: "staff-login".into(),
                outcome: "success".into(),
                message: Some("教工区认证和模式流程已完成".into()),
                ..Default::default()
            });
            Ok(())
        })
        .ok_or_else(Failure::cancelled)??;
    Ok(result(
        StaffApplyOutcome::Applied,
        if settings.mode == StaffLoginMode::MacAuth {
            "已登录并完成门户的记住密码流程，后续由校园网处理无感认证"
        } else {
            "已登录并完成不记住密码流程，将按应用自动登录开关运行"
        },
    ))
}

fn record_failure(
    store: &CampusNetworkStore,
    guard: &LoginGuard,
    failure: &Failure,
    automatic: bool,
    environment: &dyn NetworkEnvironment,
    events: &dyn NetworkEvents,
) {
    guard.commit(|| {
        if !automatic {
            let _ = mark_pending(store, events);
        }
        let enabled = settings::load(store).is_ok_and(|s| s.enabled)
            && !store.staff.suspended.load(Ordering::Acquire);
        store.change_staff_status(
            |status| {
                status.message = Some(failure.message.into());
                if !automatic || !enabled || failure.kind == FailureKind::Cancelled {
                    status.state = DormLoginState::Disabled;
                    status.next_probe_at_ms = None;
                } else if failure.kind == FailureKind::Transport {
                    status.consecutive_failures = status.consecutive_failures.saturating_add(1);
                    status.state = DormLoginState::Monitoring;
                    status.next_probe_at_ms = Some(
                        environment.now_ms()
                            + backoff_delay(status.consecutive_failures).as_millis() as u64,
                    );
                } else if failure.kind == FailureKind::Ineligible {
                    status.state = DormLoginState::Monitoring;
                    status.next_probe_at_ms = None;
                } else {
                    status.state = DormLoginState::Paused;
                    status.next_probe_at_ms = None;
                }
            },
            events,
        );
        if matches!(failure.kind, FailureKind::Business | FailureKind::Manual) {
            if let Ok(mut settings) = settings::load(store) {
                settings.pending_failure_notice = Some(DormFailureNotice {
                    message: failure.message.into(),
                    timestamp_ms: environment.now_ms(),
                });
                let _ = settings::persist(store, &settings);
            }
            events.staff_failure(failure.message);
        }
        if failure.kind != FailureKind::Ineligible {
            events.log(&DormLoginLogView {
                region: Some(NetworkRegion::Staff),
                timestamp_ms: environment.now_ms(),
                stage: "staff-login".into(),
                outcome: if failure.kind == FailureKind::Transport {
                    "unknown"
                } else {
                    "failed"
                }
                .into(),
                message: Some(failure.message.into()),
                ..Default::default()
            });
        }
    });
}
fn perform_with_guard(
    store: &CampusNetworkStore,
    guard: &LoginGuard,
    session: &dyn StaffSession,
    environment: &dyn NetworkEnvironment,
    current: &CredentialsProvider<'_>,
    events: &dyn NetworkEvents,
    confirmation: Option<&str>,
    automatic_target: Option<&StaffNetworkHint>,
) -> StaffApplyResultView {
    let automatic = automatic_target.is_some();
    match run_login(
        store,
        guard,
        session,
        environment,
        current,
        events,
        confirmation,
        automatic_target,
    ) {
        Ok(view) => view,
        Err(failure) => {
            record_failure(store, guard, &failure, automatic, environment, events);
            result(
                if matches!(
                    failure.kind,
                    FailureKind::Transport | FailureKind::Cancelled
                ) {
                    StaffApplyOutcome::Unknown
                } else {
                    StaffApplyOutcome::Failed
                },
                failure.message,
            )
        }
    }
}
fn perform_apply(
    store: &CampusNetworkStore,
    session: &dyn StaffSession,
    environment: &dyn NetworkEnvironment,
    current: &CredentialsProvider<'_>,
    events: &dyn NetworkEvents,
    confirmation: Option<&str>,
) -> Result<StaffApplyResultView, String> {
    let guard = store.begin_login()?;
    Ok(perform_with_guard(
        store,
        &guard,
        session,
        environment,
        current,
        events,
        confirmation,
        None,
    ))
}

pub(super) fn trusted_network(
    store: &CampusNetworkStore,
    snapshot: &NetworkSnapshot,
    probe: &StaffPortalProbe,
) -> bool {
    if probe.state == PortalOnlineState::Online || snapshot.ssid.as_deref() == Some("SZU_WLAN") {
        return true;
    }
    if store
        .network_overview
        .lock()
        .is_ok_and(|view| view.region == NetworkRegion::Staff)
    {
        return true;
    }
    store.staff.hint.lock().is_ok_and(|hint| {
        hint.as_ref().is_some_and(|hint| {
            snapshot.fingerprint() == hint.fingerprint
                && probe
                    .config
                    .as_ref()
                    .is_some_and(|config| config.ip == hint.ip && config.acid == hint.acid)
        })
    })
}
pub(super) fn auto_due(
    store: &CampusNetworkStore,
    observation: &NetworkObservation,
    now: u64,
    current: &CredentialsProvider<'_>,
    events: &dyn NetworkEvents,
) -> bool {
    if !trusted_network(store, &observation.snapshot, &observation.staff) {
        return false;
    }
    let Ok(settings) = settings::load(store) else {
        return false;
    };
    if settings.mode != StaffLoginMode::AppAuto
        || !settings.enabled
        || store.staff.suspended.load(Ordering::Acquire)
    {
        return false;
    }
    let stamp = settings::credentials(store, &settings, current)
        .ok()
        .map(|(card, password)| credential_stamp(&card, &password));
    if stamp
        .as_deref()
        .is_none_or(|stamp| !settings::applied(&settings, stamp))
    {
        store.staff.needs_apply.store(true, Ordering::Release);
        if let Ok(view) = settings::cached_view(store, &settings) {
            events.staff_settings_changed(&view);
        }
        store.change_staff_status(
            |status| {
                status.state = DormLoginState::Disabled;
                status.next_probe_at_ms = None;
                status.message = Some("配置或账户已变化，等待手动应用".into());
            },
            events,
        );
        return false;
    }
    store.staff.needs_apply.store(false, Ordering::Release);
    let fingerprint = observation.snapshot.fingerprint();
    let changed = store
        .staff
        .last_fingerprint
        .lock()
        .map(|mut previous| {
            let changed = previous.as_deref() != Some(fingerprint.as_str());
            *previous = Some(fingerprint);
            changed
        })
        .unwrap_or(false);
    let mut due = false;
    store.change_staff_status(
        |status| {
            if changed {
                status.consecutive_failures = 0;
                status.next_probe_at_ms = Some(now);
                status.state = DormLoginState::Monitoring;
            }
            if observation.staff.state == PortalOnlineState::Online {
                status.state = DormLoginState::Online;
                status.next_probe_at_ms = None;
                status.consecutive_failures = 0;
                status.message = Some("当前设备在线，未重复认证或替换账号".into());
            } else if observation.staff.state == PortalOnlineState::Offline {
                if status.state == DormLoginState::Online {
                    status.state = DormLoginState::Monitoring;
                    status.next_probe_at_ms = Some(now);
                }
                due = status.state != DormLoginState::Paused
                    && status.next_probe_at_ms.is_none_or(|next| next <= now);
            }
        },
        events,
    );
    due
}

#[cfg(desktop)]
pub(crate) fn keep_running_in_background(store: &CampusNetworkStore) -> bool {
    settings::load(store).is_ok_and(|s| {
        s.mode == StaffLoginMode::AppAuto
            && s.enabled
            && s.applied_revision == Some(s.revision)
            && s.applied_mode == Some(s.mode)
            && s.applied_credentials.is_some()
            && !store.staff.suspended.load(Ordering::Acquire)
    })
}

pub(super) fn start_auto(
    app: tauri::AppHandle,
    observation: &NetworkObservation,
) -> Result<(), String> {
    let config = observation
        .staff
        .config
        .as_ref()
        .ok_or("教工区网络尚未确认")?;
    let target = StaffNetworkHint {
        fingerprint: observation.snapshot.fingerprint(),
        ip: config.ip.clone(),
        acid: config.acid.clone(),
    };
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let guard = store.begin_login()?;
    std::thread::spawn(move || {
        let Some(store) = app.try_state::<CampusNetworkStore>() else {
            return;
        };
        let events = AppNetworkEvents(&app);
        let current = || crate::auth::current_network_credentials(&app);
        match StaffSessionClient::new() {
            Ok(session) => {
                perform_with_guard(
                    &store,
                    &guard,
                    &session,
                    &SystemNetworkEnvironment,
                    &current,
                    &events,
                    None,
                    Some(&target),
                );
            }
            Err(_) => record_failure(
                &store,
                &guard,
                &Failure::transport("无法建立教工区认证会话"),
                true,
                &SystemNetworkEnvironment,
                &events,
            ),
        }
    });
    Ok(())
}

pub(crate) fn get_settings(app: tauri::AppHandle) -> Result<StaffNetworkSettingsView, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let current = || crate::auth::current_network_credentials(&app);
    let view = settings::view(&store, &current)?;
    super::monitor::start_dorm_monitor(app.clone());
    Ok(view)
}
pub(crate) fn save_settings(
    app: tauri::AppHandle,
    input: StaffNetworkSettingsInput,
) -> Result<StaffNetworkSettingsView, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let current = || crate::auth::current_network_credentials(&app);
    let view = settings::save(&store, input, &current, &AppNetworkEvents(&app))?;
    super::monitor::start_dorm_monitor(app.clone());
    Ok(view)
}
pub(crate) fn apply(
    app: tauri::AppHandle,
    confirmation_token: Option<String>,
) -> Result<StaffApplyResultView, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let current = || crate::auth::current_network_credentials(&app);
    let session = StaffSessionClient::new().map_err(|_| "无法建立教工区认证会话")?;
    perform_apply(
        &store,
        &session,
        &SystemNetworkEnvironment,
        &current,
        &AppNetworkEvents(&app),
        confirmation_token.as_deref(),
    )
}
pub(crate) fn consume_failure(app: tauri::AppHandle) -> Result<Option<DormFailureNotice>, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    store.operations.settings(|| {
        let mut settings = settings::load(&store)?;
        let notice = settings.pending_failure_notice.take();
        if notice.is_some() {
            settings::persist(&store, &settings)?;
        }
        Ok(notice)
    })
}

#[cfg(test)]
mod tests;
