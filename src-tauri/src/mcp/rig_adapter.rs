use rig_core::tool::DynamicTool;
use rig_rmcp::{McpConnection, McpTool};
use rmcp::model::Tool;

/// 将 MCP 暴露的工具列表转换为 rig-core 的 DynamicTool 动态工具列表，
/// 可直接注入到应用内置智能体（如 UOOC AI 助教、OJ Companion 等）中执行。
///
/// `rig-rmcp` 0.47 起不再接受裸的 `ServerSink`：工具必须绑定一条
/// [`McpConnection`]（由内置智能体连接本地 MCP 服务后持有），并在工具存活期间
/// 保持该连接打开。
pub fn convert_mcp_tools_for_rig(
    tools: impl IntoIterator<Item = Tool>,
    connection: &McpConnection,
) -> Vec<DynamicTool> {
    tools
        .into_iter()
        .filter_map(|tool| DynamicTool::try_from(McpTool::new(tool, connection)).ok())
        .collect()
}
