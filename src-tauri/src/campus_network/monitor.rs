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
use std::{sync::mpsc, thread, time::Duration};
use tauri::Manager;

const DORM_MONITOR_TICK: Duration = Duration::from_secs(10);

pub(super) fn monitor_dorm_network(app: tauri::AppHandle, stop: mpsc::Receiver<()>) {
    let Ok(client) = PortalClient::new() else {
        return;
    };
    loop {
        match stop.recv_timeout(DORM_MONITOR_TICK) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let Some(store) = app.try_state::<CampusNetworkStore>() else {
            break;
        };
        if store.login_in_flight() {
            continue;
        }
        let token = OperationToken::Passive(store.operations.generation());
        let snapshot = network_snapshot();
        let allowed = || store.operations.current(token) && network_snapshot() == snapshot;
        let http = CheckedHttp {
            inner: &client,
            allowed: &allowed,
        };
        let observation = probe_network_overview(&http, snapshot.clone(), now_ms());
        if !allowed() {
            continue;
        }
        let events = AppNetworkEvents(&app);
        let Some(overview) = store.apply_observation(token, &observation, &events) else {
            continue;
        };
        let staff_handled = store.operations.commit(token, || {
            let known = super::staff_login::trusted_network(
                &store,
                &observation.snapshot,
                &observation.staff,
            );
            let current = || crate::auth::current_network_credentials(&app);
            let due = known
                && super::staff_login::auto_due(&store, &observation, now_ms(), &current, &events);
            (known, due)
        });
        if let Some((true, due)) = staff_handled {
            if due {
                let _ = super::staff_login::start_auto(app.clone(), &observation);
            }
            continue;
        }
        let Ok(settings) = dorm_settings(&store) else {
            continue;
        };
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
                        status.connected = snapshot.connected;
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
            let _ = start_dorm_login(app.clone(), false);
        }
    }
}

pub(crate) fn start_dorm_monitor(app: tauri::AppHandle) {
    let receiver = app
        .try_state::<CampusNetworkStore>()
        .and_then(|store| store.reserve_monitor());
    if let Some(receiver) = receiver {
        thread::spawn(move || monitor_dorm_network(app, receiver));
    }
}

pub(super) fn login_due(status: &DormLoginStatusView, now: u64, in_flight: bool) -> bool {
    !in_flight
        && !matches!(status.state, DormLoginState::Paused)
        && status.next_probe_at_ms.is_none_or(|next| next <= now)
}
