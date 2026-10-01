use crate::{
    app::AppState,
    domain::{capabilities::Capabilities, provider::Provider},
    protocol::{
        client as client_protocol, compatibility,
        prepare::prepare_request,
        respond::respond,
        responses::types::{ResponsesRequest, Tool},
        upstream::{upstream_client, UpstreamCall, UpstreamError},
    },
    proxy::{
        errors::ProxyError,
        handlers::passthrough,
        routing::{ResolvedRoute, RouteTable},
    },
    terminal,
};
use axum::{http::HeaderMap, response::Response};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Serves one model request end to end: route, prepare, then either forward it untouched
/// (same protocol upstream) or translate it through the shared Responses representation.
pub async fn handle(
    state: AppState,
    body: Value,
    protocol: &'static str,
    client: Option<String>,
    headers: HeaderMap,
) -> Response {
    match run(&state, body, protocol, client.as_deref(), &headers).await {
        Ok(response) => response,
        Err(error) => error.into_response_for(protocol),
    }
}

async fn run(
    state: &AppState,
    mut body: Value,
    protocol: &str,
    client: Option<&str>,
    headers: &HeaderMap,
) -> Result<Response, ProxyError> {
    let requested_model = body
        .get("model")
        .and_then(Value::as_str)
        .ok_or_else(|| ProxyError::bad_request("请求缺少 model 字段"))?
        .to_owned();
    tracing::info!(model = %requested_model, protocol, "model request received");
    let table = state.route_table().await?;
    let ResolvedRoute {
        mut provider,
        upstream_model,
        capabilities,
        non_standard,
        usage_model,
    } = select_route(&table, client, &requested_model, protocol)?;
    drop(table);
    if !provider.enabled {
        return Err(ProxyError::bad_request("目标 provider 已停用"));
    }
    prepare_request(&mut body, protocol, &mut provider, &capabilities)
        .map_err(ProxyError::bad_request)?;
    if non_standard {
        let report = compatibility::adapt(&mut body, protocol).map_err(ProxyError::bad_request)?;
        if report.changed() {
            tracing::info!(
                provider = provider.name,
                upstream_model,
                protocol,
                omitted_tool_types = ?report.omitted_tool_types,
                omitted_include = ?report.omitted_include,
                adjusted_fields = ?report.adjusted_fields,
                "non-standard model request adapted"
            );
        }
    }
    if provider.provider_type == protocol {
        return passthrough::forward(
            state,
            &provider,
            protocol,
            &usage_model,
            &upstream_model,
            body,
            headers,
        )
        .await;
    }
    translate(
        state,
        &body,
        protocol,
        &provider,
        &upstream_model,
        &capabilities,
        &usage_model,
    )
    .await
}

fn select_route(
    table: &RouteTable,
    client: Option<&str>,
    requested_model: &str,
    protocol: &str,
) -> Result<ResolvedRoute, ProxyError> {
    match client {
        Some(client) => terminal::resolve_in(table, client, requested_model, protocol)
            .map(terminal::ResolvedModel::into_route)
            .map_err(|error| ProxyError::bad_request(error.to_string())),
        None => table
            .resolve_public(requested_model, protocol)
            .ok_or_else(|| {
                ProxyError::bad_request(format!(
                    "没有启用 {protocol} 协议的 provider 来处理模型 {requested_model}；\
                 请在 yi-llm 中为该模型名添加映射，或设置一个支持 {protocol} 的默认 Provider"
                ))
            }),
    }
}

async fn translate(
    state: &AppState,
    body: &Value,
    protocol: &str,
    provider: &Provider,
    upstream_model: &str,
    capabilities: &Capabilities,
    usage_model: &str,
) -> Result<Response, ProxyError> {
    let upstream_type = provider.provider_type.as_str();
    client_protocol::validate_client_options(protocol, upstream_type, body)
        .map_err(ProxyError::bad_request)?;
    let normalized_body =
        client_protocol::normalize_body(protocol, body).map_err(ProxyError::bad_request)?;
    capabilities
        .validate_effort(
            normalized_body.pointer("/reasoning/effort"),
            "responses",
            upstream_type,
        )
        .map_err(ProxyError::bad_request)?;
    client_protocol::validate_for_upstream(upstream_type, &normalized_body)
        .map_err(ProxyError::bad_request)?;
    warn_omitted_tools(provider, protocol, &normalized_body);
    let tool_name_aliases = if protocol == "responses" {
        client_protocol::namespace_tool_aliases(body)
    } else {
        HashMap::new()
    };
    let request = ResponsesRequest::deserialize(&normalized_body)
        .map_err(|error| ProxyError::bad_request(format!("请求格式无效: {error}")))?;
    if upstream_type != "responses" {
        if request.store == Some(true) {
            return Err(ProxyError::bad_request(
                "store=true 不支持，请使用 store=false",
            ));
        }
        if request.previous_response_id.is_some() {
            return Err(ProxyError::bad_request(
                "previous_response_id 不支持：请发送完整 input 历史",
            ));
        }
    }
    let upstream = upstream_client(upstream_type)
        .ok_or_else(|| ProxyError::bad_request(format!("未知 provider 类型: {upstream_type}")))?;
    let events = upstream
        .stream(
            &state.http,
            UpstreamCall {
                provider,
                upstream_model,
                request: &request,
                normalized_body: &normalized_body,
            },
        )
        .await
        .map_err(|error| match error {
            UpstreamError::BadRequest(message) => ProxyError::bad_request(message),
            UpstreamError::Upstream(message) => ProxyError::upstream(message),
        })?;
    let custom_tool_names = request
        .tools
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|tool| match tool {
            Tool::Custom { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    Ok(respond(
        events,
        protocol,
        &request.model,
        usage_model,
        request.stream.unwrap_or(false),
        state.usage.clone(),
        provider.name.clone(),
        custom_tool_names,
        tool_name_aliases,
    )
    .await)
}

/// Hosted built-in tools (web_search, web_search_preview, …) cannot be served by
/// third-party anthropic/chat endpoints; the translators drop them. Surface the omission
/// instead of failing the whole request.
fn warn_omitted_tools(provider: &Provider, protocol: &str, normalized_body: &Value) {
    if !matches!(provider.provider_type.as_str(), "anthropic" | "openai_chat") {
        return;
    }
    let omitted: Vec<&str> = normalized_body
        .get("tools")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|tool| tool.get("type").and_then(Value::as_str))
        .filter(|kind| !matches!(*kind, "function" | "custom"))
        .collect();
    if !omitted.is_empty() {
        tracing::warn!(
            provider = provider.name,
            upstream_protocol = provider.provider_type,
            client_protocol = protocol,
            tools = ?omitted,
            "omitting server-side built-in tools that cannot be translated to this upstream"
        );
    }
}
