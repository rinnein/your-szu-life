use crate::mcp::service::UnifiedSzuMcpService;
use crate::uooc::{quiz::model::Question, service};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct CourseIdsParams {
    /// 目标课程 ID 列表（留空表示所有当前正在修读的课程）
    pub course_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct CourseIdParam {
    /// 目标课程 ID（留空表示查询全部课程）
    pub course_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchQaParams {
    /// 检索题目的关键词或完整题干
    pub query: String,
    /// 自定义题库服务器地址（可选，默认使用客户端配置）
    pub server_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct SolveQuestionParams {
    /// 题目列表，包含题干、选项等完整信息
    pub questions: Vec<Question>,
}

#[tool_router(router = uooc_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "uooc_get_courses",
        description = "获取当前学期正在修读的全部 UOOC 优课联盟慕课列表，以及各小节与视频的综合完成度百分比"
    )]
    pub async fn uooc_get_courses(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        let snapshot = service.snapshot();
        serde_json::to_string_pretty(&snapshot.courses).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "uooc_refresh_progress",
        description = "强制从 UOOC 优课联盟官方服务器同步指定课程或全部课程的最新章节完成进度"
    )]
    pub async fn uooc_refresh_progress(
        &self,
        Parameters(params): Parameters<CourseIdsParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let ids: Vec<String> = if let Some(ids) = params.course_ids {
            ids
        } else {
            service
                .snapshot()
                .courses
                .into_iter()
                .map(|c| c.id)
                .collect()
        };
        service.refresh_progress(ids).await;
        Ok("已刷新课程进度".to_string())
    }

    #[tool(
        name = "uooc_checkin",
        description = "对在修的 UOOC 慕课执行自动日常打卡与考勤签到任务"
    )]
    pub async fn uooc_checkin(
        &self,
        Parameters(params): Parameters<CourseIdsParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let ids = params.course_ids.unwrap_or_default();
        service.enqueue_checkins(ids, crate::uooc::checkin::CheckinSource::Manual)?;
        Ok("已启动签到任务".to_string())
    }

    #[tool(
        name = "uooc_list_quizzes_and_exams",
        description = "查询指定课程或全部在修课程的测验（Quizzes）、平时课后作业（Homeworks）与期末考试列表"
    )]
    pub async fn uooc_list_quizzes_and_exams(
        &self,
        Parameters(params): Parameters<CourseIdParam>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let cid = params.course_id.as_deref();
        let quizzes = service.list_quizzes(cid).await?;
        let homeworks = service.list_homeworks(cid).await?;
        serde_json::to_string_pretty(&serde_json::json!({
            "quizzes": quizzes,
            "homeworks": homeworks,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "uooc_search_qa_bank",
        description = "基于题干文本或关键词，通过本地向量嵌入（BGE-small-zh）检索历史题库，匹配标准答案"
    )]
    pub async fn uooc_search_qa_bank(
        &self,
        Parameters(params): Parameters<SearchQaParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let cache_path = service.path.parent().unwrap().join("qa-vector-cache.json");
        let bank_path = service.path.parent().unwrap().join("qa-bank.json");
        let url = params
            .server_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| service.snapshot().settings.qa_server_url.clone());
        let api_key = service.qa_api_key_for_server(Some(&url)).await;

        let results = crate::uooc::quiz::qa::search_questions_with_local_embedding(
            &cache_path,
            &bank_path,
            &url,
            api_key.as_deref(),
            &params.query,
            20,
        )
        .await?;

        serde_json::to_string_pretty(&results).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "uooc_solve_question_with_ai",
        description = "结合 UOOC 题库 RAG 知识检索与配置的 AI 大模型推理，自动解答试卷题目并生成解析"
    )]
    pub async fn uooc_solve_question_with_ai(
        &self,
        Parameters(params): Parameters<SolveQuestionParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let (answers, reviews) = service.solve_paper_ai(params.questions).await?;
        serde_json::to_string_pretty(&serde_json::json!({
            "answers": answers,
            "reviews": reviews,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }
}
