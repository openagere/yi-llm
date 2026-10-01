use crate::{
    domain::usage::{NewUsage, UsageDailyModel, UsageDay, UsageModel, UsageRecord, UsageSnapshot},
    error::{AppError, Result},
};
use rusqlite::{params, Connection};

const INSERT: &str = "INSERT INTO usage (requested_at, provider_name, protocol, model, input_tokens, output_tokens, cached_tokens, reasoning_tokens)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

fn insert_params(usage: &NewUsage) -> [&dyn rusqlite::ToSql; 8] {
    [
        &usage.requested_at,
        &usage.provider_name,
        &usage.protocol,
        &usage.model,
        &usage.input_tokens,
        &usage.output_tokens,
        &usage.cached_tokens,
        &usage.reasoning_tokens,
    ]
}

pub fn record(conn: &Connection, usage: &NewUsage) -> Result<()> {
    conn.execute(INSERT, insert_params(usage))?;
    Ok(())
}

/// Persists many rows in a single transaction.
pub fn record_batch(conn: &mut Connection, batch: &[NewUsage]) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare_cached(INSERT)?;
        for usage in batch {
            stmt.execute(insert_params(usage))?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn snapshot_for_days(
    conn: &Connection,
    days: u32,
    model: Option<&str>,
) -> Result<UsageSnapshot> {
    let days = days.clamp(1, 365);
    let modifier = format!("-{} days", days.saturating_sub(1));
    let (start_date, end_date): (String, String) = conn.query_row(
        "SELECT date('now', 'localtime', ?1), date('now', 'localtime')",
        params![modifier],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    snapshot_in_range(conn, &start_date, &end_date, model)
}

pub fn snapshot_in_range(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
    model: Option<&str>,
) -> Result<UsageSnapshot> {
    validate_range(conn, start_date, end_date)?;
    let totals = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0),
                COALESCE(SUM(cached_tokens),0), COALESCE(SUM(reasoning_tokens),0)
         FROM usage WHERE date(requested_at, 'unixepoch', 'localtime') BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR model = ?3)",
        params![start_date, end_date, model],
        |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, u64>(1)?,
                row.get::<_, u64>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, u64>(4)?,
            ))
        },
    )?;
    let mut day_stmt = conn.prepare(
        "SELECT date(requested_at, 'unixepoch', 'localtime'), SUM(input_tokens), SUM(output_tokens), SUM(cached_tokens), SUM(reasoning_tokens)
         FROM usage WHERE date(requested_at, 'unixepoch', 'localtime') BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR model = ?3) GROUP BY 1 ORDER BY 1",
    )?;
    let days = day_stmt
        .query_map(params![start_date, end_date, model], |row| {
            Ok(UsageDay {
                day: row.get(0)?,
                input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                cached_tokens: row.get(3)?,
                reasoning_tokens: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut daily_model_stmt = conn.prepare(
        "SELECT date(requested_at, 'unixepoch', 'localtime'), model, SUM(input_tokens + output_tokens)
         FROM usage WHERE date(requested_at, 'unixepoch', 'localtime') BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR model = ?3) GROUP BY 1, 2 ORDER BY 1, 3 DESC",
    )?;
    let daily_models = daily_model_stmt
        .query_map(params![start_date, end_date, model], |row| {
            Ok(UsageDailyModel {
                day: row.get(0)?,
                model: row.get(1)?,
                tokens: row.get(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut model_stmt = conn.prepare(
        "SELECT model, COUNT(*), SUM(input_tokens), SUM(output_tokens), SUM(cached_tokens), SUM(reasoning_tokens) FROM usage
         WHERE date(requested_at, 'unixepoch', 'localtime') BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR model = ?3) GROUP BY model ORDER BY SUM(input_tokens + output_tokens) DESC",
    )?;
    let models = model_stmt
        .query_map(params![start_date, end_date, model], |row| {
            Ok(UsageModel {
                model: row.get(0)?,
                requests: row.get(1)?,
                input_tokens: row.get(2)?,
                output_tokens: row.get(3)?,
                cached_tokens: row.get(4)?,
                reasoning_tokens: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut recent_stmt = conn.prepare(
        "SELECT id, requested_at, provider_name, protocol, model, input_tokens, output_tokens, cached_tokens, reasoning_tokens
         FROM usage WHERE date(requested_at, 'unixepoch', 'localtime') BETWEEN ?1 AND ?2
           AND (?3 IS NULL OR model = ?3) ORDER BY requested_at DESC, id DESC LIMIT 100",
    )?;
    let recent = recent_stmt
        .query_map(params![start_date, end_date, model], |row| {
            Ok(UsageRecord {
                id: row.get(0)?,
                requested_at: row.get(1)?,
                provider_name: row.get(2)?,
                protocol: row.get(3)?,
                model: row.get(4)?,
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
                cached_tokens: row.get(7)?,
                reasoning_tokens: row.get(8)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(UsageSnapshot {
        total_requests: totals.0,
        input_tokens: totals.1,
        output_tokens: totals.2,
        cached_tokens: totals.3,
        reasoning_tokens: totals.4,
        days,
        daily_models,
        models,
        recent,
    })
}

fn validate_range(conn: &Connection, start_date: &str, end_date: &str) -> Result<()> {
    let (start, end, span, today): (Option<String>, Option<String>, Option<f64>, String) = conn
        .query_row(
            "SELECT date(?1), date(?2), julianday(?2) - julianday(?1), date('now', 'localtime')",
            params![start_date, end_date],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    if start.as_deref() != Some(start_date)
        || end.as_deref() != Some(end_date)
        || start_date.len() != 10
        || end_date.len() != 10
    {
        return Err(AppError::validation("日期格式无效，请选择有效的起止日期"));
    }
    if start_date > end_date {
        return Err(AppError::validation("开始日期不能晚于结束日期"));
    }
    if end_date > today.as_str() {
        return Err(AppError::validation("结束日期不能晚于今天"));
    }
    if span.is_none_or(|value| !(0.0..365.0).contains(&value)) {
        return Err(AppError::validation("单次最多选择 365 天"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    pub fn sample(model: &str) -> NewUsage {
        NewUsage {
            provider_name: "Provider".into(),
            protocol: "responses".into(),
            model: model.into(),
            input_tokens: 100,
            output_tokens: 50,
            cached_tokens: 20,
            reasoning_tokens: 10,
            requested_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
    }

    #[test]
    fn usage_includes_all_models_and_scoped_totals() {
        let dir = tempfile::tempdir().unwrap();
        let mut conn = Db::open(&dir.path().join("test.db"))
            .unwrap()
            .conn()
            .unwrap();
        let batch: Vec<_> = (0..25)
            .map(|index| sample(&format!("model-{index}")))
            .collect();
        record_batch(&mut conn, &batch).unwrap();
        let all = snapshot_for_days(&conn, 7, None).unwrap();
        assert_eq!(all.models.len(), 25);
        assert_eq!(all.total_requests, 25);
        assert_eq!((all.input_tokens, all.output_tokens), (2500, 1250));
        assert_eq!(all.daily_models.len(), 25);
        let selected = snapshot_for_days(&conn, 7, Some("model-24")).unwrap();
        assert_eq!(selected.models.len(), 1);
        assert_eq!(selected.total_requests, 1);
        assert_eq!((selected.input_tokens, selected.output_tokens), (100, 50));
        assert_eq!(
            (selected.cached_tokens, selected.reasoning_tokens),
            (20, 10)
        );
        assert_eq!(selected.recent.len(), 1);
    }

    #[test]
    fn rejects_invalid_date_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Db::open(&dir.path().join("test.db"))
            .unwrap()
            .conn()
            .unwrap();
        assert!(snapshot_in_range(&conn, "2026-13-01", "2026-13-02", None).is_err());
        assert!(snapshot_in_range(&conn, "2026-02-02", "2026-02-01", None).is_err());
        assert!(snapshot_in_range(&conn, "2026-01-01", "2999-01-01", None).is_err());
        assert!(snapshot_in_range(&conn, "2020-01-01", "2026-01-01", None).is_err());
    }
}
