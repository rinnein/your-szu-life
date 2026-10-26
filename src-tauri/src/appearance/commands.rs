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
