use crate::appearance::{Appearance, WallpaperSeed};
use crate::mcp::service::UnifiedSzuMcpService;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::tool;
use rmcp::tool_router;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SetThemeParams {
    /// 深浅色模式：light / dark / system
    pub mode: String,
    /// 种子色，格式 #rrggbb
    pub seed: String,
    /// M3 配色变体：tonal-spot / expressive / vibrant / neutral / fidelity / content / monochrome / fruit-salad / rainbow
    pub variant: String,
}

#[tool_router(router = appearance_tools, vis = "pub(crate)")]
impl UnifiedSzuMcpService {
    #[tool(
        name = "appearance_get_theme",
        description = "获取your-szu-life当前外观主题设定：深浅色模式、种子色与 M3 配色变体。对应资源 your-szu-life://data/appearance/theme"
    )]
    pub async fn appearance_get_theme(&self) -> Result<String, String> {
        let appearance = crate::appearance::load(&self.app);
        serde_json::to_string_pretty(&appearance).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "appearance_set_theme",
        description = "设置your-szu-life外观主题。修改后客户端界面会立即换色。对应资源 your-szu-life://data/appearance/theme"
    )]
    pub async fn appearance_set_theme(
        &self,
        Parameters(params): Parameters<SetThemeParams>,
    ) -> Result<String, String> {
        let appearance = Appearance {
            mode: params.mode,
            seed: params.seed,
            variant: params.variant,
            // 经由 MCP 指定的一律视为用户显式选择，避免被壁纸色覆盖。
            seed_source: "user".into(),
        };
        let saved = crate::appearance::save(&self.app, &appearance)?;
        serde_json::to_string_pretty(&saved).map_err(|e| format!("序列化失败: {e}"))
    }

    #[tool(
        name = "appearance_get_wallpaper_seed",
        description = "读取 Android 壁纸派生的种子色（Material You 动态取色）。非 Android 平台返回 seed 为 null。对应资源 your-szu-life://data/appearance/wallpaper"
    )]
    pub async fn appearance_get_wallpaper_seed(&self) -> Result<String, String> {
        #[cfg(target_os = "android")]
        let value = crate::appearance::android::wallpaper_seed_result()?;
        #[cfg(not(target_os = "android"))]
        let value = WallpaperSeed {
            seed: None,
            source: "unavailable".into(),
            dark: false,
        };
        serde_json::to_string_pretty(&value).map_err(|e| format!("序列化失败: {e}"))
    }
}
