use super::{
    model::*,
    network::{classify_network_overview, NetworkObservation, RegionEvidence},
    operations::{LoginGuard, OperationGate, OperationToken},
};
use crate::storage::SharedCredentialStore;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
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
    dorm_monitor: Mutex<Option<mpsc::Sender<()>>>,
    pub(super) dorm_last_fingerprint: Mutex<Option<String>>,
    pub(super) network_overview: Mutex<NetworkOverviewView>,
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
            dorm_last_fingerprint: Mutex::new(None),
            network_overview: Mutex::new(NetworkOverviewView::default()),
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
    pub(super) fn reserve_monitor(&self) -> Option<mpsc::Receiver<()>> {
        let mut monitor = self.dorm_monitor.lock().ok()?;
        if monitor.is_some() {
            return None;
        }
        let (sender, receiver) = mpsc::channel();
        *monitor = Some(sender);
        Some(receiver)
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
