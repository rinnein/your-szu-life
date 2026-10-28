use super::model::{
    ElectricityCurrentCache, ElectricityLocationSettings, ElectricityMonitorRecord,
    ElectricityStatusView,
};
use crate::storage::SharedCredentialStore;
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime};
use encoding_rs::GB18030;
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

pub(crate) const ELECTRICITY_STATUS_EVENT: &str = "electricity-status-changed";
mod analytics;
pub(crate) mod db;
pub(crate) mod history;
mod providers;
mod sharing;
pub(crate) use analytics::{constants, ElectricityConstants, YUAN_PER_KWH};
pub(crate) use providers::Recharge;
pub(crate) use sharing::encode_settings;
pub(crate) fn import_settings(text: &str) -> Result<ElectricityLocationSettings, String> {
    let settings = sharing::decode_settings(text)?;
    sharing::validate_options(&settings, &options()?)?;
    Ok(settings)
}
pub(crate) fn recharges(
    input: ElectricityQueryInput,
    store: &ElectricityStore,
) -> Result<Vec<Recharge>, String> {
    history::recharges(store, input)
}
pub(crate) fn cached_detail(
    input: ElectricityQueryInput,
    store: &ElectricityStore,
) -> Result<Option<ElectricityQueryView>, String> {
    history::snapshot(store, input)
}
fn fetch_recharges(input: &ElectricityQueryInput) -> Result<Vec<Recharge>, String> {
    if input.area_id == "xili-lihuo-phase2" {
        providers::lihu_recharges(input)
    } else {
        providers::sims_recharges(input)
    }
}

pub(crate) const LOW_THRESHOLD: f64 = 5.0;

const SIMS_BASE: &str = "http://192.168.84.3:9090";
const LIHUO_BASE: &str = "http://172.25.100.105:8010";
const MAX_BODY: usize = 2 * 1024 * 1024;
// Portal limit counts both the start and end date.
const MAX_QUERY_DAYS: i64 = 20;

fn query_windows(input: &ElectricityQueryInput) -> Result<Vec<ElectricityQueryInput>, String> {
    validate(input)?;
    let mut start = query_date(&input.begin_date).unwrap();
    let end = query_date(&input.end_date).unwrap();
    let mut windows = Vec::new();
    while start <= end {
        let finish = start
            .checked_add_signed(ChronoDuration::days(MAX_QUERY_DAYS - 1))
            .unwrap_or(end)
            .min(end);
        let mut chunk = input.clone();
        chunk.room_name = input.room_name.trim().to_owned();
        chunk.begin_date = start.to_string();
        chunk.end_date = finish.to_string();
        windows.push(chunk);
        if finish == end {
            break;
        }
        start = finish.succ_opt().ok_or("查询日期超出范围")?;
    }
    Ok(windows)
}

fn query_in_windows<T>(
    input: &ElectricityQueryInput,
    mut fetch: impl FnMut(&ElectricityQueryInput) -> Result<Vec<T>, String>,
) -> Result<Vec<T>, String> {
    let mut rows = Vec::new();
    for chunk in query_windows(input)? {
        rows.extend(fetch(&chunk)?);
    }
    Ok(rows)
}

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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityReading {
    pub date: String,
    pub room_name: String,
    pub remaining: Option<f64>,
    pub total_used: Option<f64>,
    pub total_purchased: Option<f64>,
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
    pub summary: analytics::Summary,
    pub recharges: Vec<providers::Recharge>,
    pub recharge_error: Option<String>,
    pub cache: Option<history::CacheInfo>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityQueryInput {
    pub campus_id: String,
    pub area_id: String,
    pub building_id: String,
    pub building_name: String,
    pub room_name: String,
    #[serde(default)]
    pub begin_date: String,
    #[serde(default)]
    pub end_date: String,
}

#[derive(Clone, Default)]
pub(crate) struct ElectricityStore {
    pub(crate) credentials: Option<SharedCredentialStore>,
    pub(crate) runtime: Arc<Mutex<ElectricityRuntime>>,
    history: Arc<Mutex<history::History>>,
    query_lock: Arc<Mutex<()>>,
}

#[derive(Default)]
pub(crate) struct ElectricityRuntime {
    pub(crate) background: bool,
    pub(crate) in_flight: bool,
    pub(crate) generation: u64,
}

impl ElectricityStore {
    pub(crate) fn new(credentials: SharedCredentialStore) -> Self {
        Self {
            credentials: Some(credentials),
            runtime: Arc::new(Mutex::new(ElectricityRuntime::default())),
            history: Arc::default(),
            query_lock: Arc::default(),
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
    // 电费接口都在内网：走不走代理交给 netproxy 按目标地址判断。
    crate::netproxy::blocking_client_builder()
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
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut response.take(MAX_BODY as u64 + 1), &mut bytes)
        .map_err(|_| "无法读取电费接口响应".to_owned())?;
    if bytes.len() > MAX_BODY {
        return Err("电费接口响应过大".into());
    }
    Ok(GB18030.decode(&bytes).0.into_owned())
}

fn select_options(html: &str, name: &str) -> Vec<ElectricityBuilding> {
    let document = scraper::Html::parse_document(html);
    let select = scraper::Selector::parse("select").unwrap();
    let option = scraper::Selector::parse("option").unwrap();
    let Some(select) = document
        .select(&select)
        .find(|e| e.value().attr("name") == Some(name))
    else {
        return vec![];
    };
    select
        .select(&option)
        .filter_map(|e| {
            let id = e.value().attr("value")?.trim();
            let name = normalize_building(&e.text().collect::<String>());
            (!id.is_empty() && !name.is_empty() && name != "请选择" && name != "楼栋").then(|| {
                ElectricityBuilding {
                    id: id.into(),
                    name,
                }
            })
        })
        .collect()
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

fn query_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok()
}

fn validate_room_name(room_name: &str) -> Result<(), String> {
    let room_name = room_name.trim();
    if !(3..=4).contains(&room_name.len()) || !room_name.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("宿舍号需为一至两位楼层加两位房间号，例如 601 或 1201".into());
    }
    Ok(())
}

fn validate(input: &ElectricityQueryInput) -> Result<(), String> {
    validate_room_name(&input.room_name)?;
    let (Some(begin_date), Some(end_date)) =
        (query_date(&input.begin_date), query_date(&input.end_date))
    else {
        return Err("日期格式应为 YYYY-MM-DD".into());
    };
    if begin_date > end_date {
        return Err("开始日期不能晚于结束日期".into());
    }
    Ok(())
}

fn parse_number(value: &str) -> Option<f64> {
    value
        .trim()
        .replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

fn sims_query(input: &ElectricityQueryInput) -> Result<Vec<ElectricityReading>, String> {
    query_in_windows(input, sims_query_chunk)
}

fn sims_query_chunk(input: &ElectricityQueryInput) -> Result<Vec<ElectricityReading>, String> {
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
        http.post(providers::safe_url(
            &format!("{SIMS_BASE}/cgcSims/login.do"),
            &action,
        )?)
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
        .post(providers::safe_url(
            &format!("{SIMS_BASE}/cgcSims/selectList.do"),
            &query_action,
        )?)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encoded)
        .send()
        .map_err(|_| "无法连接 SIMS 电费查询接口".to_owned())?;
    let mut rows = parse_sims(&body(response)?, &input.room_name)?;
    rows.retain(|row| providers::in_window(&row.date, input));
    Ok(rows)
}

fn form_action(html: &str) -> Option<String> {
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("form[action]").unwrap();
    document
        .select(&selector)
        .next()?
        .value()
        .attr("action")
        .map(str::to_owned)
}

fn hidden(html: &str, name: &str) -> Option<String> {
    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("input[name]").unwrap();
    document
        .select(&selector)
        .find(|e| e.value().attr("name") == Some(name))?
        .value()
        .attr("value")
        .map(|v| v.trim().to_owned())
}

fn parse_sims(html: &str, room: &str) -> Result<Vec<ElectricityReading>, String> {
    let document = scraper::Html::parse_document(html);
    let table_selector = scraper::Selector::parse("#oTable").unwrap();
    let table = document
        .select(&table_selector)
        .next()
        .ok_or("无法识别 SIMS 用电记录表格")?;
    let row_selector = scraper::Selector::parse("tr").unwrap();
    let cell_selector = scraper::Selector::parse("td, th").unwrap();
    let mut result = Vec::new();
    for row in table.select(&row_selector) {
        let cells: Vec<String> = row
            .select(&cell_selector)
            .map(|c| c.text().collect::<String>().trim().to_owned())
            .collect();
        if cells.len() < 6 || reading_datetime(&cells[5]).is_none() {
            continue;
        }
        result.push(ElectricityReading {
            date: cells[5].clone(),
            room_name: if cells[1].is_empty() {
                room.into()
            } else {
                cells[1].clone()
            },
            remaining: parse_number(&cells[2]),
            total_used: parse_number(&cells[3]),
            total_purchased: parse_number(&cells[4]),
            daily_used: None,
            daily_purchased: None,
        });
    }
    Ok(result)
}

fn with_daily_changes(rows: Vec<ElectricityReading>) -> Vec<ElectricityReading> {
    analytics::normalize(rows)
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryPurpose {
    Detail,
    Monitor,
}

fn fetch_usage(input: &ElectricityQueryInput) -> Result<providers::ProviderData, String> {
    if input.area_id == "xili-lihuo-phase2" {
        providers::lihuo(input, QueryPurpose::Monitor)
    } else {
        Ok(providers::ProviderData::new(sims_query(input)?))
    }
}

fn build_view(
    input: ElectricityQueryInput,
    mut data: providers::ProviderData,
    today: NaiveDate,
    balance_date: Option<String>,
) -> ElectricityQueryView {
    data.readings.retain(|r| {
        reading_date(&r.date)
            .is_some_and(|d| d >= query_date(&input.begin_date).unwrap() && d <= today)
    });
    let readings = with_daily_changes(data.readings);
    let current = readings.iter().rev().find(|r| r.remaining.is_some());
    let remaining = data
        .current_remaining
        .or_else(|| current.and_then(|r| r.remaining));
    let observed = if data.current_remaining.is_some() {
        balance_date
    } else {
        current.map(|r| r.date.clone())
    };
    let summary = analytics::summarize(&readings, remaining, observed, today);
    ElectricityQueryView {
        campus_id: input.campus_id,
        area_id: input.area_id,
        building_name: normalize_building(&input.building_name),
        room_name: input.room_name.trim().into(),
        begin_date: input.begin_date,
        end_date: input.end_date,
        readings,
        current_remaining: remaining,
        summary,
        recharges: data.recharges,
        recharge_error: data.recharge_error,
        cache: None,
    }
}

fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

fn local_now() -> DateTime<Local> {
    Local::now()
}

fn local_date() -> String {
    analytics::today().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
fn current_reading_date() -> NaiveDate {
    // The meter backend publishes the previous day's 23:59 snapshot only.
    analytics::today() - ChronoDuration::days(1)
}

fn reading_datetime(value: &str) -> Option<NaiveDateTime> {
    let value = value.trim();
    for format in [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S%.f",
        "%Y/%m/%d %H:%M:%S",
    ] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(parsed);
        }
    }
    for format in ["%Y-%m-%d", "%Y/%m/%d"] {
        if let Ok(parsed) = NaiveDate::parse_from_str(value, format) {
            return parsed.and_hms_opt(0, 0, 0);
        }
    }
    None
}

fn reading_date(value: &str) -> Option<NaiveDate> {
    reading_datetime(value).map(|value| value.date())
}

#[cfg(test)]
fn current_reading<'a>(readings: &'a [ElectricityReading]) -> Option<&'a ElectricityReading> {
    let target = current_reading_date();
    readings
        .iter()
        .rev()
        .find(|row| reading_date(&row.date) == Some(target))
        .or_else(|| {
            readings
                .iter()
                .filter(|row| reading_date(&row.date).is_some_and(|date| date <= target))
                .max_by_key(|row| reading_datetime(&row.date))
        })
}

fn location_for(input: &ElectricityQueryInput) -> ElectricityLocationSettings {
    ElectricityLocationSettings {
        campus_id: input.campus_id.clone(),
        area_id: input.area_id.clone(),
        building_id: input.building_id.clone(),
        building_name: input.building_name.clone(),
        room_name: input.room_name.trim().to_owned(),
    }
}

fn cache_matches(cache: &ElectricityCurrentCache, location: &ElectricityLocationSettings) -> bool {
    sharing::normalize_settings(cache.location.clone())
        == sharing::normalize_settings(location.clone())
}

fn load_monitor_record(store: &ElectricityStore) -> ElectricityMonitorRecord {
    store
        .credentials
        .as_ref()
        .and_then(|credentials| credentials.load_electricity_monitor().ok().flatten())
        .unwrap_or_default()
}

fn save_monitor_record(store: &ElectricityStore, record: &ElectricityMonitorRecord) {
    if let Some(credentials) = &store.credentials {
        let _ = credentials.save_electricity_monitor(record);
    }
}

fn monitoring_settings(store: &ElectricityStore) -> Option<ElectricityLocationSettings> {
    load_settings(store)
        .ok()
        .flatten()
        .filter(|s| sharing::validate_shape(s).is_ok())
}

pub(crate) fn status(store: &ElectricityStore) -> ElectricityStatusView {
    let location = monitoring_settings(store);
    let record = load_monitor_record(store);
    let Some(cache) = record.cache.filter(|cache| {
        location
            .as_ref()
            .is_some_and(|location| cache_matches(cache, location))
    }) else {
        return ElectricityStatusView {
            remaining: None,
            observed_date: None,
            expires_at_ms: None,
            expired: false,
            low: false,
            estimated_days_left: None,
            warning_reason: None,
        };
    };
    let expired = cache.expires_at_ms <= now_ms();
    ElectricityStatusView {
        remaining: Some(cache.remaining),
        observed_date: Some(cache.observed_date),
        expires_at_ms: Some(cache.expires_at_ms),
        expired,
        low: !expired && cache.warning_active,
        estimated_days_left: if expired {
            None
        } else {
            cache.estimated_days_left
        },
        warning_reason: if expired { None } else { cache.warning_reason },
    }
}

pub(super) fn emit_status(app: &AppHandle, store: &ElectricityStore) {
    let _ = app.emit(ELECTRICITY_STATUS_EVENT, status(store));
}

fn notify_if_needed(app: &AppHandle, store: &ElectricityStore) {
    let is_background = store
        .runtime
        .lock()
        .map(|runtime| runtime.background)
        .unwrap_or(false);
    if !is_background {
        return;
    }
    let Some(location) = monitoring_settings(store) else {
        return;
    };
    let today = local_date();
    let mut record = load_monitor_record(store);
    let Some(cache) = record
        .cache
        .as_ref()
        .filter(|cache| cache_matches(cache, &location))
    else {
        return;
    };
    if cache.expires_at_ms <= now_ms()
        || !cache.warning_active
        || record.last_notified_date.as_deref() == Some(today.as_str())
    {
        return;
    }
    record.last_notified_date = Some(today);
    save_monitor_record(store, &record);
    let title = format!("当前剩余电量：{:.2} 度", cache.remaining);
    let body = cache
        .warning_reason
        .as_deref()
        .unwrap_or("宿舍电量不足，请及时充值。");
    let payload = crate::notification::NotificationPayload::new(
        crate::notification::NotificationCategory::Electricity,
        title,
        body,
    )
    .id(0x535a)
    .action_type_id("electricity-low")
    .route("/life/campus-payments/recharge")
    .action_label("前往充值")
    .auto_cancel(true);
    crate::notification::notify(app, payload);
}

fn begin_query(store: &ElectricityStore) -> Result<(), String> {
    let mut runtime = store.runtime.lock().map_err(|_| "电费查询状态不可用")?;
    if runtime.in_flight {
        return Err("电费查询正在进行，请稍后重试。".into());
    }
    runtime.in_flight = true;
    Ok(())
}

fn end_query(store: &ElectricityStore) {
    if let Ok(mut runtime) = store.runtime.lock() {
        runtime.in_flight = false;
    }
}

pub(crate) fn query_and_record(
    input: ElectricityQueryInput,
    store: &ElectricityStore,
    app: &AppHandle,
    purpose: QueryPurpose,
    force_refresh: bool,
) -> Result<ElectricityQueryView, String> {
    // Foreground detail queries may run alongside monitoring; generation guards
    // prevent a completed request for a previous configuration from being saved.
    if purpose == QueryPurpose::Monitor {
        begin_query(store)?;
    }
    let started_at_ms = now_ms();
    let generation = store
        .runtime
        .lock()
        .map_err(|_| "电费状态不可用")?
        .generation;
    let result = history::query(store, input.clone(), purpose, force_refresh);
    if purpose == QueryPurpose::Monitor {
        end_query(store);
    }
    let view = result?;
    record_view(store, &input, generation, started_at_ms, &view)?;
    emit_status(app, store);
    notify_if_needed(app, store);
    Ok(view)
}

fn record_view(
    store: &ElectricityStore,
    input: &ElectricityQueryInput,
    generation: u64,
    started_at_ms: i64,
    view: &ElectricityQueryView,
) -> Result<(), String> {
    let location = location_for(input);
    let runtime = store.runtime.lock().map_err(|_| "电费状态不可用")?;
    if runtime.generation != generation || load_settings(store)?.as_ref() != Some(&location) {
        return Ok(());
    }
    // Offline fallback must not renew the lifetime of an old warning snapshot.
    if view
        .cache
        .as_ref()
        .is_some_and(|c| c.from_cache && c.refresh_error.is_some())
    {
        return Ok(());
    }
    if let (Some(remaining), Some(observed)) =
        (view.current_remaining, view.summary.observed_date.as_ref())
    {
        let mut record = load_monitor_record(store);
        if !record.cache.as_ref().is_some_and(|cache| {
            cache_matches(cache, &location)
                && (cache.fetched_at_ms > started_at_ms
                    || reading_datetime(&cache.observed_date) > reading_datetime(observed))
        }) {
            record.cache = Some(ElectricityCurrentCache {
                location,
                remaining,
                observed_date: observed.clone(),
                fetched_at_ms: started_at_ms,
                expires_at_ms: if view.summary.expired {
                    now_ms()
                } else {
                    analytics::expiry_ms()
                },
                warning_active: view.summary.warning_reason.is_some(),
                estimated_days_left: view.summary.estimated_days_left,
                warning_reason: view.summary.warning_reason.clone(),
            });
            save_monitor_record(store, &record);
        }
    }
    Ok(())
}

fn mark_attempt(store: &ElectricityStore, background: bool, date: &str) -> bool {
    let mut record = load_monitor_record(store);
    let target = if background {
        &mut record.last_background_attempt_date
    } else {
        &mut record.last_startup_attempt_date
    };
    if target.as_deref() == Some(date) {
        return false;
    }
    *target = Some(date.to_owned());
    save_monitor_record(store, &record);
    true
}

fn automatic_query(app: &AppHandle, store: &ElectricityStore, background: bool) {
    let Some(settings) = monitoring_settings(store) else {
        return;
    };
    let date = local_date();
    let current = status(store);
    if current.remaining.is_some() && !current.expired {
        return;
    }
    if !mark_attempt(store, background, &history::attempt_key(now_ms())) {
        return;
    }
    let input = ElectricityQueryInput {
        campus_id: settings.campus_id.clone(),
        area_id: settings.area_id.clone(),
        building_id: settings.building_id.clone(),
        building_name: settings.building_name.clone(),
        room_name: settings.room_name.clone(),
        begin_date: date_offset(-2),
        end_date: date.clone(),
    };
    let _ = query_and_record(input, store, app, QueryPurpose::Monitor, false);
}

fn date_offset(days: i64) -> String {
    (local_now().date_naive() + ChronoDuration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

pub(crate) fn initialize(app: AppHandle, store: ElectricityStore) {
    if let Ok(mut cache) = store.history.lock() {
        *cache = match app.path().app_data_dir() {
            Ok(directory) => history::History::open(directory.join("electricity.sqlite")),
            Err(_) => history::History::unavailable(),
        };
    }
    thread::spawn(move || {
        automatic_query(&app, &store, false);
        loop {
            thread::sleep(Duration::from_secs(30));
            let background = store
                .runtime
                .lock()
                .map(|runtime| runtime.background)
                .unwrap_or(false);
            if background {
                automatic_query(&app, &store, true);
                notify_if_needed(&app, &store);
            }
        }
    });
}

pub(crate) fn set_background(app: AppHandle, store: ElectricityStore, background: bool) {
    if let Ok(mut runtime) = store.runtime.lock() {
        runtime.background = background;
    }
    if background {
        thread::spawn(move || {
            automatic_query(&app, &store, true);
            notify_if_needed(&app, &store);
        });
    }
}

pub(crate) fn load_settings(
    store: &ElectricityStore,
) -> Result<Option<ElectricityLocationSettings>, String> {
    store
        .credentials
        .as_ref()
        .ok_or("电费设置存储不可用")?
        .load_electricity_settings()
        .map(|settings| settings.map(sharing::normalize_settings))
}
pub(crate) fn save_settings(
    store: &ElectricityStore,
    settings: ElectricityLocationSettings,
) -> Result<(), String> {
    let settings = sharing::normalize_settings(settings);
    sharing::validate_shape(&settings)?;
    let mut runtime = store.runtime.lock().map_err(|_| "电费状态不可用")?;
    let credentials = store.credentials.as_ref().ok_or("电费设置存储不可用")?;
    let previous = credentials
        .load_electricity_settings()?
        .map(sharing::normalize_settings);
    let changed = previous.as_ref() != Some(&settings);
    credentials.save_electricity_settings(&settings)?;
    if changed {
        runtime.generation += 1;
        credentials.save_electricity_monitor(&ElectricityMonitorRecord::default())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::testing::MemoryCredentialStore;
    use chrono::NaiveDate;
    use std::sync::Arc;
    #[test]
    fn normalizes_buildings_and_parses_options() {
        let html = r#"<select name="buildingId"><option value="1">  山茶斋   </option><option value="2">B3文韬楼#</option></select>"#;
        assert_eq!(select_options(html, "buildingId")[1].name, "B3文韬楼");
    }
    #[test]
    fn parses_sims_rows_and_daily_changes() {
        let html = r#"<table id="oTable"><tr><td>1</td><td>101</td><td>10</td><td>2</td><td>4</td><td>2026-01-01</td></tr><tr><td>2</td><td>101</td><td>8</td><td>5</td><td>4</td><td>2026-01-02</td></tr></table>"#;
        let rows = with_daily_changes(parse_sims(html, "101").unwrap());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].daily_used, Some(3.0));
    }

    #[test]
    fn matches_meter_timestamp_to_previous_day_current_reading() {
        assert_eq!(
            reading_date(" 2026-09-17 23:59:01.0 "),
            NaiveDate::from_ymd_opt(2026, 9, 17)
        );
        let rows = vec![
            ElectricityReading {
                date: "2026-09-16 23:59:01.0".into(),
                room_name: "601".into(),
                remaining: Some(8.0),
                total_used: Some(1.0),
                total_purchased: Some(0.0),
                daily_used: None,
                daily_purchased: None,
            },
            ElectricityReading {
                date: format!("{} 23:59:01.0", current_reading_date()),
                room_name: "601".into(),
                remaining: Some(4.0),
                total_used: Some(2.0),
                total_purchased: Some(0.0),
                daily_used: None,
                daily_purchased: None,
            },
        ];
        assert_eq!(
            current_reading(&rows).and_then(|row| row.remaining),
            Some(4.0)
        );
    }
    #[test]
    fn validates_room_names_before_fetching_for_every_area() {
        for area_id in SIMS_AREAS
            .iter()
            .map(|area| area.id)
            .chain(["xili-lihuo-phase2"])
        {
            let mut input = ElectricityQueryInput {
                campus_id: "xili".into(),
                area_id: area_id.into(),
                building_id: "01".into(),
                building_name: "测试楼栋".into(),
                room_name: "601".into(),
                begin_date: "2026-01-01".into(),
                end_date: "2026-01-02".into(),
            };
            for room in ["601", "1201", " 601 "] {
                input.room_name = room.into();
                let rows =
                    query_in_windows(&input, |chunk| Ok(vec![chunk.room_name.clone()])).unwrap();
                assert_eq!(rows, vec![room.trim().to_owned()], "{area_id}");
            }
            for room in ["", "51", "50001", "5A01", "１２０１"] {
                input.room_name = room.into();
                let mut requested = false;
                let result = query_in_windows::<()>(&input, |_| {
                    requested = true;
                    Ok(vec![])
                });
                assert!(result.is_err(), "{area_id} accepted {room}");
                assert!(!requested, "{area_id} requested invalid room {room}");
            }
        }
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
    fn validates_calendar_dates_with_chrono() {
        let mut input = ElectricityQueryInput {
            campus_id: "x".into(),
            area_id: "x".into(),
            building_id: "x".into(),
            building_name: "x".into(),
            room_name: "101".into(),
            begin_date: "2026-02-30".into(),
            end_date: "2026-03-01".into(),
        };
        assert!(validate(&input).is_err());

        input.begin_date = "2026-02-28".into();
        assert!(validate(&input).is_ok());
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

    #[test]
    fn automatic_attempt_is_recorded_once_per_date_and_mode() {
        let credentials = Arc::new(MemoryCredentialStore::default());
        let store = ElectricityStore::new(credentials);
        assert!(mark_attempt(&store, false, "2026-09-18"));
        assert!(!mark_attempt(&store, false, "2026-09-18"));
        assert!(mark_attempt(&store, true, "2026-09-18"));
        assert!(!mark_attempt(&store, true, "2026-09-18"));
        assert!(mark_attempt(&store, true, "2026-09-19"));
    }
    fn room_settings(room: &str) -> ElectricityLocationSettings {
        ElectricityLocationSettings {
            campus_id: "yuehai".into(),
            area_id: "yuehai-main".into(),
            building_id: "1".into(),
            building_name: "山茶斋".into(),
            room_name: room.into(),
        }
    }

    #[test]
    fn padded_settings_update_status_and_do_not_reset_monitoring_when_saved() {
        for area in [
            "yuehai-main",
            "yuehai-xinzhai",
            "canghai-main",
            "xili-main",
            "xili-lihuo-phase2",
        ] {
            let credentials: SharedCredentialStore = Arc::new(MemoryCredentialStore::default());
            let mut saved = room_settings(" 601 ");
            saved.area_id = area.into();
            credentials.save_electricity_settings(&saved).unwrap();
            let store = ElectricityStore::new(credentials.clone());
            assert_eq!(load_settings(&store).unwrap().unwrap().room_name, "601");
            let generation = store.runtime.lock().unwrap().generation;
            let input = ElectricityQueryInput {
                campus_id: saved.campus_id.clone(),
                area_id: saved.area_id.clone(),
                building_id: saved.building_id.clone(),
                building_name: saved.building_name.clone(),
                room_name: saved.room_name.clone(),
                begin_date: String::new(),
                end_date: String::new(),
            };
            let view = ElectricityQueryView {
                campus_id: saved.campus_id.clone(),
                area_id: saved.area_id.clone(),
                building_name: saved.building_name.clone(),
                room_name: "601".into(),
                begin_date: String::new(),
                end_date: String::new(),
                readings: vec![],
                current_remaining: Some(4.0),
                summary: analytics::summarize(
                    &[],
                    Some(4.0),
                    Some(analytics::today().to_string()),
                    analytics::today(),
                ),
                recharges: vec![],
                recharge_error: None,
                cache: None,
            };
            record_view(&store, &input, generation, now_ms(), &view).unwrap();
            assert_eq!(status(&store).remaining, Some(4.0));
            assert!(status(&store).low);
            save_settings(&store, saved).unwrap();
            assert_eq!(
                credentials
                    .load_electricity_settings()
                    .unwrap()
                    .unwrap()
                    .room_name,
                "601"
            );
            assert_eq!(store.runtime.lock().unwrap().generation, generation);
            assert_eq!(status(&store).remaining, Some(4.0));
            let mut newer = load_monitor_record(&store);
            let cache = newer.cache.as_mut().unwrap();
            cache.location.room_name = " 601 ".into();
            let newer_time = cache.fetched_at_ms;
            save_monitor_record(&store, &newer);
            assert_eq!(status(&store).remaining, Some(4.0));
            let mut late_view = view.clone();
            late_view.current_remaining = Some(99.0);
            record_view(&store, &input, generation, newer_time - 1, &late_view).unwrap();
            assert_eq!(status(&store).remaining, Some(4.0));
        }
    }

    #[test]
    fn legacy_invalid_rooms_remain_editable_but_do_not_expose_monitor_status() {
        for room in ["51", "50001", "5A01", "１２０１", "   "] {
            let credentials: SharedCredentialStore = Arc::new(MemoryCredentialStore::default());
            let saved = room_settings(room);
            credentials.save_electricity_settings(&saved).unwrap();
            let store = ElectricityStore::new(credentials);
            let mut record = ElectricityMonitorRecord::default();
            record.cache = Some(ElectricityCurrentCache {
                location: saved,
                remaining: 1.0,
                observed_date: analytics::today().to_string(),
                fetched_at_ms: now_ms(),
                expires_at_ms: now_ms() + 60_000,
                warning_active: true,
                estimated_days_left: Some(0.2),
                warning_reason: Some("low".into()),
            });
            save_monitor_record(&store, &record);
            assert_eq!(
                load_settings(&store).unwrap().unwrap().room_name,
                room.trim()
            );
            assert_eq!(status(&store).remaining, None);
            assert!(!status(&store).low);
        }
    }

    #[test]
    fn old_monitor_cache_loads_and_room_changes_reject_inflight_results() {
        let old: ElectricityCurrentCache = serde_json::from_value(serde_json::json!({
            "location": {"campusId":"yuehai","areaId":"yuehai-main","buildingId":"1","buildingName":"山茶斋","roomName":"601"},
            "remaining": 4.0, "observedDate": "2026-09-30", "fetchedAtMs": 0, "expiresAtMs": 1, "warningActive": true
        })).unwrap();
        assert_eq!(old.estimated_days_left, None);
        assert_eq!(old.warning_reason, None);
        let store = ElectricityStore::new(Arc::new(MemoryCredentialStore::default()));
        save_settings(&store, old.location.clone()).unwrap();
        let generation = store.runtime.lock().unwrap().generation;
        let input = ElectricityQueryInput {
            campus_id: old.location.campus_id.clone(),
            area_id: old.location.area_id.clone(),
            building_id: old.location.building_id.clone(),
            building_name: old.location.building_name.clone(),
            room_name: old.location.room_name.clone(),
            begin_date: String::new(),
            end_date: String::new(),
        };
        let mut view = ElectricityQueryView {
            campus_id: input.campus_id.clone(),
            area_id: input.area_id.clone(),
            building_name: input.building_name.clone(),
            room_name: input.room_name.clone(),
            begin_date: String::new(),
            end_date: String::new(),
            readings: vec![],
            current_remaining: Some(4.0),
            summary: analytics::summarize(
                &[],
                Some(4.0),
                Some(analytics::today().to_string()),
                analytics::today(),
            ),
            recharges: vec![],
            recharge_error: None,
            cache: None,
        };
        view.cache = Some(history::CacheInfo {
            from_cache: false,
            updated_at_ms: Some(200),
            refresh_error: Some("本地保存失败".into()),
        });
        record_view(&store, &input, generation, 200, &view).unwrap();
        assert!(status(&store).low);
        view.cache = Some(history::CacheInfo {
            from_cache: true,
            updated_at_ms: Some(200),
            refresh_error: Some("离线".into()),
        });
        record_view(&store, &input, generation, 250, &view).unwrap();
        assert_eq!(
            load_monitor_record(&store).cache.unwrap().fetched_at_ms,
            200
        );
        view.cache = None;
        view.current_remaining = Some(30.0);
        view.summary = analytics::summarize(
            &[],
            Some(30.0),
            Some(analytics::today().to_string()),
            analytics::today(),
        );
        record_view(&store, &input, generation, 300, &view).unwrap();
        assert!(!status(&store).low);
        view.current_remaining = Some(1.0);
        record_view(&store, &input, generation, 100, &view).unwrap();
        assert_eq!(status(&store).remaining, Some(30.0));
        let mut other = old.location;
        other.room_name = "602".into();
        save_settings(&store, other).unwrap();
        record_view(&store, &input, generation, 400, &view).unwrap();
        assert_eq!(status(&store).remaining, None);
    }
    #[test]
    fn splits_queries_into_at_most_twenty_inclusive_days() {
        let mut input = ElectricityQueryInput {
            campus_id: "canghai".into(),
            area_id: "canghai-main".into(),
            building_id: "test".into(),
            building_name: "测试楼".into(),
            room_name: "101".into(),
            begin_date: "2026-09-01".into(),
            end_date: "2026-10-01".into(),
        };
        let windows = query_windows(&input).unwrap();
        assert_eq!(
            windows
                .iter()
                .map(|w| (w.begin_date.as_str(), w.end_date.as_str()))
                .collect::<Vec<_>>(),
            vec![("2026-09-01", "2026-09-20"), ("2026-09-21", "2026-10-01")]
        );
        for (start, days) in [
            ("2026-12-20", 31),
            ("2024-02-10", 20),
            ("2024-02-10", 21),
            ("2026-09-29", 3),
            ("2026-10-01", 1),
        ] {
            let start = query_date(start).unwrap();
            let end = start + ChronoDuration::days(days - 1);
            input.begin_date = start.to_string();
            input.end_date = end.to_string();
            let mut dates = Vec::new();
            let rows = query_in_windows(&input, |w| {
                let first = query_date(&w.begin_date).unwrap();
                let last = query_date(&w.end_date).unwrap();
                assert!((last - first).num_days() < 20);
                dates.extend(
                    first
                        .iter_days()
                        .take((last - first).num_days() as usize + 1),
                );
                Ok(vec![()])
            })
            .unwrap();
            assert_eq!(dates.len(), days as usize);
            assert_eq!(
                dates,
                start.iter_days().take(days as usize).collect::<Vec<_>>()
            );
            assert_eq!(rows.len(), ((days + 19) / 20) as usize);
        }
    }

    #[test]
    fn computes_usage_after_merging_chunks_and_rejects_partial_success() {
        let input = ElectricityQueryInput {
            campus_id: "canghai".into(),
            area_id: "canghai-main".into(),
            building_id: "test".into(),
            building_name: "测试楼".into(),
            room_name: "101".into(),
            begin_date: "2026-09-01".into(),
            end_date: "2026-10-01".into(),
        };
        let start = query_date(&input.begin_date).unwrap();
        let rows = query_in_windows(&input, |w| {
            let first = query_date(&w.begin_date).unwrap();
            let last = query_date(&w.end_date).unwrap();
            Ok(first
                .iter_days()
                .take((last - first).num_days() as usize + 1)
                .map(|date| ElectricityReading {
                    date: date.to_string(),
                    room_name: "101".into(),
                    remaining: Some(100.0),
                    total_used: Some((date - start).num_days() as f64 * 3.0),
                    total_purchased: Some(100.0),
                    daily_used: None,
                    daily_purchased: None,
                })
                .collect())
        })
        .unwrap();
        let daily = with_daily_changes(rows);
        assert_eq!(daily.len(), 31);
        assert_eq!(daily[0].daily_used, None);
        assert!(daily[1..].iter().all(|r| r.daily_used == Some(3.0)));
        let mut calls = 0;
        let failure = query_in_windows(&input, |_| {
            calls += 1;
            if calls == 2 {
                Err("第二段失败".into())
            } else {
                Ok(vec![1])
            }
        });
        assert_eq!(failure.unwrap_err(), "第二段失败");
    }
}
