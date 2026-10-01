use crate::{
    app::AppState,
    error::Result,
    proxy::monitor::Snapshot,
    service::proxy::{self, ProxyRuntime},
};
use tauri::State;

#[tauri::command]
pub fn get_proxy_status(state: State<'_, AppState>) -> bool {
    proxy::is_running(&state)
}

#[tauri::command]
pub async fn get_proxy_runtime(state: State<'_, AppState>) -> Result<ProxyRuntime> {
    proxy::runtime(&state).await
}

#[tauri::command]
pub fn control_proxy(state: State<'_, AppState>, action: String) -> Result<()> {
    proxy::control(&state, &action)
}

#[tauri::command]
pub fn get_proxy_monitor(state: State<'_, AppState>, minutes: Option<u32>) -> Snapshot {
    proxy::monitor(&state, minutes)
}
