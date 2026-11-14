//! 课表驱动的计划探测。
//!
//! 探测节奏（按需求约定）：
//! - 课程开始 / 中点 / 结束三个时刻的 ±3min 窗口内：每 30s 一探；
//! - 课中（start+3min ~ end-3min）：每 5min 一探；
//! - 无课时段：睡到下一个窗口，探测节拍随 activity 档位变化提前/放宽。
//!
//! [`next_probe`] 是纯函数，窗口计算可单测；[`run`] 是常驻循环，
//! 模式照抄 szu-oj 的 monitor（分块睡眠 + 档位变快提前 deadline）。

use std::sync::Arc;
use std::time::Duration;

use szu_core::activity::{self, ActivityTier};

use super::Service;

/// 窗口半宽：课程关键时刻 ±3 分钟。
pub const WINDOW_HALF_MS: i64 = 3 * 60 * 1000;
/// 窗口内探测间隔。
const WINDOW_CADENCE_MS: i64 = 30 * 1000;
/// 课中探测间隔。
const IN_COURSE_CADENCE_MS: i64 = 5 * 60 * 1000;

/// 一次探测计划：何时醒、醒来后的节拍。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProbePlan {
    /// 下次探测时间（epoch 毫秒）。
    pub at_ms: i64,
    /// 醒来后到再下次探测的间隔。
    pub cadence_ms: i64,
}

/// 依据课次（start/end 毫秒）与当前时间给出下一次探测计划。
/// 多节课取最早的计划；无课返回 None。
pub fn next_probe(sessions: &[(i64, i64)], now_ms: i64) -> Option<ProbePlan> {
    sessions
        .iter()
        .filter_map(|&(start_ms, end_ms)| session_next_probe(start_ms, end_ms, now_ms))
        .min_by_key(|plan| plan.at_ms)
}

/// 当前是否处于探测窗口内（三个 ±3min 窗口或课中）。
/// 首页 todos 的自动探测用它收敛节拍：窗外完全不动网络。
pub fn probe_window_active(sessions: &[(i64, i64)], now_ms: i64) -> bool {
    next_probe(sessions, now_ms).is_some_and(|plan| plan.at_ms <= now_ms)
}

fn session_next_probe(start_ms: i64, end_ms: i64, now_ms: i64) -> Option<ProbePlan> {
    let mid_ms = start_ms + (end_ms - start_ms) / 2;
    let mut points = [start_ms, mid_ms, end_ms];
    points.sort_unstable();
    // 1. 处于某个关键时刻（开始/中点/结束）的 ±3min 窗口内：立即探测，30s 节拍。
    if points.iter().any(|&point| {
        now_ms >= point - WINDOW_HALF_MS && now_ms <= point + WINDOW_HALF_MS
    }) {
        return Some(ProbePlan {
            at_ms: now_ms,
            cadence_ms: WINDOW_CADENCE_MS,
        });
    }
    // 2. 课中（已过开始窗口、未到结束窗口）：5min 节拍。
    if now_ms > points[0] + WINDOW_HALF_MS && now_ms < points[2] - WINDOW_HALF_MS {
        return Some(ProbePlan {
            at_ms: now_ms,
            cadence_ms: IN_COURSE_CADENCE_MS,
        });
    }
    // 3. 早于第一个窗口：睡到窗口开启。
    if now_ms < points[0] - WINDOW_HALF_MS {
        return Some(ProbePlan {
            at_ms: points[0] - WINDOW_HALF_MS,
            cadence_ms: WINDOW_CADENCE_MS,
        });
    }
    // 4. 全部窗口已过。
    None
}

/// 探测循环：每轮先算下次计划，分块睡到点再探测。
/// 档位变快（Active/Foreground）时把最长睡眠块压短，让探测尽快跟上。
pub async fn run(service: Arc<Service>) {
    let mut last_tier = activity::tier(Some("signin"));
    let mut deadline: Option<ProbePlan> = None;
    loop {
        let tier = activity::tier(Some("signin"));
        if tier != last_tier {
            last_tier = tier;
            // 档位变快时立即重新计划并探测一次。
            if tier != ActivityTier::Background {
                deadline = None;
            }
        }
        let now = chrono::Utc::now().timestamp_millis();
        let sessions = service.today_sessions(now);
        let plan = deadline
            .filter(|plan| plan.at_ms > now)
            .or_else(|| next_probe(&sessions, now));
        match plan {
            Some(plan) if plan.at_ms <= now => {
                service.probe().await;
                // 房间生命周期跟随探测节拍：窗口内自动建房、窗外自动回收。
                service.update_rooms(now).await;
                deadline = Some(ProbePlan {
                    at_ms: now + plan.cadence_ms,
                    cadence_ms: plan.cadence_ms,
                });
            }
            Some(plan) => {
                let remaining = (plan.at_ms - now).max(100) as u64;
                let chunk = if tier == ActivityTier::Background {
                    60_000
                } else {
                    15_000
                };
                tokio::time::sleep(Duration::from_millis(remaining.min(chunk))).await;
            }
            None => {
                // 无探测窗口：房间可能仍在 ±10min 房间窗内，先维护一次
                // （无网络，纯本地状态转移），再低频兜底轮询。
                service.update_rooms(now).await;
                let idle = activity::interval(
                    Some("signin"),
                    Duration::from_secs(120),
                    Duration::from_secs(600),
                    Duration::from_secs(1800),
                );
                let chunk = if tier == ActivityTier::Background { 60_000 } else { 30_000 };
                tokio::time::sleep(activity::sleep_chunk(idle, Duration::from_millis(chunk))).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60 * 1000;

    fn plan_at(sessions: &[(i64, i64)], now: i64) -> Option<ProbePlan> {
        next_probe(sessions, now)
    }

    #[test]
    fn probes_three_windows_per_session() {
        let start = 9 * 60 * MIN; // 09:00
        let end = 11 * 60 * MIN; // 11:00
        let sessions = [(start, end)];
        // 开课前 10 分钟：计划在 start-3min。
        let plan = plan_at(&sessions, start - 10 * MIN).unwrap();
        assert_eq!(plan.at_ms, start - WINDOW_HALF_MS);
        assert_eq!(plan.cadence_ms, WINDOW_CADENCE_MS);
        // 开课窗口内：立即探测，节拍 30s。
        let plan = plan_at(&sessions, start).unwrap();
        assert_eq!(plan.at_ms, start);
        assert_eq!(plan.cadence_ms, WINDOW_CADENCE_MS);
        // 中点窗口内：立即探测。
        let mid = start + (end - start) / 2;
        let plan = plan_at(&sessions, mid).unwrap();
        assert_eq!(plan.at_ms, mid);
        // 结束窗口内：立即探测。
        let plan = plan_at(&sessions, end + MIN).unwrap();
        assert_eq!(plan.at_ms, end + MIN);
    }

    #[test]
    fn in_course_probes_every_five_minutes() {
        let start = 9 * 60 * MIN;
        let end = 11 * 60 * MIN;
        let sessions = [(start, end)];
        // 开课 10 分钟后（已过 start 窗口、未到中点窗口）：课中节拍。
        let plan = plan_at(&sessions, start + 10 * MIN).unwrap();
        assert_eq!(plan.at_ms, start + 10 * MIN);
        assert_eq!(plan.cadence_ms, IN_COURSE_CADENCE_MS);
    }

    #[test]
    fn no_plan_outside_class() {
        let start = 9 * 60 * MIN;
        let end = 11 * 60 * MIN;
        let sessions = [(start, end)];
        assert!(plan_at(&sessions, end + 5 * MIN).is_none());
        assert!(plan_at(&[(start, end)], start - 10 * MIN).is_some());
        // 中点窗口过后、未到结束窗口：课中节拍接管（中点±3min 已过）。
        let mid = start + (end - start) / 2;
        let plan = plan_at(&sessions, mid + 10 * MIN).unwrap();
        assert_eq!(plan.cadence_ms, IN_COURSE_CADENCE_MS);
    }

    #[test]
    fn earliest_plan_wins_across_sessions() {
        let first = (8 * 60 * MIN, 9 * 60 * MIN);
        let second = (14 * 60 * MIN, 16 * 60 * MIN);
        let plan = plan_at(&[first, second], 6 * 60 * MIN).unwrap();
        assert_eq!(plan.at_ms, first.0 - WINDOW_HALF_MS);
    }

    #[test]
    fn back_to_back_classes_keep_probing() {
        // 连堂课（上一节结束窗口与下一节开始窗口重叠）不漏探。
        let a = (9 * 60 * MIN, 10 * 60 * MIN);
        let b = (10 * 60 * MIN, 11 * 60 * MIN);
        let plan = plan_at(&[a, b], 10 * 60 * MIN - 2 * MIN).unwrap();
        assert_eq!(plan.at_ms, 10 * 60 * MIN - 2 * MIN);
        assert_eq!(plan.cadence_ms, WINDOW_CADENCE_MS);
    }
}
