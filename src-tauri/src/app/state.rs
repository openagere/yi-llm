use crate::{
    catalog::CatalogService,
    db::{Db, UsageWriter},
    error::Result,
    proxy::{
        lifecycle::{ProxyController, ProxyStatus},
        monitor::Monitor,
        routing::{RouteCache, RouteTable},
    },
};
use std::{path::PathBuf, sync::Arc};
use tracing_appender::non_blocking::WorkerGuard;

pub type LogReload =
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>;

/// Shared application services. Cheap to clone; every clone shares the same state.
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    pub logs_dir: PathBuf,
    pub log_filter: Option<Arc<LogReload>>,
    pub catalog: Option<Arc<CatalogService>>,
    pub route_cache: Arc<RouteCache>,
    pub usage: UsageWriter,
    pub proxy: ProxyController,
    pub monitor: Monitor,
    /// Keeps the non-blocking log writer flushing for as long as the app lives.
    _log_guard: Option<Arc<WorkerGuard>>,
}

impl AppState {
    pub fn new(
        db: Db,
        http: reqwest::Client,
        logs_dir: PathBuf,
        log_filter: Option<Arc<LogReload>>,
    ) -> Self {
        let monitor = Monitor::default();
        Self {
            usage: UsageWriter::spawn(db.clone()),
            proxy: ProxyController::new(monitor.clone()),
            db,
            http,
            logs_dir,
            log_filter,
            catalog: None,
            route_cache: Arc::new(RouteCache::default()),
            monitor,
            _log_guard: None,
        }
    }

    pub fn with_catalog(mut self, catalog: CatalogService) -> Self {
        self.catalog = Some(Arc::new(catalog));
        self
    }

    pub fn with_log_guard(mut self, guard: WorkerGuard) -> Self {
        self._log_guard = Some(Arc::new(guard));
        self
    }

    /// Must be called after any change to providers, models, the catalog or terminal
    /// profiles that did not go through a service function.
    pub fn invalidate_routes(&self) {
        self.route_cache.invalidate();
    }

    /// Applies the shared catalog file when it changed on disk.
    pub async fn refresh_catalog(&self) -> Result<()> {
        let Some(catalog) = &self.catalog else {
            return Ok(());
        };
        if !catalog.is_stale() {
            return Ok(());
        }
        let catalog = catalog.clone();
        let db = self.db.clone();
        let changed = tokio::task::spawn_blocking(move || catalog.refresh(&db)).await??;
        if changed {
            self.invalidate_routes();
        }
        Ok(())
    }

    /// Current routing snapshot, after picking up catalog file edits.
    pub async fn route_table(&self) -> Result<Arc<RouteTable>> {
        self.refresh_catalog().await?;
        self.route_cache.get(&self.db).await
    }

    pub fn proxy_status(&self) -> ProxyStatus {
        self.proxy.status()
    }

    pub fn is_proxy_running(&self) -> bool {
        self.proxy.is_running()
    }

    pub fn control_proxy(&self, action: &str) -> Result<()> {
        self.proxy.control(action)
    }

    pub fn restart_proxy(&self) {
        self.proxy.restart();
    }
}
