use crate::{
    app::AppState,
    db::repo::settings,
    domain::settings::ServerSettings,
    error::{AppError, Result},
};
use tracing_subscriber::EnvFilter;

const LOG_LEVELS: [&str; 5] = ["error", "warn", "info", "debug", "trace"];

pub async fn get(state: &AppState) -> Result<ServerSettings> {
    state.db.run(|conn| settings::get(conn)).await
}

pub fn validate(host: &str, port: u16, log_level: &str) -> Result<()> {
    if host.trim().is_empty() || host.parse::<std::net::IpAddr>().is_err() {
        return Err("host 必须是有效 IP 地址".into());
    }
    if port == 0 {
        return Err("port 必须在 1 到 65535 之间".into());
    }
    if !LOG_LEVELS.contains(&log_level) {
        return Err("无效的日志级别".into());
    }
    Ok(())
}

pub async fn save(state: &AppState, host: String, port: u16, log_level: String) -> Result<()> {
    validate(&host, port, &log_level)?;
    let stored = (host.clone(), log_level.clone());
    let previous = state
        .db
        .run(move |conn| {
            let previous = settings::get(conn)?;
            settings::save(conn, &stored.0, port, &stored.1)?;
            Ok(previous)
        })
        .await?;
    if let Some(filter) = &state.log_filter {
        filter
            .reload(EnvFilter::new(&log_level))
            .map_err(|error| AppError::internal(error.to_string()))?;
    }
    if previous.host != host || previous.port != port {
        state.restart_proxy();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_listen_settings() {
        assert!(validate("127.0.0.1", 11435, "info").is_ok());
        assert!(validate("", 11435, "info").is_err());
        assert!(validate("localhost", 11435, "info").is_err());
        assert!(validate("127.0.0.1", 0, "info").is_err());
        assert!(validate("127.0.0.1", 1, "verbose").is_err());
    }
}
