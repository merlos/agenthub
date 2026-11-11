use crate::model::{ProviderStats, ProviderUsage, TokenBucket, TokenSummary};
use chrono::{Datelike, Months, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap()
}

fn billing_date(month: NaiveDate, day: u32) -> NaiveDate {
    let last = month
        .checked_add_months(Months::new(1))
        .unwrap()
        .pred_opt()
        .unwrap();
    month.with_day(day.min(last.day())).unwrap()
}

pub fn billing_period(today: NaiveDate, day: u32) -> anyhow::Result<(NaiveDate, NaiveDate)> {
    anyhow::ensure!(
        (1..=31).contains(&day),
        "Billing start day must be between 1 and 31"
    );
    let month = month_start(today);
    let start = billing_date(month, day);
    let month = if today < start {
        month.checked_sub_months(Months::new(1)).unwrap()
    } else {
        month
    };
    Ok((
        billing_date(month, day),
        billing_date(month.checked_add_months(Months::new(1)).unwrap(), day),
    ))
}

const AGGREGATES: &str = "COUNT(*), COALESCE(SUM(tokens_in),0), COALESCE(SUM(tokens_out),0),
    COALESCE(AVG(CASE WHEN duration_ms > 0 THEN tokens_out * 1000.0 / duration_ms END),0)";

fn summary(row: &rusqlite::Row<'_>, offset: usize) -> rusqlite::Result<TokenSummary> {
    let requests: u64 = row.get(offset)?;
    let tokens_in: u64 = row.get(offset + 1)?;
    let tokens_out: u64 = row.get(offset + 2)?;
    let total_tokens = tokens_in + tokens_out;
    Ok(TokenSummary {
        requests,
        tokens_in,
        tokens_out,
        total_tokens,
        average_tokens: if requests == 0 {
            0.0
        } else {
            total_tokens as f64 / requests as f64
        },
        average_tokens_per_second: row.get(offset + 3)?,
    })
}

fn totals(
    connection: &Connection,
    id: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<TokenSummary> {
    Ok(connection.query_row(
        &format!("SELECT {AGGREGATES} FROM requests WHERE provider_id=? AND created_at>=? AND created_at<?"),
        params![id, start, end],
        |row| summary(row, 0),
    )?)
}

fn boundary(date: NaiveDate) -> String {
    format!("{date}T00:00:00")
}

pub fn list_usage(connection: &Connection, today: NaiveDate) -> anyhow::Result<Vec<ProviderUsage>> {
    let mut query = connection.prepare("SELECT p.id,COALESCE(b.start_day,1) FROM providers p LEFT JOIN provider_billing b ON b.provider_id=p.id ORDER BY p.is_default DESC,p.name")?;
    let providers = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    providers
        .into_iter()
        .map(|(id, day)| {
            let (start, end) = billing_period(today, day)?;
            Ok(ProviderUsage {
                total_tokens: totals(connection, &id, "", "9999")?.total_tokens,
                month_tokens: totals(connection, &id, &boundary(start), &boundary(end))?
                    .total_tokens,
                provider_id: id,
                period_start: start.to_string(),
                period_end: end.pred_opt().unwrap().to_string(),
            })
        })
        .collect()
}

fn buckets(
    connection: &Connection,
    id: &str,
    dates: Vec<NaiveDate>,
    monthly: bool,
    today: NaiveDate,
) -> anyhow::Result<Vec<TokenBucket>> {
    let length = if monthly { 7 } else { 10 };
    let mut query = connection.prepare(&format!(
        "SELECT substr(created_at,1,{length}),{AGGREGATES}
        FROM requests WHERE provider_id=? AND created_at>=? AND created_at<? GROUP BY 1 ORDER BY 1"
    ))?;
    let values: HashMap<String, TokenSummary> = query
        .query_map(
            params![id, boundary(dates[0]), boundary(today.succ_opt().unwrap())],
            |row| Ok((row.get(0)?, summary(row, 1)?)),
        )?
        .collect::<Result<_, _>>()?;
    Ok(dates
        .into_iter()
        .map(|date| {
            let key = date.to_string()[..length].to_string();
            TokenBucket {
                date: key.clone(),
                usage: values.get(&key).cloned().unwrap_or_default(),
            }
        })
        .collect())
}

pub fn stats(connection: &Connection, id: &str, today: NaiveDate) -> anyhow::Result<ProviderStats> {
    let (name, day): (String, u32) = connection.query_row(
        "SELECT p.name,COALESCE(b.start_day,1) FROM providers p LEFT JOIN provider_billing b ON b.provider_id=p.id WHERE p.id=?",
        [id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?.ok_or_else(|| anyhow::anyhow!("Provider not found"))?;
    let (start, end) = billing_period(today, day)?;
    let daily = (0..30)
        .map(|offset| today - chrono::Duration::days(29 - offset))
        .collect();
    let month = month_start(today);
    let monthly = (0..12)
        .map(|offset| month.checked_sub_months(Months::new(11 - offset)).unwrap())
        .collect();
    Ok(ProviderStats {
        provider_id: id.to_string(),
        provider_name: name,
        billing_start_day: day,
        period_start: start.to_string(),
        period_end: end.pred_opt().unwrap().to_string(),
        current: totals(connection, id, &boundary(start), &boundary(end))?,
        historical: totals(connection, id, "", "9999")?,
        daily: buckets(connection, id, daily, false, today)?,
        monthly: buckets(connection, id, monthly, true, today)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        value.parse().unwrap()
    }

    #[test]
    fn billing_boundaries_and_short_months() {
        for (today, day, start, end) in [
            ("2026-11-11", 1, "2026-11-01", "2026-12-01"),
            ("2026-11-14", 15, "2026-10-15", "2026-11-15"),
            ("2026-11-15", 15, "2026-11-15", "2026-12-15"),
            ("2026-01-01", 15, "2025-12-15", "2026-01-15"),
            ("2026-02-27", 31, "2026-01-31", "2026-02-28"),
            ("2026-02-28", 31, "2026-02-28", "2026-03-31"),
            ("2024-02-29", 29, "2024-02-29", "2024-03-29"),
            ("2026-04-30", 31, "2026-04-30", "2026-05-31"),
            ("2026-02-28", 30, "2026-02-28", "2026-03-30"),
        ] {
            assert_eq!(
                billing_period(date(today), day).unwrap(),
                (date(start), date(end))
            );
        }
        assert!(billing_period(date("2026-11-11"), 0).is_err());
        assert!(billing_period(date("2026-11-11"), 32).is_err());
    }
}
