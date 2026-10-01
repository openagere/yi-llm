use crate::domain::provider::Provider;
use crate::protocol::responses::types::{
    custom_tool_description, custom_tool_parameters, InputContent, InputItem, ReasoningContent,
    ResponsesRequest, Tool, ToolChoice,
};
use crate::protocol::upstream::openai_chat::types::*;

pub fn build_request(
    req: &ResponsesRequest,
    provider: &Provider,
    upstream_model: &str,
) -> Result<ChatRequest, String> {
    if req.previous_response_id.is_some() {
        return Err("previous_response_id 不支持：请使用 store=false 全量历史模式".into());
    }
    let mut messages = Vec::new();
    if let Some(instructions) = &req.instructions {
        let text = instructions
            .as_str()
            .map(str::to_owned)
            .or_else(|| {
                instructions.as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| p.get("text").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .unwrap_or_default();
        messages.push(ChatMessage {
            role: "system".into(),
            content: Some(serde_json::Value::String(text)),
            reasoning_content: None,
            tool_call_id: None,
            tool_calls: None,
        });
    }
    let mut pending_calls = Vec::new();
    let mut pending_reasoning: Vec<String> = Vec::new();
    let flush_assistant = |messages: &mut Vec<ChatMessage>,
                           pending_calls: &mut Vec<ChatToolCall>,
                           pending_reasoning: &mut Vec<String>| {
        if pending_calls.is_empty() && pending_reasoning.is_empty() {
            return;
        }
        let reasoning = pending_reasoning.join("\n");
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: None,
            reasoning_content: (!reasoning.is_empty()).then_some(reasoning),
            tool_call_id: None,
            tool_calls: (!pending_calls.is_empty()).then(|| std::mem::take(pending_calls)),
        });
    };
    for item in req.input_items()? {
        match item {
            InputItem::Message { role, content } => {
                flush_assistant(&mut messages, &mut pending_calls, &mut pending_reasoning);
                let has_image = content
                    .iter()
                    .any(|part| matches!(part, InputContent::InputImage { .. }));
                if role == "assistant" && has_image {
                    return Err("Chat Completions 图片输入只能出现在 user 消息中".into());
                }
                let wire_content = if has_image {
                    let mut parts = Vec::with_capacity(content.len());
                    for part in &content {
                        match part {
                            InputContent::InputText { text }
                            | InputContent::Text { text }
                            | InputContent::OutputText { text } => {
                                parts.push(serde_json::json!({"type":"text","text":text}));
                            }
                            InputContent::InputImage { image_url, detail } => {
                                let mut image = serde_json::json!({"url":image_url});
                                if let Some(detail) = detail {
                                    image["detail"] = serde_json::json!(detail);
                                }
                                parts.push(
                                    serde_json::json!({"type":"image_url","image_url":image}),
                                );
                            }
                            InputContent::Unknown => {
                                return Err("Chat Completions 消息包含未知内容类型".into());
                            }
                        }
                    }
                    serde_json::Value::Array(parts)
                } else {
                    let mut text = String::new();
                    for part in &content {
                        match part {
                            InputContent::InputText { text: value }
                            | InputContent::Text { text: value }
                            | InputContent::OutputText { text: value } => text.push_str(value),
                            InputContent::InputImage { .. } => {
                                return Err(
                                    "Chat Completions 图片输入只能出现在 user 消息中".into()
                                );
                            }
                            InputContent::Unknown => {
                                return Err("Chat Completions 消息包含未知内容类型".into());
                            }
                        }
                    }
                    serde_json::Value::String(text)
                };
                messages.push(ChatMessage {
                    role,
                    content: Some(wire_content),
                    reasoning_content: None,
                    tool_call_id: None,
                    tool_calls: None,
                });
            }
            InputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => pending_calls.push(ChatToolCall {
                id: call_id,
                call_type: "function".into(),
                function: ChatFunctionCall { name, arguments },
            }),
            InputItem::FunctionCallOutput { call_id, output } => {
                flush_assistant(&mut messages, &mut pending_calls, &mut pending_reasoning);
                let text = output
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| {
                        output
                            .get("output")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned)
                    })
                    .unwrap_or_else(|| output.to_string());
                messages.push(ChatMessage {
                    role: "tool".into(),
                    content: Some(serde_json::Value::String(text)),
                    reasoning_content: None,
                    tool_call_id: Some(call_id),
                    tool_calls: None,
                });
            }
            InputItem::Reasoning {
                content,
                summary,
                encrypted_content,
                ..
            } => {
                // Chat Completions has no native reasoning item; replay the
                // text on the next assistant message so providers that read
                // `reasoning_content` keep multi-turn continuity.
                if encrypted_content
                    .as_ref()
                    .is_some_and(|value| !value.is_empty())
                {
                    tracing::debug!(
                        provider = %provider.name,
                        "Chat Completions cannot replay opaque Responses reasoning state"
                    );
                }
                for parts in content.iter().chain(summary.iter()) {
                    for part in parts {
                        match part {
                            ReasoningContent::ReasoningText { text } => {
                                pending_reasoning.push(text.clone());
                            }
                            ReasoningContent::Unknown => {
                                return Err(
                                    "reasoning 包含未知内容类型，无法转换到 Chat Completions"
                                        .into(),
                                );
                            }
                        }
                    }
                }
            }
            InputItem::Unknown => {
                return Err("Responses input 包含未知输入类型，无法转换到 Chat Completions".into())
            }
        }
    }
    flush_assistant(&mut messages, &mut pending_calls, &mut pending_reasoning);
    if messages.is_empty() {
        return Err("输入为空或全部内容被忽略".into());
    }
    let tools = req.tools.as_ref().and_then(|items| {
        let supported = items
            .iter()
            .filter_map(|tool| match tool {
                Tool::Function {
                    name,
                    description,
                    parameters,
                } => Some(ChatTool {
                    tool_type: "function".into(),
                    function: ChatFunctionDef {
                        name: name.clone(),
                        description: description.clone(),
                        parameters: parameters
                            .clone()
                            .unwrap_or(serde_json::json!({"type":"object"})),
                    },
                }),
                Tool::Custom {
                    name,
                    description,
                    format,
                } => Some(ChatTool {
                    tool_type: "function".into(),
                    function: ChatFunctionDef {
                        name: name.clone(),
                        description: custom_tool_description(
                            description.as_deref(),
                            format.as_ref(),
                        ),
                        parameters: custom_tool_parameters(),
                    },
                }),
                Tool::WebSearch | Tool::WebSearchPreview => None,
                Tool::Unsupported => None,
            })
            .collect::<Vec<_>>();
        (!supported.is_empty()).then_some(supported)
    });
    let tool_choice = tools
        .as_ref()
        .and(req.tool_choice.as_ref())
        .map(|choice| match choice {
            ToolChoice::Kind(k) => ChatToolChoiceWire::Mode(
                match k.as_str() {
                    "none" => "none",
                    "required" => "required",
                    _ => "auto",
                }
                .into(),
            ),
            ToolChoice::Object { kind, name } if kind == "function" || kind == "custom" => {
                ChatToolChoiceWire::Function {
                    choice_type: "function".into(),
                    function: ChatFunctionChoiceName {
                        name: name.clone().unwrap_or_default(),
                    },
                }
            }
            ToolChoice::Object { kind, .. } => ChatToolChoiceWire::Mode(
                match kind.as_str() {
                    "none" => "none",
                    "required" => "required",
                    _ => "auto",
                }
                .into(),
            ),
        });
    let effort = req
        .reasoning
        .as_ref()
        .and_then(|r| r.effort.as_deref())
        .unwrap_or(&provider.thinking);
    let reasoning_effort = crate::domain::capabilities::effort_levels("openai_chat")
        .contains(&effort)
        .then(|| effort.to_owned());
    let response_format = match req
        .text
        .as_ref()
        .and_then(|text| text.get("format"))
        .and_then(|format| format.get("type"))
        .and_then(serde_json::Value::as_str)
    {
        Some("json_schema") => {
            let (name, schema) = req.json_schema().ok_or("text.format.schema 缺失")?;
            Some(ChatResponseFormat {
                format_type: "json_schema".into(),
                json_schema: Some(ChatJsonSchemaFormat {
                    name,
                    schema,
                    strict: Some(true),
                }),
            })
        }
        Some("json_object") => Some(ChatResponseFormat {
            format_type: "json_object".into(),
            json_schema: None,
        }),
        Some(kind) => return Err(format!("Chat Completions 不支持响应格式: {kind}")),
        None => None,
    };
    Ok(ChatRequest {
        model: upstream_model.into(),
        messages,
        temperature: req.temperature,
        top_p: req.top_p,
        max_completion_tokens: req.max_output_tokens,
        stop: req.stop.clone(),
        tools,
        tool_choice,
        parallel_tool_calls: req.parallel_tool_calls,
        response_format,
        reasoning_effort,
        metadata: req.metadata.clone(),
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::Provider;

    fn provider() -> Provider {
        Provider {
            id: "p1".into(),
            provider_type: "openai_chat".into(),
            name: "n".into(),
            short_code: "test".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key: "k".into(),
            enabled: true,
            thinking: "medium".into(),
            extra: serde_json::json!({}),
            is_default: true,
            protocol_support: crate::domain::provider::ProtocolSupport::native("openai_chat"),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn req_from_input(input: serde_json::Value) -> ResponsesRequest {
        serde_json::from_value(serde_json::json!({
            "model": "public-model",
            "instructions": "sys",
            "input": input,
        }))
        .unwrap()
    }

    #[test]
    fn maps_json_schema_to_response_format() {
        let mut req = req_from_input(serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}
        ]));
        req.text = Some(serde_json::json!({
            "format": {"type": "json_schema", "name": "answer", "schema": {"type": "object"}}
        }));
        let out = build_request(&req, &provider(), "gpt-5").unwrap();
        let value = serde_json::to_value(out.response_format.expect("response_format")).unwrap();
        assert_eq!(value["type"], "json_schema");
        assert_eq!(value["json_schema"]["name"], "answer");
        assert_eq!(value["json_schema"]["strict"], true);
    }

    #[test]
    fn all_openai_efforts_are_sent_to_chat_upstream() {
        for effort in crate::domain::capabilities::effort_levels("openai_chat") {
            let mut req = req_from_input(serde_json::json!("hi"));
            req.reasoning = Some(crate::protocol::responses::types::ReasoningParams {
                effort: Some((*effort).into()),
            });
            let out = build_request(&req, &provider(), "gpt").unwrap();
            assert_eq!(out.reasoning_effort.as_deref(), Some(*effort));
        }
    }

    #[test]
    fn replays_reasoning_text_on_assistant_message() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "q"}]},
            {"type": "reasoning", "id": "rs", "content": [{"type": "reasoning_text", "text": "let me think"}]},
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "a"}]}
        ]);
        let req = req_from_input(input);
        let out = build_request(&req, &provider(), "gpt-5").unwrap();
        let assistant = out
            .messages
            .iter()
            .find(|m| m.role == "assistant" && m.reasoning_content.is_some())
            .expect("assistant message carrying reasoning");
        assert_eq!(assistant.reasoning_content.as_deref(), Some("let me think"));
    }

    #[test]
    fn parallel_tool_calls_passes_through_to_chat_upstream() {
        let req = req_from_input(serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}
        ]));
        let mut req = req;
        req.parallel_tool_calls = Some(false);
        let out = build_request(&req, &provider(), "gpt-5").unwrap();
        assert_eq!(out.parallel_tool_calls, Some(false));
    }

    #[test]
    fn parallel_tool_calls_fold_into_one_assistant_message() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "run both"}]},
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{}"},
            {"type": "function_call", "call_id": "call_2", "name": "shell", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "a"},
            {"type": "function_call_output", "call_id": "call_2", "output": "b"}
        ]);
        let req = req_from_input(input);
        let out = build_request(&req, &provider(), "gpt-5").unwrap();
        let assistant = out
            .messages
            .iter()
            .find(|m| m.role == "assistant")
            .expect("assistant message");
        assert_eq!(assistant.tool_calls.as_ref().unwrap().len(), 2);
    }
}
