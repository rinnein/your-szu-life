use super::{
    http::{portal_url, CheckedHttp, PortalClient, PortalHttp},
    logs::*,
    model::*,
    network::*,
    now_ms,
    operations::LoginGuard,
    portal::parse_jsonp_callback,
    settings::{dorm_settings, dormant_credentials},
    state::{set_dorm_status, AppNetworkEvents},
    CampusNetworkStore,
};
use std::{thread, time::Duration};
use tauri::Manager;

pub(super) fn backoff_delay(failures: u32) -> Duration {
    if failures <= 5 {
        Duration::from_secs(10)
    } else if failures <= 10 {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(600)
    }
}

fn transport_failure(
    app: &tauri::AppHandle,
    guard: &LoginGuard,
    message: &str,
    reachable: bool,
    http_status: Option<u16>,
) -> String {
    guard.commit(|| {
        let Some(store) = app.try_state::<CampusNetworkStore>() else {
            return;
        };
        let mut failures = (0, now_ms() + 600_000);
        store.change_dorm_status(
            |status| {
                failures = apply_transport_failure(status, reachable, now_ms());
            },
            &AppNetworkEvents(app),
        );
        append_dorm_log(
            app,
            DormLoginLogView {
                region: Some(NetworkRegion::Dormitory),
                timestamp_ms: now_ms(),
                stage: "probe".into(),
                outcome: "error".into(),
                portal_reachable: reachable,
                http_status,
                message: Some(message.into()),
                consecutive_failures: failures.0,
                next_probe_at_ms: Some(failures.1),
                ..Default::default()
            },
        );
    });
    message.into()
}

fn perform_dorm_login(
    app: tauri::AppHandle,
    manual: bool,
    guard: &LoginGuard,
) -> Result<DormLoginResultView, String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    if !manual && !dorm_settings(&store)?.enabled {
        return Err("自动登录未开启".into());
    }
    let snapshot = network_snapshot();
    guard
        .commit(|| {
            set_dorm_status(&app, |status| {
                status.state = DormLoginState::Probing;
                status.ssid = snapshot.ssid.clone();
                status.connected = snapshot.connected;
            })
        })
        .ok_or("登录已取消")?;
    let client = PortalClient::new()
        .map_err(|_| transport_failure(&app, guard, "无法建立门户客户端", false, None))?;
    let allowed = || guard.current();
    let http = CheckedHttp {
        inner: &client,
        allowed: &allowed,
    };
    let observation = probe_network_overview(&http, snapshot.clone(), now_ms());
    let overview = store
        .apply_observation(guard.token(), &observation, &AppNetworkEvents(&app))
        .ok_or("登录已取消")?;
    if overview.region != NetworkRegion::Dormitory {
        return Err("当前未确认处于宿舍区，未发送登录请求".into());
    }
    if overview.state == NetworkConnectionState::Unknown {
        return Err(transport_failure(
            &app,
            guard,
            "暂时无法确认宿舍区会话，未发送登录请求",
            overview.portal_reachable,
            None,
        ));
    }
    if overview.state == NetworkConnectionState::Connected {
        let view = DormLoginResultView {
            ok: true,
            already_online: true,
            message: Some("当前设备已在线".into()),
            ..Default::default()
        };
        guard
            .commit(|| {
                set_dorm_status(&app, |status| {
                    apply_login_status(status, &view, now_ms());
                    if !dorm_settings(&store).is_ok_and(|settings| settings.enabled) {
                        status.next_probe_at_ms = None;
                    }
                })
            })
            .ok_or("登录已取消")?;
        return Ok(view);
    }
    let context = observation
        .dorm
        .context
        .ok_or("门户未提供有效终端 IP，未发送登录请求")?;
    // Credentials are read only AFTER the same classification used by the monitor approves dorm login.
    let settings = dorm_settings(&store)?;
    let (card, password) =
        dormant_credentials(&app, &store, &settings).map_err(|_| "当前校园网凭据不可用")?;
    let callback = format!("dr{}", 1000 + now_ms() % 9000);
    let desktop = cfg!(target_os = "windows") || cfg!(target_os = "macos");
    let account = format!("{}{card}", if desktop { ",0," } else { ",1," });
    let terminal = if cfg!(target_os = "android") || cfg!(target_os = "linux") {
        "2"
    } else {
        "1"
    };
    let mut url = portal_url(DORM_PORTAL_HOST, 801, "/eportal/portal/login");
    url.query_pairs_mut()
        .append_pair("callback", &callback)
        .append_pair("login_method", "1")
        .append_pair("user_account", &account)
        .append_pair("user_password", &password)
        .append_pair("wlan_user_ip", &context.ip)
        .append_pair("wlan_user_ipv6", "")
        .append_pair("wlan_user_mac", "000000000000")
        .append_pair("wlan_ac_ip", "")
        .append_pair("wlan_ac_name", "")
        .append_pair("jsVersion", "4.1.3")
        .append_pair("terminal_type", terminal)
        .append_pair("lang", "zh-cn")
        .append_pair("v", &(now_ms() % 10000 + 500).to_string());
    guard
        .commit(|| {
            if (!manual && !dorm_settings(&store)?.enabled) || network_snapshot() != snapshot {
                return Err("自动连接已关闭或网络已变化，未发送登录请求".to_owned());
            }
            set_dorm_status(&app, |status| {
                status.state = DormLoginState::LoggingIn;
                status.portal_reachable = true;
            });
            Ok(())
        })
        .ok_or("登录已取消")??;
    let response = http
        .get(&url)
        .map_err(|error| transport_failure(&app, guard, &error.to_string(), true, None))?;
    if !(200..300).contains(&response.status) {
        return Err(transport_failure(
            &app,
            guard,
            "门户返回 HTTP 错误，稍后重试",
            true,
            Some(response.status),
        ));
    }
    let parsed = parse_jsonp_callback(&String::from_utf8_lossy(&response.body), &callback)
        .map_err(|_| {
            transport_failure(
                &app,
                guard,
                "门户登录响应无效，稍后重试",
                true,
                Some(response.status),
            )
        })?;
    let mut view = login_result_from_response(&parsed, response.status);
    if view.message.as_ref().is_some_and(|message| {
        message.contains(&card)
            || message.contains(&password)
            || message.contains("http://")
            || message.contains("https://")
            || message.len() > 512
    }) {
        view.message = Some(
            if view.ok {
                "校园网认证已完成"
            } else {
                "校园网认证失败，请检查凭据或登录门户"
            }
            .into(),
        );
    }
    guard
        .commit(|| {
            set_dorm_status(&app, |status| {
                apply_login_status(status, &view, now_ms());
                if !dorm_settings(&store).is_ok_and(|settings| settings.enabled) {
                    status.next_probe_at_ms = None;
                }
            });
            let message = if view.already_online {
                "当前设备已在线"
            } else if view.ok {
                "宿舍区校园网登录成功"
            } else {
                "宿舍区校园网认证失败，请检查凭据或登录门户"
            };
            if !view.ok {
                save_dorm_failure_notice(&app, message);
                dorm_notify_failure(&app, message);
            }
            append_dorm_log(
                &app,
                DormLoginLogView {
                    region: Some(NetworkRegion::Dormitory),
                    timestamp_ms: now_ms(),
                    stage: "login".into(),
                    outcome: if view.already_online {
                        "already-online"
                    } else if view.ok {
                        "success"
                    } else {
                        "business-failure"
                    }
                    .into(),
                    portal_reachable: true,
                    http_status: Some(response.status),
                    result: view.result,
                    ret_code: view.ret_code,
                    message: Some(message.into()),
                    next_probe_at_ms: store
                        .dorm_status
                        .lock()
                        .ok()
                        .and_then(|status| status.next_probe_at_ms),
                    // No gateway response or request URL is persisted, even on failure.
                    ..Default::default()
                },
            );
        })
        .ok_or("登录已取消")?;
    let observation = probe_network_overview(&http, network_snapshot(), now_ms());
    store.apply_observation(guard.token(), &observation, &AppNetworkEvents(&app));
    if view.ok {
        Ok(view)
    } else {
        Err("宿舍区校园网认证失败，请检查凭据或登录门户".into())
    }
}

pub(super) fn start_dorm_login(app: tauri::AppHandle, manual: bool) -> Result<(), String> {
    let store = app
        .try_state::<CampusNetworkStore>()
        .ok_or("校园网状态不可用")?;
    if !manual && !dorm_settings(&store)?.enabled {
        return Err("自动登录未开启".into());
    }
    let guard = store.begin_login()?;
    thread::spawn(move || {
        if let Err(error) = perform_dorm_login(app.clone(), manual, &guard) {
            guard.commit(|| {
                set_dorm_status(&app, |status| {
                    if !matches!(status.state, DormLoginState::Paused) {
                        status.state = DormLoginState::Monitoring;
                        status.last_result = Some(DormLoginResultView {
                            message: Some(error),
                            ..Default::default()
                        });
                    }
                })
            });
        }
    });
    Ok(())
}

pub(super) fn login_result_from_response(
    parsed: &serde_json::Value,
    login_status: u16,
) -> DormLoginResultView {
    let result = parsed.get("result").and_then(|v| v.as_i64());
    let ret_code = parsed.get("ret_code").and_then(|v| v.as_i64());
    let message = parsed
        .get("msg")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let success = (200..300).contains(&login_status)
        && (result == Some(1) || (result == Some(0) && ret_code == Some(2)));
    let already_online = success && result == Some(0) && ret_code == Some(2);
    DormLoginResultView {
        ok: success,
        already_online,
        message: message.clone(),
        result,
        ret_code,
        http_status: Some(login_status),
    }
}

pub(super) fn apply_login_status(
    status: &mut DormLoginStatusView,
    view: &DormLoginResultView,
    now: u64,
) {
    status.portal_reachable = true;
    status.last_result = Some(view.clone());
    if view.ok {
        status.state = DormLoginState::Online;
        status.consecutive_failures = 0;
        status.next_probe_at_ms = Some(now + 600_000);
    } else {
        status.state = DormLoginState::Paused;
        status.next_probe_at_ms = None;
    }
}

pub(super) fn apply_transport_failure(
    status: &mut DormLoginStatusView,
    portal_reachable: bool,
    now: u64,
) -> (u32, u64) {
    status.portal_reachable = portal_reachable;
    status.consecutive_failures = status.consecutive_failures.saturating_add(1);
    let next = now + backoff_delay(status.consecutive_failures).as_millis() as u64;
    status.next_probe_at_ms = Some(next);
    status.state = DormLoginState::Monitoring;
    (status.consecutive_failures, next)
}
