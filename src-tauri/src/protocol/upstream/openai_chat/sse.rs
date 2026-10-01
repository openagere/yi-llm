use crate::protocol::upstream::openai_chat::types::ChatChunk;
use crate::protocol::upstream::{ItemKind, UpstreamEvent};
use futures::{Stream, StreamExt};
use std::collections::{HashMap, HashSet};

pub fn translate_sse<S>(stream: S) -> impl Stream<Item = UpstreamEvent>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    async_stream::stream! {
        let mut stream = Box::pin(stream);
        let mut pending = String::new();
        let mut text_started = HashSet::new();
        let mut reasoning_started = HashSet::new();
        let mut tool_started = HashSet::new();
        let mut tool_names: HashMap<usize, String> = HashMap::new();
        let mut text_output: HashMap<usize, usize> = HashMap::new();
        let mut reasoning_output: HashMap<usize, usize> = HashMap::new();
        let mut tool_output: HashMap<usize, usize> = HashMap::new();
        let mut next_output = 0usize;
        let mut finished = false;
        while let Some(Ok(bytes)) = stream.next().await {
            pending.push_str(&String::from_utf8_lossy(&bytes));
            if pending.contains("\r\n") { pending = pending.replace("\r\n", "\n"); }
            while let Some(end) = pending.find("\n\n") {
                let frame = pending[..end].to_string();
                pending.drain(..end + 2);
                for line in frame.lines().filter_map(|line| line.strip_prefix("data:")) {
                    let data = line.trim();
                    if data == "[DONE]" { finished = true; yield UpstreamEvent::Done { reason: "stop".into() }; break; }
                    let Ok(chunk) = serde_json::from_str::<ChatChunk>(data) else { continue };
                    let mut done_reason = None;
                    for choice in chunk.choices {
                        if let Some(text) = choice.delta.content {
                            if !text.is_empty() {
                                let output = *text_output.entry(choice.index).or_insert_with(|| { let value = next_output; next_output += 1; value });
                                if text_started.insert(output) { yield UpstreamEvent::ItemStart { index: output, kind: ItemKind::Message }; }
                                yield UpstreamEvent::TextDelta { index: output, delta: text };
                            }
                        }
                        if let Some(reasoning) = choice.delta.reasoning {
                            if !reasoning.is_empty() {
                                let output = *reasoning_output.entry(choice.index).or_insert_with(|| { let value = next_output; next_output += 1; value });
                                if reasoning_started.insert(output) { yield UpstreamEvent::ItemStart { index: output, kind: ItemKind::Reasoning }; }
                                yield UpstreamEvent::ThinkingDelta { index: output, delta: reasoning };
                            }
                        }
                        for call in choice.delta.tool_calls.unwrap_or_default() {
                            let name = call.function.as_ref().and_then(|f| f.name.clone());
                            if let Some(name) = name { tool_names.insert(call.index, name); }
                            if let Some(id) = call.id {
                                let output = *tool_output.entry(call.index).or_insert_with(|| { let value = next_output; next_output += 1; value });
                                if tool_started.insert(output) {
                                    yield UpstreamEvent::ItemStart { index: output, kind: ItemKind::FunctionCall };
                                    yield UpstreamEvent::ToolCallStart { index: output, call_id: id, name: tool_names.get(&call.index).cloned().unwrap_or_default() };
                                }
                            }
                            if let Some(args) = call.function.and_then(|f| f.arguments) {
                                if let Some(output) = tool_output.get(&call.index) { yield UpstreamEvent::ToolCallArgsDelta { index: *output, delta: args }; }
                            }
                        }
                        if let Some(reason) = choice.finish_reason {
                            done_reason = Some(match reason.as_str() { "tool_calls" => "tool_calls", "length" => "length", _ => "stop" });
                        }
                    }
                    if let Some(usage) = chunk.usage { yield UpstreamEvent::Usage { input_tokens: Some(usage.prompt_tokens), output_tokens: Some(usage.completion_tokens), reasoning_tokens: usage.completion_tokens_details.and_then(|v| v.reasoning_tokens), cached_tokens: usage.prompt_tokens_details.and_then(|v| v.cached_tokens) }; }
                    if let Some(reason) = done_reason {
                        let mut indices: Vec<_> = reasoning_started.drain().collect();
                        indices.extend(text_started.drain());
                        indices.extend(tool_started.drain());
                        indices.sort_unstable();
                        indices.dedup();
                        for index in indices { yield UpstreamEvent::ItemStop { index }; }
                        finished = true;
                        yield UpstreamEvent::Done { reason: reason.into() };
                    }
                }
                if finished { break; }
            }
            if finished { break; }
        }
        if !finished { yield UpstreamEvent::RawError { message: "上游 SSE 流意外结束".into() }; }
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
    fn reasoning_content_delta_maps_to_thinking() {
        let fixture = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"answer\"}}],\"usage\":null}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":2,\"total_tokens\":5,\"prompt_tokens_details\":{\"cached_tokens\":1}}}\n\n",
            "data: [DONE]\n\n"
        );
        let events = collect(fixture);
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::ThinkingDelta { delta, .. } if delta == "think"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::TextDelta { delta, .. } if delta == "answer"
        )));
        assert!(events.iter().any(|e| matches!(
            e,
            UpstreamEvent::Usage {
                cached_tokens: Some(1),
                ..
            }
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e, UpstreamEvent::Done { .. })));
    }
}
