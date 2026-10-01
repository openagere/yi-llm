use crate::{
    app::AppState,
    db::repo::settings,
    domain::settings::ServerSettings,
    error::Result,
    proxy::{lifecycle::ProxyStatus, monitor::Snapshot},
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ProxyRuntime {
    #[serde(flatten)]
    pub status: ProxyStatus,
    pub settings: ServerSettings,
}

pub fn is_running(state: &AppState) -> bool {
    state.is_proxy_running()
}

pub async fn runtime(state: &AppState) -> Result<ProxyRuntime> {
    let settings = state.db.run(|conn| settings::get(conn)).await?;
    Ok(ProxyRuntime {
        status: state.proxy_status(),
        settings,
    })
}

pub fn control(state: &AppState, action: &str) -> Result<()> {
    state.control_proxy(action)
}

pub fn monitor(state: &AppState, minutes: Option<u32>) -> Snapshot {
    state.monitor.snapshot(minutes.unwrap_or(15))
}
