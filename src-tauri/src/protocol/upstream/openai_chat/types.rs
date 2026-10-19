use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ChatToolChoiceWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ChatResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub stream: bool,
    pub stream_options: StreamOptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    /// Reasoning text replayed for multi-turn continuity on providers that
    /// accept a `reasoning_content` field on assistant messages (DeepSeek
    /// style). OpenAI ignores the unknown field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
}

/// The role a Chat Completions upstream sees.
///
/// Codex sends its instructions as a `developer` message, but only OpenAI's own
/// models implement the newer `developer` role: most OpenAI-compatible providers
/// accept just the classic `system` / `user` / `assistant` / `tool` / `function`
/// set and reject the whole request with 400 when they see anything else. The
/// instruction text is preserved; only the role name is expressed the classic way.
pub fn upstream_role(role: &str) -> &str {
    match role {
        "developer" => "system",
        other => other,
    }
}

/// Apply [`upstream_role`] to every message role of a raw Chat Completions
/// request body. Codex sends its instructions as a `developer` message, and a
/// same-protocol forward would otherwise hand that role to an upstream that
/// only implements the classic role set.
pub fn express_developer_messages_as_system(body: &mut serde_json::Value) {
    let Some(messages) = body
        .get_mut("messages")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for message in messages {
        let Some(role) = message
            .get("role")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let mapped = upstream_role(&role);
        if mapped != role.as_str() {
            message["role"] = serde_json::Value::String(mapped.to_owned());
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String, // "json_schema"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub json_schema: Option<ChatJsonSchemaFormat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatJsonSchemaFormat {
    pub name: String,
    pub schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String,
    pub function: ChatFunctionCall,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatFunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ChatFunctionDef,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatFunctionDef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ChatToolChoiceWire {
    Mode(String),
    Function {
        #[serde(rename = "type")]
        choice_type: String,
        function: ChatFunctionChoiceName,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatFunctionChoiceName {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StreamOptions {
    pub include_usage: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatChunk {
    #[serde(default)]
    pub choices: Vec<ChatChoice>,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ChatChoice {
    pub index: usize,
    #[serde(default)]
    pub delta: ChatDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChatDelta {
    #[serde(default)]
    pub content: Option<String>,
    /// Reasoning deltas (`reasoning_content` on DeepSeek-style providers,
    /// `reasoning` on others) surface as thinking output.
    #[serde(default, alias = "reasoning_content")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub tool_calls: Option<Vec<ChatToolCallDelta>>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ChatToolCallDelta {
    pub index: usize,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub function: Option<ChatFunctionDelta>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ChatFunctionDelta {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arguments: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    #[serde(default)]
    pub prompt_tokens_details: Option<PromptTokenDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<CompletionDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PromptTokenDetails {
    #[serde(default)]
    pub cached_tokens: Option<u32>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u32>,
}
