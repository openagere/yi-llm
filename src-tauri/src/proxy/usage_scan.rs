use serde_json::Value;
use std::collections::BTreeMap;

/// Upper bounds keep a misbehaving upstream from growing the proxy's memory.
const MAX_PENDING_FRAME: usize = 4 * 1024 * 1024;
const MAX_BUFFERED_BODY: usize = 32 * 1024 * 1024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub input: u32,
    pub output: u32,
    pub cached: u32,
    pub reasoning: u32,
}

#[derive(Clone, Copy)]
enum Kind {
    Chat,
    Anthropic,
    Responses,
}

/// Extracts token usage from a native upstream response that is forwarded untouched.
///
/// Streamed bodies are scanned frame by frame and only frames that can carry usage are
/// JSON-parsed; non-streamed bodies are buffered once and parsed at the end.
pub struct UsageScanner {
    kind: Kind,
    streaming: bool,
    pending: Vec<u8>,
    body: Vec<u8>,
    tally: Counts,
    observed_events: BTreeMap<String, u32>,
}

fn number(value: &Value, pointer: &str) -> Option<u32> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .map(|n| n as u32)
}

impl UsageScanner {
    pub fn new(protocol: &str, streaming: bool) -> Self {
        let kind = match protocol {
            "anthropic" => Kind::Anthropic,
            "openai_chat" => Kind::Chat,
            _ => Kind::Responses,
        };
        Self {
            kind,
            streaming,
            pending: Vec::new(),
            body: Vec::new(),
            tally: Counts::default(),
            observed_events: BTreeMap::new(),
        }
    }

    /// Feeds one chunk and returns every usage report completed by it.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<Counts> {
        if !self.streaming {
            if self.body.len() + chunk.len() <= MAX_BUFFERED_BODY {
                self.body.extend_from_slice(chunk);
            }
            return Vec::new();
        }
        self.pending.extend_from_slice(chunk);
        let mut reports = Vec::new();
        let mut consumed = 0;
        while let Some((end, delimiter)) = frame_end(&self.pending[consumed..]) {
            let frame = &self.pending[consumed..consumed + end];
            Self::observe_event(&mut self.observed_events, frame);
            if let Some(report) = scan_frame(self.kind, frame, &mut self.tally) {
                reports.push(report);
            }
            consumed += end + delimiter;
        }
        self.pending.drain(..consumed);
        if self.pending.len() > MAX_PENDING_FRAME {
            self.pending.clear();
        }
        reports
    }

    pub fn is_streaming(&self) -> bool {
        self.streaming
    }

    /// Event names observed in the stream, capped to keep diagnostics bounded.
    pub fn observed_events(&self) -> &BTreeMap<String, u32> {
        &self.observed_events
    }

    fn observe_event(events: &mut BTreeMap<String, u32>, frame: &[u8]) {
        let Ok(text) = std::str::from_utf8(frame) else {
            return;
        };
        let explicit_name = text
            .lines()
            .find_map(|line| line.strip_prefix("event:"))
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned);
        let payload_name = || {
            let data = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n");
            let value: Value = serde_json::from_str(&data).ok()?;
            value.get("type")?.as_str().map(str::to_owned)
        };
        let Some(name) = explicit_name.or_else(payload_name) else {
            return;
        };
        let name = name.chars().take(64).collect::<String>();
        if events.contains_key(&name) || events.len() < 16 {
            *events.entry(name).or_default() += 1;
        }
    }

    /// For non-streamed bodies: the usage found in the complete response.
    pub fn finish(&mut self) -> Option<Counts> {
        if self.streaming {
            return None;
        }
        let value: Value = serde_json::from_slice(&self.body).ok()?;
        let usage = value
            .get("usage")
            .or_else(|| value.pointer("/response/usage"))?;
        let get = |pointer: &str| number(usage, pointer).unwrap_or(0);
        Some(match self.kind {
            Kind::Anthropic => Counts {
                input: get("/input_tokens"),
                output: get("/output_tokens"),
                cached: get("/cache_read_input_tokens"),
                reasoning: 0,
            },
            Kind::Chat => Counts {
                input: get("/prompt_tokens"),
                output: get("/completion_tokens"),
                cached: get("/prompt_tokens_details/cached_tokens"),
                reasoning: get("/completion_tokens_details/reasoning_tokens"),
            },
            Kind::Responses => responses_counts(usage),
        })
    }
}

pub fn responses_counts(usage: &Value) -> Counts {
    let get = |pointer: &str| number(usage, pointer).unwrap_or(0);
    Counts {
        input: get("/input_tokens"),
        output: get("/output_tokens"),
        cached: get("/input_tokens_details/cached_tokens"),
        reasoning: get("/output_tokens_details/reasoning_tokens"),
    }
}

/// Position of the first blank line and the length of its delimiter.
fn frame_end(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while let Some(offset) = buffer[index..].iter().position(|byte| *byte == b'\n') {
        let at = index + offset;
        match (buffer.get(at + 1), buffer.get(at + 2)) {
            (Some(b'\n'), _) => return Some((at, 2)),
            (Some(b'\r'), Some(b'\n')) => return Some((at, 3)),
            _ => index = at + 1,
        }
    }
    None
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn scan_frame(kind: Kind, frame: &[u8], tally: &mut Counts) -> Option<Counts> {
    let relevant = match kind {
        Kind::Chat => contains(frame, b"\"usage\""),
        Kind::Anthropic => contains(frame, b"usage") || contains(frame, b"message_stop"),
        Kind::Responses => {
            contains(frame, b"response.completed") || contains(frame, b"response.incomplete")
        }
    };
    if !relevant {
        return None;
    }
    let text = std::str::from_utf8(frame).ok()?;
    let sse_event_name = text
        .lines()
        .find_map(|line| line.strip_prefix("event:"))
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let data = text
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    let value: Value = serde_json::from_str(&data).ok()?;
    let payload_event_name = value.get("type").and_then(Value::as_str).unwrap_or("");
    let event_name = sse_event_name.unwrap_or(payload_event_name);
    match kind {
        Kind::Chat => {
            let usage = value.get("usage").filter(|usage| usage.is_object())?;
            let get = |pointer: &str| number(usage, pointer).unwrap_or(0);
            *tally = Counts {
                input: get("/prompt_tokens"),
                output: get("/completion_tokens"),
                cached: get("/prompt_tokens_details/cached_tokens"),
                reasoning: get("/completion_tokens_details/reasoning_tokens"),
            };
            Some(*tally)
        }
        Kind::Anthropic => {
            let usage = if event_name.is_empty() {
                value.get("usage")
            } else {
                value
                    .get("usage")
                    .or_else(|| value.pointer("/message/usage"))
            };
            if let Some(usage) = usage {
                tally.input = number(usage, "/input_tokens").unwrap_or(tally.input);
                tally.output = number(usage, "/output_tokens").unwrap_or(tally.output);
                tally.cached = number(usage, "/cache_read_input_tokens")
                    .unwrap_or(tally.cached)
                    .saturating_add(number(usage, "/cache_creation_input_tokens").unwrap_or(0));
            }
            (value.get("type").and_then(Value::as_str) == Some("message_stop")).then_some(*tally)
        }
        Kind::Responses => {
            if !matches!(event_name, "response.completed" | "response.incomplete") {
                return None;
            }
            Some(responses_counts(value.get("response")?.get("usage")?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(scanner: &mut UsageScanner, chunks: &[&str]) -> Vec<Counts> {
        chunks
            .iter()
            .flat_map(|chunk| scanner.feed(chunk.as_bytes()))
            .collect()
    }

    #[test]
    fn chat_stream_reports_only_the_final_usage_chunk_even_when_split() {
        let mut scanner = UsageScanner::new("openai_chat", true);
        let reports = collect(
            &mut scanner,
            &[
                "data: {\"choices\":[],\"usage\":null}\n\n",
                "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"comp",
                "letion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":2}}}\r\n\r\ndata: [DONE]\n\n",
            ],
        );
        assert_eq!(
            reports,
            [Counts {
                input: 7,
                output: 3,
                cached: 2,
                reasoning: 0
            }]
        );
    }

    #[test]
    fn anthropic_stream_combines_start_and_delta_usage_at_message_stop() {
        let mut scanner = UsageScanner::new("anthropic", true);
        let reports = collect(
            &mut scanner,
            &[
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":11,\"cache_read_input_tokens\":4}}}\n\n",
                "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":9}}\n\n",
                "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            ],
        );
        assert_eq!(
            reports,
            [Counts {
                input: 11,
                output: 9,
                cached: 4,
                reasoning: 0
            }]
        );
    }

    #[test]
    fn responses_stream_reports_on_completion_events() {
        let mut scanner = UsageScanner::new("responses", true);
        let reports = collect(
            &mut scanner,
            &[
                "event: response.output_text.delta\ndata: {\"delta\":\"hi\"}\n\n",
                "event: response.completed\ndata: {\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":6,\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
            ],
        );
        assert_eq!(
            reports,
            [Counts {
                input: 5,
                output: 6,
                cached: 0,
                reasoning: 2
            }]
        );
    }

    #[test]
    fn responses_stream_uses_payload_type_when_sse_event_header_is_absent() {
        let mut scanner = UsageScanner::new("responses", true);
        let reports = collect(
            &mut scanner,
            &["data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":9,\"output_tokens\":2}}}\n\n"],
        );
        assert_eq!(
            reports,
            [Counts {
                input: 9,
                output: 2,
                cached: 0,
                reasoning: 0
            }]
        );
        assert_eq!(
            scanner.observed_events().get("response.completed"),
            Some(&1)
        );
    }

    #[test]
    fn responses_completion_without_usage_is_visible_in_diagnostics() {
        let mut scanner = UsageScanner::new("responses", true);
        let reports = collect(
            &mut scanner,
            &["event: response.completed\ndata: {\"response\":{\"status\":\"completed\"}}\n\n"],
        );
        assert!(reports.is_empty());
        assert_eq!(
            scanner.observed_events().get("response.completed"),
            Some(&1)
        );
    }

    #[test]
    fn non_streamed_bodies_are_parsed_once_at_the_end() {
        let mut scanner = UsageScanner::new("anthropic", false);
        assert!(scanner.feed(b"{\"usage\":{\"input_tokens\":3,").is_empty());
        assert!(scanner.feed(b"\"output_tokens\":4}}").is_empty());
        assert_eq!(
            scanner.finish(),
            Some(Counts {
                input: 3,
                output: 4,
                cached: 0,
                reasoning: 0
            })
        );
        assert_eq!(UsageScanner::new("openai_chat", false).finish(), None);
    }

    #[test]
    fn oversized_frames_are_discarded() {
        let mut scanner = UsageScanner::new("openai_chat", true);
        scanner.feed(&vec![b'x'; MAX_PENDING_FRAME + 1]);
        assert!(scanner.pending.is_empty());
    }
}
