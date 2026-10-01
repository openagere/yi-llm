use crate::{app::AppState, proxy::pipeline::handle};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use serde_json::Value;

const PROTOCOL: &str = "openai_chat";

pub async fn chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, None, headers).await
}

pub async fn client_chat(
    State(state): State<AppState>,
    Path(client): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    handle(state, body, PROTOCOL, Some(client), headers).await
}
