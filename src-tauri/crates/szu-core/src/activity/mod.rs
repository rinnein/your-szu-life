use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    time::Duration,
};

pub mod commands;

/// 后端轮询使用的三档活跃度。
///
/// 第一档是「对应功能模块正在使用」，第二档是应用在前台但没打开该模块，
/// 第三档是应用在后台。除第一档外，外部数据一般不会变化很快。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActivityTier {
    Background,
    Foreground,
    Active,
}

struct ActivityMonitor {
    foreground: AtomicBool,
    /// 模块 → 正在使用它的 WebView label 集合。
    modules: Mutex<HashMap<String, HashSet<String>>>,
}

static MONITOR: OnceLock<ActivityMonitor> = OnceLock::new();

fn monitor() -> &'static ActivityMonitor {
    MONITOR.get_or_init(|| ActivityMonitor {
        // 启动时还没收到可见性上报，先按前台处理，避免首轮轮询过慢。
        foreground: AtomicBool::new(true),
        modules: Mutex::new(HashMap::new()),
    })
}

pub fn set_foreground(foreground: bool) {
    monitor().foreground.store(foreground, Ordering::Relaxed);
}

pub fn set_module_active(module: &str, owner: &str, active: bool) {
    let mut modules = monitor().modules.lock().unwrap();
    if active {
        modules
            .entry(module.to_owned())
            .or_default()
            .insert(owner.to_owned());
    } else if let Some(owners) = modules.get_mut(module) {
        owners.remove(owner);
        if owners.is_empty() {
            modules.remove(module);
        }
    }
}

pub fn tier(module: Option<&str>) -> ActivityTier {
    if module.is_some_and(|module| {
        monitor()
            .modules
            .lock()
            .unwrap()
            .get(module)
            .is_some_and(|owners| !owners.is_empty())
    }) {
        ActivityTier::Active
    } else if monitor().foreground.load(Ordering::Relaxed) {
        ActivityTier::Foreground
    } else {
        ActivityTier::Background
    }
}

/// 按当前档位选择轮询间隔。
pub fn interval(
    module: Option<&str>,
    active: Duration,
    foreground: Duration,
    background: Duration,
) -> Duration {
    match tier(module) {
        ActivityTier::Active => active,
        ActivityTier::Foreground => foreground,
        ActivityTier::Background => background,
    }
}

/// 分块睡眠长度：让档位变化最多在一个小块后生效，而不是等完整个后台间隔。
pub fn sleep_chunk(interval: Duration, max_chunk: Duration) -> Duration {
    interval.min(max_chunk).max(Duration::from_millis(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_follow_module_and_foreground_state() {
        set_foreground(true);
        set_module_active("activity-test", "test-owner", false);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Foreground);

        set_module_active("activity-test", "test-owner", true);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Active);

        // 另一个 WebView 也打开同一模块时，先关闭其中一个不应清掉 Active。
        set_module_active("activity-test", "second-owner", true);
        set_module_active("activity-test", "test-owner", false);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Active);
        set_module_active("activity-test", "second-owner", false);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Foreground);

        set_foreground(false);
        set_module_active("activity-test", "test-owner", true);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Active);

        set_module_active("activity-test", "test-owner", false);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Background);

        set_foreground(true);
        assert_eq!(tier(Some("activity-test")), ActivityTier::Foreground);
    }

    #[test]
    fn sleep_chunk_never_exceeds_the_requested_block() {
        assert_eq!(
            sleep_chunk(Duration::from_secs(900), Duration::from_secs(30)),
            Duration::from_secs(30)
        );
        assert_eq!(
            sleep_chunk(Duration::from_secs(10), Duration::from_secs(30)),
            Duration::from_secs(10)
        );
    }
}
