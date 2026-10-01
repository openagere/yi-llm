//! Bounded, redacted diagnostics for upstream rejections. Never log prompts, tool
//! arguments, credentials or the full request/response body.
use reqwest::header::HeaderMap;
use serde_json::{json, Map, Value};

pub const RESPONSE_PREVIEW_LIMIT: usize = 16 * 1024;

fn size(value: &Value) -> usize {
    value.to_string().len()
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn shape(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            json!({"type":"object", "keys":map.keys().collect::<Vec<_>>(), "bytes":size(value)})
        }
        Value::Array(items) => json!({"type":"array", "count":items.len(), "bytes":size(value)}),
        _ => json!({"type":kind(value), "bytes":size(value)}),
    }
}

fn content_shape(value: &Value) -> Value {
    match value {
        Value::Array(parts) => Value::Array(
            parts
                .iter()
                .take(32)
                .map(|part| {
                    json!({
                        "type": part.get("type").and_then(Value::as_str),
                        "keys": part.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                        "bytes": size(part),
                    })
                })
                .collect(),
        ),
        _ => shape(value),
    }
}

fn input_shape(value: &Value) -> Value {
    match value {
        Value::Array(items) => json!({
            "count": items.len(),
            "items": items.iter().take(32).map(|item| json!({
                "type": item.get("type").and_then(Value::as_str),
                "role": item.get("role").and_then(Value::as_str),
                "keys": item.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                "content": item.get("content").map(content_shape),
                "bytes": size(item),
            })).collect::<Vec<_>>(),
        }),
        _ => shape(value),
    }
}

fn tools_shape(value: &Value) -> Value {
    match value {
        Value::Array(tools) => json!({
            "count": tools.len(),
            "items": tools.iter().take(64).map(|tool| json!({
                "type": tool.get("type").and_then(Value::as_str),
                "name": tool.get("name").and_then(Value::as_str).map(|name| name.chars().take(80).collect::<String>()),
                "keys": tool.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                "schema_keys": tool.get("parameters").and_then(Value::as_object).map(|map| map.keys().collect::<Vec<_>>()),
                "property_count": tool.pointer("/parameters/properties").and_then(Value::as_object).map(Map::len),
                "bytes": size(tool),
            })).collect::<Vec<_>>(),
        }),
        _ => shape(value),
    }
}

/// The exact post-routing request structure, excluding user content and identifiers.
pub fn request_summary(body: &Value) -> Value {
    let mut summary = Map::new();
    let Some(fields) = body.as_object() else {
        return shape(body);
    };
    for (name, value) in fields {
        let safe = match name.as_str() {
            "input" | "messages" => input_shape(value),
            "tools" => tools_shape(value),
            "instructions" | "system" | "prompt_cache_key" | "metadata" | "client_metadata" => {
                shape(value)
            }
            "model" => json!(value),
            "stream"
            | "store"
            | "parallel_tool_calls"
            | "truncation"
            | "max_output_tokens"
            | "max_tokens"
            | "max_completion_tokens"
            | "temperature"
            | "top_p"
            | "service_tier" => value.clone(),
            "reasoning" => json!({
                "keys": value.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                "effort": value.get("effort"),
                "summary": value.get("summary"),
            }),
            "text" => json!({
                "keys": value.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                "verbosity": value.get("verbosity"),
                "format_type": value.pointer("/format/type"),
            }),
            "tool_choice" => json!({"type":kind(value), "value":
                value.as_str().map(|s| json!(s)).or_else(|| value.get("type").cloned())}),
            "include" => match value.as_array() {
                Some(items) => json!(items
                    .iter()
                    .take(32)
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()),
                None => shape(value),
            },
            _ => shape(value),
        };
        summary.insert(name.clone(), safe);
    }
    Value::Object(summary)
}

pub fn response_headers(headers: &HeaderMap) -> Value {
    let mut summary = Map::new();
    for name in [
        "content-type",
        "x-request-id",
        "x-trace-id",
        "retry-after",
        "x-ratelimit-remaining-requests",
    ] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            summary.insert(
                name.to_owned(),
                json!(value.chars().take(256).collect::<String>()),
            );
        }
    }
    Value::Object(summary)
}

/// Only error fields are logged. OpenAI-compatible gateways sometimes return an
/// SSE-shaped 400 body (`data: {"error":...}`) even when the HTTP status is an error.
pub fn response_summary(bytes: &[u8], truncated: bool) -> Value {
    let parsed = serde_json::from_slice::<Value>(bytes).or_else(|_| {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| serde_json::Error::io(std::io::Error::other("invalid UTF-8")))?;
        let frame = text
            .lines()
            .filter_map(|line| line.trim_start().strip_prefix("data:"))
            .map(str::trim)
            .find(|data| data.starts_with('{'))
            .ok_or_else(|| serde_json::Error::io(std::io::Error::other("no JSON SSE frame")))?;
        serde_json::from_str::<Value>(frame)
    });
    match parsed {
        Ok(value) => {
            let error = value.get("error").unwrap_or(&value);
            json!({
                "type": error.get("type"),
                "code": error.get("code"),
                "param": error.get("param").map(|v| if v.is_string() {v.clone()} else {shape(v)}),
                "message": error.get("message").and_then(Value::as_str).map(|s| s.chars().take(2048).collect::<String>()),
                "error_keys": error.as_object().map(|map| map.keys().collect::<Vec<_>>()),
                "truncated": truncated,
            })
        }
        Err(_) => {
            json!({"format":"non-json-or-truncated", "captured_bytes":bytes.len(), "truncated":truncated})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_summary_keeps_parameter_values_but_redacts_content_and_secrets() {
        let body = json!({
            "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"private prompt"}]}],
            "instructions":"secret instructions", "prompt_cache_key":"secret session",
            "client_metadata":{"id":"secret id"}, "tools":[{"type":"function","name":"shell","description":"secret tool","parameters":{"type":"object","properties":{"command":{"type":"string"}}}}],
            "reasoning":{"effort":"max","summary":"auto"}, "stream":true,
        });
        let summary = request_summary(&body).to_string();
        for secret in [
            "private prompt",
            "secret instructions",
            "secret session",
            "secret id",
            "secret tool",
        ] {
            assert!(!summary.contains(secret));
        }
        assert!(summary.contains("max"));
        assert!(summary.contains("input_text"));
        assert!(summary.contains("shell"));
    }

    #[test]
    fn response_summary_reads_sse_error_frame() {
        let body = b"data: {\"error\":{\"message\":\"model service failed\",\"code\":400}}\n\ndata: [DONE]\n\n";
        let summary = response_summary(body, false);
        assert_eq!(summary["message"], "model service failed");
        assert_eq!(summary["code"], 400);
    }

    #[test]
    fn response_summary_reports_error_without_unrelated_fields() {
        let body = br#"{"error":{"message":"bad parameter","code":400,"type":"upstream_error"},"data":"secret"}"#;
        let summary = response_summary(body, false).to_string();
        assert!(summary.contains("bad parameter"));
        assert!(!summary.contains("secret"));
    }
}
