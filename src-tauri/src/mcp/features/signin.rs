//! 签到/打卡模块的 MCP 工具胶水。

use szu_signin::signin;
use szu_signin::signin::automation::SigninAutomationSettings;
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SubmitNumberParams {
    /// 畅课点名 ID（rollcall_id，来自 signin_list_pending）
    pub rollcall_id: i64,
    /// 课程数字 ID（用于把成功的数字码自动共享到该课程的共享房间）
    pub course_id: i64,
    /// 数字签到码（通常 4 位，最长 8 字符）
    pub code: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct ShareNumberParams {
    /// 课程数字 ID（房间按课程隔离）
    pub course_id: i64,
    /// 畅课点名 ID（rollcall_id，来自 signin_list_pending）
    pub rollcall_id: i64,
    /// 要共享的数字签到码
    pub code: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct SubmitRadarParams {
    /// 畅课点名 ID（rollcall_id，来自 signin_list_pending）
    pub rollcall_id: i64,
    /// 课程数字 ID（默认点解析的记忆键；与 course_title 一起传）
    pub course_id: i64,
    /// 课程名称（用于从课表缓存解析课程楼栋位置）
    pub course_title: String,
    /// 内置楼栋 ID（signin_get_buildings 返回；与自定义坐标二选一）
    pub building_id: Option<i64>,
    /// 自定义纬度（浏览器定位读数为 WGS-84；缺省走默认点解析链）
    pub custom_latitude: Option<f64>,
    /// 自定义经度（浏览器定位读数为 WGS-84；缺省走默认点解析链）
    pub custom_longitude: Option<f64>,
    /// 自定义坐标是否为 WGS-84（默认 true）
    pub custom_is_wgs84: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct TimetableParams {
    /// 是否强制重新拉取课表（默认 false，用 6 小时 TTL 缓存）
    pub force: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct AutomationSettingsUpdate {
    /// 畅课雷达点名自动签到
    pub lms_auto_radar: Option<bool>,
    /// SZU（实验性）签到提醒与自动签到的总开关
    pub szu_enabled: Option<bool>,
}

#[tool_router(router = signin_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "signin_list_pending",
        description = "获取当前待签到的畅课点名列表（数字/雷达/二维码等类型与状态），以及签到模块缓存的待签视图；返回条目含 rollcall_id、course_id、course_title 与签到方式判定"
    )]
    pub async fn signin_list_pending(&self) -> Result<String, String> {
        let service = signin::service(&self.app)?;
        let rollcalls = service
            .lms_service()
            .map_err(|e| format!("畅课服务不可用: {e}"))?
            .rollcall_list(false)
            .await?;
        let view = service.checkin_view();
        serde_json::to_string_pretty(&serde_json::json!({
            "rollcalls": rollcalls,
            "pending_view": view,
        }))
        .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "signin_get_timetable",
        description = "获取签到模块缓存的 SZU 考勤课表（qw.szu.edu.cn student-schedule，含课次时间与楼栋/教室，楼栋坐标按signin内置地点数据解析）。默认走 6 小时 TTL 缓存"
    )]
    pub async fn signin_get_timetable(
        &self,
        Parameters(params): Parameters<TimetableParams>,
    ) -> Result<String, String> {
        let cache = signin::service(&self.app)?
            .refresh_timetable(params.force.unwrap_or(false))
            .await?;
        serde_json::to_string_pretty(&cache).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "signin_submit_number",
        description = "提交畅课数字签到：向指定点名提交数字码（通常 4 位）。deviceId 由应用按设备持久化；每场点名同账号限 10 次提交。提交成功后若该课程的共享房间已开启，会自动把数字码共享给同课程的其他设备"
    )]
    pub async fn signin_submit_number(
        &self,
        Parameters(params): Parameters<SubmitNumberParams>,
    ) -> Result<String, String> {
        signin::service(&self.app)?
            .submit_lms_number(
                params.rollcall_id,
                params.course_id,
                params.code.trim(),
            )
            .await?;
        Ok(format!("点名 {} 数字签到提交成功", params.rollcall_id))
    }

    #[tool(
        name = "signin_share_number",
        description = "把一条已知的数字签到码共享到该课程的签到码共享房间（同账号、同课程的其他设备会在收到后自动提交）。数字码在本次点名结束前一直有效；未加入房间时返回 shared=false"
    )]
    pub async fn signin_share_number(
        &self,
        Parameters(params): Parameters<ShareNumberParams>,
    ) -> Result<String, String> {
        let shared = signin::service(&self.app)?
            .share_number_code(params.course_id, params.rollcall_id, params.code.trim())
            .await?;
        serde_json::to_string_pretty(&serde_json::json!({ "shared": shared }))
            .map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "signin_submit_radar",
        description = "提交畅课雷达（GPS）签到：解析默认点（上次成功定位 → 课表课程楼栋 → 致理楼兜底）或自定义坐标（WGS-84 自动转 GCJ-02），以圆心外扩 0-150m 的随机偏移仿真真实定位后提交"
    )]
    pub async fn signin_submit_radar(
        &self,
        Parameters(params): Parameters<SubmitRadarParams>,
    ) -> Result<String, String> {
        let custom = match (params.custom_latitude, params.custom_longitude) {
            (Some(lat), Some(lng)) => Some((lat, lng, params.custom_is_wgs84.unwrap_or(true))),
            _ => None,
        };
        let resolved = signin::service(&self.app)?
            .submit_lms_radar(
                params.rollcall_id,
                params.course_id,
                &params.course_title,
                custom,
                params.building_id,
            )
            .await?;
        serde_json::to_string_pretty(&resolved).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "signin_get_automation_settings",
        description = "获取签到自动化设置：畅课雷达自动签到与 SZU（实验性）签到提醒/自动签到总开关。默认均为关闭。对应资源 your-szu-life://data/signin/settings"
    )]
    pub async fn signin_get_automation_settings(&self) -> Result<String, String> {
        let settings: SigninAutomationSettings = signin::service(&self.app)?.automation_settings();
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "signin_set_automation_settings",
        description = "修改签到自动化设置。开启畅课雷达自动签到后，检测到雷达点名会按默认地点回退链自动提交；开启 SZU 总开关后，对可签到的考勤课次自动用记忆照片提交，并发送签到提醒。对应资源 your-szu-life://data/signin/settings"
    )]
    pub async fn signin_set_automation_settings(
        &self,
        Parameters(params): Parameters<AutomationSettingsUpdate>,
    ) -> Result<String, String> {
        let service = signin::service(&self.app)?;
        let mut current = service.automation_settings();
        if let Some(value) = params.lms_auto_radar {
            current.lms_auto_radar = value;
        }
        if let Some(value) = params.szu_enabled {
            current.szu_enabled = value;
        }
        let saved = service.set_automation_settings(current)?;
        serde_json::to_string_pretty(&saved).map_err(|e| format!("序列化失败: {e}"))
    }
}
