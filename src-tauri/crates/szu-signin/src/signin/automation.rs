//! 签到自动化：设备本地开关、自动提交尝试账本与待签视图的纯函数。
//!
//! 自动签到只在探测循环里运行（LMS 雷达点名与 SZU 考勤课次），因此这里不做
//! 网络请求；[`AutoRuntime`] 只记录每个候选的尝试次数、退避与最近失败原因，
//! [`lms_items`] / [`szu_items`] 负责把结果翻译成待签视图条目——失败原因写进
//! `subtitle`，由课表模块作为签到提醒正文发送。

use std::collections::{HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::{CachedSzuCheckin, CheckinTodoItem, SzuCheckinState, TimetableSession};
use super::{matching, scheduler};

/// 自动提交最多尝试次数（之后等下一次探测窗口/进程重启再重新计数）。
pub const MAX_AUTO_ATTEMPTS: u32 = 3;
/// 两次自动提交尝试之间的最小间隔。
pub const AUTO_RETRY_BACKOFF_MS: i64 = 2 * 60 * 1000;
/// 一次探测里 LMS 待签条目的兜底截止时间（与手动探测保持一致）。
const ROLLCALL_FALLBACK_TTL_MS: i64 = 15 * 60 * 1000;
/// SZU 状态在视图里的最大陈旧时间：会话失效后不要用旧 Pending 反复提醒。
const SZU_STATE_FRESH_MS: i64 = 2 * 60 * 1000;

/// 签到自动化设置（设备本地，不参与设备同步）。
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct SigninAutomationSettings {
    /// 畅课雷达点名自动签到。
    pub lms_auto_radar: bool,
    /// SZU（实验性）签到提醒与自动签到的总开关。
    pub szu_enabled: bool,
}

impl SigninAutomationSettings {
    /// 任一自动签到能力开启（探测节拍需要据此收紧）。
    pub fn active(&self) -> bool {
        self.lms_auto_radar || self.szu_enabled
    }

    pub fn load() -> Self {
        szu_core::preferences::load::<Self>(szu_core::preferences::SIGNIN_AUTOMATION)
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<Self, String> {
        szu_core::preferences::save(szu_core::preferences::SIGNIN_AUTOMATION, self)?;
        Ok(self.clone())
    }
}

/// 单次尝试记录。`attempts` 只在真正发起提交时增加；缺照片跳过只写
/// `last_error`，不消耗尝试次数，用户之后补上照片仍可立即自动提交。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoAttempt {
    pub attempts: u32,
    pub last_attempt_ms: i64,
    pub completed: bool,
    pub last_error: Option<String>,
}

impl AutoAttempt {
    fn can_attempt(&self, now_ms: i64) -> bool {
        !self.completed
            && self.attempts < MAX_AUTO_ATTEMPTS
            && (self.attempts == 0 || now_ms - self.last_attempt_ms >= AUTO_RETRY_BACKOFF_MS)
    }
}

/// 自动签到尝试账本。键：LMS `lms:{rollcall_id}`，SZU `szu:{course}|{date}|{start}`。
#[derive(Debug, Default)]
pub struct AutoRuntime {
    attempts: HashMap<String, AutoAttempt>,
}

impl AutoRuntime {
    pub fn lms_key(rollcall_id: i64) -> String {
        format!("lms:{rollcall_id}")
    }

    /// 自动提交账本键（与课次缓存键区分，避免与 `lms:` 前缀混淆）。
    pub fn szu_key(session: &TimetableSession) -> String {
        format!("szu:{}", szu_session_key(session))
    }

    pub fn attempt(&self, key: &str) -> Option<&AutoAttempt> {
        self.attempts.get(key)
    }

    pub fn can_attempt(&self, key: &str, now_ms: i64) -> bool {
        self.attempts
            .get(key)
            .map(|attempt| attempt.can_attempt(now_ms))
            .unwrap_or(true)
    }

    /// 登记一次真正发起的提交；返回是否允许本次尝试。
    pub fn begin(&mut self, key: &str, now_ms: i64) -> bool {
        let attempt = self.attempts.entry(key.to_string()).or_default();
        if !attempt.can_attempt(now_ms) {
            return false;
        }
        attempt.attempts += 1;
        attempt.last_attempt_ms = now_ms;
        true
    }

    pub fn succeed(&mut self, key: &str) {
        let attempt = self.attempts.entry(key.to_string()).or_default();
        attempt.completed = true;
        attempt.last_error = None;
    }

    pub fn fail(&mut self, key: &str, message: impl Into<String>) {
        let attempt = self.attempts.entry(key.to_string()).or_default();
        attempt.last_error = Some(message.into());
    }

    /// 缺照片等“没有发起提交”的跳过原因：只用于视图提示，不消耗尝试次数。
    pub fn skip(&mut self, key: &str, message: impl Into<String>) {
        let attempt = self.attempts.entry(key.to_string()).or_default();
        attempt.last_error = Some(message.into());
    }

    pub fn is_completed(&self, key: &str) -> bool {
        self.attempts
            .get(key)
            .is_some_and(|attempt| attempt.completed)
    }

    pub fn last_error(&self, key: &str) -> Option<&str> {
        self.attempts
            .get(key)
            .and_then(|attempt| attempt.last_error.as_deref())
    }

    /// 丢弃不再出现在当前候选集中的记录。只在探测可信（LMS 列表成功返回）时
    /// 调用；探测失败时调用会误删仍在退避中的失败记录。
    pub fn retain(&mut self, keep: &HashSet<String>) {
        self.attempts.retain(|key, _| keep.contains(key));
    }
}

/// 课次在 `Service::szu_checkins` 与课表模块里的稳定键。
/// 必须与 `szu_timetable::timetable::model::session_id` 逐字一致。
pub fn szu_session_key(session: &TimetableSession) -> String {
    format!(
        "{}|{}|{}",
        session.course_name, session.date, session.start_ms
    )
}

/// 畅课待签视图条目：已完成签到的排除，自动失败原因写进 subtitle。
/// 与 `Service::probe` 的历史行为一致，`due_at_ms` 以当前时间为基准。
pub fn lms_items(
    items: &[szu_lms::lms::rollcall::RollcallItem],
    runtime: &AutoRuntime,
    now_ms: i64,
) -> Vec<CheckinTodoItem> {
    items
        .iter()
        .filter(|item| item.need_answer())
        .map(|item| CheckinTodoItem {
            id: format!("checkin:lms:{}", item.rollcall_id),
            title: format!("{} 签到", item.course_title),
            subtitle: runtime
                .last_error(&AutoRuntime::lms_key(item.rollcall_id))
                .filter(|message| !message.trim().is_empty())
                .map(str::to_string),
            due_at_ms: now_ms + ROLLCALL_FALLBACK_TTL_MS,
            route: "/study/signin?tab=lms".into(),
        })
        .collect()
}

/// SZU 待签视图条目：只取课次窗口内、`Pending` 且总开关开启时的课次。
/// 已自动签到成功的课次状态会先被改成 `Signed`，不会出现在这里。
pub fn szu_items(
    sessions: &[&TimetableSession],
    states: &HashMap<String, CachedSzuCheckin>,
    runtime: &AutoRuntime,
    now_ms: i64,
) -> Vec<CheckinTodoItem> {
    let mut items = Vec::new();
    for session in sessions {
        if now_ms < session.start_ms - scheduler::WINDOW_HALF_MS
            || now_ms > session.end_ms + scheduler::WINDOW_HALF_MS
        {
            continue;
        }
        let state_key = szu_session_key(session);
        let runtime_key = AutoRuntime::szu_key(session);
        let pending = states.get(&state_key).is_some_and(|cached| {
            cached.state == SzuCheckinState::Pending
                && now_ms - cached.checked_at_ms <= SZU_STATE_FRESH_MS
        });
        if !pending || runtime.is_completed(&runtime_key) {
            continue;
        }
        items.push(CheckinTodoItem {
            id: format!("checkin:szu:{state_key}"),
            title: format!("{} 签到", session.course_name),
            subtitle: runtime
                .last_error(&runtime_key)
                .filter(|message| !message.trim().is_empty())
                .map(str::to_string),
            // 课次结束时间稳定，避免每轮探测刷新 due_at_ms 造成视图抖动。
            due_at_ms: session.end_ms,
            route: "/study/signin?tab=szu".into(),
        });
    }
    items
}

/// 自动 SZU 提交的点位回退：标准链落到兜底楼栋时，改用本课次匹配到的楼栋，
/// 避免拿致理楼兜底点去签另一栋楼的课。
pub fn szu_auto_point(
    resolved: &super::model::ResolvedPoint,
    session: &TimetableSession,
) -> Option<(f64, f64, String, i64)> {
    if resolved.source != "fallback" {
        return Some((
            resolved.latitude,
            resolved.longitude,
            resolved.label.clone(),
            resolved.fence_radius_m.unwrap_or(75),
        ));
    }
    let (building, _) = matching::match_building(
        super::buildings::builtin(),
        session.building_name.as_deref(),
        session.building_name_en.as_deref(),
        session.classroom.as_deref(),
    )
    .ok()?;
    Some((
        building.latitude_gcj02,
        building.longitude_gcj02,
        building.name.clone(),
        building.radius_m,
    ))
}

/// 自动签到时写入的缺照片说明（同时作为签到提醒正文）。
pub const NO_PHOTO_MESSAGE: &str =
    "没有为该课程保存签到照片，自动签到已跳过，请手动签到或先在课次中保存照片。";

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> TimetableSession {
        TimetableSession {
            date: "2026-11-17".into(),
            course_name: "概率论与数理统计".into(),
            building_name: Some("致理楼".into()),
            building_name_en: None,
            classroom: Some("致理楼L1-203".into()),
            start_ms: 1_000_000,
            end_ms: 1_100_000,
            semester: None,
        }
    }

    fn rollcall(id: i64, radar: bool, status: &str) -> szu_lms::lms::rollcall::RollcallItem {
        szu_lms::lms::rollcall::RollcallItem {
            rollcall_id: id,
            is_number: false,
            is_radar: radar,
            course_id: 42,
            course_title: "概率论与数理统计".into(),
            student_rollcall_id: id,
            kind: None,
            status: Some(status.into()),
        }
    }

    #[test]
    fn settings_default_to_disabled() {
        let settings = SigninAutomationSettings::default();
        assert!(!settings.lms_auto_radar);
        assert!(!settings.szu_enabled);
        assert!(!settings.active());
    }

    #[test]
    fn session_key_matches_timetable_module() {
        // 必须与 szu_timetable::timetable::model::session_id 一致，
        // 否则课表模块查不到签到状态。
        assert_eq!(
            szu_session_key(&session()),
            "概率论与数理统计|2026-11-17|1000000"
        );
    }

    #[test]
    fn attempt_backoff_and_limit() {
        let mut runtime = AutoRuntime::default();
        let key = AutoRuntime::lms_key(1);
        assert!(runtime.can_attempt(&key, 0));
        assert!(runtime.begin(&key, 0));
        runtime.fail(&key, "网络错误");
        assert!(!runtime.can_attempt(&key, AUTO_RETRY_BACKOFF_MS - 1));
        assert!(runtime.can_attempt(&key, AUTO_RETRY_BACKOFF_MS));
        assert!(runtime.begin(&key, AUTO_RETRY_BACKOFF_MS));
        assert!(runtime.begin(&key, 2 * AUTO_RETRY_BACKOFF_MS));
        assert!(!runtime.can_attempt(&key, 3 * AUTO_RETRY_BACKOFF_MS));
        assert!(!runtime.begin(&key, 3 * AUTO_RETRY_BACKOFF_MS));
        assert_eq!(runtime.attempt(&key).unwrap().attempts, MAX_AUTO_ATTEMPTS);
        runtime.succeed(&key);
        assert!(!runtime.begin(&key, 10 * AUTO_RETRY_BACKOFF_MS));
        assert!(runtime.last_error(&key).is_none());
    }

    #[test]
    fn skip_does_not_consume_attempts() {
        let mut runtime = AutoRuntime::default();
        let key = AutoRuntime::szu_key(&session());
        runtime.skip(&key, NO_PHOTO_MESSAGE);
        assert!(runtime.can_attempt(&key, 0));
        let attempt = runtime.attempt(&key).unwrap();
        assert_eq!(attempt.attempts, 0);
        assert_eq!(attempt.last_error.as_deref(), Some(NO_PHOTO_MESSAGE));
    }

    #[test]
    fn lms_view_contains_only_pending_items_and_failure_reason() {
        let mut runtime = AutoRuntime::default();
        runtime.fail(&AutoRuntime::lms_key(2), "定位失败");
        let items = vec![
            rollcall(1, true, "on_call"),
            rollcall(2, true, "on_call"),
            rollcall(3, true, "on_call_fine"),
        ];
        let view = lms_items(&items, &runtime, 5_000);
        assert_eq!(view.len(), 2);
        assert_eq!(view[0].id, "checkin:lms:1");
        assert!(view[0].subtitle.is_none());
        assert_eq!(view[1].id, "checkin:lms:2");
        assert_eq!(view[1].subtitle.as_deref(), Some("定位失败"));
        assert_eq!(view[1].due_at_ms, 5_000 + ROLLCALL_FALLBACK_TTL_MS);
    }

    #[test]
    fn szu_view_only_inside_window_and_pending() {
        let item = session();
        let state_key = szu_session_key(&item);
        let mut states = HashMap::new();
        states.insert(
            state_key.clone(),
            CachedSzuCheckin {
                state: SzuCheckinState::Pending,
                checked_at_ms: 1_000_000,
            },
        );
        let sessions = vec![&item];
        let runtime = AutoRuntime::default();
        let inside = szu_items(&sessions, &states, &runtime, item.start_ms);
        assert_eq!(inside.len(), 1);
        assert_eq!(inside[0].id, format!("checkin:szu:{state_key}"));
        assert_eq!(inside[0].due_at_ms, item.end_ms);
        assert!(szu_items(&sessions, &states, &runtime, item.end_ms + scheduler::WINDOW_HALF_MS + 1)
            .is_empty());
        states.insert(
            state_key,
            CachedSzuCheckin {
                state: SzuCheckinState::Signed,
                checked_at_ms: 1_000_000,
            },
        );
        assert!(szu_items(&sessions, &states, &runtime, item.start_ms).is_empty());
    }

    #[test]
    fn szu_view_carries_skip_reason_and_completed_is_hidden() {
        let item = session();
        let runtime_key = AutoRuntime::szu_key(&item);
        let mut states = HashMap::new();
        states.insert(
            szu_session_key(&item),
            CachedSzuCheckin {
                state: SzuCheckinState::Pending,
                checked_at_ms: 1_000_000,
            },
        );
        let mut runtime = AutoRuntime::default();
        runtime.skip(&runtime_key, NO_PHOTO_MESSAGE);
        let sessions = vec![&item];
        let view = szu_items(&sessions, &states, &runtime, item.start_ms);
        assert_eq!(view[0].subtitle.as_deref(), Some(NO_PHOTO_MESSAGE));
        runtime.succeed(&runtime_key);
        assert!(szu_items(&sessions, &states, &runtime, item.start_ms).is_empty());
    }

    #[test]
    fn fallback_point_uses_session_building() {
        let resolved = super::super::model::ResolvedPoint {
            latitude: 0.0,
            longitude: 0.0,
            label: "致理楼".into(),
            source: "fallback".into(),
            fence_radius_m: Some(130),
        };
        let (lat, lng, label, radius) = szu_auto_point(&resolved, &session()).unwrap();
        assert!(lat > 20.0 && lng > 110.0, "应替换成真实楼栋坐标");
        assert_eq!(label, "致理楼");
        assert_eq!(radius, 130);
        // 非兜底点原样保留。
        let custom = super::super::model::ResolvedPoint {
            latitude: 22.5,
            longitude: 113.9,
            label: "上次定位".into(),
            source: "last_success".into(),
            fence_radius_m: None,
        };
        let (lat, lng, _, radius) = szu_auto_point(&custom, &session()).unwrap();
        assert_eq!((lat, lng), (22.5, 113.9));
        assert_eq!(radius, 75);
    }
}