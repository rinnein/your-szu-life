use crate::mcp::service::UnifiedSzuMcpService;
use std::collections::BTreeMap;
use szu_oj::oj::model::{
    ReviewCommentInput, ReviewScores, SaveReviewInput, SubmitInput,
};
use szu_oj::oj::service;
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

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManagedContestsParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageContestParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 可管理的测验 ID
    pub contest_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReviewListParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 测验 ID
    pub contest_id: String,
    /// 按用户名/姓名搜索（可选）
    pub search: Option<String>,
    /// 分页偏移，默认 0
    pub offset: Option<u32>,
    /// 每页数量，默认 20，最大 100
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ReviewDetailParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 测验 ID
    pub contest_id: String,
    /// 学生答卷记录 ID（manage/contest-submission 的 id）
    pub submission_id: String,
}

#[derive(Debug, Default, Deserialize, Serialize, JsonSchema)]
pub struct ReviewScoresParam {
    /// 编程题分数：题目 ID -> 提交 ID -> 分数
    #[serde(default)]
    pub coding: BTreeMap<String, BTreeMap<String, f64>>,
    /// 代码补全题分数：题目 ID -> 提交 ID -> 分数
    #[serde(default)]
    pub code_fill: BTreeMap<String, BTreeMap<String, f64>>,
    /// 问答题分数：题目 ID -> 提交 ID -> 分数
    #[serde(default)]
    pub qa: BTreeMap<String, BTreeMap<String, f64>>,
    /// 选择题分数：题目 ID -> 分数
    #[serde(default)]
    pub choice: BTreeMap<String, f64>,
    /// 填空题分数：题目 ID -> 分数
    #[serde(default)]
    pub fill_in: BTreeMap<String, f64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SaveReviewParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 测验 ID
    pub contest_id: String,
    /// 学生答卷记录 ID
    pub submission_id: String,
    /// 各题型分数；保存成功后服务端会把该答卷标记为已阅
    pub scores: ReviewScoresParam,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SaveReviewCommentParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 单题提交 ID
    pub submission_id: String,
    /// 题型（0 编程题 / 4 代码补全 / 3 问答题）
    pub kind: u8,
    /// 教师评语
    pub comment: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ProblemSubmissionsParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 测验 ID
    pub contest_id: String,
    /// 题目 ID
    pub problem_id: String,
    /// 题型（0 编程题 / 4 代码补全）
    pub kind: u8,
    /// 按用户名搜索（可选）
    pub search: Option<String>,
    /// 按判题状态过滤（可选，如 "0" 表示 AC）
    pub status: Option<String>,
    /// 分页偏移，默认 0
    pub offset: Option<u32>,
    /// 每页数量，默认 20，最大 100
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TaSubmissionParams {
    /// 目标 OJ 站点 ID
    pub site_id: String,
    /// 测验 ID
    pub contest_id: String,
    /// 题目 ID
    pub problem_id: String,
    /// 题型（0 编程题 / 4 代码补全）
    pub kind: u8,
    /// 单题提交 ID
    pub submission_id: String,
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
        let groups = service.groups(&params.site_id, false).await?;
        let contests = if let Some(gid) = params.group_id {
            service.contests(&params.site_id, &gid, false).await?
        } else {
            let mut all = Vec::new();
            for g in &groups {
                if let Ok(c) = service.contests(&params.site_id, &g.id, false).await {
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

    #[tool(
        name = "oj_get_managed_contests",
        description = "以助教/教师身份列出当前账号可管理的测验（不读学生班级，不进入首页待办）"
    )]
    pub async fn oj_get_managed_contests(
        &self,
        Parameters(params): Parameters<ManagedContestsParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service.managed_contests(&params.site_id).await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_manage_contest",
        description = "读取可管理测验的题目列表、分值、时间与机评状态"
    )]
    pub async fn oj_get_manage_contest(
        &self,
        Parameters(params): Parameters<ManageContestParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .manage_contest(&params.site_id, &params.contest_id)
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_review_list",
        description = "读取某测验的学生答卷批阅列表，包含总分、是否机评、是否已阅"
    )]
    pub async fn oj_get_review_list(
        &self,
        Parameters(params): Parameters<ReviewListParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .review_list(
                &params.site_id,
                &params.contest_id,
                params.search.as_deref(),
                params.offset.unwrap_or(0),
                params.limit.unwrap_or(20),
            )
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_review_detail",
        description = "读取单个学生的完整答卷：各题答案/代码、参考答案、判题结果、现有分数与评语"
    )]
    pub async fn oj_get_review_detail(
        &self,
        Parameters(params): Parameters<ReviewDetailParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .review_detail(&params.site_id, &params.contest_id, &params.submission_id)
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_save_review",
        description = "保存学生答卷分数并标记已阅。这是写操作，会修改真实成绩；uncertain=true 时必须刷新确认，不能重发"
    )]
    pub async fn oj_save_review(
        &self,
        Parameters(params): Parameters<SaveReviewParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let scores = ReviewScores {
            coding: params.scores.coding,
            code_fill: params.scores.code_fill,
            qa: params.scores.qa,
            choice: params.scores.choice,
            fill_in: params.scores.fill_in,
        };
        let result = service
            .save_review(SaveReviewInput {
                site_id: params.site_id,
                contest_id: params.contest_id,
                submission_id: params.submission_id,
                scores,
            })
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_save_review_comment",
        description = "保存单题教师评语（编程/代码补全/问答题）。写操作；uncertain=true 时必须刷新确认"
    )]
    pub async fn oj_save_review_comment(
        &self,
        Parameters(params): Parameters<SaveReviewCommentParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .save_review_comment(ReviewCommentInput {
                site_id: params.site_id,
                submission_id: params.submission_id,
                kind: params.kind,
                comment: params.comment,
            })
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_problem_submissions",
        description = "读取某道编程/代码补全题的全班提交列表（由服务端按角色过滤）"
    )]
    pub async fn oj_get_problem_submissions(
        &self,
        Parameters(params): Parameters<ProblemSubmissionsParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .problem_submissions(
                &params.site_id,
                &params.contest_id,
                &params.problem_id,
                params.kind,
                params.search.as_deref(),
                params.status.as_deref(),
                params.offset.unwrap_or(0),
                params.limit.unwrap_or(20),
            )
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "oj_get_ta_submission",
        description = "读取单份编程/代码补全提交的代码、判题结果、实际输出、期望输出与样例输出"
    )]
    pub async fn oj_get_ta_submission(
        &self,
        Parameters(params): Parameters<TaSubmissionParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let result = service
            .ta_submission(
                &params.site_id,
                &params.contest_id,
                &params.problem_id,
                params.kind,
                &params.submission_id,
            )
            .await?;
        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }
}
