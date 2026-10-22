use super::{
    http::{CheckedHttp, PortalClient},
    login::start_dorm_login,
    model::*,
    network::*,
    now_ms,
    operations::OperationToken,
    settings::dorm_settings,
    state::AppNetworkEvents,
    CampusNetworkStore,
};
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use tauri::Manager;

const DORM_MONITOR_TICK: Duration = Duration::from_secs(10);
// Android needs a lightweight snapshot poll to notice Wi-Fi/IP transitions,
// but portal requests must not follow that one-second cadence.  Portal
// sessions are refreshed at a deliberately slower cadence and can still be
// refreshed immediately through ProbeNow (page entry/manual refresh).
const ANDROID_SNAPSHOT_TICK: Duration = Duration::from_secs(1);
const ANDROID_PORTAL_PROBE_TICK: Duration = Duration::from_secs(60);
pub(crate) enum MonitorCommand {
    ProbeNow,
}

fn suspected_campus_wifi(snapshot: &NetworkSnapshot) -> bool {
    snapshot.wifi_connected && snapshot.wifi_has_private_172_ipv4
}

/// Return the identity used to de-duplicate the one-shot campus Wi-Fi probe.
///
/// Do not use `NetworkSnapshot::fingerprint` here: that fingerprint also
/// contains permission/SSID metadata, which may change after the Android
/// permission dialog without the underlying network changing.  Such metadata
/// changes must not cause another portal request.
fn campus_fast_fingerprint(snapshot: &NetworkSnapshot) -> Option<String> {
    suspected_campus_wifi(snapshot).then(|| {
        format!(
            "{:?}|{:?}|{:?}|{}",
            snapshot.transport, snapshot.ip, snapshot.wifi_ip, snapshot.wifi_has_private_172_ipv4,
        )
    })
}

#[derive(Debug, Clone, Copy)]
enum MonitorStage {
    Snapshot,
    ManualProbe,
    PeriodicProbe,
    PortalProbe,
    StateUpdate,
    AutoLogin,
}

impl MonitorStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "probe/snapshot",
            Self::ManualProbe => "probe/manual-probe",
            Self::PeriodicProbe => "probe/periodic-probe",
            Self::PortalProbe => "probe/portal-probe",
            Self::StateUpdate => "probe/state-update",
            Self::AutoLogin => "probe/auto-login",
        }
    }
}

fn record_monitor_panic(
    app: &tauri::AppHandle,
    generation: u64,
    stage: MonitorStage,
    retry_after: Duration,
) {
    let Some(store) = app.try_state::<CampusNetworkStore>() else {
        return;
    };
    super::diagnostics::record(
        app,
        &store,
        DebugLogEntry {
            timestamp_ms: now_ms(),
            monitor_generation: Some(generation),
            level: "error".into(),
            source: "monitor".into(),
            stage: stage.as_str().into(),
            retry_after_ms: Some(retry_after.as_millis().min(u64::MAX as u128) as u64),
            error_kind: Some("panic".into()),
            message: Some("监控阶段发生异常，已隔离并准备重试".into()),
            ..Default::default()
        },
    );
}

fn guarded_stage<T>(
    app: &tauri::AppHandle,
    generation: u64,
    stage: MonitorStage,
    retry_after: Duration,
    operation: impl FnOnce() -> Result<T, String>,
) -> Option<Result<T, String>> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => Some(result),
        Err(_) => {
            record_monitor_panic(app, generation, stage, retry_after);
            None
        }
    }
}

fn snapshot_stage(
    app: &tauri::AppHandle,
    generation: u64,
    retry_after: Duration,
) -> Option<Result<NetworkSnapshot, String>> {
    guarded_stage(app, generation, MonitorStage::Snapshot, retry_after, || {
        network_snapshot_result()
    })
}

fn probe_once(
    app: &tauri::AppHandle,
    client: &PortalClient,
    generation: u64,
) -> Result<(NetworkOverviewView, NetworkSnapshot), String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    if store.login_in_flight() {
        return Err("校园网登录操作正在进行中".into());
    }
    let token = OperationToken::Passive(store.operations.generation());
    let snapshot = match snapshot_stage(app, generation, Duration::from_secs(5)) {
        Some(Ok(snapshot)) => {
            super::diagnostics::record_snapshot(app, &store, &snapshot, None);
            // Publish local connectivity before the portal HTTP probes. Captive
            // Wi-Fi often keeps those probes pending for several seconds.
            store.apply_snapshot_connection(&snapshot, &AppNetworkEvents(app));
            snapshot
        }
        Some(Err(error)) => {
            super::diagnostics::record_snapshot(
                app,
                &store,
                &NetworkSnapshot::default(),
                Some(&error),
            );
            return Err("Android 网络快照暂不可用".into());
        }
        None => return Err("Android 网络快照阶段异常".into()),
    };
    let allowed = || {
        if !store.operations.current(token) {
            return false;
        }
        match network_snapshot_result() {
            Ok(current) => same_network_identity(&current, &snapshot),
            Err(_) => false,
        }
    };
    let diagnostic_http = super::diagnostics::DiagnosticHttp {
        inner: client,
        app,
        store: &store,
    };
    let http = CheckedHttp {
        inner: &diagnostic_http,
        allowed: &allowed,
    };
    let observation = match guarded_stage(
        app,
        generation,
        MonitorStage::PortalProbe,
        Duration::from_secs(10),
        || Ok(probe_network_overview(&http, snapshot.clone(), now_ms())),
    ) {
        Some(Ok(observation)) => observation,
        Some(Err(error)) => return Err(error),
        None => return Err("门户探测阶段异常".into()),
    };
    let stable = match guarded_stage(
        app,
        generation,
        MonitorStage::Snapshot,
        Duration::from_secs(5),
        || Ok(allowed()),
    ) {
        Some(Ok(stable)) => stable,
        Some(Err(error)) => return Err(error),
        None => return Err("网络稳定性检查阶段异常".into()),
    };
    if !stable {
        return Err("网络在探测期间发生变化".into());
    }
    let events = AppNetworkEvents(app);
    let overview = match guarded_stage(
        app,
        generation,
        MonitorStage::StateUpdate,
        Duration::from_secs(5),
        || {
            store
                .apply_observation(token, &observation, &events)
                .ok_or_else(|| "网络状态更新被取消".to_owned())
        },
    ) {
        Some(Ok(overview)) => overview,
        Some(Err(error)) => return Err(error),
        None => return Err("网络状态更新阶段异常".into()),
    };
    let staff_handled = store.operations.commit(token, || {
        let known =
            super::staff_login::trusted_network(&store, &observation.snapshot, &observation.staff);
        let current = || crate::auth::current_network_credentials(app);
        let due = known
            && super::staff_login::auto_due(&store, &observation, now_ms(), &current, &events);
        (known, due)
    });
    if let Some((true, due)) = staff_handled {
        if due {
            let _ = guarded_stage(
                app,
                generation,
                MonitorStage::AutoLogin,
                Duration::from_secs(5),
                || {
                    let _ = super::staff_login::start_auto(app.clone(), &observation);
                    Ok(())
                },
            );
        }
        return Ok((overview, snapshot));
    }
    let settings = dorm_settings(&store)?;
    let should_login = store
        .operations
        .commit(token, || {
            if !settings.enabled {
                store.change_dorm_status(
                    |status| {
                        status.state = DormLoginState::Disabled;
                        status.next_probe_at_ms = None;
                    },
                    &events,
                );
                return false;
            }
            if overview.region != NetworkRegion::Dormitory {
                store.change_dorm_status(
                    |status| {
                        if status.state != DormLoginState::Paused {
                            status.state = DormLoginState::Monitoring;
                        }
                    },
                    &events,
                );
                return false;
            }
            let fingerprint = snapshot.fingerprint();
            let changed = store
                .dorm_last_fingerprint
                .lock()
                .map(|mut last| {
                    let changed = last.as_deref() != Some(fingerprint.as_str());
                    *last = Some(fingerprint);
                    changed
                })
                .unwrap_or(false);
            let mut due = false;
            store.change_dorm_status(
                |status| {
                    status.ssid = snapshot.ssid.clone();
                    status.connected = snapshot.effective_connected();
                    status.portal_reachable = overview.portal_reachable;
                    if changed {
                        status.consecutive_failures = 0;
                        status.next_probe_at_ms = Some(now_ms());
                        status.state = DormLoginState::Monitoring;
                    }
                    if overview.state == NetworkConnectionState::Connected {
                        status.state = DormLoginState::Online;
                        status.consecutive_failures = 0;
                        status.next_probe_at_ms = None;
                    } else if overview.state == NetworkConnectionState::NotLoggedIn {
                        if status.state == DormLoginState::Online {
                            status.state = DormLoginState::Monitoring;
                            status.next_probe_at_ms = Some(now_ms());
                        }
                        due = login_due(status, now_ms(), false);
                    }
                },
                &events,
            );
            due
        })
        .unwrap_or(false);
    if should_login {
        let _ = guarded_stage(
            app,
            generation,
            MonitorStage::AutoLogin,
            Duration::from_secs(5),
            || {
                let _ = start_dorm_login(app.clone(), false);
                Ok(())
            },
        );
    }
    Ok((overview, snapshot))
}

fn same_network_identity(current: &NetworkSnapshot, initial: &NetworkSnapshot) -> bool {
    current.effective_connected() == initial.effective_connected()
        && current.transport == initial.transport
        && current.ip == initial.ip
        && current.has_private_172_ipv4 == initial.has_private_172_ipv4
        && current.wifi_connected == initial.wifi_connected
        && current.wifi_ip == initial.wifi_ip
        && current.wifi_has_private_172_ipv4 == initial.wifi_has_private_172_ipv4
}

pub(super) fn monitor_dorm_network(
    app: tauri::AppHandle,
    generation: u64,
    receiver: mpsc::Receiver<MonitorCommand>,
    client: PortalClient,
) {
    let poll_tick = if cfg!(target_os = "android") {
        ANDROID_SNAPSHOT_TICK
    } else {
        DORM_MONITOR_TICK
    };
    let regular_probe_interval = if cfg!(target_os = "android") {
        ANDROID_PORTAL_PROBE_TICK
    } else {
        DORM_MONITOR_TICK
    };
    let mut next_regular_probe = Instant::now();
    let mut last_fast_fingerprint: Option<String> = None;
    let mut first_probe = true;
    let mut snapshot_backoff = poll_tick;
    let mut probe_backoff = Duration::from_secs(5);
    loop {
        if !first_probe {
            let wait = next_regular_probe
                .saturating_duration_since(Instant::now())
                .min(poll_tick);
            match receiver.recv_timeout(wait) {
                Ok(MonitorCommand::ProbeNow) => {
                    if let Some(store) = app.try_state::<CampusNetworkStore>() {
                        store.probe_started();
                    }
                    let result = guarded_stage(
                        &app,
                        generation,
                        MonitorStage::ManualProbe,
                        probe_backoff,
                        || probe_once(&app, &client, generation),
                    );
                    if let Some(Ok((_, snapshot))) = result {
                        last_fast_fingerprint = if cfg!(target_os = "android") {
                            campus_fast_fingerprint(&snapshot)
                        } else {
                            None
                        };
                        probe_backoff = Duration::from_secs(5);
                        next_regular_probe = Instant::now() + regular_probe_interval;
                    } else {
                        next_regular_probe = Instant::now() + probe_backoff;
                        probe_backoff = (probe_backoff * 2).min(ANDROID_PORTAL_PROBE_TICK);
                    }
                    if let Some(store) = app.try_state::<CampusNetworkStore>() {
                        store.probe_finished();
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        first_probe = false;
        let snapshot = match snapshot_stage(&app, generation, snapshot_backoff) {
            Some(Ok(snapshot)) => {
                snapshot_backoff = poll_tick;
                snapshot
            }
            Some(Err(error)) => {
                if let Some(store) = app.try_state::<CampusNetworkStore>() {
                    super::diagnostics::record(
                        &app,
                        &store,
                        DebugLogEntry {
                            timestamp_ms: now_ms(),
                            monitor_generation: Some(generation),
                            level: "warn".into(),
                            source: "monitor".into(),
                            stage: "probe/snapshot".into(),
                            error_kind: Some("bridge".into()),
                            retry_after_ms: Some(snapshot_backoff.as_millis() as u64),
                            message: Some(error),
                            ..Default::default()
                        },
                    );
                }
                thread::sleep(snapshot_backoff);
                snapshot_backoff = (snapshot_backoff * 2).min(Duration::from_secs(60));
                continue;
            }
            None => {
                thread::sleep(snapshot_backoff);
                snapshot_backoff = (snapshot_backoff * 2).min(Duration::from_secs(60));
                continue;
            }
        };
        if snapshot.is_cellular() {
            if let Some(store) = app.try_state::<CampusNetworkStore>() {
                store.apply_snapshot_connection(&snapshot, &AppNetworkEvents(&app));
                store.change_dorm_status(
                    |status| {
                        status.ssid = snapshot.ssid.clone();
                        status.connected = snapshot.effective_connected();
                        status.portal_reachable = false;
                        status.next_probe_at_ms = None;
                        if status.state != DormLoginState::Disabled
                            && status.state != DormLoginState::Paused
                        {
                            status.state = DormLoginState::Monitoring;
                        }
                    },
                    &AppNetworkEvents(&app),
                );
            }
            last_fast_fingerprint = None;
            next_regular_probe = Instant::now() + regular_probe_interval;
            continue;
        }
        let fast_fingerprint = if cfg!(target_os = "android") {
            campus_fast_fingerprint(&snapshot)
        } else {
            None
        };
        let should_fast_probe = fast_fingerprint.as_deref() != last_fast_fingerprint.as_deref();
        if fast_fingerprint.is_none() {
            // Leaving campus Wi-Fi arms the next transition for a fresh
            // one-shot probe, without probing every snapshot tick.
            last_fast_fingerprint = None;
        }
        if should_fast_probe || Instant::now() >= next_regular_probe {
            let result = guarded_stage(
                &app,
                generation,
                MonitorStage::PeriodicProbe,
                probe_backoff,
                || probe_once(&app, &client, generation),
            );
            if let Some(Ok((_, observed_snapshot))) = result {
                // Only commit the fast fingerprint after a completed probe;
                // a panic/error must be retried with bounded backoff.
                last_fast_fingerprint = if cfg!(target_os = "android") {
                    campus_fast_fingerprint(&observed_snapshot)
                } else {
                    None
                };
                probe_backoff = Duration::from_secs(5);
                next_regular_probe = Instant::now() + regular_probe_interval;
            } else {
                next_regular_probe = Instant::now() + probe_backoff;
                probe_backoff = (probe_backoff * 2).min(ANDROID_PORTAL_PROBE_TICK);
            }
        }
    }
}

pub(crate) fn start_dorm_monitor(app: tauri::AppHandle) -> bool {
    let Some(store) = app.try_state::<CampusNetworkStore>() else {
        return false;
    };
    if store.monitor_running() {
        return true;
    }
    let Some(client) = PortalClient::new().ok() else {
        super::diagnostics::record(
            &app,
            &store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: "warn".into(),
                source: "monitor".into(),
                stage: "monitor/start".into(),
                error_kind: Some("client-init".into()),
                message: Some("门户客户端初始化失败".into()),
                ..Default::default()
            },
        );
        return false;
    };
    let Some((generation, receiver)) = store.reserve_monitor() else {
        return true;
    };
    super::diagnostics::record(
        &app,
        &store,
        DebugLogEntry {
            timestamp_ms: now_ms(),
            level: "info".into(),
            source: "monitor".into(),
            stage: "monitor/start".into(),
            monitor_generation: Some(generation),
            message: Some("校园网监控已启动".into()),
            ..Default::default()
        },
    );
    let thread_app = app.clone();
    let spawned = thread::Builder::new()
        .name("campus-network-monitor".into())
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                monitor_dorm_network(thread_app.clone(), generation, receiver, client);
            }));
            let panicked = result.is_err();
            if let Some(store) = thread_app.try_state::<CampusNetworkStore>() {
                if panicked {
                    super::diagnostics::record(
                        &thread_app,
                        &store,
                        DebugLogEntry {
                            timestamp_ms: now_ms(),
                            monitor_generation: Some(generation),
                            level: "error".into(),
                            source: "monitor".into(),
                            stage: "monitor/panic".into(),
                            error_kind: Some("thread-panic".into()),
                            message: Some("监控线程已退出，正在尝试恢复".into()),
                            ..Default::default()
                        },
                    );
                }
                store.finish_monitor(generation);
                super::diagnostics::record(
                    &thread_app,
                    &store,
                    DebugLogEntry {
                        timestamp_ms: now_ms(),
                        monitor_generation: Some(generation),
                        level: "info".into(),
                        source: "monitor".into(),
                        stage: "monitor/stopped".into(),
                        ..Default::default()
                    },
                );
                if panicked && store.reserve_monitor_restart() {
                    let restart_app = thread_app.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_secs(1));
                        if !start_dorm_monitor(restart_app.clone()) {
                            if let Some(restart_store) =
                                restart_app.try_state::<CampusNetworkStore>()
                            {
                                super::diagnostics::record(
                                    &restart_app,
                                    &restart_store,
                                    DebugLogEntry {
                                        timestamp_ms: now_ms(),
                                        level: "error".into(),
                                        source: "monitor".into(),
                                        stage: "monitor/restart".into(),
                                        error_kind: Some("restart-failed".into()),
                                        message: Some("校园网监控自动恢复失败".into()),
                                        ..Default::default()
                                    },
                                );
                            }
                        }
                    });
                }
            }
        });
    if spawned.is_err() {
        store.finish_monitor(generation);
        super::diagnostics::record(
            &app,
            &store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: "error".into(),
                source: "monitor".into(),
                stage: "monitor/start".into(),
                error_kind: Some("thread-start".into()),
                message: Some("校园网监控线程启动失败".into()),
                ..Default::default()
            },
        );
        return false;
    }
    true
}

pub(crate) fn refresh_now(app: tauri::AppHandle) -> Result<NetworkOverviewView, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    let mut queued = None;
    for attempt in 0..3 {
        if !start_dorm_monitor(app.clone()) {
            if attempt == 2 {
                return Err("校园网监控初始化失败，请稍后重试".into());
            }
            thread::yield_now();
            continue;
        }
        match store.request_probe() {
            Ok(value) => {
                queued = Some(value);
                break;
            }
            Err(_) if attempt < 2 => {
                super::diagnostics::record(
                    &app,
                    &store,
                    DebugLogEntry {
                        timestamp_ms: now_ms(),
                        level: "warn".into(),
                        source: "monitor".into(),
                        stage: "restart".into(),
                        error_kind: Some("stale-monitor".into()),
                        message: Some("正在重启校园网监控".into()),
                        ..Default::default()
                    },
                );
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => break,
        }
    }
    let queued = queued.ok_or_else(|| "校园网刷新请求未能入队，请稍后重试".to_owned())?;
    if queued {
        super::diagnostics::record(
            &app,
            &store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: "info".into(),
                source: "monitor".into(),
                stage: "probe-queued".into(),
                ..Default::default()
            },
        );
    } else {
        super::diagnostics::record(
            &app,
            &store,
            DebugLogEntry {
                timestamp_ms: now_ms(),
                level: "info".into(),
                source: "monitor".into(),
                stage: "probe-coalesced".into(),
                ..Default::default()
            },
        );
    }
    let overview = store
        .network_overview
        .lock()
        .map_err(|_| "状态锁失败")?
        .clone();
    Ok(overview)
}

pub(super) fn login_due(status: &DormLoginStatusView, now: u64, in_flight: bool) -> bool {
    !in_flight
        && !matches!(status.state, DormLoginState::Paused)
        && status.next_probe_at_ms.is_none_or(|next| next <= now)
}

#[cfg(test)]
mod tests {
    use super::{campus_fast_fingerprint, same_network_identity, suspected_campus_wifi};
    use crate::campus_network::network::NetworkSnapshot;

    #[test]
    fn fast_probe_requires_connected_wifi_with_private_172_address() {
        let mut snapshot = NetworkSnapshot {
            connected: true,
            transport: Some("cellular".into()),
            wifi_connected: true,
            has_private_172_ipv4: true,
            wifi_has_private_172_ipv4: true,
            ..Default::default()
        };
        assert!(suspected_campus_wifi(&snapshot));
        snapshot.transport = Some("wifi".into());
        assert!(suspected_campus_wifi(&snapshot));
        snapshot.wifi_connected = false;
        assert!(!suspected_campus_wifi(&snapshot));
        snapshot.wifi_connected = true;
        snapshot.wifi_has_private_172_ipv4 = false;
        assert!(!suspected_campus_wifi(&snapshot));
    }

    #[test]
    fn fast_probe_fingerprint_ignores_ssid_and_permission_metadata() {
        let initial = NetworkSnapshot {
            connected: true,
            wifi_connected: true,
            wifi_ip: Some("172.30.1.2".into()),
            wifi_has_private_172_ipv4: true,
            ssid: None,
            wifi_permission: Some("denied".into()),
            location_enabled: Some(false),
            capabilities_available: Some(false),
            ..Default::default()
        };
        let mut metadata_changed = initial.clone();
        metadata_changed.ssid = Some("SZU_WLAN".into());
        metadata_changed.wifi_permission = Some("granted".into());
        metadata_changed.location_enabled = Some(true);
        metadata_changed.capabilities_available = Some(true);
        assert_eq!(
            campus_fast_fingerprint(&initial),
            campus_fast_fingerprint(&metadata_changed)
        );

        let mut address_changed = initial.clone();
        address_changed.wifi_ip = Some("172.30.1.3".into());
        assert_ne!(
            campus_fast_fingerprint(&initial),
            campus_fast_fingerprint(&address_changed)
        );

        let mut non_campus = initial;
        non_campus.wifi_has_private_172_ipv4 = false;
        assert!(campus_fast_fingerprint(&non_campus).is_none());
    }

    #[test]
    fn network_identity_ignores_ssid_permission_metadata_but_tracks_addresses() {
        let initial = NetworkSnapshot {
            connected: false,
            wifi_connected: true,
            wifi_ip: Some("172.30.1.2".into()),
            wifi_has_private_172_ipv4: true,
            ssid: None,
            wifi_permission: Some("denied".into()),
            location_enabled: Some(false),
            capabilities_available: Some(false),
            ..Default::default()
        };
        let mut metadata_changed = initial.clone();
        metadata_changed.ssid = Some("SZU_WLAN".into());
        metadata_changed.wifi_permission = Some("granted".into());
        metadata_changed.location_enabled = Some(true);
        metadata_changed.capabilities_available = Some(true);
        assert!(same_network_identity(&metadata_changed, &initial));

        let mut ip_changed = initial.clone();
        ip_changed.wifi_ip = Some("172.30.1.3".into());
        assert!(!same_network_identity(&ip_changed, &initial));
    }
}
