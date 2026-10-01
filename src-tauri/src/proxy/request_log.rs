use crate::{app::AppState, proxy::monitor::Monitor};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures::StreamExt;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Completion record for one proxied request. Reports to the monitor and the log when
/// dropped, which happens once the response body has finished (or been abandoned).
struct RequestLog {
    monitor: Monitor,
    metric_key: String,
    started: Instant,
    method: String,
    path: String,
    request_content_length: Option<u64>,
    received_body_bytes: Arc<AtomicU64>,
    status: u16,
    completed: bool,
    body_error: bool,
}

impl Drop for RequestLog {
    fn drop(&mut self) {
        let elapsed_ms = self.started.elapsed().as_millis() as u64;
        let received_body_bytes = self.received_body_bytes.load(Ordering::Relaxed);
        self.monitor.finish(
            &self.metric_key,
            self.status >= 400 || !self.completed || self.body_error,
            elapsed_ms,
        );
        let span = tracing::info_span!(
            "proxy_request",
            method = %self.method,
            path = %self.path,
            status = self.status,
            elapsed_ms,
            request_content_length = self.request_content_length,
            received_body_bytes,
        );
        let _entered = span.enter();
        if self.status >= 500 || self.body_error {
            tracing::error!(completed = self.completed, "proxy request failed");
        } else if self.status >= 400 || !self.completed {
            tracing::warn!(
                completed = self.completed,
                "proxy request interrupted or rejected"
            );
        } else {
            tracing::info!("proxy request completed");
        }
    }
}

pub async fn log_request(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if request.uri().path() == "/health" {
        return next.run(request).await;
    }
    let mut shutdown = state.proxy.subscribe();
    let mut cancel = Box::pin(async move {
        let _ = shutdown.changed().await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let metric_key = state.monitor.begin(request.uri().path());
    let mut log = RequestLog {
        monitor: state.monitor.clone(),
        metric_key: metric_key.clone(),
        started: Instant::now(),
        method: request.method().to_string(),
        path: request.uri().path().to_owned(),
        request_content_length: request
            .headers()
            .get(axum::http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok()),
        received_body_bytes: Arc::new(AtomicU64::new(0)),
        status: 0,
        completed: false,
        body_error: false,
    };
    let (request_parts, request_body) = request.into_parts();
    let monitor = state.monitor.clone();
    let received_body_bytes = log.received_body_bytes.clone();
    let mut incoming = request_body.into_data_stream();
    let request_body = Body::from_stream(async_stream::stream! {
        while let Some(chunk) = incoming.next().await {
            if let Ok(bytes) = &chunk {
                received_body_bytes.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                monitor.payload(&metric_key, bytes.len(), 0);
            }
            yield chunk;
        }
    });
    let request = axum::extract::Request::from_parts(request_parts, request_body);
    let response = tokio::select! {
        response = next.run(request) => response,
        _ = &mut cancel => {
            log.status = 503;
            return (StatusCode::SERVICE_UNAVAILABLE, "proxy request interrupted by shutdown").into_response();
        }
    };
    log.status = response.status().as_u16();
    if log.method == "HEAD" || matches!(log.status, 204 | 304) {
        log.completed = true;
        return response;
    }
    let (parts, body) = response.into_parts();
    let mut stream = body.into_data_stream();
    let body = Body::from_stream(async_stream::stream! {
        loop {
            let chunk = tokio::select! {
                chunk = stream.next() => chunk,
                _ = &mut cancel => {
                    log.body_error = true;
                    yield Err(axum::Error::new(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "proxy stopped")));
                    break;
                }
            };
            let Some(chunk) = chunk else { break; };
            if chunk.is_err() { log.body_error = true; }
            if let Ok(bytes) = &chunk { log.monitor.payload(&log.metric_key, 0, bytes.len()); }
            yield chunk;
        }
        log.completed = !log.body_error;
        drop(log);
    });
    Response::from_parts(parts, body)
}
