#![allow(dead_code)]

use std::{
    future::IntoFuture,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    time::Duration,
};
use wiremock::MockServer;
use yi_llm::{
    app::AppState,
    db::{repo::providers, Conn, Db},
    domain::{
        capabilities::Capabilities,
        provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
    },
    proxy::{
        lifecycle::{ProxyPhase, ProxyStatus},
        monitor::TrackedListener,
        router,
    },
};

pub fn new_state(dir: &Path) -> AppState {
    AppState::new(
        Db::open(&dir.join("test.db")).unwrap(),
        reqwest::Client::new(),
        dir.join("logs"),
        None,
    )
}

pub async fn wait_for_phase(state: &AppState, phase: ProxyPhase) -> ProxyStatus {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let status = state.proxy_status();
            if status.phase == phase {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("proxy did not reach expected phase")
}

/// Serves `state` on an ephemeral port and returns its root URL.
pub async fn serve(state: &AppState) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let tracked = TrackedListener {
        listener,
        monitor: state.monitor.clone(),
    };
    let app = router(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(tracked, app).into_future().await.unwrap();
    });
    (root, task)
}

pub fn sql(state: &AppState, statement: &str) {
    state.db.conn().unwrap().execute(statement, []).unwrap();
    state.invalidate_routes();
}

pub async fn reserve_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

pub fn provider_view(
    id: &str,
    provider_type: &str,
    base_url: &str,
    api_key: &str,
    exposed_name: &str,
    upstream_model: &str,
    is_default: bool,
) -> ProviderView {
    ProviderView {
        provider: Provider {
            id: id.into(),
            provider_type: provider_type.into(),
            name: id.into(),
            short_code: id.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            enabled: true,
            thinking: "minimal".into(),
            extra: serde_json::json!({}),
            is_default,
            protocol_support: {
                let mut support = ProtocolSupport::native(provider_type);
                support.responses = true;
                support.openai_chat = true;
                support
            },
        },
        models: vec![ModelMapping {
            id: 0,
            provider_id: id.into(),
            exposed_name: exposed_name.into(),
            upstream_model: upstream_model.into(),
            standard_model_id: None,
            capabilities: Capabilities::default(),
            non_standard: false,
        }],
    }
}

/// A running proxy over a temporary database. Raw SQL and repository writes made through
/// [`Harness::conn`] invalidate the route cache, as the services do in the app.
pub struct Harness {
    dir: tempfile::TempDir,
    pub state: AppState,
    task: tokio::task::JoinHandle<()>,
}

impl Harness {
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir.path().join("test.db")
    }

    pub fn conn(&self) -> TestConn {
        TestConn {
            conn: self.state.db.conn().unwrap(),
            state: self.state.clone(),
        }
    }

    pub fn invalidate(&self) {
        self.state.invalidate_routes();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub struct TestConn {
    conn: Conn,
    state: AppState,
}

impl TestConn {
    pub fn execute<P: rusqlite::Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        let result = self.conn.execute(sql, params);
        self.state.invalidate_routes();
        result
    }
}

impl Deref for TestConn {
    type Target = rusqlite::Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl DerefMut for TestConn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state.invalidate_routes();
        &mut self.conn
    }
}

impl Drop for TestConn {
    fn drop(&mut self) {
        self.state.invalidate_routes();
    }
}

pub async fn start_proxy(provider_type: &str, upstream: &MockServer) -> (String, Harness) {
    start_proxy_with(provider_type, &upstream.uri(), "test-key", "real-model").await
}

pub async fn start_proxy_with(
    provider_type: &str,
    base_url: &str,
    api_key: &str,
    upstream_model: &str,
) -> (String, Harness) {
    let dir = tempfile::tempdir().unwrap();
    let state = new_state(dir.path());
    providers::save(
        &mut state.db.conn().unwrap(),
        &provider_view(
            "primary",
            provider_type,
            base_url,
            api_key,
            "public-model",
            upstream_model,
            true,
        ),
    )
    .unwrap();
    let (root, task) = serve(&state).await;
    (root, Harness { dir, state, task })
}
