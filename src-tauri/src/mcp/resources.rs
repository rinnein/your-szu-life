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
        Resource::new(
            "your-szu-life://data/app/announcements",
            "app_announcements",
        )
        .with_title("公告")
        .with_description("获取软件公告、电费/OJ提醒")
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
    ]
}

pub async fn read_resource_by_uri(
    app: &AppHandle,
    uri: &str,
) -> Result<ReadResourceResult, rmcp::ErrorData> {
    let clean_uri = uri.trim();
    let json_text = match clean_uri {
        "your-szu-life://data/auth/profile" => {
            if let Some(store) = app.try_state::<crate::auth::AuthStore>() {
                let user_data = crate::auth::service::get_user_data(&store).ok().flatten();
                let profile = crate::auth::service::get_account_profile(&store)
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
            if let Some(store) = app.try_state::<crate::campus_network::CampusNetworkStore>() {
                let overview = crate::campus_network::service::get_network_overview(&store).ok();
                serde_json::to_string_pretty(&overview).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/electricity/status" => {
            if let Some(store) = app.try_state::<crate::campus_payments::CampusPaymentsStore>() {
                let status = crate::campus_payments::electricity::status(&store.electricity);
                serde_json::to_string_pretty(&status).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/enrollment/timetable" => {
            if let Some(store) = app.try_state::<crate::enrollment::Store>() {
                let service = store.0.clone();
                let selected = service
                    .execute(crate::enrollment::commands::Action::Selected)
                    .ok();
                serde_json::to_string_pretty(&selected).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/uooc/courses" => {
            if let Ok(service) = crate::uooc::service(app) {
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/oj/contests" => {
            if let Ok(service) = crate::oj::service(app) {
                let snapshot = service.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/app/announcements" => {
            if let Some(store) = app.try_state::<crate::app_content::AppContentStore>() {
                let snapshot = store.snapshot();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/network/proxy" => {
            if let Some(store) = app.try_state::<crate::netproxy::ProxyStore>() {
                let view = crate::netproxy::model::view(store.credentials.as_ref()).ok();
                serde_json::to_string_pretty(&view).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/settings/snapshot" => {
            if let Some(store) = app.try_state::<crate::settings::SettingsStore>() {
                let snapshot = store.snapshot().ok();
                serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
            } else {
                "{}".to_string()
            }
        }
        "your-szu-life://data/onboarding/status" => match crate::onboarding::state_view() {
            Ok(state) => {
                serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".to_string())
            }
            Err(_) => "{}".to_string(),
        },
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
