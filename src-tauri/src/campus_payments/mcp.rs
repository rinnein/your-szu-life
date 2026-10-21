use crate::campus_payments::{
    electricity::{self, ElectricityQueryInput, QueryPurpose},
    CampusPaymentsStore,
};
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Deserialize, Serialize, JsonSchema, Default)]
pub struct QueryElectricityParams {
    /// 是否强制跳过缓存从校园缴费网拉取最新数据
    pub force_refresh: Option<bool>,
    /// 校区 ID（例如粤海校区、丽湖校区 ID，如不传则使用已配置的宿舍）
    pub campus_id: Option<String>,
    /// 区域/组团 ID（如不传则使用已配置的宿舍）
    pub area_id: Option<String>,
    /// 楼栋 ID（如不传则使用已配置的宿舍）
    pub building_id: Option<String>,
    /// 楼栋名称（如不传则使用已配置的宿舍）
    pub building_name: Option<String>,
    /// 宿舍房号（例如 401、502，如不传则使用已配置的宿舍）
    pub room_name: Option<String>,
}

#[tool_router(router = electricity_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "electricity_query_balance",
        description = "查询宿舍实时电费（剩余度数、可用余额元、今日用电、低电量预警状态）。可查询当前已配置宿舍或传入指定房间"
    )]
    pub async fn electricity_query_balance(
        &self,
        Parameters(params): Parameters<QueryElectricityParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<CampusPaymentsStore>()
            .ok_or("校园支付状态存储不可用")?;
        let electricity_store = store.electricity.clone();

        let input = match (
            params.campus_id,
            params.area_id,
            params.building_id,
            params.building_name,
            params.room_name,
        ) {
            (Some(c), Some(a), Some(b_id), Some(b_name), Some(r)) => ElectricityQueryInput {
                campus_id: c,
                area_id: a,
                building_id: b_id,
                building_name: b_name,
                room_name: r,
                begin_date: String::new(),
                end_date: String::new(),
            },
            _ => {
                let saved = electricity::load_settings(&electricity_store)?
                    .ok_or("尚未配置默认宿舍房间，请在客户端设置宿舍或传入完整房间参数")?;
                ElectricityQueryInput {
                    campus_id: saved.campus_id,
                    area_id: saved.area_id,
                    building_id: saved.building_id,
                    building_name: saved.building_name,
                    room_name: saved.room_name,
                    begin_date: String::new(),
                    end_date: String::new(),
                }
            }
        };

        let app = self.app.clone();
        let force_refresh = params.force_refresh.unwrap_or(false);
        let result = tokio::task::spawn_blocking(move || {
            electricity::query_and_record(
                input,
                &electricity_store,
                &app,
                QueryPurpose::Detail,
                force_refresh,
            )
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "electricity_get_recharges",
        description = "查询指定宿舍或已配置宿舍的历史电费充值明细记录（充值时间、充值金额、方式）"
    )]
    pub async fn electricity_get_recharges(
        &self,
        Parameters(params): Parameters<QueryElectricityParams>,
    ) -> Result<String, String> {
        let store = self
            .app
            .try_state::<CampusPaymentsStore>()
            .ok_or("校园支付状态存储不可用")?;
        let electricity_store = store.electricity.clone();

        let input = match (
            params.campus_id,
            params.area_id,
            params.building_id,
            params.building_name,
            params.room_name,
        ) {
            (Some(c), Some(a), Some(b_id), Some(b_name), Some(r)) => ElectricityQueryInput {
                campus_id: c,
                area_id: a,
                building_id: b_id,
                building_name: b_name,
                room_name: r,
                begin_date: String::new(),
                end_date: String::new(),
            },
            _ => {
                let saved = electricity::load_settings(&electricity_store)?
                    .ok_or("尚未配置默认宿舍房间，请在客户端设置宿舍或传入完整房间参数")?;
                ElectricityQueryInput {
                    campus_id: saved.campus_id,
                    area_id: saved.area_id,
                    building_id: saved.building_id,
                    building_name: saved.building_name,
                    room_name: saved.room_name,
                    begin_date: String::new(),
                    end_date: String::new(),
                }
            }
        };

        let result = tokio::task::spawn_blocking(move || {
            electricity::recharges(input, &electricity_store)
        })
        .await
        .map_err(|_| "任务执行失败".to_string())??;

        serde_json::to_string_pretty(&result).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "electricity_get_options",
        description = "获取深大宿舍电费支持的校区、组团、楼栋、房号字典层级树"
    )]
    pub async fn electricity_get_options(&self) -> Result<String, String> {
        let options = tokio::task::spawn_blocking(electricity::options)
            .await
            .map_err(|_| "任务执行失败".to_string())??;
        serde_json::to_string_pretty(&options).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "electricity_get_settings",
        description = "获取当前客户端已保存配置的宿舍楼栋及房间信息"
    )]
    pub async fn electricity_get_settings(&self) -> Result<String, String> {
        let store = self
            .app
            .try_state::<CampusPaymentsStore>()
            .ok_or("校园支付状态存储不可用")?;
        let settings = electricity::load_settings(&store.electricity)?;
        serde_json::to_string_pretty(&settings).map_err(|e| format!("序列化失败: {e}"))
    }
}
