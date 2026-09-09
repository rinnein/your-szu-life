use super::{
    model::*,
    network::{classify_network_overview, NetworkObservation, NetworkSnapshot, RegionEvidence},
    operations::{LoginGuard, OperationGate, OperationToken},
};
use crate::storage::SharedCredentialStore;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

pub(crate) struct CampusNetworkStore {
    pub(super) credentials: SharedCredentialStore,
    pub(super) staff: super::staff_login::StaffRuntime,
    pub(super) dorm_settings: Mutex<Option<DormNetworkSettings>>,
    pub(super) custom_configured: AtomicBool,
    pub(super) auto_connect_suspended: AtomicBool,
    pub(super) dorm_status: Mutex<DormLoginStatusView>,
    pub(super) dorm_logs: Mutex<VecDeque<DormLoginLogView>>,
    pub(super) operations: Arc<OperationGate>,
    dorm_monitor: Mutex<Option<(u64, mpsc::Sender<super::monitor::MonitorCommand>)>>,
    monitor_generation: AtomicU64,
    monitor_restart_window: Mutex<Option<(Instant, u8)>>,
    probe_pending: AtomicBool,
    pub(super) dorm_last_fingerprint: Mutex<Option<String>>,
    pub(super) network_overview: Mutex<NetworkOverviewView>,
    pub(super) debug_enabled: AtomicBool,
    pub(super) debug_logs: Mutex<VecDeque<DebugLogEntry>>,
    region_evidence: Mutex<RegionEvidence>,
}

impl CampusNetworkStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials,
            staff: super::staff_login::StaffRuntime::default(),
            dorm_settings: Mutex::new(None),
            custom_configured: AtomicBool::new(false),
            auto_connect_suspended: AtomicBool::new(false),
            dorm_status: Mutex::new(DormLoginStatusView::default()),
            dorm_logs: Mutex::new(VecDeque::new()),
            operations: Arc::new(OperationGate::default()),
            dorm_monitor: Mutex::new(None),
            monitor_generation: AtomicU64::new(0),
            monitor_restart_window: Mutex::new(None),
            probe_pending: AtomicBool::new(false),
            dorm_last_fingerprint: Mutex::new(None),
            network_overview: Mutex::new(NetworkOverviewView::default()),
            debug_enabled: AtomicBool::new(false),
            debug_logs: Mutex::new(VecDeque::new()),
            region_evidence: Mutex::new(RegionEvidence::default()),
        }
    }
    pub(super) fn forget_region_evidence(&self) {
        if let Ok(mut evidence) = self.region_evidence.lock() {
            *evidence = RegionEvidence::default();
        }
    }
    pub(super) fn begin_login(&self) -> Result<LoginGuard, String> {
        self.operations.begin_login()
    }
    pub(super) fn login_in_flight(&self) -> bool {
        self.operations.busy()
    }
    pub(super) fn monitor_running(&self) -> bool {
        self.dorm_monitor
            .lock()
            .map(|monitor| monitor.is_some())
            .unwrap_or(false)
    }

    pub(super) fn reserve_monitor_restart(&self) -> bool {
        let Ok(mut window) = self.monitor_restart_window.lock() else {
            return false;
        };
        let now = Instant::now();
        match *window {
            Some((started, attempts)) if now.duration_since(started) < Duration::from_secs(60) => {
                if attempts >= 3 {
                    return false;
                }
                *window = Some((started, attempts + 1));
            }
            _ => *window = Some((now, 1)),
        }
        true
    }

    pub(super) fn reserve_monitor(
        &self,
    ) -> Option<(u64, mpsc::Receiver<super::monitor::MonitorCommand>)> {
        let mut monitor = self.dorm_monitor.lock().ok()?;
        if monitor.is_some() {
            return None;
        }
        let generation = self.monitor_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let (sender, receiver) = mpsc::channel();
        *monitor = Some((generation, sender));
        self.probe_pending.store(false, Ordering::Release);
        Some((generation, receiver))
    }

    pub(super) fn finish_monitor(&self, generation: u64) {
        if let Ok(mut monitor) = self.dorm_monitor.lock() {
            if monitor
                .as_ref()
                .is_some_and(|(current, _)| *current == generation)
            {
                *monitor = None;
                self.probe_pending.store(false, Ordering::Release);
            }
        }
    }

    pub(super) fn request_probe(&self) -> Result<bool, String> {
        if self.probe_pending.swap(true, Ordering::AcqRel) {
            return Ok(false);
        }
        let monitor = match self.dorm_monitor.lock() {
            Ok(monitor) => monitor.clone(),
            Err(_) => {
                self.probe_pending.store(false, Ordering::Release);
                return Err("监控状态锁失败".into());
            }
        };
        let (generation, sender) = monitor.ok_or_else(|| {
            self.probe_pending.store(false, Ordering::Release);
            "校园网监控尚未启动".to_owned()
        })?;
        if sender
            .send(super::monitor::MonitorCommand::ProbeNow)
            .is_err()
        {
            self.finish_monitor(generation);
            return Err("校园网监控已停止".into());
        }
        Ok(true)
    }

    pub(super) fn probe_started(&self) {
        self.probe_pending.store(true, Ordering::Release);
    }

    pub(super) fn probe_finished(&self) {
        self.probe_pending.store(false, Ordering::Release);
    }

    pub(super) fn change_dorm_status(
        &self,
        update: impl FnOnce(&mut DormLoginStatusView),
        events: &dyn NetworkEvents,
    ) {
        let Ok(mut status) = self.dorm_status.lock() else {
            return;
        };
        update(&mut status);
        let next = status.clone();
        drop(status);
        events.status_changed(&next);
    }
    pub(super) fn change_staff_status(
        &self,
        update: impl FnOnce(&mut StaffLoginStatusView),
        events: &dyn NetworkEvents,
    ) {
        let Ok(mut status) = self.staff.status.lock() else {
            return;
        };
        update(&mut status);
        let view = status.clone();
        drop(status);
        events.staff_status_changed(&view);
    }
    /// Publish the connection facts as soon as a fresh snapshot is available.
    ///
    /// Portal requests can take several seconds on a captive network. Updating the
    /// cached connection flag before those requests complete prevents the UI from
    /// continuing to report the initial `NoNetwork` state while the monitor is
    /// already aware of a connected Wi-Fi interface. Region and portal fields are
    /// deliberately preserved until the full observation is applied.
    pub(super) fn apply_snapshot_connection(
        &self,
        snapshot: &NetworkSnapshot,
        events: &dyn NetworkEvents,
    ) {
        let Ok(mut current) = self.network_overview.lock() else {
            return;
        };
        let connected = snapshot.effective_connected();
        let mut changed = current.connected != connected || current.ssid != snapshot.ssid;
        current.connected = connected;
        current.ssid = snapshot.ssid.clone();
        if connected && current.state == NetworkConnectionState::NoNetwork {
            current.state = NetworkConnectionState::Unknown;
            changed = true;
        }
        if changed {
            let next = current.clone();
            drop(current);
            events.overview_changed(&next);
        }
    }

    pub(super) fn apply_observation(
        &self,
        token: OperationToken,
        observation: &NetworkObservation,
        events: &dyn NetworkEvents,
    ) -> Option<NetworkOverviewView> {
        self.operations
            .commit(token, || {
                let mut evidence = self.region_evidence.lock().ok()?;
                let overview = classify_network_overview(observation, &mut evidence);
                drop(evidence);
                let mut current = self.network_overview.lock().ok()?;
                let changed = *current != overview;
                *current = overview.clone();
                drop(current);
                if changed {
                    events.overview_changed(&overview);
                }
                Some(overview)
            })
            .flatten()
    }
}

pub(super) trait NetworkEvents {
    fn staff_settings_changed(&self, _settings: &StaffNetworkSettingsView) {}
    fn staff_status_changed(&self, _status: &StaffLoginStatusView) {}
    fn staff_failure(&self, _message: &str) {}
    fn log(&self, _log: &DormLoginLogView) {}
    fn status_changed(&self, _status: &DormLoginStatusView) {}
    fn overview_changed(&self, _overview: &NetworkOverviewView) {}
    fn settings_changed(&self, _settings: &DormNetworkSettings) {}
}
impl NetworkEvents for () {}

pub(super) struct AppNetworkEvents<'a>(pub(super) &'a tauri::AppHandle);
impl NetworkEvents for AppNetworkEvents<'_> {
    fn staff_settings_changed(&self, view: &StaffNetworkSettingsView) {
        let _ = self.0.emit("staff-network-settings-changed", view);
    }
    fn staff_status_changed(&self, view: &StaffLoginStatusView) {
        let _ = self.0.emit("staff-login-status-changed", view);
    }
    fn staff_failure(&self, message: &str) {
        super::logs::dorm_notify_failure(self.0, message);
    }
    fn log(&self, log: &DormLoginLogView) {
        super::logs::append_dorm_log(self.0, log.clone());
    }

    fn status_changed(&self, status: &DormLoginStatusView) {
        let _ = self.0.emit("dorm-login-status-changed", status);
    }
    fn overview_changed(&self, overview: &NetworkOverviewView) {
        let _ = self.0.emit("network-overview-changed", overview);
    }
    fn settings_changed(&self, settings: &DormNetworkSettings) {
        let view = DormNetworkSettingsView {
            enabled: settings.enabled,
            credential_mode: settings.credential_mode.clone(),
            custom_configured: self
                .0
                .try_state::<CampusNetworkStore>()
                .is_some_and(|store| store.custom_configured.load(Ordering::Relaxed)),
        };
        let _ = self.0.emit("dorm-network-settings-changed", view);
    }
}

pub(super) fn set_dorm_status(
    app: &tauri::AppHandle,
    update: impl FnOnce(&mut DormLoginStatusView),
) {
    if let Some(store) = app.try_state::<CampusNetworkStore>() {
        store.change_dorm_status(update, &AppNetworkEvents(app));
    }
}
