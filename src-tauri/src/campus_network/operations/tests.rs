use super::*;

#[test]
fn logout_cancels_login_and_stale_observations_before_waiting() {
    let gate = Arc::new(OperationGate::default());
    let stale = OperationToken::Passive(gate.generation());
    let login = gate.begin_login().unwrap();
    let logout = gate.begin_logout().unwrap();
    assert!(!login.current());
    assert!(login
        .commit(|| panic!("stale login must never publish"))
        .is_none());
    assert!(gate
        .commit(stale, || panic!("stale probe must never publish"))
        .is_none());
    assert!(!logout.wait_for_login(Duration::ZERO));
    assert!(gate.begin_login().is_err());
    assert!(gate.begin_logout().is_err());
    assert!(gate.settings(|| Ok(())).is_err());
    drop(login);
    assert!(logout.wait_for_login(Duration::ZERO));
    let during = OperationToken::Passive(gate.generation());
    drop(logout);
    assert!(gate
        .commit(during, || panic!(
            "probe from logout generation must expire"
        ))
        .is_none());
    assert!(!gate.busy());
    assert!(gate.begin_login().is_ok());
}

#[test]
fn completed_login_invalidates_passive_probes_and_all_guards_release_on_drop() {
    let gate = Arc::new(OperationGate::default());
    let login = gate.begin_login().unwrap();
    let token = login.token();
    assert!(login.commit(|| 42).is_some());
    drop(login);
    assert!(!gate.current(token));
    let current = OperationToken::Passive(gate.generation());
    assert_eq!(gate.commit(current, || 42), Some(42));
    assert!(gate.settings(|| Ok(42)).is_ok());
}
