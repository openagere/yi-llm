use crate::{app::AppState, error::Result, service::logs};
use tauri::State;

#[tauri::command]
pub async fn get_logs(state: State<'_, AppState>) -> Result<String> {
    logs::tail(&state).await
}
