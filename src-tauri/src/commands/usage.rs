use crate::{app::AppState, domain::usage::UsageSnapshot, error::Result, service::usage};
use tauri::State;

#[tauri::command]
pub async fn get_usage(
    state: State<'_, AppState>,
    days: Option<u32>,
    model: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
) -> Result<UsageSnapshot> {
    usage::get(&state, days, model, start_date, end_date).await
}
