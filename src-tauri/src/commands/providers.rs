use crate::{
    app::AppState,
    backup::{self, FILE_EXTENSION, FILE_FORMAT_NAME},
    domain::provider::{ModelMapping, Provider, ProviderView},
    error::{AppError, Result},
    service::providers::{self, SaveProviderResult},
};
use serde::Serialize;
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

/// 导出与导入的结果：所选文件，以及文件中包含的配置数量。
#[derive(Serialize)]
pub struct ProviderConfigTransferResult {
    pub path: String,
    pub providers: usize,
    pub models: usize,
    /// 导出：写入文件的标准模型数；导入：合并进本地模型目录的标准模型数。
    pub standard_models: usize,
    pub terminal_config_warnings: Vec<String>,
}

/// 将全部 Provider（含模型映射）与模型目录写入加密文件；用户取消保存对话框时返回 None。
#[tauri::command]
pub async fn export_provider_config(
    state: State<'_, AppState>,
    passphrase: String,
    title: String,
) -> Result<Option<ProviderConfigTransferResult>> {
    let bundle = providers::export_bundle(&state).await?;
    let contents = backup::encode(&bundle, &passphrase)?;
    let Some(file) = save_dialog(&title).await else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    tokio::fs::write(&path, contents.as_bytes())
        .await
        .map_err(|error| AppError::Io(format!("写入 {} 失败：{error}", path.display())))?;
    Ok(Some(ProviderConfigTransferResult {
        path: path.display().to_string(),
        providers: bundle.providers.len(),
        models: bundle.providers.iter().map(|view| view.models.len()).sum(),
        standard_models: bundle.standard_models.len(),
        terminal_config_warnings: Vec::new(),
    }))
}

/// 解密前端选中的配置文件，并导入其中的 Provider、模型映射与标准模型。
#[tauri::command]
pub async fn import_provider_config(
    state: State<'_, AppState>,
    passphrase: String,
    contents: String,
    filename: String,
) -> Result<ProviderConfigTransferResult> {
    let text = contents.trim();
    if text.is_empty() {
        return Err(AppError::validation("配置文件是空的"));
    }
    let bundle = backup::decode(text, &passphrase)?;
    let summary = providers::import_bundle(&state, bundle).await?;
    let name = filename.trim();
    Ok(ProviderConfigTransferResult {
        path: if name.is_empty() {
            format!("yi-llm-providers.{FILE_EXTENSION}")
        } else {
            name.to_string()
        },
        providers: summary.providers,
        models: summary.models,
        standard_models: summary.standard_models,
        terminal_config_warnings: summary.terminal_config_warnings,
    })
}

/// 原生保存对话框，默认文件名与过滤器都限定为本应用的配置备份。
async fn save_dialog(title: &str) -> Option<rfd::FileHandle> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(format!("yi-llm-providers.{FILE_EXTENSION}"))
        .add_filter(FILE_FORMAT_NAME, &[FILE_EXTENSION])
        .save_file()
        .await
}
