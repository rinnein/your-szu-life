use super::{reading_date, reading_datetime, ElectricityReading, QueryPurpose, LOW_THRESHOLD};
use chrono::{Duration, FixedOffset, NaiveDate, Utc};
use serde::Serialize;
use std::collections::BTreeMap;

pub(crate) const YUAN_PER_KWH: f64 = 0.61;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ElectricityConstants {
    pub yuan_per_kwh: f64,
}
pub(crate) fn constants() -> ElectricityConstants {
    ElectricityConstants {
        yuan_per_kwh: super::YUAN_PER_KWH,
    }
}
fn shanghai() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).unwrap()
}
pub(super) fn today() -> NaiveDate {
    Utc::now().with_timezone(&shanghai()).date_naive()
}
pub(super) fn expiry_ms() -> i64 {
    super::history::valid_until(Utc::now().timestamp_millis())
}
pub(super) fn window(today: NaiveDate, purpose: QueryPurpose) -> (String, String) {
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
pub(super) fn normalize(mut rows: Vec<ElectricityReading>) -> Vec<ElectricityReading> {
    rows.sort_by_key(|r| reading_datetime(&r.date));
    let mut days = BTreeMap::new();
    for mut row in rows {
        if let Some(date) = reading_date(&row.date) {
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
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Summary {
    pub total_used: Option<f64>,
    pub daily_average: Option<f64>,
    pub valid_days: usize,
    pub prediction_daily_average: Option<f64>,
    pub estimated_days_left: Option<f64>,
    pub observed_date: Option<String>,
    pub expired: bool,
    pub warning_reason: Option<String>,
}
pub(super) fn summarize(
    rows: &[ElectricityReading],
    remaining: Option<f64>,
    observed_date: Option<String>,
    today: NaiveDate,
) -> Summary {
    let used: Vec<_> = rows
        .iter()
        .filter_map(|r| r.daily_used)
        .filter(|n| n.is_finite() && *n >= 0.0)
        .collect();
    let total_used = (!used.is_empty()).then(|| used.iter().sum::<f64>());
    let start = today - Duration::days(2);
    let recent: Vec<_> = rows
        .iter()
        .filter(|r| {
            reading_date(&r.date).is_some_and(|d| {
                // Cumulative differences must have both samples inside the 3-day window.
                d <= today
                    && if r.total_used.is_some() {
                        d > start
                    } else {
                        d >= start
                    }
            })
        })
        .filter_map(|r| r.daily_used)
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    let prediction_daily_average =
        (!recent.is_empty()).then(|| recent.iter().sum::<f64>() / recent.len() as f64);
    let expired = !observed_date
        .as_deref()
        .and_then(reading_date)
        .is_some_and(|d| d >= today - Duration::days(1) && d <= today);
    let estimated_days_left = if expired {
        None
    } else {
        remaining
            .zip(prediction_daily_average)
            .filter(|(_, avg)| *avg > 0.0)
            .map(|(balance, avg)| balance.max(0.0) / avg)
    };
    let warning_reason = if expired {
        None
    } else if remaining.is_some_and(|n| n < LOW_THRESHOLD) {
        Some("宿舍剩余电量低于 5 度，请及时充值。".into())
    } else if estimated_days_left.is_some_and(|d| d < 1.0) {
        Some("预计可用电量不足 1 天，请及时充值。".into())
    } else {
        None
    };
    Summary {
        total_used,
        daily_average: total_used.map(|n| n / used.len() as f64),
        valid_days: used.len(),
        prediction_daily_average,
        estimated_days_left,
        observed_date,
        expired,
        warning_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(date: &str, used: f64) -> ElectricityReading {
        ElectricityReading {
            date: date.into(),
            room_name: "101".into(),
            remaining: Some(10.0),
            total_used: Some(used),
            total_purchased: Some(50.0),
            daily_used: None,
            daily_purchased: None,
        }
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
            row("2026-01-02 18:00:00", 15.0),
            row("2026-01-01", 10.0),
            row("2026-01-02 09:00:00", 12.0),
            row("2026-01-04", 18.0),
            row("2026-01-05", 2.0),
        ]);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().map(|r| r.daily_used).collect::<Vec<_>>(),
            vec![None, Some(5.0), None, None]
        );
    }
    #[test]
    fn detail_and_monitor_have_identical_prediction_and_thresholds() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let detail = normalize(vec![
            row("2026-01-01", 0.0),
            row("2026-01-02", 1.0),
            row("2026-01-03", 2.0),
            row("2026-01-04", 12.0),
        ]);
        let monitor = normalize(vec![row("2026-01-03", 2.0), row("2026-01-04", 12.0)]);
        let summary = |rows: &[ElectricityReading], balance| {
            summarize(rows, Some(balance), Some("2026-01-04".into()), today)
        };
        assert_eq!(
            summary(&detail, 9.0).estimated_days_left,
            summary(&monitor, 9.0).estimated_days_left
        );
        assert!(summary(&detail, 9.0)
            .warning_reason
            .unwrap()
            .contains("1 天"));
        assert!(summary(&detail, 10.0).warning_reason.is_none());
        assert!(summary(&detail, 4.99)
            .warning_reason
            .unwrap()
            .contains("5 度"));
        assert!(summarize(&[], Some(5.0), Some("2026-01-04".into()), today)
            .warning_reason
            .is_none());
        assert!(
            summarize(&detail, Some(1.0), Some("2026-01-01".into()), today)
                .warning_reason
                .is_none()
        );
        assert!(summary(
            &normalize(vec![row("2026-01-03", 0.0), row("2026-01-04", 0.0)]),
            10.0
        )
        .estimated_days_left
        .is_none());
    }
    #[test]
    fn lihu_direct_daily_usage_does_not_need_a_baseline() {
        let mut r = row("2026-01-04", 1.0);
        r.total_used = None;
        r.remaining = None;
        r.daily_used = Some(3.0);
        let rows = normalize(vec![r]);
        assert_eq!(rows[0].daily_used, Some(3.0));
        assert_eq!(
            summarize(
                &rows,
                Some(6.0),
                Some("2026-01-05".into()),
                NaiveDate::from_ymd_opt(2026, 1, 5).unwrap()
            )
            .estimated_days_left,
            Some(2.0)
        );
        assert_eq!(constants().yuan_per_kwh * 100.0, 61.0);
    }
}
