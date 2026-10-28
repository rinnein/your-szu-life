use super::resources;
use super::service::UnifiedSzuMcpService;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::tool_handler;

impl UnifiedSzuMcpService {
    pub fn all_tools() -> ToolRouter<Self> {
        Self::auth_tools()
            + Self::network_tools()
            + Self::electricity_tools()
            + Self::enrollment_tools()
            + Self::uooc_tools()
            + Self::oj_tools()
            + Self::ocr_tools()
            + Self::embedding_tools()
            + Self::app_content_tools()
            + Self::settings_tools()
            + Self::appearance_tools()
            + Self::notification_tools()
            + Self::netproxy_tools()
            + Self::ui_tools()
    }
}

#[tool_handler(router = (Self::all_tools()))]
impl ServerHandler for UnifiedSzuMcpService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("your-szu-life", env!("CARGO_PKG_VERSION")))
        .with_instructions(
            "深圳大学生活助手（your-szu-life）本地公开 MCP 服务。提供深大统一认证、校园网认证、宿舍电费、选课与教务成绩、UOOC慕课答题、SZU OJ 在线评测、OCR 识别与页面直达能力。",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            resources: resources::list_available_resources(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        resources::read_resource_by_uri(&self.app, &request.uri).await
    }
}
