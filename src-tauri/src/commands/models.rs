use crate::{
    app::AppState, catalog::file::CatalogInfo, domain::standard_model::StandardModel,
    error::Result, service::catalog,
};
use tauri::State;

#[tauri::command]
pub async fn list_standard_models(state: State<'_, AppState>) -> Result<Vec<StandardModel>> {
    catalog::list(&state).await
}

#[tauri::command]
pub fn get_model_catalog_info(state: State<'_, AppState>) -> Result<CatalogInfo> {
    catalog::info(&state)
}

#[tauri::command]
pub async fn save_standard_model(state: State<'_, AppState>, model: StandardModel) -> Result<()> {
    catalog::save(&state, model).await
}

#[tauri::command]
pub async fn delete_standard_model(state: State<'_, AppState>, id: String) -> Result<()> {
    catalog::delete(&state, id).await
}
