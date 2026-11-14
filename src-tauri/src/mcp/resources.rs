use rmcp::model::{ReadResourceResult, Resource, ResourceContents};
use tauri::{AppHandle, Manager};

pub fn list_available_resources() -> Vec<Resource> {
    vec![
        Resource::new("your-szu-life://data/auth/profile", "auth_profile")
            .with_title("当前统一身份认证账户资料")
            .with_description("返回当前登录深大学生的详细个人资料（学号、姓名、学院、卡号等）")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/network/status", "network_status")
            .with_title("校园网实时网络状态")
            .with_description(
                "返回当前设备 IP 地址、网关以及所在校园区域（宿舍区/教学办公区/校外）",
            )
            .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/electricity/status",
            "electricity_status",
        )
        .with_title("宿舍电费当前状态")
        .with_description("返回配置绑定的宿舍最新电量指标、剩余电费、告警阈值")
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/enrollment/timetable",
            "enrollment_timetable",
        )
        .with_title("本学期个人课表数据")
        .with_description("返回教务系统本学期已选课程明细与排课作息表")
        .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/uooc/courses", "uooc_courses")
            .with_title("UOOC 慕课学习进度")
            .with_description("返回优课联盟在修课程列表与综合小节学习完成进度百分比")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/oj/contests", "oj_contests")
            .with_title("SZU OJ 比赛与作业快照")
            .with_description("返回当前配置绑定的各 OJ 站点及关联实验比赛概览")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/lms/todos", "lms_todos")
            .with_title("畅课课程待办列表")
            .with_description(
                "返回畅课（lms.szu.edu.cn）当前课程待办（作业等）及其截止时间",
            )
            .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/jxxk/semesters",
            "jxxk_semesters",
        )
        .with_title("督导巡课学期与课程概览")
        .with_description(
            "返回督导巡课平台（jxxk.szu.edu.cn）的学年学期列表与会话状态",
        )
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/lms/notifications",
            "lms_notifications",
        )
        .with_title("畅课站内通知")
        .with_description("返回畅课最近站内通知（作业截止提醒、资料发布等）与未读数")
        .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/signin/pending", "signin_pending")
            .with_title("待签到列表")
            .with_description(
                "返回当前待签到的畅课点名列表与签到模块缓存视图（含 rollcall_id、签到方式与状态）",
            )
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/signin/timetable", "signin_timetable")
            .with_title("签到课表缓存")
            .with_description(
                "返回签到模块缓存的 SZU 考勤课表（课次时间、楼栋/教室与解析出的楼栋坐标）",
            )
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/todos", "todos")
            .with_title("首页待办聚合")
            .with_description(
                "返回首页日历聚合后的全部待办：OJ 测验、UOOC 考试/作业/结课、畅课作业、电费预估断电",
            )
            .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/todos/notification-settings",
            "todos_notification_settings",
        )
        .with_title("待办提醒配置")
        .with_description("返回待办提醒六个注册项开关与提前量列表（分钟）")
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/notification/settings",
            "notification_settings",
        )
        .with_title("通知设置")
        .with_description("返回全局通知总开关、各业务分类开关与待办提醒配置")
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/app/announcements",
            "app_announcements",
        )
        .with_title("公告")
        .with_description("获取软件公告、电费/OJ提醒")
        .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/app/update", "app_update")
            .with_title("更新状态与频道")
            .with_description(
                "返回当前版本、更新频道（stable/canary）、稳定版是否回退到测试版、是否有新版本与最新版本信息",
            )
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/network/proxy", "network_proxy")
            .with_title("全局网络代理设置")
            .with_description(
                "返回代理模式、协议、服务器地址与校内网开关（不含代理密码）",
            )
            .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/settings/snapshot",
            "settings_snapshot",
        )
        .with_title("客户端全局配置快照")
        .with_description("返回your-szu-life当前非敏感配置项集合")
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/onboarding/status",
            "onboarding_status",
        )
        .with_title("初始化设置进度")
        .with_description("返回初始化设置的步骤顺序、完成/跳过状态与下一步")
        .with_mime_type("application/json"),
        Resource::new(
            "your-szu-life://data/file-manager/state",
            "file_manager_state",
        )
        .with_title("统一文件保存目录")
        .with_description(
            "返回平台默认目录、用户已选目录、各业务模块（如 jxxk 录像）的最终落盘路径与 Android 文件夹授权状态",
        )
        .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/mesh/status", "mesh_status")
            .with_title("设备同步网络状态")
            .with_description("返回设备同步开关、节点是否运行、本机虚拟 IP、设备名与最近错误")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/mesh/devices", "mesh_devices")
            .with_title("同步网络内的设备列表")
            .with_description("返回同一账号自组网内的设备名称、虚拟 IP、在线状态与校园网检测结果")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/mesh/campus-access", "mesh_campus_access")
            .with_title("校园网访问状态")
            .with_description("返回校园网访问开关、本机是否在校内、当前出口设备与各设备的校内检测结果")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/sync/status", "sync_status")
            .with_title("设置同步状态")
            .with_description("返回同步开关、最近一次同步的对端数量、应用/推送条数与冲突数")
            .with_mime_type("application/json"),
        Resource::new("your-szu-life://data/sync/items", "sync_items")
            .with_title("可同步项状态")
            .with_description("返回白名单内的可同步项及其状态（已同步 / 等待对端 / 冲突冻结）")
            .with_mime_type("application/json"),
    ]
}

pub async fn read_resource_by_uri(
    app: &AppHandle,
    uri: &str,
) -> Result<ReadResourceResult, rmcp::ErrorData> {
    let clean_uri = uri.trim();
    let json_text = match clean_uri {
        "your-szu-life://data/auth/profile" => {
            if let Some(store) = app.try_state::<szu_core::auth::AuthStore>() {
                let user_data = szu_core::auth::service::get_user_data(&store).ok().flatten();
                let profile = szu_core::auth::service::get_account_profile(&store)
                    .ok()
                    .flatten();
                serde_json::to_string_pretty(&serde_json::json!({
                    "profile": profile,
                    "userData": user_data,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/network/status" => {
            if let Some(store) = app.try_state::<szu_core::campus_network::CampusNetworkStore>() {
                let overview = szu_core::campus_network::service::get_network_overview(&store).ok();
                serde_json::to_string_pretty(&overview).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/electricity/status" => {
            if let Some(store) = app.try_state::<szu_core::campus_payments::CampusPaymentsStore>() {
                let status = szu_core::campus_payments::electricity::status(&store.electricity);
                serde_json::to_string_pretty(&status).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/enrollment/timetable" => {
            if let Some(store) = app.try_state::<szu_enrollment::enrollment::Store>() {
                let service = store.0.clone();
                let selected = service
                    .execute(szu_enrollment::enrollment::commands::Action::Selected)
                    .ok();
                serde_json::to_string_pretty(&selected).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/uooc/courses" => {
            if let Ok(service) = szu_uooc::uooc::service(app) {
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/oj/contests" => {
            if let Ok(service) = szu_oj::oj::service(app) {
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/lms/todos" => {
            if let Ok(service) = szu_lms::lms::service(app) {
                service.ensure(false).await;
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&serde_json::json!({
                    "session": snapshot.session,
                    "profile": snapshot.profile,
                    "todos": snapshot.todos,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/lms/notifications" => {
            if let Ok(service) = szu_lms::lms::service(app) {
                service.ensure(false).await;
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&serde_json::json!({
                    "session": snapshot.session,
                    "unread_count": snapshot.unread_count,
                    "notifications": snapshot.notifications,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/signin/pending" => {
            if let Ok(service) = szu_signin::signin::service(app) {
                if let Ok(lms) = service.lms_service() {
                    if let Ok(rollcalls) = lms.rollcall_list(false).await {
                        serde_json::to_string_pretty(&serde_json::json!({
                            "rollcalls": rollcalls,
                            "pending_view": service.checkin_view(),
                        }))
                        .unwrap_or_else(|_| "{}".to_string())
                    } else {
                        "{}".to_string()
                    }
                } else {
                    "{}".to_string()
                }
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/signin/timetable" => {
            if let Ok(service) = szu_signin::signin::service(app) {
                serde_json::to_string_pretty(&service.timetable_snapshot())
                    .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/todos" => {
            let items = szu_todos::todos::aggregate::aggregate(app).items;
            serde_json::to_string_pretty(&items).unwrap_or_else(|_| "[]".to_string())
        }
        "your-szu-life://data/todos/notification-settings" => {
            let settings = szu_core::notification::settings::load(app);
            serde_json::to_string_pretty(&settings.todo).unwrap_or_else(|_| "{}".to_string())
        }
        "your-szu-life://data/notification/settings" => {
            let settings = szu_core::notification::settings::load(app);
            serde_json::to_string_pretty(&settings).unwrap_or_else(|_| "{}".to_string())
        }
        "your-szu-life://data/jxxk/semesters" => {
            if let Ok(service) = szu_jxxk::jxxk::service(app) {
                service.ensure(false).await;
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&serde_json::json!({
                    "session": snapshot.session,
                    "message": snapshot.message,
                    "semesters": snapshot.semesters,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/app/announcements" => {
            if let Some(store) = app.try_state::<szu_app_content::app_content::AppContentStore>() {
                let snapshot = store.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/app/update" => {
            if let Some(store) = app.try_state::<szu_app_content::app_content::AppContentStore>() {
                let snapshot = store.snapshot();
                serde_json::to_string_pretty(&serde_json::json!({
                    "currentVersion": snapshot.version,
                    "updateChannel": snapshot.update_channel,
                    "updateChannelFallback": snapshot.update_channel_fallback,
                    "updateAvailable": snapshot.update_available,
                    "update": snapshot.update,
                }))
                .unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/network/proxy" => {
            if let Some(store) = app.try_state::<szu_core::netproxy::ProxyStore>() {
                let view = szu_core::netproxy::model::view(store.credentials.as_ref()).ok();
                serde_json::to_string_pretty(&view).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/settings/snapshot" => {
            if let Some(store) = app.try_state::<szu_core::settings::SettingsStore>() {
                let snapshot = store.snapshot().ok();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/onboarding/status" => match szu_core::onboarding::state_view() {
            Ok(state) => serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".to_string()),
            Err(_) => "{}".to_string(),
        },
        "your-szu-life://data/file-manager/state" => match szu_download::file_manager::state(app) {
            Ok(state) => serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".to_string()),
            Err(_) => "{}".to_string(),
        },
        "your-szu-life://data/mesh/status" => {
            match app.try_state::<std::sync::Arc<szu_mesh::mesh::MeshStore>>() {
                Some(store) => serde_json::to_string_pretty(&szu_mesh::mesh::status(&store))
                    .unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            }
        }
        "your-szu-life://data/mesh/devices" => {
            match app.try_state::<std::sync::Arc<szu_mesh::mesh::MeshStore>>() {
                Some(store) => serde_json::to_string_pretty(&store.devices())
                    .unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            }
        }
        "your-szu-life://data/mesh/campus-access" => {
            match app.try_state::<std::sync::Arc<szu_mesh::mesh::MeshStore>>() {
                Some(store) => {
                    let devices = store.devices();
                    let self_device = devices.iter().find(|device| device.is_self);
                    let campus_devices: Vec<_> = devices
                        .iter()
                        .filter(|device| !device.is_self && device.reachable && device.on_campus)
                        .map(|device| {
                            serde_json::json!({
                                "deviceId": device.device_id,
                                "name": device.name,
                                "overlayIp": device.overlay_ip,
                                "campusAccess": device.campus_access,
                            })
                        })
                        .collect();
                    serde_json::to_string_pretty(&serde_json::json!({
                        "enabled": szu_mesh::mesh::campus_access_enabled(),
                        "running": szu_mesh::mesh::is_running(app),
                        "localOnCampus": self_device.is_some_and(|device| device.on_campus),
                        "egress": store.campus_peer().map(|ip| ip.to_string()),
                        "campusDevices": campus_devices,
                    }))
                    .unwrap_or_else(|_| "{}".to_string())
                }
                None => "{}".to_string(),
            }
        }
        "your-szu-life://data/sync/status" => {
            match app.try_state::<std::sync::Arc<szu_mesh::settings_sync::engine::SyncStore>>() {
                Some(store) => {
                    let view = szu_mesh::settings_sync::status(app, &store).await;
                    serde_json::to_string_pretty(&view).unwrap_or_else(|_| "{}".to_string())
                }
                None => "{}".to_string(),
            }
        }
        "your-szu-life://data/sync/items" => {
            match app.try_state::<std::sync::Arc<szu_mesh::settings_sync::engine::SyncStore>>() {
                Some(store) => serde_json::to_string_pretty(&store.item_views(app).await)
                    .unwrap_or_else(|_| "{}".to_string()),
                None => "{}".to_string(),
            }
        }
        _ => {
            return Err(rmcp::ErrorData::new(
                rmcp::model::ErrorCode::INVALID_REQUEST,
                format!("未找到指定的资源: {clean_uri}"),
                None,
            ));
        }
    };

    Ok(ReadResourceResult::new(vec![ResourceContents::text(
        clean_uri, json_text,
    )]))
}
