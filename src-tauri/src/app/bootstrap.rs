use crate::{
    app::{legacy_migration::migrate_legacy_data_dir, logging, AppState},
    catalog::{file::default_path, CatalogService},
    db::{repo::settings, Db},
    error::{AppError, Result},
};
use std::{path::Path, time::Duration};

/// Upstream client shared by every request: pooled keep-alive connections, HTTP/2 where
/// offered, and no total-duration cap so long streams are not cut off. Idle stalls are
/// still bounded by the per-read timeout.
pub fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(32)
        .pool_idle_timeout(Duration::from_secs(90))
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(30))
        .build()
        .map_err(|error| AppError::internal(format!("无法创建 HTTP 客户端: {error}")))
}

/// Opens the database, logging and catalog under `data_dir` and assembles the app state.
pub fn build_state(data_dir: &Path) -> Result<AppState> {
    if let Err(error) = migrate_legacy_data_dir(data_dir) {
        eprintln!("旧数据迁移失败：{error}");
    }
    std::fs::create_dir_all(data_dir)?;
    let db = Db::open(&data_dir.join("yi-llm.db"))?;
    let saved = {
        let conn = db.conn()?;
        settings::get(&conn)?
    };
    let logs_dir = data_dir.join("logs");
    let logging = logging::init(&logs_dir, &saved.log_level)?;

    let catalog = {
        let mut conn = db.conn()?;
        let catalog = CatalogService::open(default_path(data_dir)?, &conn)?;
        if let Err(error) = catalog.sync(&mut conn) {
            tracing::error!(%error, "model catalog could not be loaded");
        }
        catalog
    };
    Ok(
        AppState::new(db, build_http_client()?, logs_dir, Some(logging.filter))
            .with_catalog(catalog)
            .with_log_guard(logging.guard),
    )
}
