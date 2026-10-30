use super::{Appearance, WallpaperSeed};
use tauri::AppHandle;

#[tauri::command]
pub(crate) fn get_appearance(app: AppHandle) -> Appearance {
    super::load(&app)
}

#[tauri::command]
pub(crate) fn save_appearance(app: AppHandle, appearance: Appearance) -> Result<Appearance, String> {
    super::save(&app, &appearance)
}

/// 读取 Android 壁纸种子色。非 Android 平台返回 `seed: null`。
#[tauri::command]
pub(crate) fn get_wallpaper_seed() -> Result<WallpaperSeed, String> {
    #[cfg(target_os = "android")]
    {
        super::android::wallpaper_seed_result()
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(WallpaperSeed {
            seed: None,
            source: "unavailable".into(),
            dark: false,
        })
    }
}

/// 读取桌面端当前壁纸并返回缩略图，供前端提取种子色。
#[tauri::command]
pub(crate) async fn get_wallpaper_image() -> Result<super::WallpaperImage, String> {
    #[cfg(desktop)]
    {
        tauri::async_runtime::spawn_blocking(super::wallpaper::current_wallpaper_image)
            .await
            .map_err(|_| "读取当前壁纸任务失败".to_string())?
    }
    #[cfg(not(desktop))]
    {
        Err("当前平台不支持读取当前壁纸图片，请使用系统动态取色。".into())
    }
}
