use super::{ElectricityCampus, ElectricityLocationSettings};
use base16384::Base16384;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Share {
    version: u8,
    location: ElectricityLocationSettings,
}
pub(super) fn normalize_settings(
    mut settings: ElectricityLocationSettings,
) -> ElectricityLocationSettings {
    settings.room_name = settings.room_name.trim().to_owned();
    settings
}

pub(super) fn validate_shape(s: &ElectricityLocationSettings) -> Result<(), String> {
    for value in [
        &s.campus_id,
        &s.area_id,
        &s.building_id,
        &s.building_name,
        &s.room_name,
    ] {
        if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err("宿舍配置字段无效".into());
        }
    }
    super::validate_room_name(&s.room_name)?;
    Ok(())
}
pub(crate) fn encode_settings(settings: ElectricityLocationSettings) -> Result<String, String> {
    let settings = normalize_settings(settings);
    validate_shape(&settings)?;
    let data = serde_json::to_vec(&Share {
        version: 1,
        location: settings,
    })
    .map_err(|_| "无法编码宿舍配置")?;
    String::from_utf16(&Base16384::encode(&data)).map_err(|_| "无法编码宿舍配置".into())
}
pub(crate) fn decode_settings(text: &str) -> Result<ElectricityLocationSettings, String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 8192 {
        return Err("配置码为空或过长".into());
    }
    let units: Vec<_> = text.encode_utf16().collect();
    // base16384 0.1 subtracts the last chunk length before validating it.
    // Guard malformed short padding sequences so imports cannot panic.
    if let Some(padding) = units.last().copied().and_then(Base16384::padding) {
        let chunk = match padding - Base16384::PADDING_OFFSET {
            0 => 1,
            1 => 2,
            2 | 3 => 3,
            4 | 5 => 4,
            6 => 5,
            _ => return Err("配置码格式错误".into()),
        };
        if units.len() < chunk || (units.len() - chunk) % 4 != 0 {
            return Err("配置码格式错误".into());
        }
    }
    let bytes = Base16384::decode(&units).map_err(|_| "配置码格式错误")?;
    // Reject noncanonical trailing bits/invalid alphabet accepted by lenient decoders.
    if Base16384::encode(&bytes) != units {
        return Err("配置码格式错误".into());
    }
    let data: Share = serde_json::from_slice(&bytes).map_err(|_| "配置码内容无效")?;
    if data.version != 1 {
        return Err("不支持此宿舍配置版本".into());
    }
    let location = normalize_settings(data.location);
    validate_shape(&location)?;
    Ok(location)
}
pub(super) fn validate_options(
    s: &ElectricityLocationSettings,
    campuses: &[ElectricityCampus],
) -> Result<(), String> {
    let building = campuses
        .iter()
        .find(|c| c.id == s.campus_id)
        .and_then(|c| c.areas.iter().find(|a| a.id == s.area_id))
        .and_then(|a| a.buildings.iter().find(|b| b.id == s.building_id));
    if building.is_some_and(|b| b.name == s.building_name) {
        Ok(())
    } else {
        Err("配置中的校区、区域或楼栋与当前宿舍选项不匹配".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> ElectricityLocationSettings {
        ElectricityLocationSettings {
            campus_id: "yuehai".into(),
            area_id: "south".into(),
            building_id: "12".into(),
            building_name: "山茶斋".into(),
            room_name: "601".into(),
        }
    }
    #[test]
    fn rejects_invalid_rooms_in_saved_and_shared_settings_for_every_area() {
        for area in [
            "yuehai-main",
            "yuehai-xinzhai",
            "canghai-main",
            "xili-main",
            "xili-lihuo-phase2",
        ] {
            let mut location = settings();
            location.area_id = area.into();
            for room in ["601", "1201"] {
                location.room_name = room.into();
                assert!(validate_shape(&location).is_ok());
            }
            for room in ["51", "50001", "5A01", "１２０１"] {
                location.room_name = room.into();
                assert!(validate_shape(&location).is_err());
                assert!(encode_settings(location.clone()).is_err());
                let data = serde_json::to_vec(&Share {
                    version: 1,
                    location: location.clone(),
                })
                .unwrap();
                let code = String::from_utf16(&Base16384::encode(&data)).unwrap();
                assert!(decode_settings(&code).is_err());
            }
        }
    }

    #[test]
    fn saved_and_legacy_share_codes_return_canonical_room_names() {
        for area in [
            "yuehai-main",
            "yuehai-xinzhai",
            "canghai-main",
            "xili-main",
            "xili-lihuo-phase2",
        ] {
            for room in [" 601 ", " 1201 ", "\t601\n"] {
                let mut padded = settings();
                padded.area_id = area.into();
                padded.room_name = room.into();
                let mut canonical = padded.clone();
                canonical.room_name = room.trim().into();
                let code = encode_settings(padded.clone()).unwrap();
                assert_eq!(decode_settings(&code).unwrap(), canonical);
                let raw: Share = serde_json::from_slice(
                    &Base16384::decode(&code.encode_utf16().collect::<Vec<_>>()).unwrap(),
                )
                .unwrap();
                assert_eq!(raw.location, canonical);
                let legacy = serde_json::to_vec(&Share {
                    version: 1,
                    location: padded,
                })
                .unwrap();
                let legacy_code = String::from_utf16(&Base16384::encode(&legacy)).unwrap();
                assert_eq!(decode_settings(&legacy_code).unwrap(), canonical);
            }
        }
    }

    #[test]
    fn sharing_roundtrip_is_versioned_and_rejects_malformed_data() {
        let s = settings();
        let code = encode_settings(s.clone()).unwrap();
        assert_eq!(decode_settings(&format!("  {code}\n")).unwrap(), s);
        assert!(decode_settings("invalid").is_err());
        for padding in 1..=6 {
            let short = String::from_utf16(&[Base16384::PADDING_OFFSET + padding]).unwrap();
            assert!(decode_settings(&short).is_err());
        }
        assert!(decode_settings(&"一".repeat(9000)).is_err());
        let code =
            String::from_utf16(&Base16384::encode(br#"{"version":2,"location":{}}"#)).unwrap();
        assert!(decode_settings(&code).is_err());
        assert!(validate_options(&s, &[]).is_err());
        let mut bad = s;
        bad.room_name = "\n".into();
        assert!(encode_settings(bad).is_err());
    }
}
