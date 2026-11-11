use szu_jxxk::jxxk::service;
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct JxxkCourseListParams {
    /// 学年学期 id（来自 jxxk_list_semesters），留空表示全部学期
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_semester_id: Option<String>,
    /// 课程名/教师/班级搜索关键词，留空表示不过滤
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_key: Option<String>,
    /// 页码，从 1 开始，默认 1
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_index: Option<u32>,
    /// 每页条数，默认 100
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct JxxkLessonListParams {
    /// 课程 id（来自 jxxk_list_courses 的 id 字段）
    pub course_id: String,
    /// 页码，从 1 开始，默认 1
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_index: Option<u32>,
    /// 每页条数，默认 100
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct JxxkLiveIdParams {
    /// 直播/课次 id（来自 jxxk_list_lessons 的 live_id 字段）
    pub live_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct JxxkMediaVideoIdParams {
    /// 媒体视频 id（来自 jxxk_list_lessons 的 media_video_id 或 jxxk_get_lesson_streams 的 media_video_id 字段）
    pub media_video_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct JxxkTaskIdParams {
    /// 下载任务 id（来自 jxxk_list_downloads 的 id 字段）
    pub task_id: String,
}

#[tool_router(router = jxxk_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "jxxk_list_semesters",
        description = "获取督导巡课平台（jxxk.szu.edu.cn）的学年学期列表，包含学期 id 与名称（如 2026-2027-1），供课程列表过滤使用。统一认证会话在 Android 上支持原生 Cookie API 缺失时的 CDP 降级；传输失败保留已有会话，不自动提交密码"
    )]
    pub async fn jxxk_list_semesters(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        service.ensure(false).await;
        let snapshot = service.snapshot();
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_list_courses",
        description = "获取督导巡课平台当前用户可访问的课程列表，包含课程名、教师、教学班级、所属学院与课次数"
    )]
    pub async fn jxxk_list_courses(
        &self,
        Parameters(params): Parameters<JxxkCourseListParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let page = service
            .list_courses(
                params.year_semester_id.as_deref(),
                params.search_key.as_deref(),
                params.page_index.unwrap_or(1),
                params.page_size.unwrap_or(100),
            )
            .await?;
        serde_json::to_string_pretty(&page).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_list_lessons",
        description = "获取一门课程的全部课次（直播与录播）。每条包含课次名、授课时间、教室、直播状态（1=未开始 2=直播中 3=已结束 4=可看回放）与回放 media_video_id"
    )]
    pub async fn jxxk_list_lessons(
        &self,
        Parameters(params): Parameters<JxxkLessonListParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let page = service
            .list_lessons(
                &params.course_id,
                params.page_index.unwrap_or(1),
                params.page_size.unwrap_or(100),
            )
            .await?;
        serde_json::to_string_pretty(&page).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_get_lesson_streams",
        description = "获取一节课的多角度录像信息（电脑画面/教师画面/学生画面/主画面），包含每路的文件大小与时长，以及 media_video_id"
    )]
    pub async fn jxxk_get_lesson_streams(
        &self,
        Parameters(params): Parameters<JxxkLiveIdParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let streams = service.lesson_streams(&params.live_id).await?;
        serde_json::to_string_pretty(&streams).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_get_play_urls",
        description = "获取一节课各画面的 HLS 播放地址（带签名时效，应立即使用）。地址需经应用内流代理访问"
    )]
    pub async fn jxxk_get_play_urls(
        &self,
        Parameters(params): Parameters<JxxkLiveIdParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let urls = service.play_urls(&params.live_id).await?;
        serde_json::to_string_pretty(&urls).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_get_live_stream",
        description = "获取一节课的实时直播流地址（仅在直播中可用）"
    )]
    pub async fn jxxk_get_live_stream(
        &self,
        Parameters(params): Parameters<JxxkLiveIdParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let streams = service.live_stream(&params.live_id).await?;
        serde_json::to_string_pretty(&streams).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_get_download_urls",
        description = "获取一节课各画面的明文 mp4 下载直链（无加密），可用于下载录像"
    )]
    pub async fn jxxk_get_download_urls(
        &self,
        Parameters(params): Parameters<JxxkMediaVideoIdParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        let urls = service.download_urls(&params.media_video_id).await?;
        serde_json::to_string_pretty(&urls).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_list_downloads",
        description = "列出录像下载任务（含进度、速度、状态），可通过 jxxk_pause_download / jxxk_download_resume / jxxk_remove_download 控制"
    )]
    pub async fn jxxk_list_downloads(&self) -> Result<String, String> {
        let service = service(&self.app)?;
        serde_json::to_string_pretty(&service.downloads.list())
            .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "jxxk_pause_download",
        description = "暂停一个进行中的录像下载任务"
    )]
    pub async fn jxxk_pause_download(
        &self,
        Parameters(params): Parameters<JxxkTaskIdParams>,
    ) -> Result<String, String> {
        let service = service(&self.app)?;
        szu_jxxk::jxxk::download::pause_task(&service.downloads, &params.task_id)?;
        Ok("已暂停".into())
    }

    #[tool(
        name = "jxxk_download_resume",
        description = "继续一个已暂停的录像下载任务"
    )]
    pub async fn jxxk_download_resume(
        &self,
        Parameters(params): Parameters<JxxkTaskIdParams>,
    ) -> Result<String, String> {
        szu_jxxk::jxxk::download::resume_task(&self.app, &params.task_id).await?;
        Ok("已继续".into())
    }

    #[tool(
        name = "jxxk_remove_download",
        description = "删除一个录像下载任务（未完成时一并删除未完成的落盘文件）"
    )]
    pub async fn jxxk_remove_download(
        &self,
        Parameters(params): Parameters<JxxkTaskIdParams>,
    ) -> Result<String, String> {
        szu_jxxk::jxxk::download::remove_task(&self.app, &params.task_id, false).await?;
        Ok("已删除".into())
    }
}
