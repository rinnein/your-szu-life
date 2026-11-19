use super::{reading_date, reading_datetime, ElectricityReading, QueryPurpose, Recharge, LOW_THRESHOLD};
use chrono::{Duration, FixedOffset, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

pub const YUAN_PER_KWH: f64 = 0.69979;
/// 至少要有一整天的覆盖数据才给估算。
pub const MIN_PREDICTION_DAYS: f64 = 1.0;
const DAY_MS: f64 = 86_400_000.0;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ElectricityConstants {
    pub yuan_per_kwh: f64,
}
pub fn constants() -> ElectricityConstants {
    ElectricityConstants {
        yuan_per_kwh: super::YUAN_PER_KWH,
    }
}
fn shanghai() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).unwrap()
}
pub fn today() -> NaiveDate {
    Utc::now().with_timezone(&shanghai()).date_naive()
}
/// 毫秒时间戳对应的本地日期（北京时间）。
pub fn local_date(ms: i64) -> NaiveDate {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|value| value.with_timezone(&shanghai()).date_naive())
        .unwrap_or_else(today)
}
/// 把门户给出的观测时刻解析成本地毫秒时间戳。只有日期的记录返回 None，
/// 由调用方按「当天结束」解释。
pub fn observed_ms(raw: &str) -> Option<i64> {
    if !raw.contains(':') {
        return None;
    }
    let naive = reading_datetime(raw)?;
    shanghai()
        .from_local_datetime(&naive)
        .single()
        .map(|value| value.timestamp_millis())
}
/// 只有日期没有时刻的观测（例如丽湖的历史行）按当天结束处理：累计读数门户在
/// 23:59 发布，按日终解释不会把「已经统计在内的当天用电」重复扣一次。
pub fn date_end_ms(date: &str) -> Option<i64> {
    let date = reading_date(date)?;
    shanghai()
        .from_local_datetime(&date.and_hms_opt(23, 59, 0)?)
        .single()
        .map(|value| value.timestamp_millis())
}
pub fn expiry_ms() -> i64 {
    super::history::valid_until(Utc::now().timestamp_millis())
}
pub fn window(today: NaiveDate, purpose: QueryPurpose) -> (String, String) {
    (
        (today
            - Duration::days(if purpose == QueryPurpose::Detail {
                30
            } else {
                2
            }))
        .to_string(),
        today.to_string(),
    )
}

// Direct daily usage (Lihu) is preserved. Cumulative readings (SIMS) require
// consecutive dates; gaps and meter resets are not daily measurements.
pub fn normalize(mut rows: Vec<ElectricityReading>) -> Vec<ElectricityReading> {
    rows.sort_by_key(|r| reading_datetime(&r.date));
    let mut days = BTreeMap::new();
    for mut row in rows {
        if let Some(date) = reading_date(&row.date) {
            // Keep the portal timestamp: the balance is only accurate as of that
            // instant, which is what the elapsed-usage correction is anchored to.
            row.observed_at = Some(row.date.trim().to_owned());
            row.date = date.to_string();
            days.insert(date, row);
        }
    }
    let mut result: Vec<_> = days.into_values().collect();
    for i in 0..result.len() {
        if result[i].total_used.is_none() {
            continue;
        }
        result[i].daily_used = None;
        result[i].daily_purchased = None;
        if i == 0
            || reading_date(&result[i - 1].date).and_then(|d| d.succ_opt())
                != reading_date(&result[i].date)
        {
            continue;
        }
        let delta = |a: Option<f64>, b: Option<f64>| {
            a.zip(b)
                .map(|(a, b)| b - a)
                .filter(|n| n.is_finite() && *n >= 0.0)
        };
        result[i].daily_used = delta(result[i - 1].total_used, result[i].total_used);
        result[i].daily_purchased = delta(result[i - 1].total_purchased, result[i].total_purchased);
    }
    result
}

/// 预测窗口内的日耗电。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsageStats {
    /// 覆盖加权的日均用电；数据不足时为 None。
    pub daily_average: Option<f64>,
    /// 参与平均的日历天数（累计读数按差分实际覆盖的天数计）。
    pub covered_days: f64,
}

/// 日均用电的最终估计：短窗口（7 天）与长窗口（31 天）各算一次，取二者均值。
///
/// 短窗口跟得上变化，长窗口压住偶发的高/低用量；任一窗口没有数据时退回另一个。
pub const WEEKLY_DAYS: i64 = 7;
pub const MONTHLY_DAYS: i64 = 31;

/// 最近 `days` 个日历日的日均用电。
///
/// 累计读数（SIMS）用相邻两次累计值的差分除以它覆盖的天数：门户把两天没记录
/// 的用电合并到第三天时，这段耗电会被摊成两天而不是当成一天的量。直接日用电
/// （丽湖）每条算一天，缺失日不补零——抓取不完整时补零会低估日耗、把预警拖晚。
pub fn window_average(rows: &[ElectricityReading], today: NaiveDate, days: i64) -> UsageStats {
    let start = today - Duration::days(days - 1);
    let mut total = 0.0;
    let mut weight = 0.0;
    let mut previous: Option<(NaiveDate, f64)> = None;
    for row in rows {
        let Some(date) = reading_date(&row.date) else {
            continue;
        };
        if let Some(total_used) = row.total_used {
            if let Some((previous_date, previous_total)) = previous {
                let covered = (date - previous_date).num_days();
                if covered >= 1 {
                    let delta = total_used - previous_total;
                    if delta.is_finite() && delta >= 0.0 {
                        // Clip a multi-day delta to the window instead of dropping
                        // it: the sample still tells us the usage rate.
                        let first = date - Duration::days(covered - 1);
                        let from = first.max(start);
                        let to = date.min(today);
                        let overlap = (to - from).num_days() + 1;
                        if overlap > 0 {
                            total += delta * overlap as f64 / covered as f64;
                            weight += overlap as f64;
                        }
                    }
                }
            }
            previous = Some((date, total_used));
        } else if let Some(daily) = row.daily_used {
            if date >= start && date <= today && daily.is_finite() && daily >= 0.0 {
                total += daily;
                weight += 1.0;
            }
        }
    }
    UsageStats {
        daily_average: (weight >= MIN_PREDICTION_DAYS && weight > 0.0)
            .then(|| total / weight),
        covered_days: weight,
    }
}

/// 两个窗口各自算出的日均，以及它们的最终估计。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsageEstimate {
    pub weekly: UsageStats,
    pub monthly: UsageStats,
    /// 7 天与 31 天估计的算术平均；只有一侧有数据时直接采用该侧。
    pub blended: Option<f64>,
}

pub fn estimate(rows: &[ElectricityReading], today: NaiveDate) -> UsageEstimate {
    let weekly = window_average(rows, today, WEEKLY_DAYS);
    let monthly = window_average(rows, today, MONTHLY_DAYS);
    let blended = match (weekly.daily_average, monthly.daily_average) {
        (Some(weekly), Some(monthly)) => Some((weekly + monthly) / 2.0),
        (weekly, monthly) => weekly.or(monthly),
    };
    UsageEstimate {
        weekly,
        monthly,
        blended,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BalanceInput {
    /// 门户读数上的原始余额。
    pub remaining: f64,
    /// 该余额对应的观测时刻（毫秒时间戳）。
    pub observed_at_ms: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BalanceState {
    pub credits: f64,
    pub elapsed_used: f64,
    pub effective_remaining: f64,
    pub estimated_days_left: Option<f64>,
}

/// 把「观测时刻的余额」换算成「此刻的有效余额」。
///
/// 充值记录来自实时数据源，但读数最多可能落后一天（SIMS 只发布前一天 23:59 的
/// 快照），因此：有效余额 = 读数余额 + 观测之后的充值 − 观测至今按日均值估算的
/// 用电。缺口不封顶：门户连续几天没发布时，估算仍按实际经过的天数往前推，
/// 而不是把读数当成刚测到的。
///
/// 余额**允许为负**：学校允许透支，电表结算延迟时门户本身也会给出负数余额，
/// 所以这里不夹到 0；只有「预计可用天数」会夹到 0 天（已经用完就没有负数天）。
pub fn evaluate_balance(
    remaining: f64,
    credits: f64,
    daily_average: Option<f64>,
    observed_at_ms: i64,
    now: i64,
) -> BalanceState {
    let elapsed = ((now - observed_at_ms) as f64 / DAY_MS).max(0.0);
    let elapsed_used = daily_average.filter(|avg| *avg > 0.0).unwrap_or(0.0) * elapsed;
    let effective = remaining + credits - elapsed_used;
    BalanceState {
        credits,
        elapsed_used,
        effective_remaining: effective,
        estimated_days_left: daily_average
            .filter(|avg| avg.is_finite() && *avg > 0.0)
            .map(|avg| (effective / avg).max(0.0)),
    }
}

/// 充值电量：优先用门户的度数，缺失时按固定电价折算金额。
pub fn recharge_kwh(recharge: &Recharge) -> Option<f64> {
    recharge
        .kwh
        .or_else(|| recharge.yuan.map(|yuan| yuan / YUAN_PER_KWH))
        .filter(|value| value.is_finite() && *value > 0.0)
}

/// 观测时刻之后的充值电量合计。
///
/// 只有日期没有时刻的记录按日终解释：与读数同一天的充值视为已包含在该日快照里。
pub fn credits_after(recharges: &[Recharge], observed_at_ms: i64) -> f64 {
    let observed_date = local_date(observed_at_ms);
    recharges
        .iter()
        .filter_map(|recharge| {
            let kwh = recharge_kwh(recharge)?;
            let text = recharge.time.trim();
            let naive = reading_datetime(text)?;
            let time_ms = shanghai()
                .from_local_datetime(&naive)
                .single()?
                .timestamp_millis();
            let counted = if text.contains(':') {
                time_ms > observed_at_ms
            } else {
                local_date(time_ms) > observed_date
            };
            counted.then_some(kwh)
        })
        .sum()
}

/// 预警文案；没有余额时不给预警。
///
/// 不再因为读数过期而静默：有效余额已经把「读数之后的用电」按日均扣掉，
/// 门户连续几天没发布时这次推算本身就是最可信的预警依据（学校还允许透支）。
/// 过期只作为展示上的说明，由调用方另行标注。
pub fn warning_reason(balance: Option<&BalanceState>) -> Option<String> {
    let balance = balance?;
    if balance.effective_remaining < 0.0 {
        // 学校允许透支（电表结算延迟），负数余额必须显式说成透支。
        return Some(format!(
            "宿舍电量已透支 {:.1} 度，请及时充值。",
            -balance.effective_remaining
        ));
    }
    if balance.effective_remaining < LOW_THRESHOLD {
        return Some("宿舍剩余电量低于 5 度，请及时充值。".into());
    }
    if balance.estimated_days_left.is_some_and(|days| days < 1.0) {
        return Some("预计可用电量不足 1 天，请及时充值。".into());
    }
    None
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total_used: Option<f64>,
    /// 最近 7 天覆盖加权的日均。
    pub weekly_daily_average: Option<f64>,
    /// 最近 31 天覆盖加权的日均。
    pub monthly_daily_average: Option<f64>,
    /// 最终估计：上面两个值（可用的那些）的均值；余额校正与预计天数都用它。
    pub prediction_daily_average: Option<f64>,
    pub estimated_remaining: Option<f64>,
    pub recharge_credit_kwh: f64,
    pub elapsed_used_kwh: f64,
    pub estimated_days_left: Option<f64>,
    pub observed_date: Option<String>,
    pub expired: bool,
    pub warning_reason: Option<String>,
}

pub struct SummaryInput<'a> {
    /// 缓存里的全部读数：预测窗口可以跨过查询周期，不能用 3/31 天窗口去裁剪。
    pub readings: &'a [ElectricityReading],
    /// 查询周期起点，只影响展示用的周期均值/合计。
    pub period_start: NaiveDate,
    pub balance: Option<BalanceInput>,
    pub recharges: &'a [Recharge],
    pub now: i64,
    pub today: NaiveDate,
}

pub fn summarize(input: SummaryInput<'_>) -> Summary {
    let used: Vec<_> = input
        .readings
        .iter()
        .filter(|row| {
            reading_date(&row.date)
                .is_some_and(|date| date >= input.period_start && date <= input.today)
        })
        .filter_map(|row| row.daily_used)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect();
    let total_used = (!used.is_empty()).then(|| used.iter().sum::<f64>());
    let estimate = estimate(input.readings, input.today);
    let observed_date = input.balance.map(|balance| local_date(balance.observed_at_ms));
    let expired = !observed_date
        .is_some_and(|date| date >= input.today - Duration::days(1) && date <= input.today);
    let balance = input.balance.map(|balance| {
        evaluate_balance(
            balance.remaining,
            credits_after(input.recharges, balance.observed_at_ms),
            estimate.blended,
            balance.observed_at_ms,
            input.now,
        )
    });
    let warning_reason = warning_reason(balance.as_ref());
    Summary {
        total_used,
        weekly_daily_average: estimate.weekly.daily_average,
        monthly_daily_average: estimate.monthly.daily_average,
        prediction_daily_average: estimate.blended,
        estimated_remaining: balance.map(|balance| balance.effective_remaining),
        recharge_credit_kwh: balance.map(|balance| balance.credits).unwrap_or(0.0),
        elapsed_used_kwh: balance.map(|balance| balance.elapsed_used).unwrap_or(0.0),
        // 读数过期只影响是否发预警，不影响展示：按读数时刻推算的预计天数照常给出，
        // 门户几天没发布时页面不会直接变成空值。
        estimated_days_left: balance.and_then(|balance| balance.estimated_days_left),
        observed_date: observed_date.map(|date| date.to_string()),
        expired,
        warning_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reading(date: &str, used: f64) -> ElectricityReading {
        ElectricityReading {
            date: date.into(),
            observed_at: Some(date.into()),
            room_name: "101".into(),
            remaining: Some(10.0),
            total_used: Some(used),
            total_purchased: Some(50.0),
            daily_used: None,
            daily_purchased: None,
        }
    }
    fn daily(date: &str, used: f64) -> ElectricityReading {
        ElectricityReading {
            date: date.into(),
            observed_at: Some(date.into()),
            room_name: "101".into(),
            remaining: None,
            total_used: None,
            total_purchased: None,
            daily_used: Some(used),
            daily_purchased: None,
        }
    }
    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }
    fn at(text: &str) -> i64 {
        observed_ms(text).unwrap()
    }
    fn recharge(time: &str, kwh: f64) -> Recharge {
        Recharge {
            time: time.into(),
            kwh: Some(kwh),
            yuan: None,
            source: None,
            source_kind: "channel".into(),
        }
    }
    fn summarize_rows(
        rows: &[ElectricityReading],
        balance: Option<f64>,
        recharges: &[Recharge],
    ) -> Summary {
        let today = day("2026-01-05");
        summarize(SummaryInput {
            readings: rows,
            period_start: today - Duration::days(30),
            balance: balance.map(|remaining| BalanceInput {
                remaining,
                observed_at_ms: at("2026-01-05 08:00:00"),
            }),
            recharges,
            now: at("2026-01-05 08:00:00"),
            today,
        })
    }
    #[test]
    fn windows_are_inclusive_across_year_and_leap_day() {
        for today in [
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
        ] {
            for (purpose, days) in [(QueryPurpose::Detail, 31), (QueryPurpose::Monitor, 3)] {
                let (begin, end) = window(today, purpose);
                assert_eq!(
                    (reading_date(&end).unwrap() - reading_date(&begin).unwrap()).num_days() + 1,
                    days
                );
            }
        }
    }
    #[test]
    fn daily_dedup_gaps_and_resets_are_not_zero_consumption() {
        let rows = normalize(vec![
            reading("2026-01-02 18:00:00", 15.0),
            reading("2026-01-01", 10.0),
            reading("2026-01-02 09:00:00", 12.0),
            reading("2026-01-04", 18.0),
            reading("2026-01-05", 2.0),
        ]);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|r| r.daily_used).collect::<Vec<_>>(),
            vec![None, Some(5.0), None, None]
        );
        // The stored timestamp survives normalisation so the balance can be aged.
        assert_eq!(
            rows[1].observed_at.as_deref(),
            Some("2026-01-02 18:00:00")
        );
    }
    #[test]
    fn merged_days_are_spread_over_the_days_they_cover() {
        let rows = normalize(vec![
            reading("2026-01-02", 0.0),
            reading("2026-01-05", 9.0),
        ]);
        let stats = window_average(&rows, day("2026-01-05"), WEEKLY_DAYS);
        assert_eq!(stats.covered_days, 3.0);
        assert_eq!(stats.daily_average, Some(3.0));
    }
    #[test]
    fn baseline_outside_the_window_only_counts_its_overlap() {
        // 30 kWh over 16 days, but only the last 7 days fall inside the window.
        let rows = normalize(vec![
            reading("2025-12-20", 0.0),
            reading("2026-01-05", 30.0),
        ]);
        let stats = window_average(&rows, day("2026-01-05"), WEEKLY_DAYS);
        assert_eq!(stats.covered_days, 7.0);
        assert!((stats.daily_average.unwrap() - 30.0 / 16.0).abs() < 1e-9);
    }
    #[test]
    fn direct_daily_usage_ignores_missing_days() {
        let rows = normalize(vec![
            daily("2026-01-02", 3.0),
            daily("2026-01-04", 5.0),
            daily("2026-01-05", 4.0),
        ]);
        let stats = window_average(&rows, day("2026-01-05"), WEEKLY_DAYS);
        assert_eq!(stats.covered_days, 3.0);
        assert_eq!(stats.daily_average, Some(4.0));
    }
    #[test]
    fn resets_and_short_windows_do_not_invent_an_estimate() {
        let rows = normalize(vec![
            reading("2026-01-04", 40.0),
            reading("2026-01-05", 2.0),
        ]);
        assert_eq!(window_average(&rows, day("2026-01-05"), WEEKLY_DAYS).daily_average, None);
        assert_eq!(window_average(&normalize(vec![]), day("2026-01-05"), WEEKLY_DAYS).daily_average, None);
    }
    #[test]
    fn balance_uses_elapsed_usage_and_recharges_after_the_reading() {
        let credits = credits_after(
            &[
                recharge("2026-01-03 10:00:00", 99.0),
                // Date-only rows are end-of-day data: the same day is already
                // inside that day's snapshot, a later day is credited.
                Recharge {
                    time: "2026-01-04".into(),
                    kwh: Some(5.0),
                    yuan: None,
                    source: None,
                    source_kind: "channel".into(),
                },
                recharge("2026-01-05 07:00:00", 20.0),
            ],
            at("2026-01-04 23:59:01"),
        );
        assert!((credits - 20.0).abs() < 1e-6);
        // Missing kWh falls back to the fixed tariff.
        assert!((recharge_kwh(&Recharge {
            time: "2026-01-05".into(),
            kwh: None,
            yuan: Some(69.979),
            source: None,
            source_kind: "channel".into(),
        })
        .unwrap()
            - 100.0)
            .abs()
            < 1e-6);
        let state = evaluate_balance(
            10.0,
            credits,
            Some(4.0),
            at("2026-01-04 23:59:01"),
            at("2026-01-05 11:59:01"),
        );
        assert!((state.elapsed_used - 2.0).abs() < 1e-6);
        assert!((state.effective_remaining - 28.0).abs() < 1e-6);
        assert!((state.estimated_days_left.unwrap() - 7.0).abs() < 1e-6);
    }
    #[test]
    fn stale_readings_project_over_the_whole_gap() {
        let state = evaluate_balance(
            100.0,
            0.0,
            Some(4.0),
            at("2026-01-01 00:00:00"),
            at("2026-01-05 00:00:00"),
        );
        // 四天的缺口按实际经过时间扣减，而不是只扣一天。
        assert_eq!(state.elapsed_used, 16.0);
        assert_eq!(state.effective_remaining, 84.0);
        assert!((state.estimated_days_left.unwrap() - 21.0).abs() < 1e-9);
        // 余额允许透支：负数照实保留，天数夹到 0。
        let overdraft = evaluate_balance(
            10.0,
            0.0,
            Some(4.0),
            at("2026-01-01 00:00:00"),
            at("2026-01-05 00:00:00"),
        );
        assert_eq!(overdraft.effective_remaining, -6.0);
        assert_eq!(overdraft.estimated_days_left, Some(0.0));
    }
    #[test]
    fn prediction_and_thresholds_follow_the_balance() {
        let rows = normalize(vec![
            reading("2026-01-01", 0.0),
            reading("2026-01-02", 1.0),
            reading("2026-01-03", 2.0),
            reading("2026-01-04", 12.0),
        ]);
        // 12 kWh over 3 days → 4 kWh/day in both windows. 9 kWh left ≈ 2.25 days.
        let summary = summarize_rows(&rows, Some(9.0), &[]);
        assert_eq!(summary.weekly_daily_average, Some(4.0));
        assert_eq!(summary.monthly_daily_average, Some(4.0));
        assert_eq!(summary.prediction_daily_average, Some(4.0));
        assert!(summary.estimated_days_left.unwrap() > 2.0);
        assert!(summary.warning_reason.is_none());
        assert!(summarize_rows(&rows, Some(4.99), &[])
            .warning_reason
            .unwrap()
            .contains("5 度"));
        // 学校允许透支：负余额的预警要说明已透支多少。
        let overdrawn = summarize_rows(&rows, Some(-2.0), &[]);
        assert!(overdrawn.warning_reason.unwrap().contains("透支 2.0 度"));
        assert_eq!(overdrawn.estimated_days_left, Some(0.0));
        // A recharge after the reading lifts the balance out of the warning band.
        assert!(summarize_rows(&rows, Some(1.0), &[recharge("2026-01-05 09:00:00", 50.0)])
            .warning_reason
            .is_none());
        // Stale observations never produce a warning.
        let stale = summarize(SummaryInput {
            readings: &rows,
            period_start: day("2025-12-06"),
            balance: Some(BalanceInput {
                remaining: 1.0,
                observed_at_ms: at("2026-01-01 00:00:00"),
            }),
            recharges: &[],
            now: at("2026-01-05 08:00:00"),
            today: day("2026-01-05"),
        });
        assert!(stale.expired);
        // 过期读数也预警：有效余额已按实际经过的天数扣过，透支不能因为延迟发布就沉默。
        assert!(stale.warning_reason.unwrap().contains("透支"));
        assert!(stale.estimated_days_left.is_some());
        assert!(summarize_rows(&rows, None, &[]).warning_reason.is_none());
    }
    #[test]
    fn lihu_direct_daily_usage_does_not_need_a_baseline() {
        let rows = normalize(vec![daily("2026-01-04", 3.0), daily("2026-01-05", 3.0)]);
        assert_eq!(rows[1].daily_used, Some(3.0));
        let summary = summarize_rows(&rows, Some(6.0), &[]);
        assert_eq!(summary.estimated_days_left, Some(2.0));
        assert!((constants().yuan_per_kwh - 0.69979).abs() < 1e-9);
    }
    #[test]
    fn period_total_only_counts_deltas_inside_the_query_window() {
        let rows = normalize(vec![
            reading("2025-12-01", 0.0),
            reading("2025-12-02", 2.0),
            reading("2026-01-03", 20.0),
            reading("2026-01-04", 28.0),
        ]);
        let summary = summarize_rows(&rows, Some(9.0), &[]);
        // 展示用的周期合计只算落在 31 天窗口内的差分。
        assert_eq!(summary.total_used, Some(8.0));
        // 估计值仍然按两个窗口各自的覆盖天数摊平。
        assert!(summary.weekly_daily_average.unwrap() > summary.monthly_daily_average.unwrap());
    }
    #[test]
    fn blends_the_weekly_and_monthly_estimates() {
        // 25 天每天 1 度，最后 5 天每天 3 度。
        let rows = normalize(vec![
            reading("2025-12-06", 0.0),
            reading("2025-12-31", 25.0),
            reading("2026-01-05", 40.0),
        ]);
        let estimate = estimate(&rows, day("2026-01-05"));
        assert!((estimate.weekly.daily_average.unwrap() - 17.0 / 7.0).abs() < 1e-9);
        assert!((estimate.monthly.daily_average.unwrap() - 40.0 / 30.0).abs() < 1e-9);
        // 最终估计取两侧均值，余额校正与预计天数都用它。
        let blended = (17.0 / 7.0 + 40.0 / 30.0) / 2.0;
        assert!((estimate.blended.unwrap() - blended).abs() < 1e-9);
        let summary = summarize_rows(&rows, Some(10.0), &[]);
        assert_eq!(summary.prediction_daily_average, estimate.blended);
    }
}
