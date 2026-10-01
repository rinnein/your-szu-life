//! Portal adapters. Protocol reference: jinqKing/ElectrifySZU (MIT).
use super::*;
use calamine::{open_workbook_auto_from_rs, Reader};
use scraper::{Html, Selector};
use std::io::{Cursor, Read};

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Recharge {
    pub time: String,
    pub kwh: Option<f64>,
    pub yuan: Option<f64>,
    pub source: Option<String>,
    pub source_kind: String,
}
pub(super) struct ProviderData {
    pub readings: Vec<ElectricityReading>,
    pub current_remaining: Option<f64>,
    pub recharges: Vec<Recharge>,
    pub recharge_error: Option<String>,
}
impl ProviderData {
    pub fn new(readings: Vec<ElectricityReading>) -> Self {
        Self {
            readings,
            current_remaining: None,
            recharges: vec![],
            recharge_error: None,
        }
    }
}
fn selector(s: &str) -> Selector {
    Selector::parse(s).unwrap()
}
fn tables(html: &str) -> Vec<Vec<Vec<String>>> {
    let doc = Html::parse_document(html);
    doc.select(&selector("table"))
        .map(|table| {
            table
                .select(&selector("tr"))
                .filter(|row| {
                    row.ancestors()
                        .filter_map(scraper::ElementRef::wrap)
                        .find(|e| e.value().name() == "table")
                        .is_some_and(|t| t.id() == table.id())
                })
                .filter_map(|row| {
                    let cells: Vec<_> = row
                        .children()
                        .filter_map(scraper::ElementRef::wrap)
                        .filter(|e| matches!(e.value().name(), "td" | "th"))
                        .collect();
                    // Layout tables may wrap the actual record table. Do not treat
                    // their concatenated descendant text as column headings.
                    if cells
                        .iter()
                        .any(|c| c.select(&selector("table")).next().is_some())
                    {
                        return None;
                    }
                    Some(
                        cells
                            .iter()
                            .map(|c| c.text().collect::<String>().trim().to_owned())
                            .collect(),
                    )
                })
                .collect()
        })
        .collect()
}
fn column(headers: &[String], aliases: &[&str]) -> Option<usize> {
    headers.iter().position(|h| {
        aliases
            .iter()
            .any(|a| h.replace('（', "(").replace('）', ")").contains(a))
    })
}
fn cell<'a>(row: &'a [String], index: Option<usize>) -> &'a str {
    index
        .and_then(|i| row.get(i))
        .map(String::as_str)
        .unwrap_or("")
}
pub(super) fn in_window(time: &str, input: &ElectricityQueryInput) -> bool {
    reading_date(time).is_some_and(|d| {
        Some(d) >= query_date(&input.begin_date) && Some(d) <= query_date(&input.end_date)
    })
}
fn parse_recharges(
    rows: &[Vec<String>],
    input: &ElectricityQueryInput,
    person: bool,
) -> Result<Vec<Recharge>, String> {
    let Some((at, headers)) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| column(r, &["充值电量", "购买电量", "购电量"]).is_some())
    else {
        return Err("充值记录表头无法识别".into());
    };
    let time = column(headers, &["日期", "时间"]);
    let kwh = column(headers, &["充值电量", "购买电量", "购电量"]);
    let yuan = column(headers, &["金额", "费用"]);
    let source = column(
        headers,
        if person {
            &["充值人"]
        } else {
            &["方式", "渠道", "形式"]
        },
    );
    if time.is_none() {
        return Err("充值记录缺少时间列".into());
    }
    let mut result = Vec::new();
    for row in &rows[at + 1..] {
        let timestamp = cell(row, time);
        if reading_datetime(timestamp).is_none() {
            if parse_number(cell(row, kwh)).is_some() || parse_number(cell(row, yuan)).is_some() {
                return Err("充值记录包含无法识别的日期，请重试或检查官网记录".into());
            }
            continue;
        }
        if !in_window(timestamp, input) {
            continue;
        }
        result.push(Recharge {
            time: timestamp.into(),
            kwh: parse_number(cell(row, kwh)),
            yuan: parse_number(cell(row, yuan)),
            source: (!cell(row, source).is_empty()).then(|| cell(row, source).to_owned()),
            source_kind: if person { "person" } else { "channel" }.into(),
        });
    }
    result.sort_by_key(|r| std::cmp::Reverse(reading_datetime(&r.time)));
    Ok(result)
}
fn bytes(response: Response) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("电费接口返回 HTTP {}", response.status()));
    }
    let mut data = vec![];
    response
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "无法读取电费响应")?;
    if data.len() > MAX_BODY {
        return Err("电费响应过大".into());
    }
    Ok(data)
}
pub(super) fn sims_recharges(input: &ElectricityQueryInput) -> Result<Vec<Recharge>, String> {
    let mut records = query_in_windows(input, sims_recharges_chunk)?;
    records.sort_by_key(|r| std::cmp::Reverse(reading_datetime(&r.time)));
    Ok(records)
}

fn sims_recharges_chunk(input: &ElectricityQueryInput) -> Result<Vec<Recharge>, String> {
    let client_ip = SIMS_AREAS
        .iter()
        .find(|a| a.id == input.area_id)
        .ok_or("未知宿舍区域")?
        .client;
    let http = client()?;
    let station = body(
        http.get(format!("{SIMS_BASE}/cgcSims/login.do"))
            .query(&[("task", "station"), ("client", client_ip)])
            .send()
            .map_err(|_| "无法连接充值接口")?,
    )?;
    let action = safe_url(
        &format!("{SIMS_BASE}/cgcSims/login.do"),
        form_action(&station)
            .as_deref()
            .unwrap_or("/cgcSims/login.do"),
    )?;
    let room = body(
        http.post(action)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&[
                ("client", client_ip),
                ("buildingId", &input.building_id),
                ("buildingName", &input.building_name),
                ("roomName", &input.room_name),
                ("select", " 查询 "),
            ]))
            .send()
            .map_err(|_| "无法读取充值宿舍信息")?,
    )?;
    let room_id = hidden(&room, "roomId").ok_or("充值接口未找到宿舍")?;
    let data = bytes(
        http.get(format!("{SIMS_BASE}/cgcSims/selectList.do"))
            .query(&[
                ("type", "3"),
                ("client", client_ip),
                ("roomId", &room_id),
                ("roomName", &input.room_name),
                ("beginTime", &input.begin_date),
                ("endTime", &input.end_date),
            ])
            .send()
            .map_err(|_| "无法查询充值记录")?,
    )?;
    parse_sims_recharge_bytes(data, input)
}
fn parse_sims_recharge_bytes(
    data: Vec<u8>,
    input: &ElectricityQueryInput,
) -> Result<Vec<Recharge>, String> {
    // Some installations return an HTML table with an .xls content type.
    let text = GB18030.decode(&data).0.into_owned();
    if text.to_ascii_lowercase().contains("<table") {
        return tables(&text)
            .iter()
            .find_map(|rows| parse_recharges(rows, input, false).ok())
            .ok_or("无法识别充值记录表格".into());
    }
    let mut workbook = open_workbook_auto_from_rs(Cursor::new(data.as_slice()))
        .map_err(|_| "无法解析充值 Excel 文件")?;
    if let Some(records) = recharge_sheets(&mut workbook, input) {
        return records;
    }
    // SIMS' legacy XLS exporter declares an ANSI code page although its text
    // cells contain UTF-16LE. Retry only OLE/XLS files whose normal decoding
    // cannot identify the recharge columns; do not override valid workbooks.
    if data.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        let mut options = calamine::XlsOptions::default();
        options.force_codepage = Some(1200);
        let mut workbook = calamine::Xls::new_with_options(Cursor::new(data), options)
            .map_err(|_| "无法解析充值 Excel 文件")?;
        if let Some(records) = recharge_sheets(&mut workbook, input) {
            return records;
        }
    }
    Err("充值 Excel 缺少记录列".into())
}
fn recharge_sheets<RS: Read + std::io::Seek, R: Reader<RS>>(
    workbook: &mut R,
    input: &ElectricityQueryInput,
) -> Option<Result<Vec<Recharge>, String>> {
    for (_, sheet) in workbook.worksheets() {
        let rows: Vec<Vec<String>> = sheet
            .rows()
            .map(|r| r.iter().map(ToString::to_string).collect())
            .collect();
        if rows
            .iter()
            .any(|r| column(r, &["充值电量", "购买电量", "购电量"]).is_some())
        {
            return Some(parse_recharges(&rows, input, false));
        }
    }
    None
}

pub(super) fn safe_url(base: &str, action: &str) -> Result<String, String> {
    let base = reqwest::Url::parse(base).map_err(|_| "门户地址无效")?;
    let url = base.join(action).map_err(|_| "门户地址无效")?;
    if url.origin() != base.origin() {
        return Err("门户返回了非预期地址".into());
    }
    Ok(url.to_string())
}
fn hidden_fields(html: &str) -> BTreeMap<String, String> {
    Html::parse_document(html)
        .select(&selector("input[type=hidden][name]"))
        .map(|e| {
            (
                e.value().attr("name").unwrap().to_owned(),
                e.value().attr("value").unwrap_or("").to_owned(),
            )
        })
        .collect()
}
fn post(http: &Client, url: &str, html: &str, values: &[(&str, &str)]) -> Result<String, String> {
    let mut fields = hidden_fields(html);
    for (k, v) in values {
        fields.insert((*k).into(), (*v).into());
    }
    let fields: Vec<_> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    body(
        http.post(url)
            .header("Referer", url)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form_gb18030(&fields))
            .send()
            .map_err(|_| "丽湖电费请求失败")?,
    )
}
fn lihu_pages(
    input: &ElectricityQueryInput,
    recharge: bool,
) -> Result<(String, Vec<String>), String> {
    lihu_pages_at(input, recharge, LIHUO_BASE)
}
fn lihu_pages_at(
    input: &ElectricityQueryInput,
    recharge: bool,
    base: &str,
) -> Result<(String, Vec<String>), String> {
    let http = client()?;
    let home = body(http.get(base).send().map_err(|_| "无法连接丽湖电费接口")?)?;
    let digits: String = input
        .room_name
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    if digits.len() < 3 {
        return Err("宿舍号必须包含楼层和房间号".into());
    }
    let floor = format!("{}{:0>2}", input.building_id, &digits[..digits.len() - 2]);
    let building = post(
        &http,
        base,
        &home,
        &[
            ("__EVENTTARGET", "drlouming"),
            ("__EVENTARGUMENT", ""),
            ("drlouming", &input.building_id),
        ],
    )?;
    let rooms = post(
        &http,
        base,
        &building,
        &[
            ("__EVENTTARGET", "drceng"),
            ("__EVENTARGUMENT", ""),
            ("drlouming", &input.building_id),
            ("drceng", &floor),
        ],
    )?;
    let room = select_options(&rooms, "drfangjian")
        .into_iter()
        .find(|r| r.name.ends_with(&digits))
        .ok_or("未找到丽湖宿舍")?;
    let landing = post(
        &http,
        base,
        &rooms,
        &[
            ("__EVENTTARGET", ""),
            ("__EVENTARGUMENT", ""),
            ("drlouming", &input.building_id),
            ("drceng", &floor),
            ("drfangjian", &room.id),
            ("radio", if recharge { "buyR" } else { "usedR" }),
            ("ImageButton1.x", "20"),
            ("ImageButton1.y", "10"),
        ],
    )?;
    let action = safe_url(
        &format!("{base}/"),
        form_action(&landing).as_deref().unwrap_or(if recharge {
            "buyRecord.aspx"
        } else {
            "usedRecord.aspx"
        }),
    )?;
    let first = post(
        &http,
        &action,
        &landing,
        &[
            ("txtstart", &input.begin_date),
            ("txtend", &input.end_date),
            ("btnser", "查询"),
        ],
    )?;
    let count = page_count(&first)?;
    let mut pages = vec![first];
    for n in 2..=count {
        // Keep the date filter explicit even when the server retains it in session.
        pages.push(body(
            http.get(&action)
                .header("Referer", &action)
                .query(&[
                    ("p", n.to_string()),
                    ("txtstart", input.begin_date.clone()),
                    ("txtend", input.end_date.clone()),
                ])
                .send()
                .map_err(|_| "丽湖电费分页查询失败")?,
        )?);
    }
    Ok((landing, pages))
}
fn page_count(html: &str) -> Result<usize, String> {
    let text = Html::parse_document(html)
        .root_element()
        .text()
        .collect::<String>();
    let count = text
        .split('共')
        .find_map(|s| {
            let s = s.trim_start();
            let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
            if !digits.is_empty() && s[digits.len()..].trim_start().starts_with('页') {
                digits.parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(1);
    if count > 100 {
        return Err("丽湖接口分页数量异常".into());
    }
    Ok(count.max(1))
}
fn remaining(html: &str) -> Option<f64> {
    let text = Html::parse_document(html)
        .root_element()
        .text()
        .collect::<String>();
    let after = text
        .split("剩余电量")
        .nth(1)?
        .trim_start_matches(['：', ':', ' ', '\n', '\r', '\t']);
    let number: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    parse_number(&number)
}
fn lihu_usage(
    html: &str,
    input: &ElectricityQueryInput,
) -> Result<Vec<ElectricityReading>, String> {
    for table in tables(html) {
        let Some((at, headers)) = table
            .iter()
            .enumerate()
            .find(|(_, r)| column(r, &["用量(度)"]).is_some())
        else {
            continue;
        };
        let date = column(headers, &["日期"]);
        let amount = column(headers, &["用量(度)"]);
        if date.is_none() {
            continue;
        }
        return Ok(table[at + 1..]
            .iter()
            .filter(|r| in_window(cell(r, date), input))
            .map(|r| ElectricityReading {
                date: cell(r, date).into(),
                room_name: input.room_name.clone(),
                remaining: None,
                total_used: None,
                total_purchased: None,
                daily_used: parse_number(cell(r, amount)).filter(|v| *v >= 0.0),
                daily_purchased: None,
            })
            .collect());
    }
    Err("无法识别丽湖每日用电表格".into())
}
pub(super) fn lihu_recharges(input: &ElectricityQueryInput) -> Result<Vec<Recharge>, String> {
    let mut records = query_in_windows(input, lihu_recharges_chunk)?;
    records.sort_by_key(|r| std::cmp::Reverse(reading_datetime(&r.time)));
    Ok(records)
}

fn lihu_recharges_chunk(input: &ElectricityQueryInput) -> Result<Vec<Recharge>, String> {
    let (_, pages) = lihu_pages(input, true)?;
    let mut records = vec![];
    for page in pages {
        records.extend(
            tables(&page)
                .iter()
                .find_map(|t| parse_recharges(t, input, true).ok())
                .ok_or("无法识别丽湖充值表格")?,
        );
    }
    records.sort_by_key(|r| std::cmp::Reverse(reading_datetime(&r.time)));
    Ok(records)
}
pub(super) fn lihuo(
    input: &ElectricityQueryInput,
    purpose: QueryPurpose,
) -> Result<ProviderData, String> {
    let mut data = ProviderData::new(vec![]);
    for chunk in query_windows(input)? {
        let (landing, pages) = lihu_pages(&chunk, false)?;
        data.current_remaining = pages
            .first()
            .and_then(|p| remaining(p))
            .or_else(|| remaining(&landing));
        for page in pages {
            data.readings.extend(lihu_usage(&page, &chunk)?);
        }
    }
    if purpose == QueryPurpose::Detail {
        match lihu_recharges(input) {
            Ok(records) => data.recharges = records,
            Err(e) => data.recharge_error = Some(e),
        }
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> ElectricityQueryInput {
        ElectricityQueryInput {
            campus_id: "xili".into(),
            area_id: "xili-lihuo-phase2".into(),
            building_id: "01".into(),
            building_name: "梧桐树".into(),
            room_name: "501".into(),
            begin_date: "2026-09-29".into(),
            end_date: "2026-10-01".into(),
        }
    }
    #[test]
    fn parses_direct_usage_without_inventing_historical_balances() {
        let html = "<table><tr><th>日期</th><th>用量(度)</th></tr><tr><td>2026-09-28</td><td>9</td></tr><tr><td>2026-09-30</td><td>3.2</td></tr></table>";
        let rows = lihu_usage(
            &format!("<table><tr><td>{html}</td></tr></table>"),
            &input(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].daily_used, Some(3.2));
        assert_eq!(rows[0].remaining, None);
        assert_eq!(remaining("剩余电量：<span>12.35</span>度"), Some(12.35));
        assert!(lihu_usage("<html>登录失败</html>", &input()).is_err());
    }
    #[test]
    fn sources_and_actual_amount_are_not_inferred() {
        let table = vec![
            vec!["日期", "充值电量(度)", "充值金额(元)", "充值人"],
            vec!["2026-09-30", "10", "7.5", "张同学"],
            vec!["2026-09-29", "2", "", ""],
        ]
        .into_iter()
        .map(|r| r.into_iter().map(str::to_owned).collect())
        .collect::<Vec<_>>();
        let rows = parse_recharges(&table, &input(), true).unwrap();
        assert_eq!(rows[0].source.as_deref(), Some("张同学"));
        assert_eq!(rows[0].yuan, Some(7.5));
        assert_eq!(rows[1].source, None);
        assert_eq!(rows[1].yuan, None);
        let html="<table><tr><th>充值时间</th><th>购买电量</th><th>充值金额</th><th>购买形式</th></tr><tr><td>2026-09-30</td><td>10</td><td>6.1</td><td>微信</td></tr></table>";
        let bytes = GB18030.encode(html).0.to_vec();
        let rows = parse_sims_recharge_bytes(bytes, &input()).unwrap();
        assert_eq!(rows[0].source_kind, "channel");
        assert_eq!(rows[0].source.as_deref(), Some("微信"));
        assert!(parse_sims_recharge_bytes(vec![1, 2, 3], &input()).is_err());
    }
    #[test]
    fn reads_real_excel_container() {
        let rows =
            parse_sims_recharge_bytes(include_bytes!("fixtures/recharges.xlsx").to_vec(), &input())
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].yuan, Some(8.5));
        assert_eq!(rows[0].source.as_deref(), Some("校园卡"));
    }
    #[test]
    fn hidden_state_and_paging_are_bounded() {
        let fields=hidden_fields("<input type='hidden' name='__VIEWSTATE' value='v&amp;2'><input type='hidden' name='__EVENTVALIDATION' value='e'>");
        assert_eq!(fields.get("__VIEWSTATE").unwrap(), "v&2");
        assert_eq!(page_count("第 1 页 / 共 3 页").unwrap(), 3);
        assert!(page_count("第 1 页 / 共 1000 页").is_err());
        assert!(safe_url(LIHUO_BASE, "https://example.com").is_err());
    }
    #[test]
    fn lihu_submits_date_range_and_keeps_it_on_pagination() {
        use std::io::Write;
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let thread = std::thread::spawn(move || {
            let mut requests = vec![];
            for i in 0..6 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut buffer = vec![];
                let mut chunk = [0; 4096];
                loop {
                    let count = stream.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..count]);
                    if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&buffer[..at]).to_lowercase();
                        let length = header
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length: ")
                                    .and_then(|n| n.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if buffer.len() >= at + 4 + length {
                            break;
                        }
                    }
                }
                requests.push(String::from_utf8(buffer).unwrap());
                let html=match i {
                    0|1 => "<input type='hidden' name='__VIEWSTATE' value='next'>",
                    2 => "<input type='hidden' name='__EVENTVALIDATION' value='valid'><select name=\"drfangjian\"><option value=\"010501\">501</option></select>",
                    3 => "<form action=\"./usedRecord.aspx\"><input type='hidden' name='__VIEWSTATE' value='landing'></form>",
                    4 => "第 1 页 / 共 2 页",
                    _ => "第 2 页 / 共 2 页",
                };
                let bytes = GB18030.encode(html).0.into_owned();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                stream.write_all(&bytes).unwrap();
            }
            requests
        });
        let (_, pages) = lihu_pages_at(&input(), false, &base).unwrap();
        assert_eq!(pages.len(), 2);
        let requests = thread.join().unwrap();
        assert!(requests[1].contains("__EVENTTARGET=drlouming"));
        assert!(requests[3].contains("__EVENTVALIDATION=valid"));
        assert!(requests[3].contains("radio=usedR"));
        assert!(requests[4].contains("txtstart=2026-09-29"));
        assert!(requests[4].contains("txtend=2026-10-01"));
        assert!(requests[4].contains("__VIEWSTATE=landing"));
        assert!(requests[5].contains("p=2&txtstart=2026-09-29&txtend=2026-10-01"));
    }
    #[test]
    fn handles_sims_xls_with_mislabeled_codepage() {
        let bytes = include_bytes!("fixtures/recharges-mislabeled-codepage.xls").to_vec();
        let mut normally_decoded = open_workbook_auto_from_rs(Cursor::new(bytes.clone())).unwrap();
        assert!(recharge_sheets(&mut normally_decoded, &input()).is_none());
        let records = parse_sims_recharge_bytes(bytes, &input()).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].source.as_deref(), Some("微信"));
        assert_eq!(records[0].kwh, Some(10.0));
        assert_eq!(records[0].yuan, Some(6.1));
        assert_eq!(records[0].time, "2026-09-30 12:00:00.0");
    }

    #[test]
    fn invalid_transaction_date_is_not_reported_as_empty_history() {
        let rows = vec![
            vec!["购买电量".into(), "购买日期".into()],
            vec!["10".into(), "乱码".into()],
        ];
        assert!(parse_recharges(&rows, &input(), false)
            .unwrap_err()
            .contains("日期"));
        assert!(parse_recharges(&rows[..1], &input(), false)
            .unwrap()
            .is_empty());
    }
}
