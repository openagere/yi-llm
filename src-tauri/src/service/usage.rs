use crate::{app::AppState, db::repo::usage, domain::usage::UsageSnapshot, error::Result};

pub async fn get(
    state: &AppState,
    days: Option<u32>,
    model: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<UsageSnapshot> {
    state
        .db
        .run(move |conn| match (start_date, end_date) {
            (Some(start), Some(end)) => {
                usage::snapshot_in_range(conn, &start, &end, model.as_deref())
            }
            (None, None) => usage::snapshot_for_days(conn, days.unwrap_or(7), model.as_deref()),
            _ => Err("请同时选择开始日期和结束日期".into()),
        })
        .await
}
