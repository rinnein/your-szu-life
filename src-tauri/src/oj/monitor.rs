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
        if let Some((event, label)) = event {
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
    use crate::activity::ActivityTier;
    tauri::async_runtime::spawn(async move {
        let mut last_tier = crate::activity::tier(Some("oj"));
        // 启动时先扫一次，后续按当前档位节奏执行。
        service.scan(false).await;
        let mut last_scan = std::time::Instant::now();
        loop {
            let interval = crate::activity::interval(
                Some("oj"),
                Duration::from_secs(15),
                Duration::from_secs(60),
                Duration::from_secs(300),
            );
            let tier = crate::activity::tier(Some("oj"));
            if tier != last_tier {
                last_tier = tier;
                if tier != ActivityTier::Background {
                    // 打开 OJ 页或回到前台时，把原本排到 1 小时后的同步提前。
                    let sync_ms = crate::activity::interval(
                        Some("oj"),
                        Duration::from_secs(300),
                        Duration::from_secs(900),
                        Duration::from_secs(3600),
                    )
                    .as_millis() as i64;
                    service.clamp_next_sync(sync_ms);
                }
            }
            if last_scan.elapsed() >= interval {
                service.scan(false).await;
                last_scan = std::time::Instant::now();
            }
            // 分块睡眠让「打开 OJ 页」或「回到前台」最多一个块后就能提速。
            tokio::time::sleep(crate::activity::sleep_chunk(
                interval,
                Duration::from_secs(15),
            ))
            .await;
        }
    });
}
impl Service {
    pub(crate) async fn scan(&self, force: bool) {
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
                let mut groups = Vec::new();
                let mut group_contests: Vec<(String, Vec<Contest>)> = Vec::new();
                let mut contests = BTreeMap::new();
                for g in rows {
                    let group = protocol::group(&g).map_err(|e| e.message)?;
                    let gid = group.id.clone();
                    let list = e
                        .client
                        .list("contests/", &[("group", gid.clone())])
                        .await
                        .map_err(|e| e.message)?;
                    let mut rows = Vec::new();
                    for v in list {
                        let c = protocol::contest(&v).map_err(|e| e.message)?;
                        contests.insert(c.id.clone(), c.clone());
                        rows.push(c);
                    }
                    group_contests.push((gid, rows));
                    groups.push(group);
                }
                // 后台扫描同时刷新列表缓存，首页月历无需打开 OJ 页面也能拿到最新数据。
                self.store_lists(&site.id, &groups, &group_contests);
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
                        // 打开 OJ 页时 5 分钟一拉；前台 15 分钟；后台 1 小时。
                        s.next_sync = now
                            + crate::activity::interval(
                                Some("oj"),
                                Duration::from_secs(300),
                                Duration::from_secs(900),
                                Duration::from_secs(3600),
                            )
                            .as_millis() as i64;
                        s.failures = 0;
                        s.offset = offset;
                    }
                    self.process_contests(&site, &e, &contests, now + offset);
                    let _ = self.save_cookies(&site.id, &e);
                    self.emit_cache_changed("all");
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
            self.notify(&notice);
        }
    }
    pub(super) fn notify(&self, n: &Notice) {
        let Some(app) = self.app.as_ref() else { return };
        let route = format!("/study/oj?site={}&contest={}", n.site_id, n.contest_id);
        let payload = crate::notification::NotificationPayload::new(
            crate::notification::NotificationCategory::Oj,
            &n.title,
            &n.message,
        )
        .route(route)
        .action_label("查看")
        .extra(serde_json::json!({ "ojNotice": n }));
        crate::notification::notify(app, payload);
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

    /// 1 小时截止提醒已移入 todos 模块；清理旧版本留下的原生排程，避免升级后重复通知。
    pub(super) fn cancel_legacy_scheduled(&self) {
        let mut r = self.runtime.lock().unwrap();
        if r.data.scheduled.is_empty() {
            return;
        }
        let ids: Vec<i32> = r.data.scheduled.values().copied().collect();
        #[cfg(mobile)]
        if let Some(app) = &self.app {
            use tauri_plugin_notification::NotificationExt;
            let _ = app.notification().cancel(ids.clone());
            let _ = app.notification().remove_active(ids);
        }
        let _ = ids;
        r.data.scheduled.clear();
        let _ = self.persist(&r);
    }
}
