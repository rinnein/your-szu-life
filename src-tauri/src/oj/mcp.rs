use crate::mcp::service::UnifiedSzuMcpService;
use crate::oj::{service, SubmitInput};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SiteParam {
    /// 目标 OJ 站点 ID（可在 oj_get_snapshot 中获取）
    pub site_id: String,
    /// 目标班级/实验组 ID（可选，若不填则自动查询该站点下所有分组的所有比赛）
    pub group_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContestPaperParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 目标比赛/作业/考核 ID
    pub contest_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SubmissionRecordsParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 比赛 ID
    pub contest_id: String,
    /// 题型类型（0: 编程题, 1: 单选题, 2: 多选题, 3: 问答题, 4: 填空题）
    pub kind: u8,
    /// 题目 ID（可选，过滤特定题目）
    pub problem_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SubmissionResultParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 提交记录编号（submission id）
    pub submission_id: String,
    /// 题型类型（0: 编程题, 1: 单选, 2: 多选, 3: 问答, 4: 填空）
    pub kind: u8,
    /// 题目 ID（可选）
    pub problem_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SubmitCodeParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 比赛 ID
    pub contest_id: String,
    /// 题目 Key / ID
    pub problem_key: String,
    /// 提交的源代码
    pub source_code: String,
    /// 编程语言（如 "C", "C++", "Python3", "Java" 等，如不填使用默认语言）
    pub language: Option<String>,
}

#[tool_router(router = oj_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "oj_get_snapshot",
        description = "获取深大 OJ 站点配置列表、账户绑定状态及全局同步快照"
    )]
    pub async fn oj_get_snapshot(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        let snapshot = service.snapshot();
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_contests_and_groups",
        description = "查询指定 OJ 站点中已加入的班级课程分组、日常练习题单与考核比赛列表"
    )]
    pub async fn oj_get_contests_and_groups(
        &self,
        Parameters(params): Parameters<SiteParam>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let groups = service.groups(&params.site_id).await?;
        let contests = if let Some(gid) = params.group_id {
            service.contests(&params.site_id, &gid).await?
        } else {
            let mut all = Vec::new();
            for g in &groups {
                if let Ok(c) = service.contests(&params.site_id, &g.id).await {
                    all.extend(c);
                }
            }
            all
        };
        serde_json::to_string_pretty(&serde_json::json!({
            "groups": groups,
            "contests": contests,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_paper",
        description = "获取指定比赛、考核或作业的完整试卷与题面详情（题目列表、分值、限制条件）"
    )]
    pub async fn oj_get_paper(
        &self,
        Parameters(params): Parameters<ContestPaperParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let paper = service.paper(&params.site_id, &params.contest_id).await?;
        serde_json::to_string_pretty(&paper).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_records",
        description = "获取指定比赛或题目的最近代码提交记录列表"
    )]
    pub async fn oj_get_records(
        &self,
        Parameters(params): Parameters<SubmissionRecordsParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let records = service
            .records(
                &params.site_id,
                &params.contest_id,
                params.kind,
                params.problem_id.as_deref(),
            )
            .await?;
        serde_json::to_string_pretty(&records).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_result",
        description = "查询某次代码提交的实时判题结果状态（AC、WA、TLE、CE 等）及错误详情与运行指标"
    )]
    pub async fn oj_get_result(
        &self,
        Parameters(params): Parameters<SubmissionResultParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .result(
                &params.site_id,
                &params.submission_id,
                params.kind,
                params.problem_id.as_deref(),
            )
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_submit_code",
        description = "向深大 OJ 提交指定题目的源代码进行在线评测"
    )]
    pub async fn oj_submit_code(
        &self,
        Parameters(params): Parameters<SubmitCodeParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let (account, epoch) = {
            let r = service.runtime.lock().unwrap();
            let e = r.entries.get(&params.site_id).ok_or("OJ 站点未配置")?;
            (e.username.clone(), e.epoch)
        };

        let input = SubmitInput {
            site_id: params.site_id,
            contest_id: params.contest_id,
            problem_key: Some(params.problem_key),
            account,
            epoch,
        };

        let result = service
            .submit_with_code(input, Some(params.source_code))
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }
}
