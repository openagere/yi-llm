use crate::{app::AppState, domain::settings::ServerSettings, error::Result, service::settings};
use tauri::State;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<ServerSettings> {
    settings::get(&state).await
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    host: String,
    port: u16,
    log_level: String,
) -> Result<()> {
    settings::save(&state, host, port, log_level).await
}
