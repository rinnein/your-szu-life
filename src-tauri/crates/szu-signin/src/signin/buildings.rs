//! 内置教学地点列表（项目根目录 `signin-buildings.json`）。
//!
//! 数据由 `scripts/gen-signin-buildings.ts` 从 docs/qw-attendance 抓取产物
//! 生成：剔除 placeholder 占位坐标（28 条共用假坐标），保留 listed 与
//! provisional。坐标系为 GCJ-02，直接用于提交与围栏判定。

use std::sync::OnceLock;
use super::model::Building;

const EMBEDDED_JSON: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../signin-buildings.json"));

#[derive(serde::Deserialize)]
struct Embedded {
    buildings: Vec<Building>,
}

static BUILDINGS: OnceLock<Vec<Building>> = OnceLock::new();

/// 内置地点列表（59 条：43 listed + 16 provisional）。
pub fn builtin() -> &'static [Building] {
    BUILDINGS
        .get_or_init(|| {
            serde_json::from_str::<Embedded>(EMBEDDED_JSON)
                .map(|embedded| embedded.buildings)
                .unwrap_or_else(|error| {
                    log::error!("内置签到地点数据损坏: {error}");
                    Vec::new()
                })
        })
        .as_slice()
}

/// 兜底楼栋：致理楼（id 79，沧海校区主教学楼）。
pub fn fallback() -> Option<&'static Building> {
    builtin().iter().find(|b| b.id == 79)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_list_parses_and_fallback_exists() {
        let list = builtin();
        assert_eq!(list.len(), 59, "内置列表应为 59 条（43 listed + 16 provisional）");
        assert_eq!(list.iter().filter(|b| b.status == "listed").count(), 43);
        assert_eq!(list.iter().filter(|b| b.status == "provisional").count(), 16);
        assert!(
            list.iter().all(|b| b.status != "placeholder"),
            "placeholder 占位坐标不应进入内置列表"
        );
        let fallback = fallback().expect("致理楼兜底必须存在");
        assert_eq!(fallback.name, "致理楼");
        assert_eq!(fallback.radius_m, 130);
    }

    #[test]
    fn zhiliao_coordinates_match_dataset() {
        let building = fallback().unwrap();
        assert!((building.latitude_gcj02 - 22.528).abs() < 1e-9);
        assert!((building.longitude_gcj02 - 113.940).abs() < 1e-9);
    }
}
