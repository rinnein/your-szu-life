//! 坐标工具：WGS-84 ↔ GCJ-02 转换、距离与随机偏移。
//!
//! 楼栋内置数据与服务端围栏判定都是 GCJ-02；浏览器 geolocation 读数是
//! WGS-84，提交前必须转换（深大考勤前端企业微信分支即 type: 'gcj02'）。

use rand::Rng;

const PI: f64 = std::f64::consts::PI;
/// 克拉索夫斯基椭球长半轴。
const A: f64 = 6378245.0;
/// 第一偏心率平方。
const EE: f64 = 0.006_693_421_622_965_943;

/// 粗判是否在中国大陆范围外（范围外不做偏移，原样返回）。
fn out_of_china(lat: f64, lng: f64) -> bool {
    !(lng > 72.004 && lng < 137.8347 && lat > 0.8293 && lat < 55.8271)
}

fn transform_lat(x: f64, y: f64) -> f64 {
    let mut ret = -100.0
        + 2.0 * x
        + 3.0 * y
        + 0.2 * y * y
        + 0.1 * x * y
        + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn transform_lng(x: f64, y: f64) -> f64 {
    let mut ret =
        300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;
    ret
}

/// WGS-84 → GCJ-02（火星坐标）。标准国测局算法。
pub fn wgs84_to_gcj02(wgs_lat: f64, wgs_lng: f64) -> (f64, f64) {
    if out_of_china(wgs_lat, wgs_lng) {
        return (wgs_lat, wgs_lng);
    }
    let mut d_lat = transform_lat(wgs_lng - 105.0, wgs_lat - 35.0);
    let mut d_lng = transform_lng(wgs_lng - 105.0, wgs_lat - 35.0);
    let rad_lat = wgs_lat / 180.0 * PI;
    let magic = 1.0 - EE * rad_lat.sin() * rad_lat.sin();
    let sqrt_magic = magic.sqrt();
    d_lat = (d_lat * 180.0) / ((A * (1.0 - EE)) / (magic * sqrt_magic) * PI);
    d_lng = (d_lng * 180.0) / (A / sqrt_magic * rad_lat.cos() * PI);
    (wgs_lat + d_lat, wgs_lng + d_lng)
}

/// GCJ-02 → WGS-84。单步近似逆变换（深圳区域误差 < 1m），仅用于展示，
/// 不参与提交链路。
pub fn gcj02_to_wgs84(gcj_lat: f64, gcj_lng: f64) -> (f64, f64) {
    let (d_lat, d_lng) = {
        let (lat, lng) = wgs84_to_gcj02(gcj_lat, gcj_lng);
        (lat - gcj_lat, lng - gcj_lng)
    };
    (gcj_lat - d_lat, gcj_lng - d_lng)
}

/// 球面距离（米），haversine。
pub fn distance_m(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const R: f64 = 6_371_000.0;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

/// 以 (lat, lng) 为起点、按方位角（弧度，正北为 0）平面近似移动指定米数。
/// 150m 量级的平面近似误差远小于围栏判定粒度。
pub fn offset_point(lat: f64, lng: f64, bearing_rad: f64, dist_m: f64) -> (f64, f64) {
    let d_lat = dist_m * bearing_rad.cos() / 111_320.0;
    let cos_lat = lat.to_radians().cos().max(1e-9);
    let d_lng = dist_m * bearing_rad.sin() / (111_320.0 * cos_lat);
    (lat + d_lat, lng + d_lng)
}

/// 雷达签到仿真：以给定点为圆心随机外扩。
///
/// 距离按比例带采样：75% 概率落在 cap 的 1/6–2/3（cap=150m 时即 25–100m
/// 的偏好区间），其余落在 2/3–1；方向均匀分布。`cap_m` 由调用方按围栏收敛：
/// LMS 传 150（服务端围栏 200m），SZU 传楼栋 `radius_m`（服务端强校验）。
pub fn jitter_point<R: Rng>(rng: &mut R, lat: f64, lng: f64, cap_m: f64) -> (f64, f64) {
    let cap = cap_m.max(1.0);
    let frac = if rng.random_bool(0.75) {
        rng.random_range(1.0 / 6.0..2.0 / 3.0)
    } else {
        rng.random_range(2.0 / 3.0..1.0)
    };
    let bearing = rng.random_range(0.0..(2.0 * PI));
    offset_point(lat, lng, bearing, frac * cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn wgs84_to_gcj02_matches_builtin_dataset() {
        // 内置数据集同时给出 GCJ-02（接口原值）与 WGS-84（反算）两列；
        // 正向转换应还原接口值（4 位小数约 10m 粒度）。
        let cases = [
            // (wgs_lat, wgs_lng, gcj_lat, gcj_lng) —— 致理楼 / 汇文楼 / 致腾楼
            (22.531_022, 113.935_128, 22.528, 113.940),
            (22.540_779, 113.930_114, 22.537_7, 113.935),
            (22.529_069, 113.932_077, 22.526, 113.937),
        ];
        for (wgs_lat, wgs_lng, gcj_lat, gcj_lng) in cases {
            let (lat, lng) = wgs84_to_gcj02(wgs_lat, wgs_lng);
            let offset = distance_m(lat, lng, gcj_lat, gcj_lng);
            assert!(offset < 30.0, "wgs({wgs_lat},{wgs_lng}) -> ({lat},{lng}) vs gcj({gcj_lat},{gcj_lng}) 偏差 {offset}m");
        }
    }

    #[test]
    fn gcj02_roundtrip_error_is_sub_meter() {
        let (lat, lng) = (22.528, 113.940);
        let (back_lat, back_lng) = {
            let (w_lat, w_lng) = gcj02_to_wgs84(lat, lng);
            wgs84_to_gcj02(w_lat, w_lng)
        };
        assert!(distance_m(lat, lng, back_lat, back_lng) < 1.0);
    }

    #[test]
    fn jitter_respects_cap_and_preferred_band() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let (lat, lng) = (22.528, 113.940);
        let mut in_band = 0;
        for _ in 0..400 {
            let (j_lat, j_lng) = jitter_point(&mut rng, lat, lng, 150.0);
            let dist = distance_m(lat, lng, j_lat, j_lng);
            assert!(dist <= 150.0 + 1.0, "偏移 {dist}m 超出上限");
            if dist >= 25.0 && dist <= 100.0 {
                in_band += 1;
            }
        }
        // 75% 命中偏好带，允许抽样波动。
        assert!(
            in_band > 260,
            "偏好带命中 {in_band}/400，偏离 75% 设定"
        );
    }

    #[test]
    fn jitter_small_fence_stays_inside() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let (lat, lng) = (22.526, 113.937);
        for _ in 0..200 {
            let (j_lat, j_lng) = jitter_point(&mut rng, lat, lng, 75.0);
            let dist = distance_m(lat, lng, j_lat, j_lng);
            assert!(dist <= 75.0 + 1.0, "小围栏偏移 {dist}m 超出 radius_m");
        }
    }

    #[test]
    fn distance_m_known_value() {
        // 致理楼 → 致腾楼约 380m（两楼相邻，haversine 实算）。
        let d = distance_m(22.528, 113.940, 22.526, 113.937);
        assert!(d > 300.0 && d < 450.0, "致理楼→致腾楼 {d}m");
    }
}
