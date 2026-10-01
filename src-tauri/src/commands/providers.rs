use crate::{
    app::AppState,
    domain::provider::{ModelMapping, Provider, ProviderView},
    error::Result,
    service::providers::{self, SaveProviderResult},
};
use tauri::State;

#[tauri::command]
pub async fn list_providers(state: State<'_, AppState>) -> Result<Vec<ProviderView>> {
    providers::list(&state).await
}

#[tauri::command]
pub async fn save_provider(
    state: State<'_, AppState>,
    provider: Provider,
    models: Vec<ModelMapping>,
) -> Result<SaveProviderResult> {
    providers::save(&state, provider, models).await
}

#[tauri::command]
pub async fn delete_provider(state: State<'_, AppState>, id: String) -> Result<()> {
    providers::delete(&state, id).await
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    provider: Provider,
    models: Vec<ModelMapping>,
    model: String,
) -> Result<String> {
    providers::test(&state, provider, models, model).await
}
