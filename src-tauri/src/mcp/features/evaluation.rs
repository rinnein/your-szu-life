//! 学生评教（jxpj.szu.edu.cn）MCP 工具。

use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct EvaluationRunParams {
    /// 只构造提交内容、不真正写入，默认 `true`。
    /// 必须显式传 `false` 才会提交；提交后不可撤销。
    pub dry_run: Option<bool>,
    /// 开放题回答，省略时使用应用内配置（默认「无」）。
    pub comment: Option<String>,
}

#[tool_router(router = evaluation_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "evaluation_list_pending",
        description = "获取深大教学质量综合评价平台（jxpj.szu.edu.cn）当前学期的学生评教清单：每个评价目标的待评/已评记录、题目与选项、截止日期，以及「一键评教」将采用的评价档位说明。读取会通过统一认证自动建立评教会话，结果缓存 30 分钟。对应资源 your-szu-life://data/evaluation/pending"
    )]
    pub async fn evaluation_list_pending(&self) -> Result<String, String> {
        let app = self.app.clone();
        let overview = tokio::task::spawn_blocking(move || {
            szu_enrollment::enrollment::evaluation::overview(&app, false)
                .map_err(|error| error.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;
        serde_json::to_string_pretty(&overview).map_err(|error| format!("序列化失败: {error}"))
    }

    #[tool(
        name = "evaluation_run",
        description = "对当前学期全部待评记录执行「一键评教」：每题按选项分值区间中点的次高档作答（4 档时为良好/比较清楚/比较符合/比较满意），开放题按参数或应用配置填写（默认「无」）。默认 dryRun=true 仅返回将提交的内容；dryRun=false 会真实提交且不可撤销，请先向用户确认。"
    )]
    pub async fn evaluation_run(
        &self,
        Parameters(params): Parameters<EvaluationRunParams>,
    ) -> Result<String, String> {
        let app = self.app.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            szu_enrollment::enrollment::evaluation::run(
                &app,
                params.comment.as_deref(),
                params.dry_run.unwrap_or(true),
                &mut |_| {},
            )
            .map_err(|error| error.message)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;
        serde_json::to_string_pretty(&outcome).map_err(|error| format!("序列化失败: {error}"))
    }
}
