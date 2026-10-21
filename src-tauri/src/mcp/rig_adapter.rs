use rig_core::tool::DynamicTool;
use rig_rmcp::{tools_from_server, McpTool};
use rmcp::model::Tool;
use rmcp::service::ServerSink;

/// 将 MCP 暴露的工具列表转换为 rig-core 的 DynamicTool 动态工具列表，
/// 可直接注入到应用内置智能体（如 UOOC AI 助教、OJ Companion 等）中执行。
pub fn convert_mcp_tools_for_rig(
    tools: impl IntoIterator<Item = Tool>,
    sink: &ServerSink,
) -> Vec<DynamicTool> {
    let mcp_tools: Vec<McpTool> = tools_from_server(tools, sink);
    mcp_tools
        .into_iter()
        .filter_map(|tool| DynamicTool::try_from(tool).ok())
        .collect()
}
