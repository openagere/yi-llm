//! Protocol-scoped compatibility rules for upstream mappings explicitly marked
//! non-standard. The normal path remains an exact native pass-through.
//!
//! Keep each rule small and evidence-based. In particular, do not silently discard
//! conversation history, a forced tool choice, or a request to persist a response.
use serde_json::Value;

/// Responses tools rejected by the observed partial Responses gateway. Ordinary
/// function tools are retained, so coding tools remain usable.
const UNSUPPORTED_RESPONSES_TOOLS: &[&str] = &["namespace", "web_search", "web_search_preview"];
const UNSUPPORTED_RESPONSES_INCLUDE: &[&str] = &["message.annotations"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub omitted_tool_types: Vec<String>,
    pub omitted_include: Vec<String>,
    pub adjusted_fields: Vec<&'static str>,
}

impl CompatibilityReport {
    pub fn changed(&self) -> bool {
        !self.omitted_tool_types.is_empty()
            || !self.omitted_include.is_empty()
            || !self.adjusted_fields.is_empty()
    }
}

/// Applied once to the routed model request, before either native forwarding or
/// protocol translation. Only the Responses rules are currently known.
pub fn adapt(body: &mut Value, client_protocol: &str) -> Result<CompatibilityReport, String> {
    let mut report = CompatibilityReport::default();
    if client_protocol != "responses" {
        return Ok(report);
    }

    if let Some(tools) = body.get_mut("tools").and_then(Value::as_array_mut) {
        tools.retain(|tool| {
            let kind = tool.get("type").and_then(Value::as_str).unwrap_or_default();
            if UNSUPPORTED_RESPONSES_TOOLS.contains(&kind) {
                report.omitted_tool_types.push(kind.to_owned());
                false
            } else {
                true
            }
        });
        if tools.is_empty() {
            body.as_object_mut()
                .expect("request is an object")
                .remove("tools");
        }
    }

    let remaining_tools = body.get("tools").and_then(Value::as_array);
    if let Some(choice) = body.get("tool_choice") {
        let kind = choice
            .as_str()
            .or_else(|| choice.get("type").and_then(Value::as_str));
        match kind {
            Some("required") if remaining_tools.is_none_or(Vec::is_empty) => {
                return Err("非标准模型不支持请求中的工具，tool_choice=required 无法满足".into());
            }
            Some(kind) if UNSUPPORTED_RESPONSES_TOOLS.contains(&kind) => {
                return Err(format!("非标准模型不支持强制调用 {kind} 工具"));
            }
            Some("function") => {
                let name = choice
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !remaining_tools.is_some_and(|tools| {
                    tools.iter().any(|tool| {
                        tool.get("type").and_then(Value::as_str) == Some("function")
                            && tool.get("name").and_then(Value::as_str) == Some(name)
                    })
                }) {
                    return Err("非标准模型强制指定的 function 工具不可用".into());
                }
            }
            _ => {}
        }
        if remaining_tools.is_none_or(Vec::is_empty) && kind == Some("auto") {
            body.as_object_mut()
                .expect("request is an object")
                .remove("tool_choice");
            report.adjusted_fields.push("/tool_choice");
        } else if choice.is_object() && matches!(kind, Some("auto" | "none" | "required")) {
            body["tool_choice"] = Value::String(kind.unwrap().to_owned());
            report.adjusted_fields.push("/tool_choice");
        }
    }

    if let Some(include) = body.get_mut("include").and_then(Value::as_array_mut) {
        include.retain(|item| {
            let Some(kind) = item.as_str() else {
                return true;
            };
            if UNSUPPORTED_RESPONSES_INCLUDE.contains(&kind) {
                report.omitted_include.push(kind.to_owned());
                false
            } else {
                true
            }
        });
        if include.is_empty() {
            body.as_object_mut()
                .expect("request is an object")
                .remove("include");
        }
    }

    if let Some(text) = body.get_mut("text").and_then(Value::as_object_mut) {
        if text.remove("verbosity").is_some() {
            report.adjusted_fields.push("/text/verbosity");
        }
        if text.is_empty() {
            body.as_object_mut()
                .expect("request is an object")
                .remove("text");
        }
    }
    if body.get("truncation").and_then(Value::as_str) == Some("auto") {
        body.as_object_mut()
            .expect("request is an object")
            .remove("truncation");
        report.adjusted_fields.push("/truncation");
    }
    if body
        .as_object_mut()
        .expect("request is an object")
        .remove("stream_options")
        .is_some()
    {
        report.adjusted_fields.push("/stream_options");
    }
    // These have stateful semantics and must not be silently dropped.
    if body.get("store") == Some(&Value::Bool(true)) {
        return Err("非标准模型不支持 store=true；请使用 store=false".into());
    }
    if body
        .get("previous_response_id")
        .is_some_and(|value| !value.is_null())
    {
        return Err("非标准模型不支持 previous_response_id；请发送完整 input 历史".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn removes_only_rejected_tools_and_optional_fields() {
        let mut body = json!({
            "model":"test", "input":"hello", "store":false,
            "tools":[
                {"type":"function","name":"shell","parameters":{"type":"object"}},
                {"type":"namespace","name":"mcp","tools":[]},
                {"type":"web_search"}, {"type":"web_search_preview"}
            ],
            "tool_choice":{"type":"auto"},
            "include":["reasoning.encrypted_content","message.annotations"],
            "text":{"verbosity":"medium","format":{"type":"text"}},
            "truncation":"auto", "stream_options":{"include_usage":true}
        });
        let report = adapt(&mut body, "responses").unwrap();
        assert_eq!(
            report.omitted_tool_types,
            ["namespace", "web_search", "web_search_preview"]
        );
        assert_eq!(report.omitted_include, ["message.annotations"]);
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["name"], "shell");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
        assert_eq!(body["text"], json!({"format":{"type":"text"}}));
        for key in ["truncation", "stream_options"] {
            assert!(body.get(key).is_none());
        }
        assert_eq!(body["input"], "hello");
        assert_eq!(body["store"], false);
    }

    #[test]
    fn refuses_to_silently_drop_stateful_or_forced_tool_requests() {
        for mut body in [
            json!({"store":true}),
            json!({"previous_response_id":"resp_1"}),
            json!({"tools":[{"type":"namespace","name":"mcp"}],"tool_choice":"required"}),
            json!({"tools":[{"type":"web_search"}],"tool_choice":{"type":"web_search"}}),
        ] {
            assert!(adapt(&mut body, "responses").is_err());
        }
    }

    #[test]
    fn drops_auto_choice_when_every_tool_was_omitted() {
        let mut body =
            json!({"tools":[{"type":"namespace","name":"mcp"}],"tool_choice":{"type":"auto"}});
        let report = adapt(&mut body, "responses").unwrap();
        assert_eq!(report.omitted_tool_types, ["namespace"]);
        assert!(body.get("tools").is_none());
        assert!(body.get("tool_choice").is_none());
    }

    #[test]
    fn leaves_other_protocols_and_unmatched_fields_untouched() {
        let original = json!({"tools":[{"type":"namespace","name":"mcp"}],"custom":{"kept":true}});
        let mut body = original.clone();
        assert!(!adapt(&mut body, "anthropic").unwrap().changed());
        assert_eq!(body, original);
    }
}
