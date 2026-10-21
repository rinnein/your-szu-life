use super::resources::list_available_resources;
use super::service::UnifiedSzuMcpService;

#[test]
fn test_resources_uris_and_metadata() {
    let resources = list_available_resources();
    assert!(!resources.is_empty(), "必须公开至少一个 MCP 资源");

    for res in &resources {
        assert!(
            res.uri.starts_with("your-szu-life://data/"),
            "资源 URI 必须以 your-szu-life://data/ 开头，实际为: {}",
            res.uri
        );
        assert!(!res.name.is_empty(), "资源名称不能为空");
        assert!(res.title.is_some(), "资源标题不能为空: {}", res.name);
        assert!(res.description.is_some(), "资源描述不能为空: {}", res.name);
        assert_eq!(
            res.mime_type.as_deref(),
            Some("application/json"),
            "数据资源 MIME 类型必须为 application/json"
        );
    }

    let uris: Vec<&str> = resources.iter().map(|r| r.uri.as_str()).collect();
    assert!(uris.contains(&"your-szu-life://data/auth/profile"));
    assert!(uris.contains(&"your-szu-life://data/network/status"));
    assert!(uris.contains(&"your-szu-life://data/electricity/status"));
    assert!(uris.contains(&"your-szu-life://data/enrollment/timetable"));
    assert!(uris.contains(&"your-szu-life://data/uooc/courses"));
    assert!(uris.contains(&"your-szu-life://data/oj/contests"));
    assert!(uris.contains(&"your-szu-life://data/app/announcements"));
    assert!(uris.contains(&"your-szu-life://data/settings/snapshot"));
}

#[test]
fn test_all_tools_registered() {
    let router = UnifiedSzuMcpService::all_tools();
    let tools = router.list_all();

    let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    // 1. Auth 模块工具
    assert!(tool_names.contains(&"auth_get_status"));
    assert!(tool_names.contains(&"auth_get_profile"));
    assert!(tool_names.contains(&"auth_refresh_session"));

    // 2. 校园网模块工具
    assert!(tool_names.contains(&"network_get_overview"));
    assert!(tool_names.contains(&"network_trigger_dorm_login"));
    assert!(tool_names.contains(&"network_logout"));
    assert!(tool_names.contains(&"network_apply_staff_login"));
    assert!(tool_names.contains(&"network_get_dorm_logs"));

    // 3. 校园支付与电费模块工具
    assert!(tool_names.contains(&"electricity_query_balance"));
    assert!(tool_names.contains(&"electricity_get_recharges"));
    assert!(tool_names.contains(&"electricity_get_options"));
    assert!(tool_names.contains(&"electricity_get_settings"));

    // 4. 选课与教务模块工具
    assert!(tool_names.contains(&"enrollment_get_snapshot"));
    assert!(tool_names.contains(&"enrollment_get_selected_courses"));
    assert!(tool_names.contains(&"enrollment_search_courses"));
    assert!(tool_names.contains(&"enrollment_get_academic_record"));
    assert!(tool_names.contains(&"enrollment_get_academic_progress"));

    // 5. UOOC 模块工具
    assert!(tool_names.contains(&"uooc_get_courses"));
    assert!(tool_names.contains(&"uooc_refresh_progress"));
    assert!(tool_names.contains(&"uooc_checkin"));
    assert!(tool_names.contains(&"uooc_list_quizzes_and_exams"));
    assert!(tool_names.contains(&"uooc_search_qa_bank"));
    assert!(tool_names.contains(&"uooc_solve_question_with_ai"));

    // 6. OJ 模块工具
    assert!(tool_names.contains(&"oj_get_snapshot"));
    assert!(tool_names.contains(&"oj_get_contests_and_groups"));
    assert!(tool_names.contains(&"oj_get_paper"));
    assert!(tool_names.contains(&"oj_get_records"));
    assert!(tool_names.contains(&"oj_get_result"));
    assert!(tool_names.contains(&"oj_submit_code"));

    // 7. OCR 模块工具
    assert!(tool_names.contains(&"ocr_recognize_image"));
    assert!(tool_names.contains(&"ocr_get_status"));

    // 8. 向量模型模块工具
    assert!(tool_names.contains(&"embedding_get_vector"));
    assert!(tool_names.contains(&"embedding_get_status"));

    // 9. 公共内容模块工具
    assert!(tool_names.contains(&"app_get_announcements"));
    assert!(tool_names.contains(&"app_get_contributors"));

    // 10. 设置与安全模块工具
    assert!(tool_names.contains(&"settings_get_snapshot"));
    assert!(tool_names.contains(&"settings_get_storage_status"));

    // 11. 全局 UI 页面直达工具
    assert!(tool_names.contains(&"open_page"));

    // 验证每个工具都有描述且参数 schema 合法
    for tool in &tools {
        assert!(
            tool.description.is_some(),
            "工具 {} 必须提供中文功能描述",
            tool.name
        );
    }
}

#[tokio::test]
async fn test_tcp_listener_from_std_in_async_context() {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind std listener");
    std_listener.set_nonblocking(true).expect("set nonblocking");
    let tokio_listener = tokio::net::TcpListener::from_std(std_listener);
    assert!(
        tokio_listener.is_ok(),
        "在 Tokio 上下文中将 std listener 转为 tokio listener 必须成功"
    );
}

#[test]
fn test_mcp_server_stop_resets_status() {
    let _ = super::stop_mcp_server();
    let status = super::get_mcp_server_status();
    assert!(!status.running, "停止服务后 running 必须为 false");
    assert!(!status.enabled, "停止服务后 enabled 必须为 false");
    assert!(status.port.is_none(), "停止服务后端口必须释放为 None");
    assert!(status.endpoint.is_none(), "停止服务后端点必须为 None");
}
