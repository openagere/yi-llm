mod common;

use common::*;
use yi_llm::{
    app::AppState,
    db::repo::{providers, settings},
    db::Db,
    domain::provider::{Provider, ProviderView},
    proxy::ProxyPhase,
};

#[tokio::test]
async fn managed_proxy_reports_port_conflicts_and_preserves_manual_stop() {
    let dir = tempfile::tempdir().unwrap();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    settings::save(&db.conn().unwrap(), "127.0.0.1", port, "info").unwrap();
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    let task = tokio::spawn(yi_llm::proxy::serve_managed(state.clone()));
    let failed = wait_for_phase(&state, ProxyPhase::Error).await;
    assert!(failed.last_error.unwrap().contains(&port.to_string()));
    assert!(!state.is_proxy_running());
    assert!(state.control_proxy("invalid").is_err());

    state.control_proxy("stop").unwrap();
    wait_for_phase(&state, ProxyPhase::Stopped).await;
    drop(occupied);
    state.restart_proxy();
    tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
    let stopped = state.proxy_status();
    assert_eq!(stopped.phase, ProxyPhase::Stopped);
    assert!(!stopped.desired_running);
    assert!(stopped.started_at.is_none());

    state.control_proxy("start").unwrap();
    let running = wait_for_phase(&state, ProxyPhase::Running).await;
    assert!(running.last_error.is_none());
    assert!(running.started_at.is_some());
    assert_eq!(
        running.address.as_deref(),
        Some(format!("127.0.0.1:{port}").as_str())
    );
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
        "ok"
    );
    state.control_proxy("restart").unwrap();
    wait_for_phase(&state, ProxyPhase::Running).await;
    state.control_proxy("stop").unwrap();
    state.control_proxy("start").unwrap();
    wait_for_phase(&state, ProxyPhase::Running).await;
    state.control_proxy("stop").unwrap();
    wait_for_phase(&state, ProxyPhase::Stopped).await;
    assert!(tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
        .await
        .is_ok());
    task.abort();
}

#[tokio::test]
async fn managed_proxy_counts_streams_and_bounds_shutdown_time() {
    use axum::{body::Body, routing::post, Router};
    let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_url = format!("http://{}", upstream.local_addr().unwrap());
    let upstream_task = tokio::spawn(async move {
        let app = Router::new().route("/responses", post(|| async {
            let body = Body::from_stream(async_stream::stream! {
                yield Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"data: {\"type\":\"response.created\"}\n\n"));
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                yield Ok(bytes::Bytes::from_static(b"data: [DONE]\n\n"));
            });
            ([("content-type", "text/event-stream")], body)
        }));
        axum::serve(upstream, app).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    let port_reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = port_reservation.local_addr().unwrap().port();
    drop(port_reservation);
    settings::save(&db.conn().unwrap(), "127.0.0.1", port, "info").unwrap();
    providers::save(
        &mut db.conn().unwrap(),
        &ProviderView {
            provider: Provider {
                id: "native".into(),
                provider_type: "responses".into(),
                name: "Native".into(),
                short_code: "native".into(),
                base_url: upstream_url,
                api_key: "secret".into(),
                enabled: true,
                thinking: "minimal".into(),
                extra: serde_json::json!({}),
                is_default: true,
                protocol_support: yi_llm::domain::provider::ProtocolSupport::native("responses"),
            },
            models: vec![],
        },
    )
    .unwrap();
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    let task = tokio::spawn(yi_llm::proxy::serve_managed(state.clone()));
    wait_for_phase(&state, ProxyPhase::Running).await;
    let response = reqwest::Client::new().post(format!("http://127.0.0.1:{port}/v1/responses"))
        .json(&serde_json::json!({"model": "test-model", "input": "hello", "stream": true, "store": false}))
        .send().await.unwrap();
    assert_eq!(state.proxy_status().active_requests, 1);
    assert_eq!(state.proxy_status().active_connections, 1);
    let metrics = state.monitor.snapshot(5);
    assert_eq!(metrics.totals.requests, 1);
    assert!(metrics.totals.received_bytes > 0);
    assert!(metrics.totals.sent_bytes > 0);
    state.control_proxy("stop").unwrap();
    let body = tokio::time::timeout(std::time::Duration::from_secs(8), response.text())
        .await
        .unwrap();
    assert!(body.is_err(), "shutdown should close an unfinished stream");
    wait_for_phase(&state, ProxyPhase::Stopped).await;
    assert_eq!(state.proxy_status().active_requests, 0);
    assert_eq!(state.monitor.snapshot(5).totals.errors, 1);
    task.abort();
    upstream_task.abort();
}

#[tokio::test]
async fn monitor_counts_real_idle_connections_and_health_bytes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    let reservation = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = reservation.local_addr().unwrap();
    drop(reservation);
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    let server_state = state.clone();
    let task =
        tokio::spawn(async move { yi_llm::proxy::serve(address, server_state).await.unwrap() });
    let mut stream = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if let Ok(stream) = tokio::net::TcpStream::connect(address).await {
                break stream;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let request = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    stream.write_all(request).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    drop(stream);
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if state.monitor.totals().connections == 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let snapshot = state.monitor.snapshot(5);
    assert_eq!(snapshot.totals.accepted_connections, 1);
    assert_eq!(snapshot.totals.peak_connections, 1);
    assert_eq!(snapshot.totals.received_bytes, request.len() as u64);
    assert_eq!(snapshot.totals.sent_bytes, response.len() as u64);
    assert_eq!(snapshot.totals.requests, 0);
    assert!(snapshot.routes.is_empty());
    task.abort();
}

#[tokio::test]
async fn monitor_finishes_head_requests_without_body_errors() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    let (root, task) = serve(&state).await;
    let response = reqwest::Client::new()
        .head(format!("{root}/v1/models"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.bytes().await.unwrap().len(), 0);
    let totals = state.monitor.snapshot(5).totals;
    assert_eq!(totals.requests, 1);
    assert_eq!(totals.completed, 1);
    assert_eq!(totals.active_requests, 0);
    assert_eq!(totals.errors, 0);
    task.abort();
}

#[tokio::test]
async fn listener_restarts_on_saved_address_change() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    let reserve_port = || async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };
    let first_port = reserve_port().await;
    let mut second_port = reserve_port().await;
    while second_port == first_port {
        second_port = reserve_port().await;
    }
    settings::save(&db.conn().unwrap(), "127.0.0.1", first_port, "info").unwrap();
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    tokio::spawn(yi_llm::proxy::serve_managed(state.clone()));
    let client = reqwest::Client::new();
    let wait_healthy = |port: u16| {
        let client = client.clone();
        async move {
            for _ in 0..80 {
                if let Ok(response) = client
                    .get(format!("http://127.0.0.1:{port}/health"))
                    .send()
                    .await
                {
                    if response.status().is_success() {
                        return true;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            false
        }
    };
    assert!(wait_healthy(first_port).await);
    assert!(state.is_proxy_running());
    {
        let conn = state.db.conn().unwrap();
        settings::save(&conn, "127.0.0.1", second_port, "debug").unwrap();
    }
    state.restart_proxy();
    assert!(wait_healthy(second_port).await);
    assert!(client
        .get(format!("http://127.0.0.1:{first_port}/health"))
        .send()
        .await
        .is_err());
}
