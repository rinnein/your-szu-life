use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 会话状态。与畅课模块语义一致：Ready 可用、NeedsAuth 需先统一认证、
/// Unavailable 网络/接口临时故障。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SessionState {
    Ready,
    NeedsAuth,
    Unavailable,
}

/// 返回给前端 / MCP 的快照。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub session: SessionState,
    pub message: String,
    /// 学年学期列表（未建立会话时为空）。
    pub semesters: Vec<Semester>,
    /// 学期列表拉取时间（Unix 毫秒；未拉取过为空）。
    pub fetched_at: Option<i64>,
}

/// 巡课页面需要的缓存快照：学期列表 + 课程页 + 课次页。
/// `courses` 的键是学期 ID，`lessons` 的键是课程 ID。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JxxkState {
    pub snapshot: Snapshot,
    pub courses: BTreeMap<String, CoursePage>,
    pub lessons: BTreeMap<String, LessonPage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Semester {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub begin_date: Option<String>,
    #[serde(default)]
    pub end_date: Option<String>,
}

/// 学年学期列表解析：实测 `data` 直接就是数组；兼容分页包裹形状。
pub(crate) fn parse_semester_list(value: &serde_json::Value) -> Result<Vec<Semester>, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Page {
        #[serde(default)]
        content: Vec<Semester>,
        #[serde(default)]
        data: Vec<Semester>,
    }
    if let Ok(semesters) = serde_json::from_value::<Vec<Semester>>(value.clone()) {
        return Ok(semesters);
    }
    match serde_json::from_value::<Page>(value.clone()) {
        Ok(page) if !page.content.is_empty() => Ok(page.content),
        Ok(page) => Ok(page.data),
        Err(_) => Err("学期列表响应格式不兼容".to_string()),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Course {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub teacher: Option<String>,
    #[serde(default)]
    pub year_semester_id: Option<String>,
    #[serde(default)]
    pub year_semester_name: Option<String>,
    #[serde(default)]
    pub category_section_name: Option<String>,
    /// 教学班级号（多个以「、」分隔）。
    #[serde(default)]
    pub clazzes: Option<String>,
    #[serde(default)]
    pub course_resource_count: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoursePage {
    pub page_index: i64,
    pub page_size: i64,
    pub record_count: i64,
    pub content: Vec<Course>,
}

/// 课次：一门课程下的一次授课（直播或录播）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Lesson {
    pub id: String,
    /// 课次名，如「数据结构第1周星期5第3-5节」。
    pub target_name: String,
    pub live_id: Option<String>,
    /// 直播状态：1=未开始 2=直播中 3=已结束 4=可看回放。
    #[serde(default)]
    pub live_state: Option<i64>,
    #[serde(default)]
    pub live_state_text: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    /// 授课教室（来自录播设备名）。
    #[serde(default)]
    pub classroom: Option<String>,
    /// 回放对应的媒体视频 id（mediaVideoId），无回放时为空。
    #[serde(default)]
    pub media_video_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LessonPage {
    pub page_index: i64,
    pub page_size: i64,
    pub record_count: i64,
    pub content: Vec<Lesson>,
}

/// 单路画面（角度）描述。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoStream {
    pub id: i64,
    /// 官方名称：电脑画面 / 教师画面 / 学生画面 / 主画面。
    pub name: String,
    /// 流号：30=电脑 1=教师 2=学生 0=主。
    #[serde(default)]
    pub stream_no: Option<String>,
    #[serde(default)]
    pub stream_label: Option<String>,
    #[serde(default)]
    pub file_size: Option<i64>,
    /// 时长（秒）。
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default)]
    pub file_name: Option<String>,
}

/// 一节课的完整视频信息（video/page 单条记录的精简投影）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LessonStreams {
    /// 视频记录 id（官方播放页 URL 的 videoId）。
    pub video_id: String,
    /// 媒体视频 id（mediaVideoId），播放地址与下载直链都以它为参数。
    pub media_video_id: String,
    pub title: String,
    #[serde(default)]
    pub duration: Option<i64>,
    /// 平台是否允许下载。
    pub download: bool,
    #[serde(default)]
    pub record_equipment_name: Option<String>,
    pub streams: Vec<VideoStream>,
}

/// 播放地址接口里的一路画面。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlayUrlScreen {
    pub screen_tag: Option<String>,
    pub stream_name: Option<String>,
    #[serde(default)]
    pub stream_no: Option<String>,
    #[serde(default)]
    pub stream_label: Option<String>,
    #[serde(default)]
    pub stream_permission_type: Option<String>,
    /// 带签名的 HLS 地址（有时效，即取即用）。
    pub play_url: String,
}

/// 下载直链接口里的一路画面。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadUrl {
    pub stream_name: Option<String>,
    #[serde(default)]
    pub stream_no: Option<String>,
    #[serde(default)]
    pub stream_label: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
    /// 明文 mp4 直链（无加密无签名）。
    pub file_url: String,
}

/// 直播流地址（best-effort 提取，接口结构未完全实测）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LiveStreamSource {
    pub url: String,
    /// 流标签（若有），如 TP/SP/PC/PRIMARY 或画质描述。
    #[serde(default)]
    pub label: Option<String>,
}

/// 平台统一响应包裹的精简形状。`code` 在不同接口可能是数字或字符串。
#[derive(Deserialize)]
pub(crate) struct Envelope {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub(crate) code: Option<serde_json::Value>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: serde_json::Value,
}

impl Envelope {
    pub(crate) fn code_is(&self, expected: i64) -> bool {
        match &self.code {
            Some(serde_json::Value::Number(n)) => n.as_i64() == Some(expected),
            Some(serde_json::Value::String(s)) => s.parse::<i64>().ok() == Some(expected),
            _ => false,
        }
    }
}
