pub mod app;
pub mod catalog;
mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod protocol;
pub mod proxy;
pub mod service;
pub mod terminal;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .runtime(tauri_runtime_wry::Wry::default())
        .setup(|app| {
            let state = app::bootstrap::build_state(&app.path().app_data_dir()?)?;
            tauri::async_runtime::spawn(proxy::serve_managed(state.clone()));
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::providers::list_providers,
            commands::providers::save_provider,
            commands::providers::delete_provider,
            commands::providers::test_provider,
            commands::models::list_standard_models,
            commands::models::get_model_catalog_info,
            commands::models::save_standard_model,
            commands::models::delete_standard_model,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::proxy::get_proxy_status,
            commands::proxy::get_proxy_runtime,
            commands::proxy::get_proxy_monitor,
            commands::proxy::control_proxy,
            commands::logs::get_logs,
            commands::usage::get_usage,
            commands::terminal::get_terminal_profiles,
            commands::terminal::preview_terminal_config,
            commands::terminal::apply_terminal_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
