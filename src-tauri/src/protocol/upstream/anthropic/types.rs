use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RequestMetadata>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub role: String,
    pub content: Vec<MessageContent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThinkingConfig {
    #[serde(rename = "type")]
    pub thinking_type: String, // "enabled"
    pub budget_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OutputFormat>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputFormat {
    #[serde(rename = "json_schema")]
    JsonSchema { schema: serde_json::Value },
}

#[derive(Debug, Clone, Serialize)]
pub struct AnthropicTool {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ToolChoice {
    #[serde(rename = "auto")]
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "any")]
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "none")]
    None,
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ─── Streaming wire events ─────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SseEvent {
    MessageStart {
        message: MessageStartBody,
    },
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: usize,
        delta: ContentDelta,
    },
    ContentBlockStop {
        index: usize,
    },
    MessageDelta {
        delta: MessageDeltaBody,
        usage: Option<UsageBody>,
    },
    MessageStop,
    Ping,
    Error {
        error: ApiErrorBody,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageStartBody {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub usage: Option<UsageBody>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Option<serde_json::Value>,
    },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentDelta {
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "signature_delta")]
    SignatureDelta { signature: String },
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageDeltaBody {
    #[serde(default, rename = "stop_reason")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UsageBody {
    #[serde(default)]
    pub input_tokens: Option<u32>,
    #[serde(default)]
    pub output_tokens: Option<u32>,
    #[serde(default, rename = "cache_read_input_tokens")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(default, rename = "cache_creation_input_tokens")]
    pub cache_creation_input_tokens: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    #[serde(default, rename = "type")]
    pub error_type: String,
    pub message: String,
}

// ─── Non-streaming response ────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesResponse {
    pub content: Vec<ResponseContentBlock>,
    pub usage: ResponseUsage,
    #[serde(default, rename = "stop_reason")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(default, rename = "cache_read_input_tokens")]
    pub cache_read_input_tokens: u32,
    #[serde(default, rename = "cache_creation_input_tokens")]
    pub cache_creation_input_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_full_request() {
        let req = MessagesRequest {
            model: "claude-sonnet-4-5".into(),
            messages: vec![Message {
                role: "user".into(),
                content: vec![MessageContent::Text { text: "hi".into() }],
            }],
            system: Some(SystemPrompt::Text("sys".into())),
            max_tokens: 16384,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            thinking: Some(ThinkingConfig {
                thinking_type: "enabled".into(),
                budget_tokens: 32000,
            }),
            output_config: None,
            tools: Some(vec![AnthropicTool {
                tool_type: None,
                name: "shell".into(),
                description: Some("run".into()),
                input_schema: Some(serde_json::json!({"type": "object"})),
                max_uses: None,
            }]),
            tool_choice: Some(ToolChoice::Auto {
                disable_parallel_tool_use: None,
            }),
            stream: true,
            metadata: Some(RequestMetadata {
                user_id: Some("cache-key".into()),
            }),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["system"], "sys");
        assert_eq!(v["messages"][0]["content"][0]["type"], "text");
        assert_eq!(v["thinking"]["budget_tokens"], 32000);
        assert_eq!(v["tools"][0]["input_schema"]["type"], "object");
        assert_eq!(v["metadata"]["user_id"], "cache-key");
    }

    #[test]
    fn parses_sse_events() {
        let ev: SseEvent = serde_json::from_str(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}"#,
        )
        .unwrap();
        assert!(matches!(ev, SseEvent::ContentBlockStart { index: 0, .. }));

        let ev: SseEvent = serde_json::from_str(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"input_tokens":10}}"#,
        )
        .unwrap();
        assert!(matches!(ev, SseEvent::MessageDelta { .. }));
    }

    #[test]
    fn parses_real_shaped_events() {
        let ev: SseEvent = serde_json::from_str(
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"shell"}}"#,
        )
        .unwrap();
        match ev {
            SseEvent::ContentBlockStart {
                index,
                content_block,
            } => {
                assert_eq!(index, 1);
                assert!(
                    matches!(content_block, ContentBlock::ToolUse { id, name, input: None } if id == "toolu_1" && name == "shell")
                );
            }
            _ => panic!("expected ContentBlockStart"),
        }

        let ev: SseEvent = serde_json::from_str(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig1"}}"#,
        )
        .unwrap();
        assert!(
            matches!(ev, SseEvent::ContentBlockDelta { delta: ContentDelta::SignatureDelta { ref signature }, .. } if signature == "sig1")
        );

        let ev: SseEvent = serde_json::from_str(
            r#"{"type":"message_start","message":{"model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":1}}}"#,
        )
        .unwrap();
        match ev {
            SseEvent::MessageStart { message } => {
                assert_eq!(message.model.as_deref(), Some("claude-sonnet-4-5"));
                let usage = message.usage.expect("message_start must carry usage");
                assert_eq!(usage.input_tokens, Some(10));
                assert_eq!(usage.output_tokens, Some(1));
            }
            _ => panic!("expected MessageStart"),
        }
    }
}
