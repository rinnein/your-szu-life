//! 课表教室/楼栋名 → 内置楼栋围栏的匹配。
//!
//! 算法与验证结论来自 `docs/qw-attendance/course-room-matching.md`：
//! 中文名精确匹配优先、英文名精确匹配（重复视为歧义）、classroom 前缀取
//! 最长唯一、子串兜底。禁止按列表顺序取第一个命中——「父楼栋 + 具体场馆」
//! 的名称结构会让顺序匹配错 46/174 次。

use super::model::Building;

/// 匹配依据。中文精确 > 英文精确 > 教室前缀 > 教室包含。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchedBy {
    NameExact,
    NameEnExact,
    ClassroomPrefix,
    ClassroomContains,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchError {
    /// 多个候选并列（含英文名二义性），宁可停止也不要猜。
    Ambiguous,
    NotFound,
}

/// 规范化：trim + 全角转半角（NFKC 的常用子集）+ 去掉所有空白。
pub fn normalize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.trim().chars() {
        match ch {
            '\u{3000}' => {} // 全角空格
            ch if ('\u{FF01}'..='\u{FF5E}').contains(&ch) => {
                // 全角 ASCII 区（！～）平移回半角；全角空格已单独剔除
                let half = (ch as u32 - 0xFEE0) as u8 as char;
                if !half.is_whitespace() {
                    out.push(half);
                }
            }
            ch if ch.is_whitespace() => {}
            ch => out.push(ch),
        }
    }
    out
}

/// 按文档建议算法匹配楼栋。
pub fn match_building<'a>(
    buildings: &'a [Building],
    building_name: Option<&str>,
    building_name_en: Option<&str>,
    classroom: Option<&str>,
) -> Result<(&'a Building, MatchedBy), MatchError> {
    // 1. 课表自带 building_name：中文名精确优先（中文名无重名）。
    if let Some(name) = non_empty(building_name) {
        let key = normalize(name);
        let hits: Vec<&Building> = buildings
            .iter()
            .filter(|b| normalize(&b.name) == key)
            .collect();
        match hits.len() {
            1 => return Ok((hits[0], MatchedBy::NameExact)),
            2.. => return Err(MatchError::Ambiguous),
            _ => {}
        }
        // 1.2 英文名精确；Zhiyuan Building 同对应致远楼/致原楼两处，>1 即歧义。
        if let Some(en) = non_empty(building_name_en) {
            let key = normalize(en);
            let hits: Vec<&Building> = buildings
                .iter()
                .filter(|b| normalize(&b.name_en) == key)
                .collect();
            match hits.len() {
                1 => return Ok((hits[0], MatchedBy::NameEnExact)),
                2.. => return Err(MatchError::Ambiguous),
                _ => {}
            }
        }
    }
    // 2. classroom 兜底：前缀匹配按命中名长度降序取最长且唯一。
    let Some(room) = non_empty(classroom) else {
        return Err(MatchError::NotFound);
    };
    let room = normalize(room);
    let mut prefixed: Vec<(&Building, usize)> = buildings
        .iter()
        .filter_map(|b| {
            for candidate in [normalize(&b.name), normalize(&b.name_en)] {
                if candidate.is_empty() {
                    continue;
                }
                if room == candidate || room.starts_with(&candidate) {
                    return Some((b, candidate.chars().count()));
                }
            }
            None
        })
        .collect();
    if !prefixed.is_empty() {
        if let Some(hit) = longest_unique(&mut prefixed) {
            return Ok((hit, MatchedBy::ClassroomPrefix));
        }
        return Err(MatchError::Ambiguous);
    }
    // 2.2 仍无结果：楼栋名出现在 classroom 中，同样取最长且唯一。
    let mut contained: Vec<(&Building, usize)> = buildings
        .iter()
        .filter_map(|b| {
            let candidate = normalize(&b.name);
            (!candidate.is_empty() && room.contains(&candidate))
                .then(|| (b, candidate.chars().count()))
        })
        .collect();
    if !contained.is_empty() {
        if let Some(hit) = longest_unique(&mut contained) {
            return Ok((hit, MatchedBy::ClassroomContains));
        }
        return Err(MatchError::Ambiguous);
    }
    Err(MatchError::NotFound)
}

fn non_empty(raw: Option<&str>) -> Option<&str> {
    raw.map(str::trim).filter(|s| !s.is_empty())
}

/// 长度降序后取最大长度组；组内多于一个即歧义。
fn longest_unique<'a, T>(candidates: &mut Vec<(&'a T, usize)>) -> Option<&'a T> {
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let (first, first_len) = candidates[0];
    if candidates.len() > 1 && candidates[1].1 == first_len {
        return None;
    }
    Some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signin::model::Building;

    fn building(id: i64, name: &str, name_en: &str, status: &str) -> Building {
        Building {
            id,
            name: name.into(),
            name_en: name_en.into(),
            campus: "深圳大学".into(),
            latitude_gcj02: 22.5,
            longitude_gcj02: 113.9,
            radius_m: 75,
            status: status.into(),
        }
    }

    fn fixtures() -> Vec<Building> {
        vec![
            building(79, "致理楼", "Zhili Building", "listed"),
            building(73, "致腾楼", "Zhiteng Building", "listed"),
            building(74, "致远楼", "Zhiyuan Building", "listed"),
            building(152, "致原楼", "Zhiyuan Building", "provisional"),
            building(84, "汇文楼", "Huiwen Building", "listed"),
            building(101, "致工楼", "Zhigong Building", "listed"),
            building(102, "致工楼二楼", "Zhigong Building 2F", "listed"),
            building(135, "南秋馆游泳池", "Nanqiu Swimming Pool", "placeholder"),
        ]
    }

    #[test]
    fn normalize_fullwidth_and_spaces() {
        assert_eq!(normalize("  致理楼 L1-203 "), "致理楼L1-203");
        assert_eq!(normalize("致理楼Ｌ１－２０３"), "致理楼L1-203");
        assert_eq!(normalize("汇文楼\u{3000}H-04"), "汇文楼H-04");
    }

    #[test]
    fn building_name_exact_beats_everything() {
        let list = fixtures();
        let (hit, by) = match_building(&list, Some("致理楼"), Some("Zhili Building"), Some("别的楼X"))
            .expect("应精确命中");
        assert_eq!(hit.id, 79);
        assert_eq!(by, MatchedBy::NameExact);
    }

    #[test]
    fn english_ambiguity_stops_instead_of_guessing() {
        let list = fixtures();
        // 中文名未命中时回退英文名：Zhiyuan Building 同时对应致远楼/致原楼，
        // 必须歧义停止（文档中的真实二义案例）。
        let result = match_building(&list, Some("不存在的中文"), Some("Zhiyuan Building"), None);
        assert!(matches!(result, Err(MatchError::Ambiguous)));
        // 中文名精确命中时不受英文名二义性影响。
        let (hit, by) =
            match_building(&list, Some("致原楼"), Some("Zhiyuan Building"), None).expect("中文优先");
        assert_eq!(hit.id, 152);
        assert_eq!(by, MatchedBy::NameExact);
        // building_name 与 classroom 全空：按文档算法直接 NotFound。
        let result = match_building(&list, None, Some("Zhiyuan Building"), None);
        assert!(matches!(result, Err(MatchError::NotFound)));
    }

    #[test]
    fn classroom_prefix_prefers_longest() {
        let list = fixtures();
        // 致工楼二楼 必须命中子楼栋而不是父楼栋（顺序匹配会错）。
        let (hit, by) =
            match_building(&list, None, None, Some("致工楼二楼201")).expect("前缀应命中");
        assert_eq!(hit.id, 102);
        assert_eq!(by, MatchedBy::ClassroomPrefix);
    }

    #[test]
    fn classroom_prefix_normal_classrooms() {
        let list = fixtures();
        for (room, expected) in [
            ("致理楼L1-203", 79),
            ("致腾楼328", 73),
            ("汇文楼H-04", 84),
        ] {
            let (hit, _) = match_building(&list, None, None, Some(room)).expect(room);
            assert_eq!(hit.id, expected, "{room}");
        }
    }

    #[test]
    fn classroom_contains_fallback() {
        let list = fixtures();
        // 不是前缀，但楼栋名出现在教室名中间。
        let (hit, by) =
            match_building(&list, None, None, Some("深大致理楼L301")).expect("包含应命中");
        assert_eq!(hit.id, 79);
        assert_eq!(by, MatchedBy::ClassroomContains);
    }

    #[test]
    fn not_found_and_empty_inputs() {
        let list = fixtures();
        let result = match_building(&list, None, None, Some("不存在的教室"));
        assert!(matches!(result, Err(MatchError::NotFound)));
        let result = match_building(&list, Some("不存在的楼"), None, None);
        assert!(matches!(result, Err(MatchError::NotFound)));
        let result = match_building(&list, None, None, None);
        assert!(matches!(result, Err(MatchError::NotFound)));
    }
}
