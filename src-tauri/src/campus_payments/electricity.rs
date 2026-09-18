use super::model::ElectricityLocationSettings;
use crate::storage::SharedCredentialStore;
use encoding_rs::GB18030;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};

const SIMS_BASE: &str = "http://192.168.84.3:9090";
const LIHUO_BASE: &str = "http://172.25.100.105:8010";
const MAX_BODY: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityBuilding {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityArea {
    pub id: String,
    pub name: String,
    pub campus_id: String,
    pub buildings: Vec<ElectricityBuilding>,
    pub provider: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityCampus {
    pub id: String,
    pub name: String,
    pub areas: Vec<ElectricityArea>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityReading {
    pub date: String,
    pub room_name: String,
    pub remaining: f64,
    pub total_used: f64,
    pub total_purchased: f64,
    pub daily_used: Option<f64>,
    pub daily_purchased: Option<f64>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityQueryView {
    pub campus_id: String,
    pub area_id: String,
    pub building_name: String,
    pub room_name: String,
    pub begin_date: String,
    pub end_date: String,
    pub readings: Vec<ElectricityReading>,
    pub current_remaining: Option<f64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityQueryInput {
    pub campus_id: String,
    pub area_id: String,
    pub building_id: String,
    pub building_name: String,
    pub room_name: String,
    pub begin_date: String,
    pub end_date: String,
}

#[derive(Clone, Default)]
pub(crate) struct ElectricityStore {
    pub(crate) credentials: Option<SharedCredentialStore>,
}

impl ElectricityStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials: Some(credentials),
        }
    }
}

#[derive(Clone, Copy)]
struct SimsArea {
    id: &'static str,
    name: &'static str,
    campus_id: &'static str,
    client: &'static str,
}

const SIMS_AREAS: [SimsArea; 4] = [
    SimsArea {
        id: "yuehai-main",
        name: "粤海校区宿舍",
        campus_id: "yuehai",
        client: "192.168.84.1",
    },
    SimsArea {
        id: "yuehai-xinzhai",
        name: "深大新斋区",
        campus_id: "yuehai",
        client: "192.168.84.87",
    },
    SimsArea {
        id: "canghai-main",
        name: "沧海校区宿舍",
        campus_id: "canghai",
        client: "192.168.84.110",
    },
    SimsArea {
        id: "xili-main",
        name: "丽湖校区宿舍",
        campus_id: "xili",
        client: "172.21.101.11",
    },
];

fn client() -> Result<Client, String> {
    Client::builder()
        .no_proxy()
        .cookie_store(true)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(12))
        .user_agent("your-szu-life/0.1")
        .build()
        .map_err(|_| "无法初始化电费查询网络客户端".into())
}

fn body(response: Response) -> Result<String, String> {
    if !response.status().is_success() {
        return Err(format!("电费接口返回 HTTP {}", response.status().as_u16()));
    }
    let bytes = response
        .bytes()
        .map_err(|_| "无法读取电费接口响应".to_owned())?;
    if bytes.len() > MAX_BODY {
        return Err("电费接口响应过大".into());
    }
    Ok(GB18030.decode(&bytes).0.into_owned())
}

fn clean_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&#160;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn select_options(html: &str, name: &str) -> Vec<ElectricityBuilding> {
    let marker = format!("name=\"{name}\"");
    let Some(start) = html.find(&marker) else {
        return Vec::new();
    };
    let Some(select_start) = html[..start].rfind("<select") else {
        return Vec::new();
    };
    let Some(end_rel) = html[start..].find("</select>") else {
        return Vec::new();
    };
    let block = &html[select_start..start + end_rel];
    let mut result = Vec::new();
    let mut cursor = 0;
    while let Some(pos) = block[cursor..].find("<option") {
        let begin = cursor + pos;
        let Some(tag_end) = block[begin..].find('>') else {
            break;
        };
        let tag = &block[begin..begin + tag_end];
        let Some(value_pos) = tag.find("value=\"") else {
            cursor = begin + tag_end + 1;
            continue;
        };
        let value_start = value_pos + 7;
        let Some(value_end) = tag[value_start..].find('"') else {
            cursor = begin + tag_end + 1;
            continue;
        };
        let id = tag[value_start..value_start + value_end].trim();
        let content_start = begin + tag_end + 1;
        let Some(close_rel) = block[content_start..].find("</option>") else {
            break;
        };
        let name = clean_html(&block[content_start..content_start + close_rel]);
        if !id.is_empty() && !name.is_empty() && name != "请选择" && name != "楼栋" {
            result.push(ElectricityBuilding {
                id: id.into(),
                name: normalize_building(&name),
            });
        }
        cursor = content_start + close_rel + 9;
    }
    result
}

fn normalize_building(value: &str) -> String {
    let mut value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    while value.ends_with('#') {
        value.pop();
    }
    value.trim().to_owned()
}

pub(crate) fn options() -> Result<Vec<ElectricityCampus>, String> {
    let http = client()?;
    let mut campuses: BTreeMap<&str, ElectricityCampus> = BTreeMap::new();
    for (id, name) in [
        ("yuehai", "北校区（粤海校区）"),
        ("canghai", "南校区（沧海校区）"),
        ("xili", "西丽校区（丽湖校区）"),
    ] {
        campuses.insert(
            id,
            ElectricityCampus {
                id: id.into(),
                name: name.into(),
                areas: Vec::new(),
            },
        );
    }
    for area in SIMS_AREAS {
        let url = format!(
            "{SIMS_BASE}/cgcSims/login.do?task=station&client={}",
            area.client
        );
        let html = body(
            http.get(url)
                .send()
                .map_err(|_| "无法连接 SIMS 电费接口".to_owned())?,
        )?;
        campuses
            .get_mut(area.campus_id)
            .unwrap()
            .areas
            .push(ElectricityArea {
                id: area.id.into(),
                name: area.name.into(),
                campus_id: area.campus_id.into(),
                provider: "sims".into(),
                buildings: select_options(&html, "buildingId"),
            });
    }
    let html = body(
        http.get(LIHUO_BASE)
            .send()
            .map_err(|_| "无法连接丽湖二期电费接口".to_owned())?,
    )?;
    campuses
        .get_mut("xili")
        .unwrap()
        .areas
        .push(ElectricityArea {
            id: "xili-lihuo-phase2".into(),
            name: "丽湖二期".into(),
            campus_id: "xili".into(),
            provider: "lihuoPhase2".into(),
            buildings: select_options(&html, "drlouming"),
        });
    Ok(campuses.into_values().collect())
}

fn valid_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
}

fn validate(input: &ElectricityQueryInput) -> Result<(), String> {
    if input.room_name.trim().is_empty() || input.room_name.len() > 32 {
        return Err("请输入有效的宿舍号".into());
    }
    if !valid_date(&input.begin_date) || !valid_date(&input.end_date) {
        return Err("日期格式应为 YYYY-MM-DD".into());
    }
    if input.begin_date > input.end_date {
        return Err("开始日期不能晚于结束日期".into());
    }
    Ok(())
}

fn parse_number(value: &str) -> Option<f64> {
    value.trim().replace(',', "").parse().ok()
}

fn sims_query(input: &ElectricityQueryInput) -> Result<Vec<ElectricityReading>, String> {
    let client_ip = SIMS_AREAS
        .iter()
        .find(|area| area.id == input.area_id)
        .map(|x| x.client)
        .ok_or("未知的电费宿舍区域")?;
    let http = client()?;
    let station = body(
        http.get(format!(
            "{SIMS_BASE}/cgcSims/login.do?task=station&client={client_ip}"
        ))
        .send()
        .map_err(|_| "无法连接 SIMS 电费接口".to_owned())?,
    )?;
    let action = form_action(&station).unwrap_or_else(|| "/cgcSims/login.do".into());
    let room_form = [
        ("client", client_ip),
        ("buildingId", input.building_id.as_str()),
        ("buildingName", input.building_name.as_str()),
        ("roomName", input.room_name.as_str()),
        ("select", " 查询 "),
    ];
    let room_page = body(
        http.post(format!("{SIMS_BASE}{action}"))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&room_form))
            .send()
            .map_err(|_| "无法解析 SIMS 宿舍信息".to_owned())?,
    )?;
    let room_id = hidden(&room_page, "roomId").ok_or("SIMS 未找到该宿舍，请检查楼栋和宿舍号")?;
    let building_name =
        hidden(&room_page, "building").unwrap_or_else(|| input.building_name.clone());
    let form = [
        ("hiddenType", ""),
        ("isHost", "0"),
        ("beginTime", &input.begin_date),
        ("endTime", &input.end_date),
        ("type", "2"),
        ("client", client_ip),
        ("roomId", room_id.as_str()),
        ("roomName", input.room_name.as_str()),
        ("building", building_name.as_str()),
    ];
    let encoded = encode_form_gb18030(&form);
    let query_action = form_action(&room_page).unwrap_or_else(|| "/cgcSims/selectList.do".into());
    let response = http
        .post(format!("{SIMS_BASE}{query_action}"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encoded)
        .send()
        .map_err(|_| "无法连接 SIMS 电费查询接口".to_owned())?;
    parse_sims(&body(response)?, &input.room_name)
}

fn form_action(html: &str) -> Option<String> {
    let start = html.find("<form")?;
    let end = start + html[start..].find('>')?;
    let tag = &html[start..end];
    let marker = "action=\"";
    let at = tag.find(marker)? + marker.len();
    let close = tag[at..].find('"')?;
    Some(tag[at..at + close].to_owned())
}

fn hidden(html: &str, name: &str) -> Option<String> {
    let marker = format!("name=\"{name}\"");
    let start = html.find(&marker)?;
    let value_start = start + html[start..].find("value=\"")? + 7;
    let end = html[value_start..].find('"')?;
    Some(html[value_start..value_start + end].trim().to_owned())
}

fn row_cells(row: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(pos) = row[cursor..].find("<td") {
        let begin = cursor + pos;
        let Some(tag_end) = row[begin..].find('>') else {
            break;
        };
        let content = begin + tag_end + 1;
        let Some(end) = row[content..].find("</td>") else {
            break;
        };
        out.push(clean_html(&row[content..content + end]));
        cursor = content + end + 5;
    }
    out
}

fn parse_sims(html: &str, room: &str) -> Result<Vec<ElectricityReading>, String> {
    let Some(table_start) = html.find("id=\"oTable\"") else {
        return Ok(Vec::new());
    };
    let table = &html[html[..table_start].rfind("<table").unwrap_or(table_start)..];
    let mut rows = Vec::new();
    let mut cursor = 0;
    while let Some(pos) = table[cursor..].find("<tr") {
        let begin = cursor + pos;
        let Some(open) = table[begin..].find('>') else {
            break;
        };
        let content = begin + open + 1;
        let Some(end) = table[content..].find("</tr>") else {
            break;
        };
        rows.push(row_cells(&table[content..content + end]));
        cursor = content + end + 5;
    }
    let mut result = Vec::new();
    for cells in rows.into_iter().filter(|cells| cells.len() >= 6) {
        let (Some(remaining), Some(total_used), Some(total_purchased)) = (
            parse_number(&cells[2]),
            parse_number(&cells[3]),
            parse_number(&cells[4]),
        ) else {
            continue;
        };
        result.push(ElectricityReading {
            date: cells[5].clone(),
            room_name: if cells[1].is_empty() {
                room.into()
            } else {
                cells[1].clone()
            },
            remaining,
            total_used,
            total_purchased,
            daily_used: None,
            daily_purchased: None,
        });
    }
    result.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(with_daily_changes(result))
}

fn with_daily_changes(mut rows: Vec<ElectricityReading>) -> Vec<ElectricityReading> {
    for index in 1..rows.len() {
        let (previous, current) = (&rows[index - 1].clone(), &mut rows[index]);
        current.daily_used = Some((current.total_used - previous.total_used).max(0.0));
        current.daily_purchased =
            Some((current.total_purchased - previous.total_purchased).max(0.0));
    }
    rows
}

fn li_huo_query(input: &ElectricityQueryInput) -> Result<Vec<ElectricityReading>, String> {
    let http = client()?;
    let mut html = body(
        http.get(LIHUO_BASE)
            .send()
            .map_err(|_| "无法连接丽湖二期电费接口".to_owned())?,
    )?;
    let mut viewstate = hidden(&html, "__VIEWSTATE").unwrap_or_default();
    let generator = hidden(&html, "__VIEWSTATEGENERATOR").unwrap_or_default();
    let floor = input
        .room_name
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    let floor = if floor.len() > 2 {
        &floor[..floor.len() - 2]
    } else {
        return Err("丽湖二期宿舍号至少需要包含楼层和房间号".into());
    };
    let floor = format!("{}{:0>2}", input.building_id, floor);
    let params = [
        ("__VIEWSTATE", viewstate.as_str()),
        ("__VIEWSTATEGENERATOR", generator.as_str()),
        ("drlouming", input.building_id.as_str()),
        ("drceng", ""),
        ("drfangjian", ""),
        ("radio", "usedR"),
    ];
    html = body(
        http.post(LIHUO_BASE)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&params))
            .send()
            .map_err(|_| "无法加载丽湖二期楼层信息".to_owned())?,
    )?;
    viewstate = hidden(&html, "__VIEWSTATE").unwrap_or_default();
    let generator = hidden(&html, "__VIEWSTATEGENERATOR").unwrap_or_default();
    let params = [
        ("__VIEWSTATE", viewstate.as_str()),
        ("__VIEWSTATEGENERATOR", generator.as_str()),
        ("drlouming", input.building_id.as_str()),
        ("drceng", floor.as_str()),
        ("drfangjian", ""),
        ("radio", "usedR"),
    ];
    html = body(
        http.post(LIHUO_BASE)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&params))
            .send()
            .map_err(|_| "无法加载丽湖二期楼层信息".to_owned())?,
    )?;
    viewstate = hidden(&html, "__VIEWSTATE").unwrap_or_default();
    let generator = hidden(&html, "__VIEWSTATEGENERATOR").unwrap_or_default();
    let room_digits = input
        .room_name
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    let room_id = select_options(&html, "drfangjian")
        .into_iter()
        .find(|x| x.name.ends_with(&room_digits))
        .map(|x| x.id)
        .ok_or("丽湖二期未找到该宿舍号")?;
    let params = [
        ("__VIEWSTATE", viewstate.as_str()),
        ("__VIEWSTATEGENERATOR", generator.as_str()),
        ("drlouming", input.building_id.as_str()),
        ("drceng", floor.as_str()),
        ("drfangjian", room_id.as_str()),
        ("radio", "usedR"),
        ("ImageButton1.x", "1"),
        ("ImageButton1.y", "1"),
    ];
    html = body(
        http.post(LIHUO_BASE)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&params))
            .send()
            .map_err(|_| "无法查询丽湖二期电费记录".to_owned())?,
    )?;
    parse_lihuo(&html, &input.room_name)
}

fn encode_form_gb18030(values: &[(&str, &str)]) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{}={}", form_component(key), form_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_component(value: &str) -> String {
    let (bytes, _, _) = GB18030.encode(value);
    bytes
        .iter()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(*byte).to_string()
            }
            b' ' => "+".to_owned(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn parse_lihuo(html: &str, room: &str) -> Result<Vec<ElectricityReading>, String> {
    parse_sims(html, room)
}

pub(crate) fn query(input: ElectricityQueryInput) -> Result<ElectricityQueryView, String> {
    validate(&input)?;
    let readings = if input.area_id == "xili-lihuo-phase2" {
        li_huo_query(&input)?
    } else {
        sims_query(&input)?
    };
    let current_remaining = readings.last().map(|row| row.remaining);
    Ok(ElectricityQueryView {
        campus_id: input.campus_id,
        area_id: input.area_id,
        building_name: normalize_building(&input.building_name),
        room_name: input.room_name.trim().into(),
        begin_date: input.begin_date,
        end_date: input.end_date,
        readings,
        current_remaining,
    })
}

pub(crate) fn load_settings(
    store: &ElectricityStore,
) -> Result<Option<ElectricityLocationSettings>, String> {
    store
        .credentials
        .as_ref()
        .ok_or("电费设置存储不可用")?
        .load_electricity_settings()
}
pub(crate) fn save_settings(
    store: &ElectricityStore,
    settings: ElectricityLocationSettings,
) -> Result<(), String> {
    store
        .credentials
        .as_ref()
        .ok_or("电费设置存储不可用")?
        .save_electricity_settings(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_buildings_and_parses_options() {
        let html = r#"<select name="buildingId"><option value="1">  山茶斋   </option><option value="2">B3文韬楼#</option></select>"#;
        assert_eq!(select_options(html, "buildingId")[1].name, "B3文韬楼");
    }
    #[test]
    fn parses_sims_rows_and_daily_changes() {
        let html = r#"<table id="oTable"><tr><td>1</td><td>101</td><td>10</td><td>2</td><td>4</td><td>2026-01-01</td></tr><tr><td>2</td><td>101</td><td>8</td><td>5</td><td>4</td><td>2026-01-02</td></tr></table>"#;
        let rows = parse_sims(html, "101").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].daily_used, Some(3.0));
    }
    #[test]
    fn validates_dates_and_room() {
        let input = ElectricityQueryInput {
            campus_id: "x".into(),
            area_id: "x".into(),
            building_id: "x".into(),
            building_name: "x".into(),
            room_name: "101".into(),
            begin_date: "2026-01-02".into(),
            end_date: "2026-01-01".into(),
        };
        assert!(validate(&input).is_err());
    }

    #[test]
    fn encodes_sims_form_like_the_gb2312_page() {
        let encoded = encode_form_gb18030(&[
            ("client", "192.168.84.110"),
            ("buildingName", ""),
            ("buildingId", "6878"),
            ("roomName", "648"),
            ("select", " 查询 "),
        ]);
        assert_eq!(
            encoded,
            "client=192.168.84.110&buildingName=&buildingId=6878&roomName=648&select=+%B2%E9%D1%AF+"
        );
    }
}
