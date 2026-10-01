use crate::app::state::LogReload;
use std::{path::Path, sync::Arc};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{prelude::*, reload, EnvFilter};

pub struct Logging {
    pub filter: Arc<LogReload>,
    pub guard: WorkerGuard,
}

/// Installs a daily-rolling file logger whose level can be changed at runtime.
pub fn init(logs_dir: &Path, level: &str) -> std::io::Result<Logging> {
    std::fs::create_dir_all(logs_dir)?;
    let appender = tracing_appender::rolling::daily(logs_dir, "yi-llm.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let (filter, handle) = reload::Layer::new(EnvFilter::new(level));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        )
        .try_init();
    Ok(Logging {
        filter: Arc::new(handle),
        guard,
    })
}
