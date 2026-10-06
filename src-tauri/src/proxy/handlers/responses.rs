use crate::{app::AppState, proxy::pipeline::handle};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde_json::Value;

const PROTOCOL: &str = "responses";

pub async fn responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, None, headers).await
}

pub async fn client_responses(
    State(state): State<AppState>,
    Path(client): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, Some(client), headers).await
}

pub async fn codex_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, Some("codex".into()), headers).await
}

pub async fn pi_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, Some("pi".into()), headers).await
}

pub async fn opencode_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, Some("opencode".into()), headers).await
}

pub async fn deepseek_harness_responses(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(
        state,
        body,
        PROTOCOL,
        Some("deepseek-harness".into()),
        headers,
    )
    .await
}
