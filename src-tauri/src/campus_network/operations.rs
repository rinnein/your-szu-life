use std::{
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

#[derive(Debug, Clone, Copy)]
pub(super) enum OperationToken {
    Passive(u64),
    Login(u64),
    Logout(u64),
}
#[derive(Default)]
struct OperationState {
    generation: u64,
    login: Option<u64>,
    logout: Option<u64>,
}
#[derive(Default)]
pub(super) struct OperationGate {
    state: Mutex<OperationState>,
    settled: Condvar,
}

impl OperationGate {
    pub(super) fn generation(&self) -> u64 {
        self.state
            .lock()
            .map(|state| state.generation)
            .unwrap_or(u64::MAX)
    }
    pub(super) fn busy(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.login.is_some() || state.logout.is_some())
            .unwrap_or(true)
    }
    pub(super) fn begin_login(self: &Arc<Self>) -> Result<LoginGuard, String> {
        let mut state = self.state.lock().map_err(|_| "校园网操作锁失败")?;
        if state.login.is_some() || state.logout.is_some() {
            return Err("校园网登录或注销正在进行中".into());
        }
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.login = Some(generation);
        Ok(LoginGuard {
            gate: self.clone(),
            generation,
        })
    }
    pub(super) fn begin_logout(self: &Arc<Self>) -> Result<LogoutGuard, String> {
        let mut state = self.state.lock().map_err(|_| "校园网操作锁失败")?;
        if state.logout.is_some() {
            return Err("校园网注销正在进行中".into());
        }
        // Invalidate every earlier probe/login result before waiting for an admitted request.
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        state.logout = Some(generation);
        Ok(LogoutGuard {
            gate: self.clone(),
            generation,
        })
    }
    fn permits(state: &OperationState, token: OperationToken) -> bool {
        match token {
            OperationToken::Passive(generation) => {
                state.generation == generation && state.login.is_none() && state.logout.is_none()
            }
            OperationToken::Login(generation) => {
                state.generation == generation
                    && state.login == Some(generation)
                    && state.logout.is_none()
            }
            OperationToken::Logout(generation) => {
                state.generation == generation && state.logout == Some(generation)
            }
        }
    }
    pub(super) fn current(&self, token: OperationToken) -> bool {
        self.state
            .lock()
            .map(|state| Self::permits(&state, token))
            .unwrap_or(false)
    }
    /// Commit and emit under the same gate, so a stale event cannot be delivered after logout starts.
    pub(super) fn commit<T>(&self, token: OperationToken, update: impl FnOnce() -> T) -> Option<T> {
        let state = self.state.lock().ok()?;
        Self::permits(&state, token).then(update)
    }
    pub(super) fn settings<T>(
        &self,
        update: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let state = self.state.lock().map_err(|_| "校园网操作锁失败")?;
        if state.logout.is_some() {
            return Err("正在注销，请稍后保存设置".into());
        }
        update()
    }
}

pub(super) struct LoginGuard {
    gate: Arc<OperationGate>,
    generation: u64,
}
impl LoginGuard {
    pub(super) fn token(&self) -> OperationToken {
        OperationToken::Login(self.generation)
    }
    pub(super) fn current(&self) -> bool {
        self.gate.current(self.token())
    }
    pub(super) fn commit<T>(&self, update: impl FnOnce() -> T) -> Option<T> {
        self.gate.commit(self.token(), update)
    }
}
impl Drop for LoginGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            if state.login == Some(self.generation) {
                state.login = None;
                if state.generation == self.generation {
                    state.generation = state.generation.wrapping_add(1);
                }
            }
            self.gate.settled.notify_all();
        }
    }
}

pub(super) struct LogoutGuard {
    gate: Arc<OperationGate>,
    generation: u64,
}
impl LogoutGuard {
    pub(super) fn token(&self) -> OperationToken {
        OperationToken::Logout(self.generation)
    }
    pub(super) fn current(&self) -> bool {
        self.gate.current(self.token())
    }
    pub(super) fn commit<T>(&self, update: impl FnOnce() -> T) -> Option<T> {
        self.gate.commit(self.token(), update)
    }
    pub(super) fn wait_for_login(&self, timeout: Duration) -> bool {
        let Ok(state) = self.gate.state.lock() else {
            return false;
        };
        self.gate
            .settled
            .wait_timeout_while(state, timeout, |state| state.login.is_some())
            .is_ok_and(|(state, _)| {
                state.login.is_none() && Self::still_active(&state, self.generation)
            })
    }
    fn still_active(state: &OperationState, generation: u64) -> bool {
        state.logout == Some(generation)
    }
}
impl Drop for LogoutGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            if state.logout == Some(self.generation) {
                state.logout = None;
                state.generation = state.generation.wrapping_add(1);
            }
            self.gate.settled.notify_all();
        }
    }
}

#[cfg(test)]
mod tests;
