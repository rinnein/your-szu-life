//! 设备同步的本地脏信号。
//!
//! 业务模块在“正常保存/登录”成功后只调用 [`mark_dirty`]，不直接依赖同步引擎；
//! `settings_sync` 的后台任务负责防抖、合并，并在真正有变化时发起一次同步。
//! 信号只记录同步项 id，不保存任何配置值。

use std::collections::BTreeSet;
use std::sync::{Mutex, OnceLock};

use tokio::sync::Notify;

#[derive(Clone, Debug, Default)]
pub struct Request {
    /// 是否有需要完整交换的请求（设备上线、启用同步等）。
    pub full: bool,
    /// 本地修改过的同步项 id。
    pub items: BTreeSet<String>,
}

fn state() -> &'static Mutex<Request> {
    static REQUEST: OnceLock<Mutex<Request>> = OnceLock::new();
    REQUEST.get_or_init(|| Mutex::new(Request::default()))
}

fn notify() -> &'static Notify {
    static NOTIFY: OnceLock<Notify> = OnceLock::new();
    NOTIFY.get_or_init(Notify::new)
}

/// 记录某个同步项被本地修改。
pub fn mark_dirty(item: &str) {
    {
        let mut request = state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        request.items.insert(item.to_owned());
    }
    notify().notify_one();
}

/// 请求一次完整同步（设备上线、启用同步等）。
pub fn request_full() {
    {
        let mut request = state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        request.full = true;
    }
    notify().notify_one();
}

/// 取走并清空当前请求。同步引擎在防抖窗口结束后调用。
pub fn take() -> Request {
    let mut request = state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::mem::take(&mut *request)
}

/// 等待下一次本地修改或完整同步请求。
pub async fn wait_for_change() {
    notify().notified().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn requests_coalesce_and_full_wins() {
        let _ = take();
        mark_dirty("a");
        mark_dirty("a");
        mark_dirty("b");
        let request = take();
        assert!(!request.full);
        assert_eq!(
            request.items.into_iter().collect::<Vec<_>>(),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert!(take().items.is_empty());

        request_full();
        mark_dirty("c");
        let request = take();
        assert!(request.full);
        assert!(request.items.contains("c"));
        assert!(take().items.is_empty());
    }

    #[tokio::test]
    async fn wait_is_woken_by_mark_dirty() {
        let _ = take();
        let waiter = tokio::spawn(async { wait_for_change().await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        mark_dirty("x");
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .expect("wait_for_change should be woken")
            .expect("waiter should not panic");
        let _ = take();
    }
}
