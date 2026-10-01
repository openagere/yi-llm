use crate::{
    db::UsageWriter,
    protocol::client::NamespaceToolAlias,
    protocol::translate::anthropic::ResponseTranslator,
    protocol::upstream::{ItemKind, UpstreamEvent},
};
use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[allow(clippy::too_many_arguments)]
pub async fn respond<S>(
    mut events: S,
    protocol: &str,
    model: &str,
    usage_model: &str,
    streaming: bool,
    usage: UsageWriter,
    provider_name: String,
    custom_tool_names: Vec<String>,
    tool_name_aliases: HashMap<String, NamespaceToolAlias>,
) -> Response
where
    S: Stream<Item = UpstreamEvent> + Send + Unpin + 'static,
{
    let response_id = format!("resp_{}", uuidless_id());
    let mut translator = ResponseTranslator::new_with_tool_mappings(
        response_id.clone(),
        model.to_owned(),
        custom_tool_names.into_iter().collect::<HashSet<_>>(),
        tool_name_aliases.clone(),
    );
    if streaming {
        let protocol = protocol.to_owned();
        let model = model.to_owned();
        let usage_model = usage_model.to_owned();
        let created = model_created();
        let body = async_stream::stream! {
            if protocol == "responses" {
                let (name, data) = translator.created();
                yield Ok::<_, std::io::Error>(format!("event: {name}\ndata: {data}\n\n"));
            } else if protocol == "openai_chat" {
                yield Ok(chat_chunk(&response_id, created, &model, json!({"role":"assistant"}), None));
            } else {
                yield Ok(anthropic_event("message_start", json!({
                    "type":"message_start",
                    "message":{"id":format!("msg_{}", uuidless_id()),"type":"message","role":"assistant","model":model,"content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":0,"output_tokens":0}}
                })));
            }
            let mut tool_indices = HashMap::new();
            let mut next_tool_index = 0usize;
            while let Some(event) = events.next().await {
                let terminal = matches!(event, UpstreamEvent::Done { .. } | UpstreamEvent::RawError { .. });
                let mut final_response = None;
                let translated = translator.feed(event.clone());
                for (name, data) in translated {
                    if name == "response.completed" || name == "response.incomplete" || name == "response.failed" {
                        final_response = data.get("response").cloned();
                    }
                    if protocol == "responses" {
                        yield Ok(format!("event: {name}\ndata: {data}\n\n"));
                    }
                }
                if protocol == "openai_chat" {
                    match &event {
                        UpstreamEvent::TextDelta { delta, .. } => yield Ok(chat_chunk(&response_id, created, &model, json!({"content":delta}), None)),
                        UpstreamEvent::ThinkingDelta { delta, .. } => yield Ok(chat_chunk(&response_id, created, &model, json!({"reasoning_content":delta}), None)),
                        UpstreamEvent::ToolCallStart { index, call_id, name } => {
                            let chat_index = *tool_indices.entry(*index).or_insert_with(|| { let value = next_tool_index; next_tool_index += 1; value });
                            let name = client_tool_name(name, &tool_name_aliases);
                            yield Ok(chat_chunk(&response_id, created, &model, json!({"tool_calls":[{"index":chat_index,"id":call_id,"type":"function","function":{"name":name,"arguments":""}}]}), None));
                        }
                        UpstreamEvent::ToolCallArgsDelta { index, delta } => if let Some(chat_index) = tool_indices.get(index) {
                            yield Ok(chat_chunk(&response_id, created, &model, json!({"tool_calls":[{"index":chat_index,"function":{"arguments":delta}}]}), None));
                        },
                        UpstreamEvent::RawError { message } => yield Ok(format!("data: {}\n\n", json!({"error":{"message":message,"type":"upstream_error"}}))),
                        _ => {}
                    }
                } else if protocol == "anthropic" {
                    match &event {
                        UpstreamEvent::ItemStart { index, kind: ItemKind::Message } => yield Ok(anthropic_event("content_block_start", json!({"type":"content_block_start","index":index,"content_block":{"type":"text","text":""}}))),
                        UpstreamEvent::ItemStart { index, kind: ItemKind::Reasoning } => yield Ok(anthropic_event("content_block_start", json!({"type":"content_block_start","index":index,"content_block":{"type":"thinking","thinking":""}}))),
                        UpstreamEvent::ToolCallStart { index, call_id, name } => {
                            let name = client_tool_name(name, &tool_name_aliases);
                            yield Ok(anthropic_event("content_block_start", json!({"type":"content_block_start","index":index,"content_block":{"type":"tool_use","id":call_id,"name":name,"input":{}}})));
                        },
                        UpstreamEvent::TextDelta { index, delta } => yield Ok(anthropic_event("content_block_delta", json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":delta}}))),
                        UpstreamEvent::ThinkingDelta { index, delta } => yield Ok(anthropic_event("content_block_delta", json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":delta}}))),
                        UpstreamEvent::ToolCallArgsDelta { index, delta } => yield Ok(anthropic_event("content_block_delta", json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":delta}}))),
                        UpstreamEvent::ReasoningData { index, data } => yield Ok(anthropic_event("content_block_delta", json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":data}}))),
                        UpstreamEvent::ItemStop { index } => yield Ok(anthropic_event("content_block_stop", json!({"type":"content_block_stop","index":index}))),
                        UpstreamEvent::RawError { message } => yield Ok(anthropic_event("error", json!({"type":"error","error":{"type":"api_error","message":message}}))),
                        _ => {}
                    }
                }
                if terminal {
                    if let Some(response) = final_response.as_ref() {
                        record_response_usage(&usage, &provider_name, protocol_for_storage(&protocol), &usage_model, response);
                    }
                    let reason = match &event { UpstreamEvent::Done { reason } => reason.as_str(), _ => "error" };
                    if protocol == "openai_chat" {
                        let finish = if reason == "length" { "length" } else if reason == "tool_calls" { "tool_calls" } else { "stop" };
                        yield Ok(chat_chunk(&response_id, created, &model, json!({}), Some(finish)));
                        if let Some(usage) = final_response.as_ref().and_then(|v| v.get("usage")) {
                            yield Ok(format!("data: {}\n\n", json!({"id":response_id,"object":"chat.completion.chunk","created":created,"model":model,"choices":[],"usage":chat_usage(usage)})));
                        }
                        yield Ok("data: [DONE]\n\n".to_owned());
                    } else if protocol == "anthropic" && reason != "error" {
                        let usage = final_response.as_ref().and_then(|v| v.get("usage")).cloned().unwrap_or(Value::Null);
                        yield Ok(anthropic_event("message_delta", json!({"type":"message_delta","delta":{"stop_reason":anthropic_stop_reason(reason),"stop_sequence":null},"usage":{"input_tokens":usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),"output_tokens":usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0)}})));
                        yield Ok(anthropic_event("message_stop", json!({"type":"message_stop"})));
                    }
                    break;
                }
            }
        };
        let mut response = axum::body::Body::from_stream(body).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        return response;
    }

    let mut final_response = None;
    while let Some(event) = events.next().await {
        let terminal = matches!(
            event,
            UpstreamEvent::Done { .. } | UpstreamEvent::RawError { .. }
        );
        for (name, data) in translator.feed(event) {
            if matches!(
                name.as_str(),
                "response.completed" | "response.incomplete" | "response.failed"
            ) {
                final_response = data.get("response").cloned();
            }
        }
        if terminal {
            break;
        }
    }
    let Some(value) = final_response else {
        return protocol_error(protocol, StatusCode::BAD_GATEWAY, "上游没有返回完整响应");
    };
    if value.get("status").and_then(Value::as_str) == Some("failed") {
        return protocol_error(
            protocol,
            StatusCode::BAD_GATEWAY,
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .unwrap_or("上游请求失败"),
        );
    }
    record_response_usage(
        &usage,
        &provider_name,
        protocol_for_storage(protocol),
        usage_model,
        &value,
    );
    match protocol {
        "responses" => Json(value).into_response(),
        "openai_chat" => Json(to_chat_response(&value)).into_response(),
        "anthropic" => Json(to_anthropic_response(&value)).into_response(),
        _ => protocol_error(protocol, StatusCode::BAD_REQUEST, "未知客户端协议"),
    }
}

fn to_chat_response(response: &Value) -> Value {
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut text = String::new();
    let mut reasoning = String::new();
    let mut tools = Vec::new();
    for item in output {
        match item.get("type").and_then(Value::as_str).unwrap_or("") {
            "message" => for part in item.get("content").and_then(Value::as_array).into_iter().flatten() {
                if let Some(value) = part.get("text").and_then(Value::as_str) { text.push_str(value); }
            },
            "reasoning" => for part in item.get("summary").and_then(Value::as_array).into_iter().flatten() {
                if let Some(value) = part.get("text").and_then(Value::as_str) { reasoning.push_str(value); }
            },
            "function_call" => tools.push(json!({"id":item.get("call_id"),"type":"function","function":{"name":response_tool_name(&item),"arguments":item.get("arguments")}})),
            "custom_tool_call" => {
                let input = item.get("input").and_then(Value::as_str).unwrap_or_default();
                tools.push(json!({
                    "id":item.get("call_id"),
                    "type":"function",
                    "function":{"name":response_tool_name(&item),"arguments":json!({"input":input}).to_string()}
                }));
            }
            _ => {}
        }
    }
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    json!({
        "id":response.get("id"), "object":"chat.completion", "created":response.get("created_at"), "model":response.get("model"),
        "choices":[{"index":0,"message":{"role":"assistant","content":if text.is_empty() {Value::Null} else {json!(text)},"reasoning_content":if reasoning.is_empty() {Value::Null} else {json!(reasoning)},"tool_calls":if tools.is_empty() {Value::Null} else {json!(tools)}},"finish_reason":chat_finish_reason(response)}],
        "usage":chat_usage(&usage)
    })
}

fn to_anthropic_response(response: &Value) -> Value {
    let mut content = Vec::new();
    let output = response
        .get("output")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in &output {
        match item.get("type").and_then(Value::as_str).unwrap_or("") {
            "message" => {
                for part in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        content.push(json!({"type":"text","text":text}));
                    }
                }
            }
            "reasoning" => {
                let signature = item.get("encrypted_content").and_then(Value::as_str);
                let mut emitted = false;
                for part in item
                    .get("summary")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(text) = part
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                    {
                        content
                            .push(json!({"type":"thinking","thinking":text,"signature":signature}));
                        emitted = true;
                    }
                }
                if !emitted {
                    if let Some(signature) = signature {
                        content.push(json!({"type":"redacted_thinking","data":signature}));
                    }
                }
            }
            "function_call" => {
                let input = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .and_then(|value| serde_json::from_str::<Value>(value).ok())
                    .unwrap_or_else(|| json!({}));
                content.push(json!({"type":"tool_use","id":item.get("call_id"),"name":response_tool_name(item),"input":input}));
            }
            "custom_tool_call" => {
                let input = item
                    .get("input")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                content.push(json!({
                    "type":"tool_use",
                    "id":item.get("call_id"),
                    "name":response_tool_name(item),
                    "input":{"input":input}
                }));
            }
            _ => {}
        }
    }
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    json!({
        "id":response.get("id").and_then(Value::as_str).map(|id| id.replace("resp_", "msg_")),
        "type":"message","role":"assistant","model":response.get("model"),"content":content,
        "stop_reason":anthropic_stop_reason(if output.iter().any(|item| matches!(item.get("type").and_then(Value::as_str), Some("function_call" | "custom_tool_call"))) {"tool_calls"} else if response.get("status").and_then(Value::as_str) == Some("incomplete") {"length"} else {"stop"}),
        "stop_sequence":Value::Null,
        "usage":{"input_tokens":usage.get("input_tokens"),"output_tokens":usage.get("output_tokens"),"cache_read_input_tokens":usage.pointer("/input_tokens_details/cached_tokens")}
    })
}

fn chat_usage(usage: &Value) -> Value {
    json!({
        "prompt_tokens":usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
        "completion_tokens":usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        "total_tokens":usage.get("total_tokens").and_then(Value::as_u64).unwrap_or_else(|| usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0) + usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0)),
        "prompt_tokens_details":{"cached_tokens":usage.pointer("/input_tokens_details/cached_tokens").and_then(Value::as_u64).unwrap_or(0)},
        "completion_tokens_details":{"reasoning_tokens":usage.pointer("/output_tokens_details/reasoning_tokens").and_then(Value::as_u64).unwrap_or(0)}
    })
}

fn chat_finish_reason(response: &Value) -> &'static str {
    if response.get("status").and_then(Value::as_str) == Some("incomplete") {
        return "length";
    }
    if response
        .pointer("/output")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|item| {
                matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("function_call" | "custom_tool_call")
                )
            })
        })
    {
        "tool_calls"
    } else {
        "stop"
    }
}

fn anthropic_stop_reason(reason: &str) -> &'static str {
    match reason {
        "length" => "max_tokens",
        "tool_calls" => "tool_use",
        "error" => "end_turn",
        _ => "end_turn",
    }
}

fn client_tool_name(name: &str, aliases: &HashMap<String, NamespaceToolAlias>) -> String {
    aliases
        .get(name)
        .map(|alias| format!("{}.{}", alias.namespace, alias.name))
        .unwrap_or_else(|| name.to_owned())
}

fn response_tool_name(item: &Value) -> String {
    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
    item.get("namespace")
        .and_then(Value::as_str)
        .map(|namespace| format!("{namespace}.{name}"))
        .unwrap_or_else(|| name.to_owned())
}

fn protocol_for_storage(protocol: &str) -> &str {
    match protocol {
        "anthropic" | "openai_chat" | "responses" => protocol,
        _ => "responses",
    }
}

pub fn record_response_usage(
    writer: &UsageWriter,
    provider_name: &str,
    protocol: &str,
    model: &str,
    response: &Value,
) {
    let Some(usage) = response.get("usage") else {
        return;
    };
    let count = |pointer: &str| usage.pointer(pointer).and_then(Value::as_u64).unwrap_or(0) as u32;
    writer.record_counts(
        provider_name,
        protocol,
        model,
        count("/input_tokens"),
        count("/output_tokens"),
        count("/input_tokens_details/cached_tokens"),
        count("/output_tokens_details/reasoning_tokens"),
    );
}

#[allow(clippy::needless_pass_by_value)]
fn chat_chunk(
    id: &str,
    created: u64,
    model: &str,
    delta: Value,
    finish_reason: Option<&str>,
) -> String {
    format!(
        "data: {}\n\n",
        json!({"id":id,"object":"chat.completion.chunk","created":created,"model":model,"choices":[{"index":0,"delta":delta,"finish_reason":finish_reason}]})
    )
}

#[allow(clippy::needless_pass_by_value)]
fn anthropic_event(name: &str, data: Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

fn model_created() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn uuidless_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "{}{}",
        model_created(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn protocol_error(protocol: &str, status: StatusCode, message: &str) -> Response {
    if protocol == "anthropic" {
        return (
            status,
            Json(json!({
                "type":"error",
                "error":{"type":"api_error","message":message}
            })),
        )
            .into_response();
    }
    let error_type = if status.is_client_error() {
        "invalid_request_error"
    } else {
        "server_error"
    };
    (
        status,
        Json(json!({"error":{"message":message,"type":error_type}})),
    )
        .into_response()
}
