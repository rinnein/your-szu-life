//! Bounded, persistent per-dorm history. Network work never holds the snapshot lock.
use super::*;
use chrono::{FixedOffset, TimeZone, Timelike, Utc};
use std::{fs, io::Write, path::PathBuf};

const RETAIN_DAYS: i64 = 62;
const MAX_ROOMS: usize = 8;
const MAX_FILE: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheInfo {
    pub from_cache: bool,
    pub updated_at_ms: Option<i64>,
    pub refresh_error: Option<String>,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
struct Check {
    fetched_at: i64,
    expires_at: i64,
}
#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
struct Room {
    location: ElectricityLocationSettings,
    readings: Vec<ElectricityReading>,
    recharges: Vec<Recharge>,
    usage_checks: BTreeMap<String, Check>,
    recharge_checks: BTreeMap<String, Check>,
    balance: Option<f64>,
    balance_date: Option<String>,
    usage_updated_at: Option<i64>,
    recharge_updated_at: Option<i64>,
    #[serde(default)]
    retry_after: i64,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    recharge_error: Option<String>,
    #[serde(default)]
    recharge_retry_after: i64,
}
#[derive(Default, Deserialize, Serialize)]
struct DiskCache {
    version: u8,
    rooms: Vec<Room>,
}
#[derive(Default)]
pub(super) struct History {
    path: Option<PathBuf>,
    rooms: Vec<Room>,
    persistence_error: Option<String>,
}
fn today_at(now: i64) -> NaiveDate {
    Utc.timestamp_millis_opt(now)
        .unwrap()
        .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap())
        .date_naive()
}
pub(super) fn valid_until(now: i64) -> i64 {
    let zone = FixedOffset::east_opt(8 * 3600).unwrap();
    let local = Utc.timestamp_millis_opt(now).unwrap().with_timezone(&zone);
    let boundary = zone
        .from_local_datetime(&local.date_naive().and_hms_opt(23, 59, 0).unwrap())
        .unwrap();
    // Publication is approximate. During the midnight window, retry after five
    // minutes instead of caching an early/empty response for another whole day.
    if local >= boundary || (local.hour() == 0 && local.minute() < 10) {
        now + 5 * 60 * 1000
    } else {
        boundary.timestamp_millis()
    }
}
pub(super) fn attempt_key(now: i64) -> String {
    let local = Utc
        .timestamp_millis_opt(now)
        .unwrap()
        .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap());
    if (local.hour() == 23 && local.minute() == 59) || (local.hour() == 0 && local.minute() < 10) {
        format!("publish-{}", now / 300_000)
    } else {
        today_at(now).to_string()
    }
}
impl History {
    pub(super) fn unavailable() -> Self {
        Self {
            persistence_error: Some("无法确定本地缓存目录，当前仅在本次运行中缓存。".into()),
            ..Self::default()
        }
    }
    pub(super) fn open(path: PathBuf) -> Self {
        let rooms = fs::File::open(&path)
            .ok()
            .and_then(|file| {
                let mut data = vec![];
                file.take(MAX_FILE + 1).read_to_end(&mut data).ok()?;
                if data.len() as u64 > MAX_FILE {
                    return None;
                }
                let disk: DiskCache = serde_json::from_slice(&data).ok()?;
                (disk.version == 1 && disk.rooms.len() <= MAX_ROOMS).then_some(disk.rooms)
            })
            .unwrap_or_default();
        Self {
            path: Some(path),
            rooms,
            persistence_error: None,
        }
    }
    fn room(&self, location: &ElectricityLocationSettings) -> Room {
        self.rooms
            .iter()
            .find(|r| &r.location == location)
            .cloned()
            .unwrap_or_else(|| Room {
                location: location.clone(),
                ..Default::default()
            })
    }
    fn save(&mut self, mut room: Room, now: i64) {
        let cutoff = (today_at(now) - ChronoDuration::days(RETAIN_DAYS - 1)).to_string();
        room.readings
            .retain(|r| reading_date(&r.date).is_some_and(|d| d.to_string() >= cutoff));
        room.recharges
            .retain(|r| reading_date(&r.time).is_some_and(|d| d.to_string() >= cutoff));
        room.usage_checks.retain(|date, _| date >= &cutoff);
        room.recharge_checks.retain(|date, _| date >= &cutoff);
        self.rooms.retain(|r| r.location != room.location);
        self.rooms.push(room);
        if self.rooms.len() > MAX_ROOMS {
            self.rooms.remove(0);
        }
        let Some(path) = self.path.as_ref() else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let temp = parent.join(format!(".electricity-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::create_dir_all(parent)?;
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp)?;
            let bytes = serde_json::to_vec(&DiskCache {
                version: 1,
                rooms: self.rooms.clone(),
            })?;
            if bytes.len() as u64 > MAX_FILE {
                return Err("electricity cache exceeds file limit".into());
            }
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temp, path)?;
            Ok(())
        })();
        self.persistence_error = result
            .err()
            .map(|_| "本地电量缓存保存失败，当前结果仍可查看。".into());
        let _ = fs::remove_file(&temp);
    }
}
fn required_windows(
    input: &ElectricityQueryInput,
    room: &Room,
    now: i64,
    force: bool,
    recharge: bool,
) -> Vec<ElectricityQueryInput> {
    let today = today_at(now);
    let checks = if recharge {
        &room.recharge_checks
    } else {
        &room.usage_checks
    };
    let start = query_date(&input.begin_date).unwrap();
    let end = query_date(&input.end_date).unwrap();
    let mut ranges = Vec::new();
    let mut range: Option<ElectricityQueryInput> = None;
    for date in start
        .iter_days()
        .take((end - start).num_days() as usize + 1)
    {
        let key = date.to_string();
        let old = date < today - ChronoDuration::days(2);
        // Old positive usage readings and confirmed historical recharge windows
        // are stable. Empty usage days are retried once per publication cycle.
        let settled_at = FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .from_local_datetime(&date.succ_opt().unwrap().and_hms_opt(0, 10, 0).unwrap())
            .unwrap()
            .timestamp_millis();
        let settled = checks
            .get(&key)
            .is_some_and(|check| check.fetched_at >= settled_at);
        let stable = old
            && settled
            && if recharge {
                checks.contains_key(&key)
            } else {
                room.readings.iter().any(|r| {
                    reading_date(&r.date) == Some(date)
                        && (r.total_used.is_some() || r.daily_used.is_some())
                })
            };
        let fresh = checks.get(&key).is_some_and(|check| {
            check.expires_at > now && check.expires_at <= now + 24 * 60 * 60 * 1000
        });
        let needed = (!stable && !fresh) || (force && !old);
        if needed {
            if let Some(r) = range.as_mut() {
                r.end_date = key;
            } else {
                let mut r = input.clone();
                r.begin_date = key.clone();
                r.end_date = key;
                range = Some(r);
            }
        } else if let Some(r) = range.take() {
            ranges.push(r);
        }
    }
    if let Some(r) = range {
        ranges.push(r);
    }
    ranges
        .into_iter()
        .flat_map(|r| query_windows(&r).unwrap())
        .collect()
}
fn mark(checks: &mut BTreeMap<String, Check>, input: &ElectricityQueryInput, now: i64) {
    let first = query_date(&input.begin_date).unwrap();
    let last = query_date(&input.end_date).unwrap();
    for d in first
        .iter_days()
        .take((last - first).num_days() as usize + 1)
    {
        checks.insert(
            d.to_string(),
            Check {
                fetched_at: now,
                expires_at: valid_until(now),
            },
        );
    }
}
fn view(
    room: &Room,
    mut input: ElectricityQueryInput,
    purpose: QueryPurpose,
    now: i64,
    from_cache: bool,
    persistence_error: Option<String>,
) -> ElectricityQueryView {
    (input.begin_date, input.end_date) = analytics::window(today_at(now), purpose);
    let data = providers::ProviderData {
        readings: room
            .readings
            .iter()
            .filter(|r| providers::in_window(&r.date, &input))
            .cloned()
            .collect(),
        current_remaining: room.balance,
        recharges: if purpose == QueryPurpose::Detail {
            room.recharges
                .iter()
                .filter(|r| providers::in_window(&r.time, &input))
                .cloned()
                .collect()
        } else {
            vec![]
        },
        recharge_error: if purpose == QueryPurpose::Detail {
            room.recharge_error.clone()
        } else {
            None
        },
    };
    let mut result = build_view(input, data, today_at(now), room.balance_date.clone());
    result.cache = Some(CacheInfo {
        from_cache,
        updated_at_ms: room.usage_updated_at,
        refresh_error: room.error.clone().or(persistence_error),
    });
    result
}
pub(super) fn snapshot(
    store: &ElectricityStore,
    input: ElectricityQueryInput,
) -> Result<Option<ElectricityQueryView>, String> {
    let history = store.history.lock().map_err(|_| "电量缓存不可用")?;
    let room = history.room(&location_for(&input));
    if room.usage_updated_at.is_none() {
        return Ok(None);
    }
    Ok(Some(view(
        &room,
        input,
        QueryPurpose::Detail,
        now_ms(),
        true,
        history.persistence_error.clone(),
    )))
}
pub(super) fn query(
    store: &ElectricityStore,
    input: ElectricityQueryInput,
    purpose: QueryPurpose,
    force: bool,
) -> Result<ElectricityQueryView, String> {
    query_with(
        store,
        input,
        purpose,
        force,
        now_ms,
        fetch_usage,
        fetch_recharges,
    )
}
fn query_with(
    store: &ElectricityStore,
    mut input: ElectricityQueryInput,
    purpose: QueryPurpose,
    force: bool,
    clock: impl Fn() -> i64,
    mut fetch: impl FnMut(&ElectricityQueryInput) -> Result<providers::ProviderData, String>,
    mut recharge: impl FnMut(&ElectricityQueryInput) -> Result<Vec<Recharge>, String>,
) -> Result<ElectricityQueryView, String> {
    // Waiting callers re-read the cache after the first request finishes. Thus
    // overlapping foreground/background calls share successful fetched days.
    let _query = store.query_lock.lock().map_err(|_| "电量查询状态不可用")?;
    let now = clock();
    (input.begin_date, input.end_date) = analytics::window(today_at(now), purpose);
    validate(&input)?;
    let mut room = store
        .history
        .lock()
        .map_err(|_| "电量缓存不可用")?
        .room(&location_for(&input));
    if !force && room.retry_after > now && room.retry_after <= now + 60_000 {
        if room.usage_updated_at.is_none() {
            return Err(room
                .error
                .clone()
                .unwrap_or("电量查询暂不可用，请稍后重试".into()));
        }
        return Ok(view(&room, input, purpose, now, true, None));
    }
    let original = room.clone();
    let mut fetched = false;
    room.error = None;
    for chunk in required_windows(&input, &room, now, force, false) {
        match fetch(&chunk) {
            Ok(mut data) => {
                data.readings.sort_by_key(|r| reading_datetime(&r.date));
                // Keep prior values if a temporarily incomplete response omits
                // them; new rows replace the same day's previous snapshot.
                for reading in data.readings {
                    if !providers::in_window(&reading.date, &chunk) {
                        continue;
                    }
                    room.readings
                        .retain(|r| reading_date(&r.date) != reading_date(&reading.date));
                    room.readings.push(reading);
                }
                if let Some(balance) = data.current_remaining {
                    room.balance = Some(balance);
                    room.balance_date = Some(today_at(clock()).to_string());
                }
                mark(&mut room.usage_checks, &chunk, clock());
                room.usage_updated_at = Some(clock());
                fetched = true;
                room.retry_after = 0;
            }
            Err(error) => {
                room.error = Some(error);
                room.retry_after = clock() + 60_000;
                break;
            }
        }
    }
    if purpose == QueryPurpose::Detail {
        refresh_recharges(&mut room, &input, now, force, &clock, &mut recharge);
    }
    let mut history = store.history.lock().map_err(|_| "电量缓存不可用")?;
    if room != original {
        history.save(room.clone(), clock());
    }
    if room.usage_updated_at.is_none() {
        return Err(room.error.unwrap_or("尚无本地用电数据".into()));
    }
    Ok(view(
        &room,
        input,
        purpose,
        clock(),
        !fetched,
        history.persistence_error.clone(),
    ))
}
fn refresh_recharges(
    room: &mut Room,
    input: &ElectricityQueryInput,
    now: i64,
    force: bool,
    clock: &impl Fn() -> i64,
    fetch: &mut impl FnMut(&ElectricityQueryInput) -> Result<Vec<Recharge>, String>,
) {
    if !force && room.recharge_retry_after > now && room.recharge_retry_after <= now + 60_000 {
        return;
    }
    room.recharge_error = None;
    for chunk in required_windows(input, room, now, force, true) {
        match fetch(&chunk) {
            Ok(rows) => {
                room.recharges
                    .retain(|r| !providers::in_window(&r.time, &chunk));
                room.recharges.extend(
                    rows.into_iter()
                        .filter(|r| providers::in_window(&r.time, &chunk)),
                );
                mark(&mut room.recharge_checks, &chunk, clock());
                room.recharge_updated_at = Some(clock());
                room.recharge_retry_after = 0;
            }
            Err(error) => {
                room.recharge_error = Some(error);
                room.recharge_retry_after = clock() + 60_000;
                break;
            }
        }
    }
    room.recharges
        .sort_by_key(|r| std::cmp::Reverse(reading_datetime(&r.time)));
}
pub(super) fn recharges(
    store: &ElectricityStore,
    mut input: ElectricityQueryInput,
) -> Result<Vec<Recharge>, String> {
    let _query = store.query_lock.lock().map_err(|_| "电量查询状态不可用")?;
    let now = now_ms();
    (input.begin_date, input.end_date) = analytics::window(today_at(now), QueryPurpose::Detail);
    validate(&input)?;
    let mut room = store
        .history
        .lock()
        .map_err(|_| "电量缓存不可用")?
        .room(&location_for(&input));
    refresh_recharges(&mut room, &input, now, true, &now_ms, &mut fetch_recharges);
    store
        .history
        .lock()
        .map_err(|_| "电量缓存不可用")?
        .save(room.clone(), now_ms());
    if let Some(error) = room.recharge_error {
        return Err(error);
    }
    Ok(room
        .recharges
        .into_iter()
        .filter(|r| providers::in_window(&r.time, &input))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier,
    };
    fn time(text: &str) -> i64 {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .from_local_datetime(&NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap())
            .unwrap()
            .timestamp_millis()
    }
    fn input() -> ElectricityQueryInput {
        ElectricityQueryInput {
            campus_id: "canghai".into(),
            area_id: "canghai-main".into(),
            building_id: "test".into(),
            building_name: "测试楼".into(),
            room_name: "101".into(),
            begin_date: String::new(),
            end_date: String::new(),
        }
    }
    fn sample(input: &ElectricityQueryInput, now: i64) -> providers::ProviderData {
        let start = query_date(&input.begin_date).unwrap();
        let end = query_date(&input.end_date).unwrap();
        providers::ProviderData::new(
            start
                .iter_days()
                .take((end - start).num_days() as usize + 1)
                .filter(|d| *d < today_at(now))
                .map(|d| ElectricityReading {
                    date: d.to_string(),
                    room_name: input.room_name.clone(),
                    remaining: Some(20.0),
                    total_used: Some(d.num_days_from_ce() as f64 * 2.0),
                    total_purchased: Some(100.0),
                    daily_used: None,
                    daily_purchased: None,
                })
                .collect(),
        )
    }
    fn run(
        store: &ElectricityStore,
        now: i64,
        force: bool,
        purpose: QueryPurpose,
    ) -> (ElectricityQueryView, Vec<(String, String)>, usize) {
        let mut calls = vec![];
        let mut charges = 0;
        let view = query_with(
            store,
            input(),
            purpose,
            force,
            || now,
            |i| {
                calls.push((i.begin_date.clone(), i.end_date.clone()));
                Ok(sample(i, now))
            },
            |_| {
                charges += 1;
                Ok(vec![])
            },
        )
        .unwrap();
        (view, calls, charges)
    }
    #[test]
    fn repeat_and_restarted_detail_reads_make_no_network_requests() {
        let directory =
            std::env::temp_dir().join(format!("electricity-cache-test-{}", uuid::Uuid::new_v4()));
        let path = directory.join("history.json");
        let store = ElectricityStore::default();
        *store.history.lock().unwrap() = History::open(path.clone());
        let now = time("2026-10-07 12:00:00");
        let (first, calls, charges) = run(&store, now, false, QueryPurpose::Detail);
        assert_eq!(calls.len(), 2);
        assert_eq!(charges, 2);
        assert_eq!(first.readings.len(), 30);
        assert_eq!(first.readings[20].daily_used, Some(2.0));
        let saved = fs::read(&path).unwrap();
        let (second, calls, charges) = run(&store, now + 60_000, false, QueryPurpose::Detail);
        assert!(calls.is_empty());
        assert_eq!(charges, 0);
        assert!(second.cache.unwrap().from_cache);
        assert_eq!(fs::read(&path).unwrap(), saved);
        let restarted = ElectricityStore::default();
        *restarted.history.lock().unwrap() = History::open(path);
        let (view, calls, charges) = run(&restarted, now + 120_000, false, QueryPurpose::Detail);
        assert!(calls.is_empty());
        assert_eq!(charges, 0);
        assert_eq!(view.readings, first.readings);
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn publication_window_and_next_day_only_refresh_recent_days() {
        let store = ElectricityStore::default();
        run(
            &store,
            time("2026-10-07 23:58:00"),
            false,
            QueryPurpose::Detail,
        );
        let (_, calls, _) = run(
            &store,
            time("2026-10-07 23:59:10"),
            false,
            QueryPurpose::Detail,
        );
        assert_eq!(calls, vec![("2026-10-05".into(), "2026-10-07".into())]);
        let (_, calls, _) = run(
            &store,
            time("2026-10-07 23:59:30"),
            false,
            QueryPurpose::Detail,
        );
        assert!(calls.is_empty());
        let (next, calls, _) = run(
            &store,
            time("2026-10-08 00:06:00"),
            false,
            QueryPurpose::Detail,
        );
        assert_eq!(calls, vec![("2026-10-06".into(), "2026-10-08".into())]);
        assert_eq!(next.readings.last().unwrap().date, "2026-10-07");
        let (_, calls, _) = run(
            &store,
            time("2026-10-08 00:07:00"),
            false,
            QueryPurpose::Detail,
        );
        assert!(calls.is_empty());
        assert_ne!(
            attempt_key(time("2026-10-07 12:00:00")),
            attempt_key(time("2026-10-07 23:59:00"))
        );
    }
    #[test]
    fn force_refresh_does_not_download_stable_history() {
        let store = ElectricityStore::default();
        let now = time("2026-10-07 12:00:00");
        run(&store, now, false, QueryPurpose::Detail);
        let (_, calls, charges) = run(&store, now + 1000, true, QueryPurpose::Detail);
        assert_eq!(calls, vec![("2026-10-05".into(), "2026-10-07".into())]);
        assert_eq!(charges, 1);
    }
    #[test]
    fn monitor_is_three_days_only_and_detail_reuses_it() {
        let store = ElectricityStore::default();
        let now = time("2026-10-07 12:00:00");
        let (_, calls, charges) = run(&store, now, false, QueryPurpose::Monitor);
        assert_eq!(calls, vec![("2026-10-05".into(), "2026-10-07".into())]);
        assert_eq!(charges, 0);
        let (_, calls, _) = run(&store, now + 1000, false, QueryPurpose::Detail);
        assert_eq!(
            calls,
            vec![
                ("2026-09-07".into(), "2026-09-26".into()),
                ("2026-09-27".into(), "2026-10-04".into())
            ]
        );
        assert!(run(&store, now + 2000, false, QueryPurpose::Monitor)
            .1
            .is_empty());
    }
    #[test]
    fn offline_preserves_records_without_marking_missing_days_as_fetched() {
        let store = ElectricityStore::default();
        run(
            &store,
            time("2026-10-07 12:00:00"),
            false,
            QueryPurpose::Detail,
        );
        let now = time("2026-10-08 12:00:00");
        let view = query_with(
            &store,
            input(),
            QueryPurpose::Detail,
            false,
            || now,
            |_| Err("离线".into()),
            |_| Err("离线".into()),
        )
        .unwrap();
        assert!(!view.readings.is_empty());
        assert_eq!(view.cache.unwrap().refresh_error.as_deref(), Some("离线"));
        let view = query_with(
            &store,
            input(),
            QueryPurpose::Detail,
            false,
            || now + 1000,
            |_| panic!("backoff"),
            |_| panic!("backoff"),
        )
        .unwrap();
        assert_eq!(view.cache.unwrap().refresh_error.as_deref(), Some("离线"));
        let (_, calls, _) = run(&store, now + 61_000, false, QueryPurpose::Detail);
        assert_eq!(calls, vec![("2026-10-06".into(), "2026-10-08".into())]);
    }
    #[test]
    fn rooms_are_isolated_and_concurrent_requests_share_successful_fetches() {
        let store = ElectricityStore::default();
        let now = time("2026-10-07 12:00:00");
        let requests = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let store = store.clone();
                let requests = requests.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    query_with(
                        &store,
                        input(),
                        QueryPurpose::Detail,
                        false,
                        || now,
                        |i| {
                            requests.fetch_add(1, Ordering::SeqCst);
                            Ok(sample(i, now))
                        },
                        |_| Ok(vec![]),
                    )
                    .unwrap()
                })
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap().readings.len(), 30);
        }
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        let mut other = input();
        other.room_name = "102".into();
        assert!(snapshot(&store, other.clone()).unwrap().is_none());
        query_with(
            &store,
            other,
            QueryPurpose::Monitor,
            false,
            || now,
            |i| {
                requests.fetch_add(1, Ordering::SeqCst);
                Ok(sample(i, now))
            },
            |_| panic!("monitor charges"),
        )
        .unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 3);
    }
    #[test]
    fn corrupt_disk_cache_is_ignored() {
        let path =
            std::env::temp_dir().join(format!("electricity-corrupt-{}", uuid::Uuid::new_v4()));
        fs::write(&path, b"{bad json").unwrap();
        assert!(History::open(path.clone()).rooms.is_empty());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn reopening_after_days_rechecks_previously_provisional_dates() {
        let store = ElectricityStore::default();
        let first = time("2026-10-01 12:00:00");
        query_with(
            &store,
            input(),
            QueryPurpose::Detail,
            false,
            || first,
            |i| {
                let mut data = sample(i, first);
                if query_date(&i.end_date) == Some(today_at(first)) {
                    data.readings.push(ElectricityReading {
                        date: "2026-10-01".into(),
                        room_name: "101".into(),
                        remaining: Some(20.0),
                        total_used: None,
                        total_purchased: None,
                        daily_used: Some(0.5),
                        daily_purchased: None,
                    });
                }
                Ok(data)
            },
            |_| Ok(vec![]),
        )
        .unwrap();
        let later = time("2026-10-07 12:00:00");
        let mut charges = vec![];
        let mut usage = vec![];
        query_with(
            &store,
            input(),
            QueryPurpose::Detail,
            false,
            || later,
            |i| {
                usage.push((i.begin_date.clone(), i.end_date.clone()));
                Ok(sample(i, later))
            },
            |i| {
                charges.push((i.begin_date.clone(), i.end_date.clone()));
                Ok(vec![])
            },
        )
        .unwrap();
        assert_eq!(usage, vec![("2026-10-01".into(), "2026-10-07".into())]);
        assert_eq!(charges, usage);
    }
}
