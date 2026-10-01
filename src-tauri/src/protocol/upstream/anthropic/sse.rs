use crate::protocol::upstream::anthropic::types::{ContentBlock, ContentDelta, SseEvent};
use crate::protocol::upstream::{ItemKind, UpstreamEvent};
use futures::{Stream, StreamExt};

pub fn translate_sse<S>(stream: S) -> impl Stream<Item = UpstreamEvent>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    async_stream::stream! {
        let mut stream = Box::pin(stream);
        let mut pending = String::new();
        let mut stop_reason = String::new();
        let mut saw_stop = false;
        while let Some(Ok(bytes)) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&bytes));
            if pending.contains("\r\n") { pending = pending.replace("\r\n", "\n"); }
            while let Some(end) = pending.find("\n\n") {
                let frame = pending[..end].to_string();
                pending.drain(..end + 2);
                let data = frame.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim).collect::<Vec<_>>().join("\n");
                if data.is_empty() { continue; }
                let Ok(event) = serde_json::from_str::<SseEvent>(&data) else { continue };
                match event {
                    SseEvent::MessageStart { message } => {
                        // Anthropic reports input tokens (and cache creation) on
                        // message_start; output tokens only become final in
                        // message_delta. The placeholder output_tokens on this
                        // event is intentionally ignored.
                        if let Some(u) = message.usage {
                            yield UpstreamEvent::Usage {
                                input_tokens: u.input_tokens,
                                output_tokens: None,
                                reasoning_tokens: None,
                                cached_tokens: u.cache_read_input_tokens,
                            };
                        }
                    }
                    SseEvent::ContentBlockStart { index, content_block } => match content_block {
                        ContentBlock::Thinking { .. } => yield UpstreamEvent::ItemStart { index, kind: ItemKind::Reasoning },
                        ContentBlock::Text { .. } => yield UpstreamEvent::ItemStart { index, kind: ItemKind::Message },
                        ContentBlock::ToolUse { id, name, input } => {
                            yield UpstreamEvent::ItemStart { index, kind: ItemKind::FunctionCall };
                            yield UpstreamEvent::ToolCallStart { index, call_id: id, name };
                            if let Some(input) = input.filter(|v| v.as_object().is_some_and(|o| !o.is_empty())) {
                                yield UpstreamEvent::ToolCallArgsDelta { index, delta: input.to_string() };
                            }
                        }
                        ContentBlock::RedactedThinking { data } => {
                            yield UpstreamEvent::ItemStart { index, kind: ItemKind::Reasoning };
                            yield UpstreamEvent::ReasoningData { index, data };
                        }
                    },
                    SseEvent::ContentBlockDelta { index, delta } => match delta {
                        ContentDelta::ThinkingDelta { thinking } => yield UpstreamEvent::ThinkingDelta { index, delta: thinking },
                        ContentDelta::TextDelta { text } => yield UpstreamEvent::TextDelta { index, delta: text },
                        ContentDelta::InputJsonDelta { partial_json } => yield UpstreamEvent::ToolCallArgsDelta { index, delta: partial_json },
                        ContentDelta::SignatureDelta { signature } => yield UpstreamEvent::ReasoningData { index, data: signature },
                    },
                    SseEvent::ContentBlockStop { index } => yield UpstreamEvent::ItemStop { index },
                    SseEvent::MessageDelta { delta, usage } => {
                        if let Some(reason) = delta.stop_reason { stop_reason = reason; }
                        if let Some(u) = usage { yield UpstreamEvent::Usage { input_tokens: u.input_tokens, output_tokens: u.output_tokens, reasoning_tokens: None, cached_tokens: match (u.cache_read_input_tokens, u.cache_creation_input_tokens) { (None, None) => None, (read, created) => Some(read.unwrap_or(0).saturating_add(created.unwrap_or(0))) } }; }
                    }
                    SseEvent::MessageStop => { saw_stop = true; yield UpstreamEvent::Done { reason: match stop_reason.as_str() { "tool_use" => "tool_calls", "max_tokens" => "length", _ => "stop" }.into() }; break; }
                    SseEvent::Error { error } => { yield UpstreamEvent::RawError { message: error.message }; break; }
                    _ => {}
                }
            }
            if saw_stop { break; }
        }
        if !saw_stop { yield UpstreamEvent::RawError { message: "上游 SSE 流意外结束".into() }; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(fixture: &'static str) -> Vec<UpstreamEvent> {
        let stream: Vec<Result<bytes::Bytes, reqwest::Error>> =
            vec![Ok(bytes::Bytes::from_static(fixture.as_bytes()))];
        tokio_test::block_on(translate_sse(futures::stream::iter(stream)).collect())
    }

    #[test]
    fn message_start_usage_and_signature_are_captured() {
        let fixture = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m1\",\"model\":\"claude\",\"usage\":{\"input_tokens\":7,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"sig1\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hmm\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":4}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let events = collect(fixture);
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::Usage {
                input_tokens: Some(7),
                output_tokens: None,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::ReasoningData { data, .. } if data == "sig1"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::Usage {
                output_tokens: Some(4),
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e, UpstreamEvent::Done { .. })));
    }

    #[test]
    fn redacted_thinking_is_preserved_as_reasoning_data() {
        let fixture = concat!(
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"ZW5j\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
        );
        let events = collect(fixture);
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::ItemStart {
                kind: ItemKind::Reasoning,
                ..
            }
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::ReasoningData { data, .. } if data == "ZW5j"
        )));
    }
}
