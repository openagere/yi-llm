use crate::{
    app::AppState,
    domain::terminal::Profile,
    error::Result,
    service::terminal,
    terminal::{ApplyResult, Preview, Status},
};
use tauri::State;

#[tauri::command]
pub async fn get_terminal_profiles(state: State<'_, AppState>) -> Result<Vec<Status>> {
    terminal::list_status(&state).await
}

#[tauri::command]
pub async fn preview_terminal_config(
    state: State<'_, AppState>,
    profile: Profile,
) -> Result<Preview> {
    terminal::preview(&state, profile).await
}

#[tauri::command]
pub async fn apply_terminal_config(
    state: State<'_, AppState>,
    profile: Profile,
) -> Result<ApplyResult> {
    terminal::apply(&state, profile).await
}
