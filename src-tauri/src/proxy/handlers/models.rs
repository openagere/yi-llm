use crate::{
    app::AppState,
    proxy::errors::{protocol_error, ProxyError},
    terminal,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub async fn models(State(state): State<AppState>) -> Response {
    let table = match state.route_table().await {
        Ok(table) => table,
        Err(error) => return ProxyError::from(error).into_response_for("openai_chat"),
    };
    let mut entries: Vec<(&str, &str)> = table.enabled_models().collect();
    entries.sort_by_key(|(id, _)| *id);
    entries.dedup_by_key(|(id, _)| *id);
    let data: Vec<_> = entries
        .into_iter()
        .map(|(id, owner)| json!({"id":id,"object":"model","created":0,"owned_by":owner}))
        .collect();
    Json(json!({"object":"list","data":data})).into_response()
}

pub async fn client_models(State(state): State<AppState>, Path(client): Path<String>) -> Response {
    let table = match state.route_table().await {
        Ok(table) => table,
        Err(error) => return ProxyError::from(error).into_response_for("openai_chat"),
    };
    match terminal::model_list_in(&table, &client) {
        Ok(value) => Json(value).into_response(),
        Err(error) => protocol_error("openai_chat", StatusCode::BAD_REQUEST, &error.to_string()),
    }
}
