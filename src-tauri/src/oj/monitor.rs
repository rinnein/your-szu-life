use super::*;
use std::time::Duration;
pub(super) fn changes(
    known: &mut Known,
    contests: &[Contest],
    now: i64,
) -> Vec<(Contest, String, String)> {
    let mut notices = vec![];
    for c in contests {
        let phase = c.phase(now);
        let previous = known.contests.get(&c.id);
        let event = if known.initialized && previous.is_none() && phase < 2 {
            Some(("new".to_owned(), "新增测验"))
        } else if previous.is_some_and(|p| p.phase(p.now) == 0) && phase == 1 {
            Some((format!("start:{}", c.begin), "测验已开始"))
        } else {
            None
        };
        let due = phase == 1 && c.end - now <= 3_600_000;
        for (event, label) in event
            .into_iter()
            .chain(due.then(|| (format!("due:{}", c.end), "测验将在 1 小时内截止")))
        {
            let key = format!("{}:{event}", c.id);
            if !known.events.contains_key(&key) {
                known.events.insert(key, now);
                notices.push((c.clone(), event, label.into()));
            }
        }
    }
    known.contests = contests
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.now = now;
            (c.id.clone(), c)
        })
        .collect();
    known.initialized = true;
    notices
}
pub(super) fn start(service: Arc<Service>) {
    tauri::async_runtime::spawn(async move {
        loop {
            service.scan(false).await;
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    });
}
impl Service {
    pub(super) async fn scan(&self, force: bool) {
        let entries: Vec<_> = {
            let r = self.runtime.lock().unwrap();
            r.data
                .sites
                .iter()
                .filter(|s| s.reminders)
                .filter_map(|s| r.entries.get(&s.id).map(|e| (s.clone(), e.clone())))
                .collect()
        };
        for (site, e) in entries {
            let Ok(_guard) = e.gate.try_lock() else {
                continue;
            };
            let now = chrono::Utc::now().timestamp_millis();
            let (next, offset, status) = {
                let s = e.state.lock().unwrap();
                (s.next_sync, s.offset, s.session)
            };
            if matches!(status, Session::CaptchaRequired | Session::LoginFailed) {
                continue;
            }
            if !force && now < next {
                self.tick(&site, &e, now + offset);
                continue;
            }
            let result = async {
                self.ensure(&site.id, &e).await?;
                let rows = e.client.list("groups/", &[]).await.map_err(|error| {
                    if error.status == Some(401) {
                        self.update_status(&site.id, &e, Session::Disabled, "登录已过期");
                    }
                    error.message
                })?;
                let mut contests = BTreeMap::new();
                for g in rows {
                    let gid = protocol::id(&g["id"]).map_err(|e| e.message)?;
                    let list = e
                        .client
                        .list("contests/", &[("group", gid)])
                        .await
                        .map_err(|e| e.message)?;
                    for v in list {
                        let c = protocol::contest(&v).map_err(|e| e.message)?;
                        contests.insert(c.id.clone(), c);
                    }
                }
                Ok::<_, String>(contests.into_values().collect::<Vec<_>>())
            }
            .await;
            if !self.current(&site.id, &e) {
                continue;
            }
            match result {
                Ok(contests) => {
                    let offset = contests.first().map(|c| c.now - now).unwrap_or(offset);
                    {
                        let mut s = e.state.lock().unwrap();
                        s.last_sync = Some(now);
                        s.next_sync = now + 300_000;
                        s.failures = 0;
                        s.offset = offset;
                    }
                    self.process_contests(&site, &e, &contests, now + offset);
                    self.schedule_deadlines(&site, &e, &contests, offset);
                    let _ = self.save_cookies(&site.id, &e);
                }
                Err(message) => {
                    let mut s = e.state.lock().unwrap();
                    s.failures = s.failures.saturating_add(1);
                    s.next_sync = now + (30_000_i64 * (1_i64 << s.failures.min(5))).min(900_000);
                    s.message = message;
                    let manual =
                        matches!(s.session, Session::CaptchaRequired | Session::LoginFailed);
                    drop(s);
                    if manual {
                        self.login_notice(&site, &e);
                    }
                }
            }
        }
        self.publish();
    }
    fn login_notice(&self, site: &Site, e: &Entry) {
        let id = format!("{}:{}:login", site.id, e.epoch);
        let mut r = self.runtime.lock().unwrap();
        if r.data.notices.iter().any(|n| n.id == id) {
            return;
        }
        let notice = Notice {
            id,
            site_id: site.id.clone(),
            account: e.username.clone(),
            contest_id: String::new(),
            title: site.name.clone(),
            message: "OJ 登录需要处理，请打开填写验证码或检查账号".into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            read: false,
        };
        r.data.notices.push(notice.clone());
        if self.persist(&r).is_err() {
            return;
        }
        drop(r);
        self.notify(&notice);
    }
    fn tick(&self, site: &Site, e: &Entry, now: i64) {
        let contests = {
            let r = self.runtime.lock().unwrap();
            r.data
                .known
                .get(&scope(&site.id, &e.username))
                .map(|k| k.contests.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        };
        if !contests.is_empty() {
            self.process_contests(site, e, &contests, now)
        }
    }
    fn process_contests(&self, site: &Site, e: &Entry, contests: &[Contest], now: i64) {
        let mut r = self.runtime.lock().unwrap();
        if r.entries
            .get(&site.id)
            .is_none_or(|current| current.epoch != e.epoch)
        {
            return;
        }
        let mut next = r.data.clone();
        let known = next.known.entry(scope(&site.id, &e.username)).or_default();
        let events = changes(known, contests, now);
        let notices: Vec<_> = events
            .into_iter()
            .map(|(c, event, message)| Notice {
                id: format!("{}:{}:{}:{}", site.id, e.epoch, c.id, event),
                site_id: site.id.clone(),
                account: e.username.clone(),
                contest_id: c.id,
                title: c.title,
                message,
                timestamp: now,
                read: false,
            })
            .collect();
        #[cfg(mobile)]
        let already_scheduled: std::collections::BTreeSet<String> = notices
            .iter()
            .filter(|notice| {
                notice.id.contains(":due:")
                    && contests
                        .iter()
                        .find(|c| c.id == notice.contest_id)
                        .is_some_and(|c| {
                            next.scheduled.contains_key(&format!(
                                "{}:{}:{}:{}",
                                site.id, e.username, c.id, c.end
                            ))
                        })
            })
            .map(|n| n.id.clone())
            .collect();
        next.notices.extend(notices.clone());
        if next.notices.len() > 500 {
            next.notices.drain(..next.notices.len() - 500);
        }
        if persistence::write(&self.path, &next).is_err() {
            return;
        }
        r.data = next;
        drop(r);
        for notice in notices {
            #[cfg(mobile)]
            if already_scheduled.contains(&notice.id) {
                continue;
            }
            self.notify(&notice);
        }
    }
    pub(super) fn notify(&self, n: &Notice) {
        let Some(app) = self.app.as_ref() else { return };
        use tauri_plugin_notification::NotificationExt;
        let _ = app
            .notification()
            .builder()
            .title(&n.title)
            .body(&n.message)
            .extra("ojNotice", n)
            .show();
    }
    pub(super) fn cancel_notifications(&self, id: &str) {
        let mut r = self.runtime.lock().unwrap();
        let prefix = format!("{id}:");
        let ids: Vec<_> = r
            .data
            .scheduled
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(_, v)| *v)
            .collect();
        #[cfg(mobile)]
        if let Some(app) = &self.app {
            use tauri_plugin_notification::NotificationExt;
            let _ = app.notification().cancel(ids.clone());
            let _ = app.notification().remove_active(ids.clone());
        }
        let _ = ids;
        r.data.scheduled.retain(|k, _| !k.starts_with(&prefix));
        let _ = self.persist(&r);
    }
    fn schedule_deadlines(&self, site: &Site, e: &Entry, contests: &[Contest], offset: i64) {
        #[cfg(mobile)]
        {
            use sha2::{Digest, Sha256};
            use tauri_plugin_notification::{NotificationExt, Schedule};
            let Some(app) = &self.app else { return };
            let now = chrono::Utc::now().timestamp_millis();
            let mut desired = BTreeMap::new();
            for c in contests {
                let at = c.end - 3_600_000 - offset;
                if at <= now {
                    continue;
                }
                let key = format!("{}:{}:{}:{}", site.id, e.username, c.id, c.end);
                let hash = Sha256::digest(key.as_bytes());
                let id = i32::from_le_bytes([hash[0], hash[1], hash[2], hash[3]]) & 0x3fff_ffff;
                desired.insert(key.clone(), id);
                if self
                    .runtime
                    .lock()
                    .unwrap()
                    .data
                    .scheduled
                    .contains_key(&key)
                {
                    continue;
                }
                let Ok(date) =
                    time::OffsetDateTime::from_unix_timestamp_nanos(at as i128 * 1_000_000)
                else {
                    continue;
                };
                let notice = Notice {
                    id: key.clone(),
                    site_id: site.id.clone(),
                    account: e.username.clone(),
                    contest_id: c.id.clone(),
                    title: c.title.clone(),
                    message: "测验将在 1 小时后截止".into(),
                    timestamp: at,
                    read: false,
                };
                if app
                    .notification()
                    .builder()
                    .id(id)
                    .title(&c.title)
                    .body(&notice.message)
                    .extra("ojNotice", &notice)
                    .schedule(Schedule::At {
                        date,
                        repeating: false,
                        allow_while_idle: true,
                    })
                    .show()
                    .is_err()
                {
                    desired.remove(&key);
                }
            }
            let mut r = self.runtime.lock().unwrap();
            let prefix = format!("{}:", site.id);
            let stale: Vec<_> = r
                .data
                .scheduled
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix) && !desired.contains_key(*k))
                .map(|(_, v)| *v)
                .collect();
            let _ = app.notification().cancel(stale);
            r.data.scheduled.retain(|k, _| !k.starts_with(&prefix));
            r.data.scheduled.extend(desired);
            let _ = self.persist(&r);
        }
        #[cfg(not(mobile))]
        let _ = (site, e, contests, offset);
    }
}
