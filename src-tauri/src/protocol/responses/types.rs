use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── Request ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ResponsesRequest {
    pub model: String,
    #[serde(default)]
    pub instructions: Option<Value>,
    #[serde(default)]
    pub input: Value, // string or array of InputItem
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub reasoning: Option<ReasoningParams>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub store: Option<bool>,
    #[serde(default, rename = "max_output_tokens")]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub stop: Option<Value>,
    #[serde(default)]
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default, rename = "top_p")]
    pub top_p: Option<f32>,
    /// `text.format` — e.g. `{"format": {"type": "json_schema", ...}}`.
    /// Kept as raw JSON because only the schema is forwarded upstream.
    #[serde(default)]
    pub text: Option<Value>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
    #[serde(default, rename = "prompt_cache_key")]
    pub prompt_cache_key: Option<String>,
    #[serde(default, rename = "previous_response_id")]
    pub previous_response_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        parameters: Option<Value>,
    },
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    Custom {
        name: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        format: Option<Value>,
    },
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    Object {
        #[serde(rename = "type")]
        kind: String,
        name: Option<String>,
    },
    Kind(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReasoningParams {
    #[serde(default)]
    pub effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    Message {
        role: String,
        #[serde(default)]
        content: Vec<InputContent>,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput { call_id: String, output: Value },
    Reasoning {
        #[serde(default)]
        id: String,
        #[serde(default)]
        content: Option<Vec<ReasoningContent>>,
        #[serde(default)]
        summary: Option<Vec<ReasoningContent>>,
        #[serde(default, rename = "encrypted_content")]
        encrypted_content: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    InputText {
        text: String,
    },
    Text {
        text: String,
    },
    OutputText {
        text: String,
    },
    InputImage {
        image_url: String,
        #[serde(default)]
        detail: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningContent {
    #[serde(alias = "summary_text")]
    ReasoningText { text: String },
    #[serde(other)]
    Unknown,
}

impl ResponsesRequest {
    /// Normalize input to a list of items. A plain string input becomes one user message.
    pub fn input_items(&self) -> Result<Vec<InputItem>, String> {
        if let Some(s) = self.input.as_str() {
            return Ok(vec![InputItem::Message {
                role: "user".into(),
                content: vec![InputContent::InputText {
                    text: s.to_string(),
                }],
            }]);
        }
        let items = self
            .input
            .as_array()
            .ok_or_else(|| "input 必须是文本或消息数组".to_string())?;
        let normalized = items
            .iter()
            .map(|item| {
                let mut item = item.clone();
                if item.get("type").and_then(Value::as_str) == Some("message") {
                    if let Some(text) = item
                        .get("content")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                    {
                        item["content"] = serde_json::json!([{"type":"input_text","text":text}]);
                    }
                }
                item
            })
            .collect::<Vec<_>>();
        normalized
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                serde_json::from_value::<InputItem>(item)
                    .map_err(|error| format!("input[{index}] 格式无效: {error}"))
            })
            .collect()
    }

    pub fn unsupported_tool_names(&self) -> Vec<String> {
        self.tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter_map(|t| match t {
                Tool::Function { name, .. } => Some(name.clone()),
                Tool::WebSearch | Tool::WebSearchPreview | Tool::Custom { .. } => None,
                Tool::Unsupported => None,
            })
            .collect()
    }

    pub fn has_unsupported_tools(&self) -> bool {
        self.tools
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|t| matches!(t, Tool::Unsupported))
    }

    /// Extract a JSON schema from `text.format` when the client requests
    /// structured output. Returns `(name, schema)` where name falls back to
    /// a stable default when the client omits it.
    pub fn json_schema(&self) -> Option<(String, serde_json::Value)> {
        let format = self.text.as_ref()?.get("format")?;
        if format.get("type").and_then(Value::as_str) != Some("json_schema") {
            return None;
        }
        let schema = format.get("schema")?.clone();
        let name = format
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("yi_llm_output")
            .to_string();
        Some((name, schema))
    }
}

pub fn custom_tool_parameters() -> Value {
    serde_json::json!({
        "type":"object",
        "properties":{"input":{"type":"string"}},
        "required":["input"],
        "additionalProperties":false
    })
}

pub fn custom_tool_description(
    description: Option<&str>,
    format: Option<&Value>,
) -> Option<String> {
    let mut parts = description
        .map(str::to_owned)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(format) = format {
        parts.push(format!(
            "Pass the custom tool input as raw text matching this format: {format}"
        ));
    } else {
        parts.push("Pass the custom tool input as raw text in the input field.".into());
    }
    Some(parts.join("\n\n"))
}

pub fn custom_tool_input(arguments: &str) -> String {
    serde_json::from_str::<Value>(arguments)
        .ok()
        .and_then(|value| {
            value
                .get("input")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| arguments.to_owned())
}

// ─── Response object ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ResponseObject {
    pub id: String,
    pub object: String,
    pub created_at: u64,
    pub model: String,
    pub status: String, // in_progress | completed | incomplete | failed
    pub output: Vec<OutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl ResponseObject {
    pub fn shell(id: &str, model: &str) -> Self {
        ResponseObject {
            id: id.into(),
            object: "response".into(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            model: model.into(),
            status: "in_progress".into(),
            output: vec![],
            usage: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem {
    Message {
        id: String,
        role: String,
        status: String,
        content: Vec<OutputContent>,
    },
    Reasoning {
        id: String,
        status: String,
        content: Vec<ReasoningContentOut>,
        summary: Vec<serde_json::Value>,
        /// Anthropic thinking signature / redacted thinking payload. Echoed
        /// back by clients on the next turn inside a reasoning input item.
        #[serde(skip_serializing_if = "Option::is_none", rename = "encrypted_content")]
        encrypted_content: Option<String>,
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        namespace: Option<String>,
        arguments: String,
        status: String,
    },
    CustomToolCall {
        id: String,
        call_id: String,
        name: String,
        input: String,
        status: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContent {
    OutputText {
        text: String,
        annotations: Vec<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningContentOut {
    ReasoningText { text: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens_details: OutputTokensDetails,
}

#[derive(Debug, Clone, Serialize)]
pub struct InputTokensDetails {
    pub cached_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub message: String,
    pub code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_shaped_request() {
        let json = serde_json::json!({
            "model": "gpt-5-codex",
            "instructions": "You are a coding agent.",
            "store": false,
            "stream": true,
            "include": ["reasoning.encrypted_content"],
            "reasoning": { "effort": "high" },
            "prompt_cache_key": "abc",
            "input": [
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "fix the bug"}]},
                {"type": "reasoning", "id": "rs_1", "content": [
                    {"type": "reasoning_text", "text": "thinking..."}]},
                {"type": "function_call", "call_id": "call_1", "name": "shell",
                 "arguments": "{\"cmd\":\"ls\"}"},
                {"type": "function_call_output", "call_id": "call_1",
                 "output": "file.txt"}
            ],
            "tools": [
                {"type": "function", "name": "shell", "description": "run shell",
                 "parameters": {"type": "object"}},
                {"type": "computer_use_preview"}
            ],
            "tool_choice": {"type": "auto"}
        });
        let req: ResponsesRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.model, "gpt-5-codex");
        assert_eq!(req.input_items().unwrap().len(), 4);
        assert!(req.has_unsupported_tools());
        let names = req.unsupported_tool_names();
        assert_eq!(names, vec!["shell".to_string()]);
    }

    #[test]
    fn string_input_becomes_user_message() {
        let req: ResponsesRequest =
            serde_json::from_value(serde_json::json!({"model": "m", "input": "hi"})).unwrap();
        let items = req.input_items().unwrap();
        assert!(matches!(&items[0], InputItem::Message { role, .. } if role == "user"));
    }
}
