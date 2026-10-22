use crate::{
    app::{
        legacy_migration::{migrate_from_legacy_sources, migrate_legacy_data_dir},
        logging, AppState,
    },
    catalog::{file::default_path, CatalogService},
    db::{repo::settings, Db},
    error::{AppError, Result},
};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};

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

/// Resolves the application data directory:
/// 1. `YI_LLM_HOME` environment variable (must be an absolute path)
/// 2. `<user_home>/.yi-llm` (survives OS installer uninstall/reinstall cycles)
/// 3. `fallback_app_data_dir` when user home cannot be determined
pub fn resolve_data_dir(fallback_app_data_dir: &Path) -> Result<PathBuf> {
    let configured = std::env::var_os("YI_LLM_HOME");
    let home = user_home();
    resolve_data_dir_from(
        configured.as_deref(),
        home.as_deref(),
        fallback_app_data_dir,
    )
}

fn resolve_data_dir_from(
    configured: Option<&OsStr>,
    home: Option<&Path>,
    fallback_app_data_dir: &Path,
) -> Result<PathBuf> {
    if let Some(raw) = configured {
        let path = PathBuf::from(raw);
        if !path.is_absolute() {
            return Err(AppError::validation("YI_LLM_HOME 必须是绝对目录路径"));
        }
        return Ok(path);
    }
    if let Some(home) = home {
        return Ok(home.join(".yi-llm"));
    }
    Ok(fallback_app_data_dir.to_path_buf())
}

fn user_home() -> Option<PathBuf> {
    let primary = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(primary)
        .or_else(|| std::env::var_os("HOME"))
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)
}

/// Opens the database, logging and catalog under `data_dir` and assembles the app state.
pub fn build_state(data_dir: &Path) -> Result<AppState> {
    build_state_with_legacy(data_dir, None)
}

/// Opens the database, logging and catalog under `data_dir`, migrating from
/// `legacy_app_data_dir` (e.g. `%APPDATA%\com.yi-llm.app`) when needed.
pub fn build_state_with_legacy(
    data_dir: &Path,
    legacy_app_data_dir: Option<&Path>,
) -> Result<AppState> {
    let migration_result = match legacy_app_data_dir {
        Some(legacy) => migrate_from_legacy_sources(data_dir, Some(legacy)),
        None => migrate_legacy_data_dir(data_dir),
    };
    if let Err(error) = migration_result {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_default_data_dir_under_user_home() {
        let home = Path::new("/tmp/fake-home");
        let fallback = Path::new("/tmp/appdata/com.yi-llm.app");
        let resolved = resolve_data_dir_from(None, Some(home), fallback).unwrap();
        assert_eq!(resolved, home.join(".yi-llm"));
    }

    #[test]
    fn falls_back_to_app_data_dir_when_home_is_unavailable() {
        let fallback = Path::new("/tmp/appdata/com.yi-llm.app");
        let resolved = resolve_data_dir_from(None, None, fallback).unwrap();
        assert_eq!(resolved, fallback);
    }

    #[test]
    fn honors_absolute_yi_llm_home_override_and_rejects_relative() {
        let root = tempfile::tempdir().unwrap();
        let custom = root.path().join("custom-yi-llm");
        let fallback = root.path().join("appdata");
        let resolved =
            resolve_data_dir_from(Some(custom.as_os_str()), Some(root.path()), &fallback).unwrap();
        assert_eq!(resolved, custom);

        assert!(resolve_data_dir_from(
            Some(OsStr::new("relative/path")),
            Some(root.path()),
            &fallback,
        )
        .is_err());
    }
}
