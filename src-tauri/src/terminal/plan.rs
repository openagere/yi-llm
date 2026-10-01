use crate::{
    db::repo::settings,
    domain::terminal::{protocol, Profile},
    error::{AppError, Result},
    proxy::routing::RouteTable,
    terminal::{
        changes::{read_optional, FileChange},
        clients::{configurator, PlanInput},
        profile,
        resolve::validate_in,
    },
};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Status {
    pub client: String,
    pub config_path: String,
    pub exists: bool,
    pub active: bool,
    pub config_error: Option<String>,
    pub profile: Option<Profile>,
}

pub fn config_path(client: &str) -> Result<PathBuf> {
    protocol(client)?;
    configurator(client)?.config_path()
}

/// Base URL a client should use to reach the proxy's client-scoped entrypoint.
pub fn endpoint(conn: &Connection, client: &str) -> Result<String> {
    protocol(client)?;
    let settings = settings::get(conn)?;
    let ip: std::net::IpAddr = settings
        .host
        .parse()
        .map_err(|error| AppError::validation(format!("无效监听地址: {error}")))?;
    let ip = if ip.is_unspecified() {
        if ip.is_ipv4() {
            std::net::IpAddr::from([127, 0, 0, 1])
        } else {
            std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST)
        }
    } else {
        ip
    };
    let host = if ip.is_ipv6() {
        format!("[{ip}]")
    } else {
        ip.to_string()
    };
    Ok(format!(
        "http://{host}:{}/clients/{client}{}",
        settings.port,
        if client == "claude-code" { "" } else { "/v1" }
    ))
}

pub fn status(conn: &Connection, client: &str) -> Result<Status> {
    let path = config_path(client)?;
    let configurator = configurator(client)?;
    let expected_endpoint = endpoint(conn, client)?;
    let exists = path.exists();
    let read_active = || -> Result<bool> {
        if !exists {
            return Ok(false);
        }
        let source = fs::read_to_string(&path)?;
        configurator.is_active(&path, &source, &expected_endpoint)
    };
    let (active, config_error) = match read_active() {
        Ok(active) => (active, None),
        Err(error) => (false, Some(error.to_string())),
    };
    Ok(Status {
        client: client.into(),
        config_path: path.display().to_string(),
        exists,
        active,
        config_error,
        profile: profile::load(conn, client)?,
    })
}

/// Validates `profile` and computes every file change needed to apply it at `path`.
pub fn plan(
    conn: &Connection,
    profile: &Profile,
    path: &Path,
) -> Result<(String, Vec<FileChange>)> {
    let models = validate_in(&RouteTable::load(conn)?, profile)?;
    let endpoint = endpoint(conn, &profile.client)?;
    let source = read_optional(path)?.unwrap_or_default();
    let built = configurator(&profile.client)?.build(
        &PlanInput {
            profile,
            models: &models,
            endpoint: &endpoint,
        },
        path,
        &source,
    )?;
    let mut files = built.extra;
    files.push(FileChange::new(path.to_owned(), built.main)?);
    Ok((endpoint, files))
}
