//! 签到/打卡模块的共享数据模型。
//!
//! 序列化命名沿用仓库惯例：顶层快照结构用 camelCase（前端字段直读），
//! 嵌套条目（楼栋、课次、待签项）保持 snake_case。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 内置教学地点：qw.szu.edu.cn 考勤系统楼栋围栏（GCJ-02 坐标）。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Building {
    pub id: i64,
    pub name: String,
    pub name_en: String,
    pub campus: String,
    pub latitude_gcj02: f64,
    pub longitude_gcj02: f64,
    /// 服务端圆形围栏半径（米），SZU 签到用它收敛随机偏移。
    pub radius_m: i64,
    /// `listed`（正式坐标）| `provisional`（待实地校准）。
    pub status: String,
}

impl Building {
    pub fn is_provisional(&self) -> bool {
        self.status == "provisional"
    }
}

/// 课表中的一个课次（qw.szu.edu.cn `student-schedule` 的行）。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct TimetableSession {
    /// 本地日期 `YYYY-MM-DD`。
    pub date: String,
    pub course_name: String,
    pub building_name: Option<String>,
    pub building_name_en: Option<String>,
    pub classroom: Option<String>,
    /// 课程开始/结束时间（epoch 毫秒，本地时区解析）。
    pub start_ms: i64,
    pub end_ms: i64,
    pub semester: Option<String>,
}

/// 课表缓存整体快照（单行 JSON 落盘）。
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
pub struct TimetableCache {
    pub fetched_at_ms: i64,
    pub semester: Option<String>,
    pub sessions: Vec<TimetableSession>,
}

impl TimetableCache {
    pub fn is_empty(&self) -> bool {
        self.fetched_at_ms <= 0 && self.sessions.is_empty()
    }

    /// 指定本地日期（`YYYY-MM-DD`）的课次，按开始时间升序。
    pub fn sessions_on(&self, date: &str) -> Vec<&TimetableSession> {
        let mut rows: Vec<&TimetableSession> =
            self.sessions.iter().filter(|s| s.date == date).collect();
        rows.sort_by_key(|s| s.start_ms);
        rows
    }
}

/// 上一次成功提交的定位点（按课程记忆）。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct LastPoint {
    pub latitude: f64,
    pub longitude: f64,
    /// 展示名：楼栋中文名，或「浏览器定位」。
    pub label: String,
    /// `building`（楼栋内置点）| `geolocation`（浏览器定位）。
    pub source: String,
}

/// 畅课待签项类型复用 szu-lms 的 [`szu_lms::lms::rollcall::RollcallItem`]，
/// 避免两份模型漂移。

/// 网关（qw.szu.edu.cn）会话状态：正常 / 需要重新统一认证 / 暂不可用。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QwSessionState {
    Ready,
    NeedsAuth,
    Unavailable,
}

/// 返回给前端的网关会话状态视图。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwStatusView {
    pub state: QwSessionState,
    pub message: String,
}

/// SZU 某课程上次签到的照片视图。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LastPhotoView {
    pub has_photo: bool,
    /// 缩略图 data URI（预览用）。
    pub thumbnail: Option<String>,
}

/// 提供给 szu-todos 的待签到待办条目。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct CheckinTodoItem {
    /// `checkin:lms:{rollcall_id}` / `checkin:szu:{session}`。
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub due_at_ms: i64,
    pub route: String,
}

/// 待签到视图 + 缓存完整性标记（szu-todos 的 trusted 判定依据）。
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckinView {
    pub items: Vec<CheckinTodoItem>,
    /// false 表示上次探测失败/会话不可用，聚合层不得据此关闭提醒。
    pub complete: bool,
}

/// 雷达默认点解析结果。
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPoint {
    pub latitude: f64,
    pub longitude: f64,
    /// 展示名（楼栋名 /「浏览器定位」/「致理楼」兜底）。
    pub label: String,
    /// `last_success` | `timetable` | `fallback`。
    pub source: String,
    /// 该点所属楼栋围栏半径（米）；非楼栋点（浏览器定位）为 None。
    pub fence_radius_m: Option<i64>,
}
