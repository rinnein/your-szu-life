//! 外观偏好：深浅色模式、种子色与配色变体。
//!
//! 与通知、日志、调试开关一致，存于非机密设置库 `settings.sqlite`，
//! 不进入加密 vault —— 这些不是凭据，也不需要随设置包导出。

use serde::{Deserialize, Serialize};
use tauri::Emitter;

#[cfg(target_os = "android")]
pub(crate) mod android;
pub(crate) mod commands;
pub(crate) mod mcp;
#[cfg(desktop)]
mod wallpaper;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Appearance {
    /// `light` | `dark` | `system`
    pub mode: String,
    /// `#rrggbb`
    pub seed: String,
    /// M3 scheme 变体，如 `tonal-spot`
    pub variant: String,
    /// `wallpaper` | `user` | `default`
    pub seed_source: String,
    /// 桌面端是否在启动时自动从当前壁纸提取种子色。Android 由系统桥驱动，不受此开关影响。
    #[serde(default)]
    pub auto_wallpaper: bool,
    /// 桌面端壁纸检查间隔（分钟）；无法监听壁纸变更事件时使用。
    #[serde(default = "default_wallpaper_poll_minutes")]
    pub wallpaper_poll_minutes: u32,
}

fn default_wallpaper_poll_minutes() -> u32 {
    15
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: "system".into(),
            seed: crate::appearance::DEFAULT_SEED.into(),
            variant: crate::appearance::DEFAULT_VARIANT.into(),
            seed_source: "default".into(),
            auto_wallpaper: false,
            wallpaper_poll_minutes: default_wallpaper_poll_minutes(),
        }
    }
}

pub(crate) const DEFAULT_SEED: &str = "#5d5fef";
pub(crate) const DEFAULT_VARIANT: &str = "tonal-spot";

const MODES: [&str; 3] = ["light", "dark", "system"];
const SEED_SOURCES: [&str; 3] = ["wallpaper", "user", "default"];
const VARIANTS: [&str; 9] = [
    "tonal-spot",
    "expressive",
    "vibrant",
    "neutral",
    "fidelity",
    "content",
    "monochrome",
    "fruit-salad",
    "rainbow",
];

fn is_hex_color(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes[0] == b'#'
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit())
}

impl Appearance {
    /// 拒绝任何越界取值，避免损坏的 appearance.json 把 UI 锁死在非法状态。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !MODES.contains(&self.mode.as_str()) {
            return Err(format!("未知的外观模式：{}", self.mode));
        }
        if !is_hex_color(&self.seed) {
            return Err(format!("种子色必须是 #rrggbb：{}", self.seed));
        }
        if !VARIANTS.contains(&self.variant.as_str()) {
            return Err(format!("未知的配色变体：{}", self.variant));
        }
        if !SEED_SOURCES.contains(&self.seed_source.as_str()) {
            return Err(format!("未知的种子色来源：{}", self.seed_source));
        }
        if !(1..=1440).contains(&self.wallpaper_poll_minutes) {
            return Err(format!(
                "壁纸检查间隔必须是 1 到 1440 分钟：{}",
                self.wallpaper_poll_minutes
            ));
        }
        Ok(())
    }
}

pub(crate) fn load(_app: &tauri::AppHandle) -> Appearance {
    match crate::preferences::load::<Appearance>(crate::preferences::APPEARANCE) {
        Ok(Some(value)) if value.validate().is_ok() => value,
        // 存储损坏或字段非法时退回默认值，而不是让应用启动失败。
        _ => Appearance::default(),
    }
}

pub(crate) fn save(app: &tauri::AppHandle, value: &Appearance) -> Result<Appearance, String> {
    value.validate()?;
    crate::preferences::save(crate::preferences::APPEARANCE, value)?;
    let _ = app.emit("appearance-changed", value);
    Ok(value.clone())
}

/// 桌面端当前壁纸的缩略图，用于前端提取种子色。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallpaperImage {
    /// `data:image/png;base64,...`
    pub data_url: String,
    pub width: u32,
    pub height: u32,
}

/// Kotlin 传来的壁纸取色结果。
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WallpaperSeed {
    /// `#rrggbb`，平台无壁纸色时为 null。
    pub seed: Option<String>,
    /// `wallpaper` | `system-accent` | `unavailable`
    pub source: String,
    /// 系统当前是否处于深色模式。
    pub dark: bool,
}

/// 解析 Kotlin 手写 JSON 并归一化种子色。
///
/// Kotlin 侧已保证格式，这里仍做校验：JNI 边界上的异常值不应让 UI 崩溃。
#[cfg(any(target_os = "android", test))]
fn parse_wallpaper_seed(raw: &str) -> Result<WallpaperSeed, String> {
    let mut parsed: WallpaperSeed =
        serde_json::from_str(raw).map_err(|error| format!("壁纸取色结果无法解析：{error}"))?;
    match parsed.seed.take() {
        Some(seed) if is_hex_color(&seed) => parsed.seed = Some(seed.to_ascii_lowercase()),
        Some(seed) => return Err(format!("壁纸取色结果含非法颜色：{seed}")),
        None => {}
    }
    Ok(parsed)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> Appearance {
        Appearance::default()
    }
    #[test]
    fn default_is_valid() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn rejects_bad_mode() {
        let mut value = valid();
        value.mode = "sepia".into();
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_bad_seed() {
        for seed in ["5d5fef", "#5d5fe", "#5d5fef0", "#gggggg", ""] {
            let mut value = valid();
            value.seed = seed.into();
            assert!(value.validate().is_err(), "should reject {seed}");
        }
    }

    #[test]
    fn accepts_uppercase_seed() {
        let mut value = valid();
        value.seed = "#5D5FEF".into();
        assert!(value.validate().is_ok());
    }

    #[test]
    fn rejects_bad_variant() {
        let mut value = valid();
        value.variant = "neon".into();
        assert!(value.validate().is_err());
    }

    #[test]
    fn rejects_bad_source() {
        let mut value = valid();
        value.seed_source = "guess".into();
        assert!(value.validate().is_err());
    }

    #[test]
    fn round_trips_json() {
        let value = Appearance {
            mode: "dark".into(),
            seed: "#123456".into(),
            variant: "expressive".into(),
            seed_source: "user".into(),
            auto_wallpaper: true,
            wallpaper_poll_minutes: 15,
        };
        let raw = serde_json::to_string(&value).unwrap();
        let back: Appearance = serde_json::from_str(&raw).unwrap();
        assert_eq!(value, back);
    }

    #[test]
    fn legacy_appearance_defaults_auto_wallpaper_off() {
        let raw = r##"{"mode":"system","seed":"#5d5fef","variant":"tonal-spot","seedSource":"default"}"##;
        let value: Appearance = serde_json::from_str(raw).unwrap();
        assert!(!value.auto_wallpaper);
        assert_eq!(value.wallpaper_poll_minutes, 15);
        assert!(value.validate().is_ok());
    }

    #[test]
    fn rejects_wallpaper_poll_interval_out_of_range() {
        for minutes in [0, 1441] {
            let mut value = valid();
            value.wallpaper_poll_minutes = minutes;
            assert!(value.validate().is_err(), "should reject {minutes}");
        }
    }

    #[test]
    fn parses_wallpaper_seed() {
        let raw = r##"{"seed":"#5D5FEF","source":"wallpaper","dark":true}"##;
        let parsed = parse_wallpaper_seed(raw).unwrap();
        assert_eq!(parsed.seed.as_deref(), Some("#5d5fef"));
        assert_eq!(parsed.source, "wallpaper");
        assert!(parsed.dark);
    }

    #[test]
    fn parses_missing_wallpaper_seed_as_none() {
        let raw = r#"{"seed":null,"source":"unavailable","dark":false}"#;
        let parsed = parse_wallpaper_seed(raw).unwrap();
        assert_eq!(parsed.seed, None);
        assert_eq!(parsed.source, "unavailable");
        assert!(!parsed.dark);
    }

    #[test]
    fn rejects_malformed_wallpaper_json() {
        assert!(parse_wallpaper_seed("not json").is_err());
        assert!(parse_wallpaper_seed(r#"{"seed":"red","source":"wallpaper","dark":false}"#).is_err());
        assert!(
            parse_wallpaper_seed(r##"{"seed":"#gggggg","source":"wallpaper","dark":false}"##)
                .is_err()
        );
    }
}
