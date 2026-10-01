use crate::domain::provider::Provider;
use crate::protocol::client::NamespaceToolAlias;
use crate::protocol::responses::types::{
    custom_tool_description, custom_tool_parameters, InputContent, InputItem, ReasoningContent,
    ResponsesRequest, Tool, ToolChoice,
};
use crate::protocol::upstream::anthropic::types::{
    self as at, AnthropicTool, ImageSource, Message, MessageContent, MessagesRequest, OutputConfig,
    OutputFormat, RequestMetadata, SystemPrompt, ThinkingConfig,
};
use std::collections::HashSet;

pub const DEFAULT_MAX_TOKENS: u32 = 16384;

/// effort → thinking budget heuristic (spec §5.1).
pub fn effort_to_budget(effort: &str) -> Option<u32> {
    match effort {
        "minimal" => None,
        "low" => Some(4096),
        "medium" => Some(10000),
        "high" => Some(32000),
        "xhigh" => Some(64000),
        _ => None,
    }
}

fn parse_stop_sequences(value: &Value) -> Result<Vec<String>, String> {
    if let Some(stop) = value.as_str() {
        if stop.is_empty() {
            return Err("stop 不能为空".into());
        }
        return Ok(vec![stop.to_owned()]);
    }
    let stops = value
        .as_array()
        .ok_or("stop 必须是文本或文本数组")?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .filter(|stop| !stop.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("stop[{index}] 必须是非空文本"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if stops.is_empty() {
        return Err("stop 数组不能为空".into());
    }
    Ok(stops)
}

use crate::protocol::responses::types::{
    custom_tool_input, ErrorBody, InputTokensDetails, OutputContent, OutputItem,
    OutputTokensDetails, ResponseObject, Usage,
};
use crate::protocol::upstream::{ItemKind, UpstreamEvent};
use serde_json::Value;
use std::collections::HashMap;

struct OpenItem {
    kind: ItemKind,
    id: String,
    text: String,
    /// Accumulated thinking signature / redacted payload for reasoning items.
    encrypted: String,
    call_id: String,
    name: String,
    args: String,
    custom_tool: bool,
    namespace: Option<String>,
}

pub struct ResponseTranslator {
    response_id: String,
    model: String,
    created_at: u64,
    items: HashMap<usize, OpenItem>,
    completed: Vec<(usize, OutputItem)>,
    usage: Option<Usage>,
    custom_tool_names: HashSet<String>,
    tool_name_aliases: HashMap<String, NamespaceToolAlias>,
}

impl ResponseTranslator {
    pub fn new(response_id: String, model: String) -> Self {
        Self::new_with_custom_tool_names(response_id, model, HashSet::new())
    }

    pub fn new_with_custom_tool_names(
        response_id: String,
        model: String,
        custom_tool_names: HashSet<String>,
    ) -> Self {
        Self::new_with_tool_mappings(response_id, model, custom_tool_names, HashMap::new())
    }

    pub fn new_with_tool_mappings(
        response_id: String,
        model: String,
        custom_tool_names: HashSet<String>,
        tool_name_aliases: HashMap<String, NamespaceToolAlias>,
    ) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self {
            response_id,
            model,
            created_at,
            items: HashMap::new(),
            completed: Vec::new(),
            usage: None,
            custom_tool_names,
            tool_name_aliases,
        }
    }

    pub fn created(&self) -> (String, serde_json::Value) {
        let response = ResponseObject::shell(&self.response_id, &self.model);
        (
            "response.created".into(),
            serde_json::json!({"type":"response.created","response":response}),
        )
    }

    pub fn feed(&mut self, event: UpstreamEvent) -> Vec<(String, serde_json::Value)> {
        let mut out = Vec::new();
        match event {
            UpstreamEvent::ItemStart { index, kind } => {
                let id = format!("{}_item_{index}", self.response_id);
                let item = match kind {
                    ItemKind::Message => {
                        serde_json::json!({"id":id,"type":"message","role":"assistant","status":"in_progress","content":[]})
                    }
                    ItemKind::Reasoning => {
                        serde_json::json!({"id":id,"type":"reasoning","status":"in_progress","content":[],"summary":[]})
                    }
                    ItemKind::FunctionCall => serde_json::Value::Null,
                };
                if kind != ItemKind::FunctionCall {
                    out.push(("response.output_item.added".into(), serde_json::json!({"type":"response.output_item.added","output_index":index,"item":item})));
                }
                if kind == ItemKind::Message {
                    out.push(("response.content_part.added".into(), serde_json::json!({"type":"response.content_part.added","item_id":id,"output_index":index,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})));
                }
                self.items.insert(
                    index,
                    OpenItem {
                        kind,
                        id,
                        text: String::new(),
                        encrypted: String::new(),
                        call_id: String::new(),
                        name: String::new(),
                        args: String::new(),
                        custom_tool: false,
                        namespace: None,
                    },
                );
            }
            UpstreamEvent::TextDelta { index, delta } => {
                if let Some(item) = self.items.get_mut(&index) {
                    item.text.push_str(&delta);
                    out.push(("response.output_text.delta".into(), serde_json::json!({"type":"response.output_text.delta","item_id":item.id,"output_index":index,"content_index":0,"delta":delta})));
                }
            }
            UpstreamEvent::ThinkingDelta { index, delta } => {
                if let Some(item) = self.items.get_mut(&index) {
                    item.text.push_str(&delta);
                    out.push(("response.reasoning_summary_text.delta".into(), serde_json::json!({"type":"response.reasoning_summary_text.delta","item_id":item.id,"output_index":index,"summary_index":0,"delta":delta})));
                }
            }
            UpstreamEvent::ToolCallStart {
                index,
                call_id,
                name,
            } => {
                if let Some(item) = self.items.get_mut(&index) {
                    item.call_id = call_id;
                    item.custom_tool = self.custom_tool_names.contains(&name);
                    if let Some(alias) = self.tool_name_aliases.get(&name) {
                        item.name = alias.name.clone();
                        item.namespace = Some(alias.namespace.clone());
                    } else {
                        item.name = name;
                    }
                    let mut added = if item.custom_tool {
                        serde_json::json!({"id":item.id,"type":"custom_tool_call","call_id":item.call_id,"name":item.name,"namespace":item.namespace,"input":"","status":"in_progress"})
                    } else {
                        serde_json::json!({"id":item.id,"type":"function_call","call_id":item.call_id,"name":item.name,"namespace":item.namespace,"arguments":"","status":"in_progress"})
                    };
                    if item.namespace.is_none() {
                        if let Some(object) = added.as_object_mut() {
                            object.remove("namespace");
                        }
                    }
                    out.push(("response.output_item.added".into(), serde_json::json!({"type":"response.output_item.added","output_index":index,"item":added})));
                }
            }
            UpstreamEvent::ToolCallArgsDelta { index, delta } => {
                if let Some(item) = self.items.get_mut(&index) {
                    item.args.push_str(&delta);
                    if !item.custom_tool {
                        out.push(("response.function_call_arguments.delta".into(), serde_json::json!({"type":"response.function_call_arguments.delta","item_id":item.id,"output_index":index,"delta":delta})));
                    }
                }
            }
            UpstreamEvent::ItemStop { index } => {
                if let Some(item) = self.items.remove(&index) {
                    let output = match item.kind {
                        ItemKind::Message => OutputItem::Message {
                            id: item.id.clone(),
                            role: "assistant".into(),
                            status: "completed".into(),
                            content: vec![OutputContent::OutputText {
                                text: item.text,
                                annotations: vec![],
                            }],
                        },
                        ItemKind::Reasoning => OutputItem::Reasoning {
                            id: item.id.clone(),
                            status: "completed".into(),
                            content: vec![],
                            summary: vec![
                                serde_json::json!({"type":"summary_text","text":item.text}),
                            ],
                            encrypted_content: (!item.encrypted.is_empty())
                                .then_some(item.encrypted),
                        },
                        ItemKind::FunctionCall if item.custom_tool => OutputItem::CustomToolCall {
                            id: item.id.clone(),
                            call_id: item.call_id,
                            name: item.name,
                            input: custom_tool_input(&item.args),
                            status: "completed".into(),
                        },
                        ItemKind::FunctionCall => OutputItem::FunctionCall {
                            id: item.id.clone(),
                            call_id: item.call_id,
                            name: item.name,
                            namespace: item.namespace,
                            arguments: item.args,
                            status: "completed".into(),
                        },
                    };
                    match &output {
                    OutputItem::Message { id, content, .. } => {
                        let text = match &content[0] { OutputContent::OutputText { text, .. } => text, };
                        out.push(("response.output_text.done".into(), serde_json::json!({"type":"response.output_text.done","item_id":id,"output_index":index,"content_index":0,"text":text})));
                        out.push(("response.content_part.done".into(), serde_json::json!({"type":"response.content_part.done","item_id":id,"output_index":index,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}})));
                    }
                    OutputItem::Reasoning { id, summary, .. } => {
                        let text = summary.first().and_then(|item| item.get("text")).and_then(serde_json::Value::as_str).unwrap_or_default();
                        out.push(("response.reasoning_summary_text.done".into(), serde_json::json!({"type":"response.reasoning_summary_text.done","item_id":id,"output_index":index,"summary_index":0,"text":text})));
                    }
                    OutputItem::FunctionCall { id, arguments, .. } => out.push(("response.function_call_arguments.done".into(), serde_json::json!({"type":"response.function_call_arguments.done","item_id":id,"output_index":index,"arguments":arguments}))),
                    OutputItem::CustomToolCall { id, input, .. } => out.push(("response.custom_tool_call_input.done".into(), serde_json::json!({"type":"response.custom_tool_call_input.done","item_id":id,"output_index":index,"input":input}))),
                }
                    out.push(("response.output_item.done".into(), serde_json::json!({"type":"response.output_item.done","output_index":index,"item":output})));
                    self.completed.push((index, output));
                }
            }
            UpstreamEvent::Usage {
                input_tokens,
                output_tokens,
                reasoning_tokens,
                cached_tokens,
            } => {
                // Usage arrives in fragments (Anthropic: input on
                // message_start, output on message_delta), so merge into the
                // accumulated snapshot instead of replacing it.
                let merged = match self.usage.take() {
                    Some(prev) => Usage {
                        input_tokens: input_tokens.unwrap_or(prev.input_tokens),
                        output_tokens: output_tokens.unwrap_or(prev.output_tokens),
                        total_tokens: 0,
                        input_tokens_details: InputTokensDetails {
                            cached_tokens: cached_tokens
                                .unwrap_or(prev.input_tokens_details.cached_tokens),
                        },
                        output_tokens_details: OutputTokensDetails {
                            reasoning_tokens: reasoning_tokens
                                .unwrap_or(prev.output_tokens_details.reasoning_tokens),
                        },
                    },
                    None => Usage {
                        input_tokens: input_tokens.unwrap_or(0),
                        output_tokens: output_tokens.unwrap_or(0),
                        total_tokens: 0,
                        input_tokens_details: InputTokensDetails {
                            cached_tokens: cached_tokens.unwrap_or(0),
                        },
                        output_tokens_details: OutputTokensDetails {
                            reasoning_tokens: reasoning_tokens.unwrap_or(0),
                        },
                    },
                };
                self.usage = Some(Usage {
                    total_tokens: merged.input_tokens.saturating_add(merged.output_tokens),
                    ..merged
                });
            }
            UpstreamEvent::ReasoningData { index, data } => {
                if let Some(item) = self.items.get_mut(&index) {
                    item.encrypted.push_str(&data);
                }
            }
            UpstreamEvent::Done { reason } => {
                let status = if reason == "length" {
                    "incomplete"
                } else {
                    "completed"
                };
                let response = self.response(status, None);
                out.push((if status == "completed" { "response.completed" } else { "response.incomplete" }.into(), serde_json::json!({"type":if status == "completed" { "response.completed" } else { "response.incomplete" },"response":response})));
            }
            UpstreamEvent::RawError { message } => {
                let response = self.response(
                    "failed",
                    Some(ErrorBody {
                        message,
                        code: Some("upstream_error".into()),
                    }),
                );
                out.push((
                    "response.failed".into(),
                    serde_json::json!({"type":"response.failed","response":response}),
                ));
            }
        }
        out
    }

    fn response(&self, status: &str, error: Option<ErrorBody>) -> ResponseObject {
        let mut completed = self.completed.clone();
        completed.sort_by_key(|(index, _)| *index);
        ResponseObject {
            id: self.response_id.clone(),
            object: "response".into(),
            created_at: self.created_at,
            model: self.model.clone(),
            status: status.into(),
            output: completed.into_iter().map(|(_, item)| item).collect(),
            usage: self.usage.clone(),
            error,
        }
    }
}

pub fn build_request(
    req: &ResponsesRequest,
    provider: &Provider,
    upstream_model: &str,
) -> Result<MessagesRequest, String> {
    if req.previous_response_id.is_some() {
        return Err("previous_response_id 不支持：请使用 store=false 全量历史模式".into());
    }

    let max_tokens = req.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    let stop_sequences = req.stop.as_ref().map(parse_stop_sequences).transpose()?;

    let mut system_parts = Vec::new();
    if let Some(instructions) = &req.instructions {
        if let Some(text) = instructions.as_str() {
            system_parts.push(text.to_owned());
        } else if let Some(parts) = instructions.as_array() {
            for (index, part) in parts.iter().enumerate() {
                let kind = part.get("type").and_then(Value::as_str).unwrap_or("text");
                if !matches!(kind, "text" | "input_text" | "output_text") {
                    return Err(format!("instructions[{index}] 内容类型不支持: {kind}"));
                }
                system_parts.push(
                    part.get("text")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("instructions[{index}] 缺少 text"))?
                        .to_owned(),
                );
            }
        } else {
            return Err("instructions 必须是文本或文本块数组".into());
        }
    }

    let mut conversation = Vec::new();
    for (index, item) in req.input_items()?.into_iter().enumerate() {
        match item {
            InputItem::Message { role, content }
                if matches!(role.as_str(), "system" | "developer") =>
            {
                for (part_index, part) in content.into_iter().enumerate() {
                    match part {
                        InputContent::InputText { text }
                        | InputContent::Text { text }
                        | InputContent::OutputText { text } => system_parts.push(text),
                        InputContent::InputImage { .. } => {
                            return Err(format!(
                                "input[{index}].content[{part_index}] system/developer 消息不支持图片"
                            ));
                        }
                        InputContent::Unknown => {
                            return Err(format!(
                                "input[{index}].content[{part_index}] 内容类型无法转换到 Anthropic"
                            ));
                        }
                    }
                }
            }
            other => conversation.push(other),
        }
    }
    let system = (!system_parts.is_empty()).then(|| SystemPrompt::Text(system_parts.join("\n\n")));
    let messages = build_messages(&conversation)?;

    let tools = req.tools.as_ref().and_then(|tools| {
        let supported = tools
            .iter()
            .filter_map(|t| match t {
                Tool::Function {
                    name,
                    description,
                    parameters,
                } => Some(AnthropicTool {
                    tool_type: None,
                    name: name.clone(),
                    description: description.clone(),
                    input_schema: Some(
                        parameters
                            .clone()
                            .unwrap_or(serde_json::json!({"type": "object"})),
                    ),
                    max_uses: None,
                }),
                Tool::Custom {
                    name,
                    description,
                    format,
                } => Some(AnthropicTool {
                    tool_type: None,
                    name: name.clone(),
                    description: custom_tool_description(description.as_deref(), format.as_ref()),
                    input_schema: Some(custom_tool_parameters()),
                    max_uses: None,
                }),
                // Anthropic-compatible endpoints vary in native tool support. Function
                // tools are portable across them; built-in tools are omitted.
                Tool::WebSearch | Tool::WebSearchPreview | Tool::Unsupported => None,
            })
            .collect::<Vec<_>>();
        (!supported.is_empty()).then_some(supported)
    });

    // OpenAI `parallel_tool_calls=false` (canonical: parallel_tool_calls =
    // Some(false)) maps onto Anthropic's standard tool_choice field
    // `disable_parallel_tool_use`. `Some(true)`/unset both mean "upstream
    // default" and stay omitted.
    let disable_parallel_tool_use = (req.parallel_tool_calls == Some(false)).then_some(true);
    let tool_choice = tools.as_ref().and_then(|_| {
        let Some(choice) = req.tool_choice.as_ref() else {
            // No explicit choice: emit Anthropic's default "auto" only when a
            // parallel-tool restriction must ride along with it.
            return disable_parallel_tool_use.map(|disable| at::ToolChoice::Auto {
                disable_parallel_tool_use: Some(disable),
            });
        };
        let (kind, name) = match choice {
            ToolChoice::Kind(k) => (k.as_str(), None),
            ToolChoice::Object { kind, name } => (kind.as_str(), name.as_deref()),
        };
        Some(match kind {
            // "none" already forbids any tool call, so a parallel restriction
            // is moot and the flag is dropped.
            "none" => at::ToolChoice::None,
            "required" => at::ToolChoice::Any {
                disable_parallel_tool_use,
            },
            "function" | "custom" => at::ToolChoice::Tool {
                name: name.unwrap_or_default().to_owned(),
                disable_parallel_tool_use,
            },
            _ => at::ToolChoice::Auto {
                disable_parallel_tool_use,
            },
        })
    });

    let explicit_effort = req
        .reasoning
        .as_ref()
        .and_then(|r| r.effort.as_ref())
        .map(|effort| effort.to_lowercase());
    if explicit_effort.as_ref().is_some_and(|effort| {
        !crate::domain::capabilities::effort_levels("anthropic").contains(&effort.as_str())
    }) {
        return Err("Effort 档位不符合 Anthropic 协议".into());
    }
    let format = req
        .json_schema()
        .map(|(_, schema)| OutputFormat::JsonSchema { schema });
    let output_config = if explicit_effort.is_some() || format.is_some() {
        Some(OutputConfig {
            effort: explicit_effort.clone(),
            format,
        })
    } else {
        None
    };
    let thinking = if explicit_effort.is_some() || max_tokens < 2 {
        // Explicit effort uses output_config; legacy budgets need room below max_tokens.
        None
    } else {
        effort_to_budget(&provider.thinking.to_lowercase()).map(|budget| {
            // Anthropic requires budget_tokens < max_tokens.
            let budget = if budget >= max_tokens {
                max_tokens.saturating_sub(1)
            } else {
                budget
            };
            ThinkingConfig {
                thinking_type: "enabled".into(),
                budget_tokens: budget,
            }
        })
    };

    Ok(MessagesRequest {
        model: upstream_model.to_string(),
        messages,
        system,
        max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        top_k: None,
        stop_sequences,
        thinking,
        output_config,
        tools,
        tool_choice,
        stream: true,
        metadata: req
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("user_id"))
            .or(req.prompt_cache_key.as_ref())
            .map(|k| RequestMetadata {
                user_id: Some(k.clone()),
            }),
    })
}

/// Fold Responses input items into Anthropic messages.
/// Consecutive assistant-output items (reasoning/message/function_call) merge into one
/// assistant message; function_call_output becomes a user message with tool_result.
pub fn build_messages(items: &[InputItem]) -> Result<Vec<Message>, String> {
    let mut messages: Vec<Message> = vec![];
    let mut assistant_buf: Vec<MessageContent> = vec![];
    let mut seen_call_ids: HashSet<String> = HashSet::new();

    // Flush the pending assistant buffer, returning whether anything was flushed.
    // All thinking blocks must precede the other blocks (Anthropic requirement).
    let flush = |messages: &mut Vec<Message>, buf: &mut Vec<MessageContent>| -> bool {
        if buf.is_empty() {
            return false;
        }
        let mut taken = std::mem::take(buf);
        let (thinkings, others): (Vec<_>, Vec<_>) = taken
            .drain(..)
            .partition(|c| matches!(c, MessageContent::Thinking { .. }));
        let mut reordered = thinkings;
        reordered.extend(others);
        messages.push(Message {
            role: "assistant".into(),
            content: reordered,
        });
        true
    };

    for item in items {
        match item {
            InputItem::Message { role, content } if role == "assistant" => {
                for part in content {
                    match part {
                        InputContent::Text { text }
                        | InputContent::OutputText { text }
                        | InputContent::InputText { text } => {
                            assistant_buf.push(MessageContent::Text { text: text.clone() })
                        }
                        InputContent::InputImage { .. } => {
                            return Err("Anthropic 图片输入只能出现在 user 消息中".into())
                        }
                        InputContent::Unknown => {
                            return Err("Anthropic assistant 消息包含未知内容类型".into())
                        }
                    }
                }
            }
            InputItem::Message { role, content } if role == "user" => {
                flush(&mut messages, &mut assistant_buf);
                let mut parts = Vec::new();
                for part in content {
                    match part {
                        InputContent::InputText { text }
                        | InputContent::Text { text }
                        | InputContent::OutputText { text } => {
                            parts.push(MessageContent::Text { text: text.clone() })
                        }
                        InputContent::InputImage { image_url, .. } => {
                            parts.push(MessageContent::Image {
                                source: anthropic_image_source(image_url)?,
                            })
                        }
                        InputContent::Unknown => {
                            return Err("Anthropic user 消息包含未知内容类型".into())
                        }
                    }
                }
                messages.push(Message {
                    role: "user".into(),
                    content: parts,
                });
            }
            InputItem::Message { role, .. } => {
                return Err(format!("Responses 消息角色无法转换到 Anthropic: {role}"))
            }
            InputItem::Reasoning {
                content,
                summary,
                encrypted_content,
                ..
            } => {
                if content
                    .iter()
                    .chain(summary.iter())
                    .flatten()
                    .any(|part| matches!(part, ReasoningContent::Unknown))
                {
                    return Err("Anthropic reasoning 包含未知内容类型".into());
                }
                let text = content
                    .iter()
                    .chain(summary.iter())
                    .flatten()
                    .filter_map(|p| match p {
                        ReasoningContent::ReasoningText { text } => Some(text.as_str()),
                        ReasoningContent::Unknown => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                // Attach the echoed thinking signature (Responses
                // `encrypted_content`) so the upstream can verify the
                // thinking chain across turns. A signature without text is
                // still replayed to keep redacted blocks addressable.
                if !text.is_empty() || encrypted_content.is_some() {
                    assistant_buf.push(MessageContent::Thinking {
                        thinking: text,
                        signature: encrypted_content.clone().filter(|s| !s.is_empty()),
                    });
                }
            }
            InputItem::FunctionCall {
                call_id,
                name,
                arguments,
            } => {
                let input = serde_json::from_str(arguments)
                    .map_err(|e| format!("function_call 参数不是合法 JSON ({name}): {e}"))?;
                seen_call_ids.insert(call_id.clone());
                assistant_buf.push(MessageContent::ToolUse {
                    id: call_id.clone(),
                    name: name.clone(),
                    input,
                });
            }
            InputItem::FunctionCallOutput { call_id, output } => {
                if !seen_call_ids.contains(call_id) {
                    return Err(format!(
                        "function_call_output 引用了不存在的 call_id: {call_id}"
                    ));
                }
                let text = match output {
                    serde_json::Value::String(s) => s.clone(),
                    other => other
                        .get("output")
                        .and_then(|o| o.as_str())
                        .map(String::from)
                        .unwrap_or_else(|| other.to_string()),
                };
                let is_error = output.get("is_error").and_then(|v| v.as_bool());
                let new_block = MessageContent::ToolResult {
                    tool_use_id: call_id.clone(),
                    content: text,
                    is_error,
                };
                // After flushing any pending assistant content, consecutive tool outputs
                // (parallel tool calls) fold into a single user message.
                let flushed = flush(&mut messages, &mut assistant_buf);
                let can_merge = !flushed
                    && messages.last().is_some_and(|m| {
                        m.role == "user"
                            && !m.content.is_empty()
                            && m.content
                                .iter()
                                .all(|c| matches!(c, MessageContent::ToolResult { .. }))
                    });
                if can_merge {
                    if let Some(last) = messages.last_mut() {
                        last.content.push(new_block);
                    }
                } else {
                    messages.push(Message {
                        role: "user".into(),
                        content: vec![new_block],
                    });
                }
            }
            InputItem::Unknown => {
                return Err("Responses input 包含未知输入类型，无法转换到 Anthropic".into())
            }
        }
    }
    flush(&mut messages, &mut assistant_buf);
    if messages.is_empty() {
        return Err("输入为空或全部内容被忽略".into());
    }
    if messages[0].role == "assistant" {
        return Err("输入必须以 user 消息开始".into());
    }
    Ok(messages)
}

fn anthropic_image_source(image_url: &str) -> Result<ImageSource, String> {
    let encoded = image_url
        .strip_prefix("data:")
        .ok_or("Anthropic 上游只支持 base64 图片，请使用 data URL")?;
    let (metadata, data) = encoded.split_once(',').ok_or("图片 data URL 格式无效")?;
    let media_type = metadata
        .strip_suffix(";base64")
        .ok_or("图片 data URL 必须使用 base64 编码")?;
    if !media_type.starts_with("image/") || data.is_empty() {
        return Err("图片 data URL 缺少有效的图片类型或数据".into());
    }
    Ok(ImageSource {
        source_type: "base64".into(),
        media_type: media_type.to_owned(),
        data: data.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::provider::Provider;
    use crate::protocol::responses::types::ReasoningParams;

    fn provider() -> Provider {
        Provider {
            id: "p1".into(),
            provider_type: "anthropic".into(),
            name: "n".into(),
            short_code: "test".into(),
            base_url: "https://api.anthropic.com".into(),
            api_key: "k".into(),
            enabled: true,
            thinking: "medium".into(),
            extra: serde_json::json!({}),
            is_default: true,
            protocol_support: crate::domain::provider::ProtocolSupport::native("anthropic"),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn req_from_input(input: serde_json::Value) -> ResponsesRequest {
        serde_json::from_value(serde_json::json!({
            "model": "gpt-5-codex",
            "instructions": "sys prompt",
            "input": input,
            "tools": [{"type": "function", "name": "shell", "description": "run",
                       "parameters": {"type": "object"}}],
        }))
        .unwrap()
    }

    fn simple_user_input() -> serde_json::Value {
        serde_json::json!([{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}])
    }

    #[test]
    fn maps_system_tools_and_model() {
        let req = req_from_input(simple_user_input());
        let out = build_request(&req, &provider(), "claude-sonnet-4-5").unwrap();
        assert_eq!(out.model, "claude-sonnet-4-5");
        assert!(matches!(out.system, Some(SystemPrompt::Text(ref s)) if s == "sys prompt"));
        assert_eq!(out.tools.unwrap()[0].name, "shell");
        assert!(out.stream);
        // provider default thinking=medium → budget 10000
        assert_eq!(out.thinking.unwrap().budget_tokens, 10000);
    }

    #[test]
    fn folds_items_into_alternating_messages() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "fix bug"}]},
            {"type": "reasoning", "id": "rs", "content": [{"type": "reasoning_text", "text": "hmm"}]},
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{\"cmd\":\"ls\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "out.txt"}
        ]);
        let req = req_from_input(input);
        let msgs = build_messages(&req.input_items().unwrap()).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert!(matches!(
            msgs[1].content[0],
            MessageContent::Thinking { .. }
        ));
        assert!(
            matches!(msgs[1].content[1], MessageContent::ToolUse { ref id, .. } if id == "call_1")
        );
        assert!(
            matches!(msgs[2].content[0], MessageContent::ToolResult { ref tool_use_id, .. } if tool_use_id == "call_1")
        );
    }

    #[test]
    fn request_effort_overrides_provider_default() {
        let mut req = req_from_input(simple_user_input());
        req.reasoning = Some(ReasoningParams {
            effort: Some("low".into()),
        });
        let out = build_request(&req, &provider(), "claude").unwrap();
        assert_eq!(out.output_config.unwrap().effort.as_deref(), Some("low"));
        assert!(out.thinking.is_none());
    }

    #[test]
    fn all_native_efforts_preserve_structured_output() {
        for effort in crate::domain::capabilities::effort_levels("anthropic") {
            let mut req = req_from_input(simple_user_input());
            req.reasoning = Some(ReasoningParams {
                effort: Some((*effort).into()),
            });
            req.text = Some(
                serde_json::json!({"format":{"type":"json_schema","name":"answer","schema":{"type":"object"}}}),
            );
            let out = build_request(&req, &provider(), "claude").unwrap();
            let config = out.output_config.unwrap();
            assert_eq!(config.effort.as_deref(), Some(*effort));
            assert!(config.format.is_some());
            assert!(out.thinking.is_none());
        }
    }

    #[test]
    fn clamps_budget_below_max_tokens() {
        let mut req = req_from_input(simple_user_input());
        req.max_output_tokens = Some(2000);
        let out = build_request(&req, &provider(), "claude").unwrap();
        assert_eq!(out.thinking.unwrap().budget_tokens, 1999);
    }

    #[test]
    fn omits_unsupported_tools_and_rejects_previous_response_id() {
        let mut req = req_from_input(simple_user_input());
        req.tools = Some(vec![Tool::Unsupported]);
        let out = build_request(&req, &provider(), "m").unwrap();
        assert!(out.tools.is_none());
        assert!(out.tool_choice.is_none());

        let mut req = req_from_input(simple_user_input());
        req.previous_response_id = Some("resp_1".into());
        assert!(build_request(&req, &provider(), "m").is_err());
    }

    #[test]
    fn parallel_tool_calls_false_sets_disable_parallel_tool_use() {
        let mut req = req_from_input(simple_user_input());
        req.parallel_tool_calls = Some(false);
        let out = build_request(&req, &provider(), "claude").unwrap();
        let value = serde_json::to_value(out.tool_choice.expect("tool_choice")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"type": "auto", "disable_parallel_tool_use": true})
        );
    }

    #[test]
    fn parallel_tool_calls_false_merges_into_explicit_tool_choice() {
        let mut req = req_from_input(simple_user_input());
        req.tool_choice = Some(ToolChoice::Object {
            kind: "function".into(),
            name: Some("shell".into()),
        });
        req.parallel_tool_calls = Some(false);
        let out = build_request(&req, &provider(), "claude").unwrap();
        let value = serde_json::to_value(out.tool_choice.expect("tool_choice")).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "type": "tool",
                "name": "shell",
                "disable_parallel_tool_use": true,
            })
        );
    }

    #[test]
    fn parallel_tool_calls_true_or_unset_omits_disable_flag() {
        let mut req = req_from_input(simple_user_input());
        req.parallel_tool_calls = Some(true);
        let out = build_request(&req, &provider(), "claude").unwrap();
        assert!(out.tool_choice.is_none());

        let req = req_from_input(simple_user_input());
        let out = build_request(&req, &provider(), "claude").unwrap();
        assert!(out.tool_choice.is_none());
    }

    #[test]
    fn merges_consecutive_tool_outputs_into_one_user_message() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "run both"}]},
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{\"cmd\":\"ls\"}"},
            {"type": "function_call", "call_id": "call_2", "name": "shell", "arguments": "{\"cmd\":\"pwd\"}"},
            {"type": "function_call_output", "call_id": "call_1", "output": "a.txt"},
            {"type": "function_call_output", "call_id": "call_2", "output": "b.txt"}
        ]);
        let req = req_from_input(input);
        let msgs = build_messages(&req.input_items().unwrap()).unwrap();
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content.len(), 2);
        assert!(msgs[1]
            .content
            .iter()
            .all(|c| matches!(c, MessageContent::ToolUse { .. })));
        assert_eq!(msgs[2].role, "user");
        assert_eq!(msgs[2].content.len(), 2);
        assert!(msgs[2]
            .content
            .iter()
            .all(|c| matches!(c, MessageContent::ToolResult { .. })));
    }

    #[test]
    fn thinking_blocks_ordered_first_in_merged_assistant() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "q"}]},
            {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "answer"}]},
            {"type": "reasoning", "id": "rs", "content": [{"type": "reasoning_text", "text": "think"}]},
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{}"}
        ]);
        let req = req_from_input(input);
        let msgs = build_messages(&req.input_items().unwrap()).unwrap();
        assert_eq!(msgs.len(), 2);
        let assistant = &msgs[1];
        assert!(matches!(
            assistant.content[0],
            MessageContent::Thinking { .. }
        ));
        assert!(assistant
            .content
            .iter()
            .any(|c| matches!(c, MessageContent::Text { ref text } if text == "answer")));
    }

    #[test]
    fn rejects_message_not_starting_with_user() {
        let input = serde_json::json!([
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{}"}
        ]);
        let req = req_from_input(input);
        let err = build_messages(&req.input_items().unwrap()).unwrap_err();
        assert!(err.contains("user"));
    }

    #[test]
    fn rejects_function_call_output_with_unknown_call_id() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "q"}]},
            {"type": "function_call_output", "call_id": "ghost", "output": "x"}
        ]);
        let req = req_from_input(input);
        let err = build_messages(&req.input_items().unwrap()).unwrap_err();
        assert!(err.contains("ghost"));
    }

    #[test]
    fn rejects_empty_input() {
        let req = req_from_input(serde_json::json!([]));
        let err = build_messages(&req.input_items().unwrap()).unwrap_err();
        assert!(err.contains("输入为空"));
    }

    #[test]
    fn effort_is_case_insensitive() {
        let mut req = req_from_input(simple_user_input());
        req.reasoning = Some(ReasoningParams {
            effort: Some("HIGH".into()),
        });
        req.max_output_tokens = Some(65536);
        let out = build_request(&req, &provider(), "claude").unwrap();
        assert_eq!(out.output_config.unwrap().effort.as_deref(), Some("high"));
        assert!(out.thinking.is_none());
    }

    #[test]
    fn extracts_text_and_error_from_object_output() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "q"}]},
            {"type": "function_call", "call_id": "call_1", "name": "shell", "arguments": "{}"},
            {"type": "function_call_output", "call_id": "call_1", "output": {"output": "result line", "is_error": false}}
        ]);
        let req = req_from_input(input);
        let msgs = build_messages(&req.input_items().unwrap()).unwrap();
        let user = msgs.last().unwrap();
        match &user.content[0] {
            MessageContent::ToolResult {
                content, is_error, ..
            } => {
                assert_eq!(content, "result line");
                assert_eq!(*is_error, Some(false));
            }
            _ => panic!("expected tool_result"),
        }
    }

    #[test]
    fn replays_reasoning_signature_into_thinking_block() {
        let input = serde_json::json!([
            {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "q"}]},
            {"type": "reasoning", "id": "rs", "content": [{"type": "reasoning_text", "text": "plan"}], "encrypted_content": "sig123"}
        ]);
        let req = req_from_input(input);
        let msgs = build_messages(&req.input_items().unwrap()).unwrap();
        match &msgs[1].content[0] {
            MessageContent::Thinking {
                thinking,
                signature,
            } => {
                assert_eq!(thinking, "plan");
                assert_eq!(signature.as_deref(), Some("sig123"));
            }
            other => panic!("expected thinking block, got {other:?}"),
        }
    }

    #[test]
    fn maps_text_format_json_schema_to_output_config() {
        let mut req = req_from_input(simple_user_input());
        req.text = Some(serde_json::json!({
            "format": {"type": "json_schema", "name": "answer", "schema": {"type": "object"}}
        }));
        let out = build_request(&req, &provider(), "claude").unwrap();
        let value = serde_json::to_value(out.output_config.expect("output_config")).unwrap();
        assert_eq!(value["format"]["type"], "json_schema");
        assert_eq!(value["format"]["schema"]["type"], "object");
    }

    #[test]
    fn reasoning_signature_and_fragmented_usage_are_preserved() {
        let mut translator = ResponseTranslator::new("resp_1".into(), "m".into());
        translator.feed(UpstreamEvent::Usage {
            input_tokens: Some(10),
            output_tokens: None,
            reasoning_tokens: None,
            cached_tokens: Some(3),
        });
        translator.feed(UpstreamEvent::ItemStart {
            index: 0,
            kind: ItemKind::Reasoning,
        });
        translator.feed(UpstreamEvent::ThinkingDelta {
            index: 0,
            delta: "hmm".into(),
        });
        translator.feed(UpstreamEvent::ReasoningData {
            index: 0,
            data: "sig".into(),
        });
        let stopped = translator.feed(UpstreamEvent::ItemStop { index: 0 });
        assert!(stopped
            .iter()
            .any(|(name, _)| name == "response.output_item.done"));
        translator.feed(UpstreamEvent::Usage {
            input_tokens: None,
            output_tokens: Some(5),
            reasoning_tokens: Some(2),
            cached_tokens: None,
        });
        let done = translator.feed(UpstreamEvent::Done {
            reason: "stop".into(),
        });
        let (_, completed) = done
            .iter()
            .find(|(name, _)| name == "response.completed")
            .expect("response.completed");
        let response = &completed["response"];
        assert_eq!(response["usage"]["input_tokens"], 10);
        assert_eq!(response["usage"]["output_tokens"], 5);
        assert_eq!(response["usage"]["total_tokens"], 15);
        assert_eq!(
            response["usage"]["input_tokens_details"]["cached_tokens"],
            3
        );
        assert_eq!(
            response["usage"]["output_tokens_details"]["reasoning_tokens"],
            2
        );
        assert_eq!(response["output"][0]["encrypted_content"], "sig");
    }
}
