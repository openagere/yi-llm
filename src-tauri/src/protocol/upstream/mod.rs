pub mod anthropic;
pub mod client;
pub mod headers;
pub mod openai_chat;
pub mod responses;

/// Normalized upstream event stream. All adapters translate into this.
#[derive(Debug, Clone, PartialEq)]
pub enum UpstreamEvent {
    /// Start of an output item at the given output index.
    ItemStart {
        index: usize,
        kind: ItemKind,
    },
    TextDelta {
        index: usize,
        delta: String,
    },
    ThinkingDelta {
        index: usize,
        delta: String,
    },
    ToolCallStart {
        index: usize,
        call_id: String,
        name: String,
    },
    ToolCallArgsDelta {
        index: usize,
        delta: String,
    },
    ItemStop {
        index: usize,
    },
    /// Encrypted thinking payload (Anthropic `redacted_thinking` data or
    /// `signature_delta`). Must be echoed back on the next turn so the
    /// upstream can resume interrupted or redacted thinking chains.
    ReasoningData {
        index: usize,
        data: String,
    },
    Usage {
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
        reasoning_tokens: Option<u32>,
        cached_tokens: Option<u32>,
    },
    /// Terminal marker. reason: "stop" | "tool_calls" | "length" | "error"
    Done {
        reason: String,
    },
    RawError {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    Message,
    Reasoning,
    FunctionCall,
}

pub use client::{upstream_client, EventStream, UpstreamCall, UpstreamClient, UpstreamError};
