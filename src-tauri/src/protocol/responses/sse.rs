use crate::protocol::upstream::{ItemKind, UpstreamEvent};
use futures::{Stream, StreamExt};
use serde_json::Value;

pub fn translate_sse<S>(stream: S) -> impl Stream<Item = UpstreamEvent>
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    async_stream::stream! {
        let mut stream = Box::pin(stream);
        let mut pending = String::new();
        let mut kinds = std::collections::HashMap::new();
        let mut finished = false;
        while let Some(chunk) = stream.next().await {
            let Ok(bytes) = chunk else {
                yield UpstreamEvent::RawError { message: "读取上游 Responses 流失败".into() };
                finished = true;
                break;
            };
            pending.push_str(&String::from_utf8_lossy(&bytes));
            if pending.contains("\r\n") { pending = pending.replace("\r\n", "\n"); }
            while let Some(end) = pending.find("\n\n") {
                let frame = pending[..end].to_owned();
                pending.drain(..end + 2);
                let event_name = frame.lines().find_map(|line| line.strip_prefix("event:")).map(str::trim).unwrap_or("");
                let data = frame.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim).collect::<Vec<_>>().join("\n");
                if data == "[DONE]" {
                    yield UpstreamEvent::Done { reason: "stop".into() };
                    finished = true;
                    break;
                }
                let Ok(value) = serde_json::from_str::<Value>(&data) else { continue };
                let kind = value.get("type").and_then(Value::as_str).unwrap_or(event_name);
                match kind {
                    "response.output_item.added" => {
                        let index = value.get("output_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        let item = value.get("item").unwrap_or(&Value::Null);
                        match item.get("type").and_then(Value::as_str).unwrap_or("") {
                            "message" => { kinds.insert(index, ItemKind::Message); yield UpstreamEvent::ItemStart { index, kind: ItemKind::Message }; }
                            "reasoning" => { kinds.insert(index, ItemKind::Reasoning); yield UpstreamEvent::ItemStart { index, kind: ItemKind::Reasoning }; }
                            "function_call" => {
                                kinds.insert(index, ItemKind::FunctionCall);
                                yield UpstreamEvent::ItemStart { index, kind: ItemKind::FunctionCall };
                                yield UpstreamEvent::ToolCallStart {
                                    index,
                                    call_id:item.get("call_id").and_then(Value::as_str).unwrap_or("").to_owned(),
                                    name:item.get("name").and_then(Value::as_str).unwrap_or("").to_owned(),
                                };
                            }
                            _ => {}
                        }
                    }
                    "response.output_text.delta" => yield UpstreamEvent::TextDelta {
                        index:value.get("output_index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        delta:value.get("delta").and_then(Value::as_str).unwrap_or("").to_owned(),
                    },
                    "response.reasoning_summary_text.delta" => yield UpstreamEvent::ThinkingDelta {
                        index:value.get("output_index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        delta:value.get("delta").and_then(Value::as_str).unwrap_or("").to_owned(),
                    },
                    "response.function_call_arguments.delta" => yield UpstreamEvent::ToolCallArgsDelta {
                        index:value.get("output_index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        delta:value.get("delta").and_then(Value::as_str).unwrap_or("").to_owned(),
                    },
                    "response.output_item.done" => {
                        let index = value.get("output_index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        if kinds.remove(&index).is_some() { yield UpstreamEvent::ItemStop { index }; }
                    }
                    "response.completed" | "response.incomplete" => {
                        let response = value.get("response").unwrap_or(&Value::Null);
                        if let Some(usage) = response.get("usage") {
                            yield UpstreamEvent::Usage {
                                input_tokens:usage.get("input_tokens").and_then(Value::as_u64).map(|v| v as u32),
                                output_tokens:usage.get("output_tokens").and_then(Value::as_u64).map(|v| v as u32),
                                cached_tokens:usage.get("input_tokens_details").and_then(|v| v.get("cached_tokens")).and_then(Value::as_u64).map(|v| v as u32),
                                reasoning_tokens:usage.get("output_tokens_details").and_then(|v| v.get("reasoning_tokens")).and_then(Value::as_u64).map(|v| v as u32),
                            };
                        }
                        yield UpstreamEvent::Done { reason:if kind == "response.incomplete" { "length" } else { "stop" }.into() };
                        finished = true;
                        break;
                    }
                    "response.failed" | "error" => {
                        yield UpstreamEvent::RawError {
                            message:value.pointer("/response/error/message").or_else(|| value.pointer("/error/message")).and_then(Value::as_str).unwrap_or("上游 Responses 请求失败").to_owned(),
                        };
                        finished = true;
                        break;
                    }
                    _ => {}
                }
            }
            if finished { break; }
        }
        if !finished { yield UpstreamEvent::RawError { message:"上游 Responses 流提前结束".into() }; }
    }
}
