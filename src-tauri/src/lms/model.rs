use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// 畅课（lms.szu.edu.cn）待办条目，来自 `GET /api/todos` 的 `todo_list`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct Todo {
    /// 课程选课编号（如 202620271150037000307）
    #[serde(default)]
    pub course_code: String,
    /// 课程数字 ID，用于构造任务页面链接
    pub course_id: i64,
    /// 课程名称（如「数据库系统」）
    #[serde(default)]
    pub course_name: String,
    /// 课程类型
    #[serde(default)]
    pub course_type: i64,
    /// 截止时间（ISO 8601 UTC，如 2026-11-03T15:59:00Z）
    #[serde(default)]
    pub end_time: String,
    /// 待办条目 ID，与课程 ID 一起构造任务页面链接
    pub id: i64,
    /// 是否已被锁定（不可再提交）
    #[serde(default)]
    pub is_locked: bool,
    /// 当前账户是否为学生
    #[serde(default)]
    pub is_student: bool,
    /// 前置任务列表
    #[serde(default)]
    pub prerequisites: Vec<Value>,
    /// 待办标题（如「ADVANCED SQL」）
    #[serde(default)]
    pub title: String,
    /// 待办类型（如 homework）
    #[serde(default)]
    pub kind: String,
}

/// 通知正文。不同 `type` 的通知 payload 字段不同，这里保留常见字段，
/// 其余字段放进 `extra` 以免丢失。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) struct NotificationPayload {
    /// 活动 ID（作业/资料类通知）
    #[serde(default)]
    pub activity_id: Option<i64>,
    /// 活动标题（如「第五次作业」）
    #[serde(default)]
    pub activity_title: Option<String>,
    /// 活动类型（如 homework、material）
    #[serde(default)]
    pub activity_type: Option<String>,
    /// 课程数字 ID
    #[serde(default)]
    pub course_id: Option<i64>,
    /// 课程名称
    #[serde(default)]
    pub course_name: Option<String>,
    /// 通知对应内容的创建时间
    #[serde(default)]
    pub created_at: Option<String>,
    /// 截止时间
    #[serde(default)]
    pub end_time: Option<String>,
    /// 作业 ID（作业类通知）
    #[serde(default)]
    pub homework_id: Option<i64>,
    /// 作业标题（作业类通知）
    #[serde(default)]
    pub homework_title: Option<String>,
    /// 提交关闭时间
    #[serde(default)]
    pub submit_closed_time: Option<String>,
    /// 开始时间（资料发布类通知）
    #[serde(default)]
    pub start_time: Option<String>,
    /// 关联附件列表
    #[serde(default)]
    pub uploads: Vec<Value>,
    /// 其余未建模字段
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// 畅课站内通知，来自 `GET /ntf/users/{uid}/notifications`。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct Notification {
    /// 通知 ID（如 MongoDB ObjectId）
    pub id: String,
    /// 通知类型（如 homework_expiring、activity_opened）
    #[serde(rename = "type", default)]
    pub kind: String,
    /// 是否置顶
    #[serde(default)]
    pub top: bool,
    /// 通知时间（Unix 毫秒）
    #[serde(default)]
    pub timestamp: i64,
    /// 通知正文
    #[serde(default)]
    pub payload: NotificationPayload,
    /// 是否未读
    #[serde(default)]
    pub unread: bool,
}

/// 通知接口的响应包装。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) struct NotificationsResponse {
    /// 通知列表
    #[serde(default)]
    pub notifications: Vec<Notification>,
    /// 未读通知数
    #[serde(default)]
    pub unread_count: i64,
}

/// 畅课用户资料，从页面内注入的 `globalData` 脚本解析得到。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Profile {
    /// 用户数字 ID（通知接口路径中的 uid）
    pub uid: i64,
    /// 姓名
    #[serde(default)]
    pub name: String,
    /// 学号
    #[serde(default)]
    pub user_no: String,
}

/// 待办接口的响应包装。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) struct TodosResponse {
    /// 待办列表
    #[serde(default)]
    pub todo_list: Vec<Todo>,
}

/// 本地缓存的一份数据快照（持久化到 state.sqlite）。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Cache {
    /// 用户资料
    #[serde(default)]
    pub profile: Option<Profile>,
    /// 课程待办列表
    #[serde(default)]
    pub todos: Vec<Todo>,
    /// 最近通知列表
    #[serde(default)]
    pub notifications: Vec<Notification>,
    /// 未读通知数
    #[serde(default)]
    pub unread_count: i64,
    /// 拉取时间（Unix 毫秒）
    #[serde(default)]
    pub fetched_at: i64,
}

/// 会话状态：正常 / 需要重新统一认证 / 暂不可用（网络等错误）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SessionState {
    /// 会话正常
    Ready,
    /// 需要先完成应用统一认证（authserver 会话失效）
    NeedsAuth,
    /// 网络等临时错误，稍后重试
    Unavailable,
}

/// 返回给前端 / MCP 的完整快照。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Snapshot {
    /// 会话状态
    pub session: SessionState,
    /// 状态说明（失败原因等，正常时为空）
    pub message: String,
    /// 用户资料（未建立会话时为空）
    pub profile: Option<Profile>,
    /// 课程待办列表
    pub todos: Vec<Todo>,
    /// 最近通知列表
    pub notifications: Vec<Notification>,
    /// 未读通知数
    pub unread_count: i64,
    /// 数据拉取时间（Unix 毫秒；未拉取过为空）
    pub fetched_at: Option<i64>,
}
