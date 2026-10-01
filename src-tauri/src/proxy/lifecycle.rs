use crate::{
    app::AppState,
    db::repo::settings,
    error::{AppError, Result},
    proxy::{monitor::Monitor, router::router},
};
use std::{
    future::IntoFuture,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::watch;

#[derive(Clone, Debug, serde::Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyPhase {
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ProxyStatus {
    pub phase: ProxyPhase,
    pub desired_running: bool,
    pub started_at: Option<u64>,
    pub address: Option<String>,
    pub last_error: Option<String>,
    pub active_requests: u64,
    pub active_connections: u64,
}

#[derive(Clone, Copy)]
pub struct ProxyControl {
    pub enabled: bool,
    pub generation: u64,
}

/// Desired-state channel plus observable status for the managed proxy listener.
///
/// Every start/stop/restart bumps `generation`; status updates carrying an older
/// generation are ignored so a slow, superseded listener cannot overwrite newer state.
#[derive(Clone)]
pub struct ProxyController {
    control: Arc<watch::Sender<ProxyControl>>,
    status: Arc<Mutex<ProxyStatus>>,
    monitor: Monitor,
}

impl ProxyController {
    pub fn new(monitor: Monitor) -> Self {
        Self {
            control: Arc::new(
                watch::channel(ProxyControl {
                    enabled: true,
                    generation: 0,
                })
                .0,
            ),
            status: Arc::new(Mutex::new(ProxyStatus {
                phase: ProxyPhase::Starting,
                desired_running: true,
                started_at: None,
                address: None,
                last_error: None,
                active_requests: 0,
                active_connections: 0,
            })),
            monitor,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<ProxyControl> {
        self.control.subscribe()
    }

    fn lock_status(&self) -> std::sync::MutexGuard<'_, ProxyStatus> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn restart(&self) {
        let mut status = self.lock_status();
        if !status.desired_running {
            return;
        }
        status.phase = ProxyPhase::Starting;
        status.last_error = None;
        self.control.send_modify(|control| control.generation += 1);
    }

    pub fn control(&self, action: &str) -> Result<()> {
        let enabled = match action {
            "start" | "restart" => true,
            "stop" => false,
            _ => return Err(AppError::validation("无效的代理操作")),
        };
        let mut status = self.lock_status();
        if action == "start" && status.phase == ProxyPhase::Running {
            return Ok(());
        }
        status.phase = if enabled {
            ProxyPhase::Starting
        } else {
            ProxyPhase::Stopping
        };
        status.desired_running = enabled;
        status.last_error = None;
        self.control.send_modify(|control| {
            control.enabled = enabled;
            control.generation += 1;
        });
        Ok(())
    }

    pub fn status(&self) -> ProxyStatus {
        let mut status = self.lock_status().clone();
        let totals = self.monitor.totals();
        status.active_requests = totals.active_requests;
        status.active_connections = totals.connections;
        status
    }

    pub fn is_running(&self) -> bool {
        self.status().phase == ProxyPhase::Running
    }

    fn update(
        &self,
        generation: u64,
        phase: ProxyPhase,
        address: Option<String>,
        error: Option<String>,
    ) {
        let mut status = self.lock_status();
        if self.control.borrow().generation != generation {
            return;
        }
        status.phase = phase;
        status.address = address;
        status.last_error = error;
        status.started_at = (status.phase == ProxyPhase::Running).then(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        });
    }
}

/// Serves on a fixed address until the future is dropped.
pub async fn serve(addr: SocketAddr, state: AppState) -> Result<()> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Io(format!("监听 {addr} 失败: {e}")))?;
    let listener = crate::proxy::monitor::TrackedListener {
        listener,
        monitor: state.monitor.clone(),
    };
    axum::serve(listener, router(state))
        .await
        .map_err(|e| AppError::Io(e.to_string()))
}

/// Keeps the proxy listener aligned with [`ProxyController`]'s desired state and the
/// saved host/port, retrying failed binds every couple of seconds.
pub async fn serve_managed(state: AppState) {
    let proxy = state.proxy.clone();
    let mut control = proxy.subscribe();
    loop {
        let current = *control.borrow_and_update();
        if !current.enabled {
            proxy.update(current.generation, ProxyPhase::Stopped, None, None);
            if control.changed().await.is_err() {
                return;
            }
            continue;
        }
        let settings = match state.db.run(|conn| settings::get(conn)).await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::error!(%error, "cannot read proxy settings");
                proxy.update(
                    current.generation,
                    ProxyPhase::Error,
                    None,
                    Some(error.to_string()),
                );
                wait_to_retry(&mut control).await;
                continue;
            }
        };
        let ip = match settings.host.parse::<std::net::IpAddr>() {
            Ok(ip) => ip,
            Err(error) => {
                tracing::error!(%error, "invalid proxy listen address");
                proxy.update(
                    current.generation,
                    ProxyPhase::Error,
                    None,
                    Some(format!("监听地址无效: {error}")),
                );
                wait_to_retry(&mut control).await;
                continue;
            }
        };
        let address = SocketAddr::new(ip, settings.port);
        let listener = match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(%error, %address, "proxy listener failed");
                proxy.update(
                    current.generation,
                    ProxyPhase::Error,
                    None,
                    Some(format!("监听 {address} 失败: {error}")),
                );
                wait_to_retry(&mut control).await;
                continue;
            }
        };
        if control.borrow().generation != current.generation {
            continue;
        }
        tracing::info!(%address, "proxy listener started");
        proxy.update(
            current.generation,
            ProxyPhase::Running,
            Some(address.to_string()),
            None,
        );
        let mut shutdown = control.clone();
        let listener = crate::proxy::monitor::TrackedListener {
            listener,
            monitor: state.monitor.clone(),
        };
        let mut serving = Box::pin(
            axum::serve(listener, router(state.clone()))
                .with_graceful_shutdown(async move {
                    let _ = shutdown.changed().await;
                })
                .into_future(),
        );
        let result = tokio::select! {
            result = &mut serving => result,
            _ = control.changed() => {
                // Drain current requests briefly before releasing the old listener.
                match tokio::time::timeout(Duration::from_secs(6), &mut serving).await {
                    Ok(result) => result,
                    Err(_) => {
                        tracing::warn!(%address, "proxy shutdown timed out");
                        Ok(())
                    }
                }
            }
        };
        if let Err(error) = result {
            tracing::error!(%error, "proxy listener stopped unexpectedly");
            proxy.update(
                current.generation,
                ProxyPhase::Error,
                None,
                Some(error.to_string()),
            );
            wait_to_retry(&mut control).await;
        } else {
            tracing::info!(%address, "proxy listener stopped");
        }
    }
}

async fn wait_to_retry(control: &mut watch::Receiver<ProxyControl>) {
    tokio::select! {
        _ = control.changed() => {},
        _ = tokio::time::sleep(Duration::from_secs(2)) => {},
    }
}
