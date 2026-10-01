use crate::{
    domain::provider::Provider,
    protocol::{
        responses::{sse as responses_sse, types::ResponsesRequest},
        translate::{anthropic as anthropic_translate, chat_completions as chat_translate},
        upstream::{anthropic, openai_chat, responses, UpstreamEvent},
    },
};
use futures::{future::BoxFuture, Stream};
use reqwest::header::HeaderMap;
use serde_json::{json, Value};
use std::pin::Pin;

pub type EventStream = Pin<Box<dyn Stream<Item = UpstreamEvent> + Send>>;

/// Why an upstream call could not produce an event stream.
#[derive(Debug)]
pub enum UpstreamError {
    /// The translated request was invalid; the caller sent something unsupported.
    BadRequest(String),
    /// The upstream was unreachable, timed out or rejected the request.
    Upstream(String),
}

/// Everything needed to call an upstream with an already-normalized Responses request.
pub struct UpstreamCall<'a> {
    pub provider: &'a Provider,
    pub upstream_model: &'a str,
    pub request: &'a ResponsesRequest,
    pub normalized_body: &'a Value,
}

/// One upstream wire protocol. Adding a protocol means adding one implementation and one
/// arm in [`upstream_client`]; routing and handlers stay untouched.
pub trait UpstreamClient: Send + Sync {
    /// Absolute URL for a native (same-protocol) request.
    fn endpoint(&self, base_url: &str) -> String;
    /// Provider-specific headers for a native request carrying `body`.
    fn native_headers(&self, provider: &Provider, body: &Value) -> HeaderMap;
    /// Minimal request used to verify a provider's credentials and model name.
    fn probe_body(&self, model: &str) -> Value;
    /// Translates the request to this protocol, calls the upstream and normalizes its stream.
    fn stream<'a>(
        &'a self,
        http: &'a reqwest::Client,
        call: UpstreamCall<'a>,
    ) -> BoxFuture<'a, Result<EventStream, UpstreamError>>;
}

pub fn upstream_client(provider_type: &str) -> Option<&'static dyn UpstreamClient> {
    match provider_type {
        "anthropic" => Some(&AnthropicUpstream),
        "openai_chat" => Some(&OpenAiChatUpstream),
        "responses" => Some(&ResponsesUpstream),
        _ => None,
    }
}

fn trimmed(base_url: &str) -> &str {
    base_url.trim_end_matches('/')
}

struct AnthropicUpstream;

impl UpstreamClient for AnthropicUpstream {
    fn endpoint(&self, base_url: &str) -> String {
        format!("{}/v1/messages", trimmed(base_url))
    }

    fn native_headers(&self, provider: &Provider, body: &Value) -> HeaderMap {
        let web_search = body
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| {
                tools.iter().any(|tool| {
                    matches!(
                        tool.get("type").and_then(Value::as_str),
                        Some("web_search" | "web_search_preview")
                    )
                })
            });
        anthropic::client::build_headers(&provider.api_key, &provider.extra, web_search)
    }

    fn probe_body(&self, model: &str) -> Value {
        json!({"model":model,"max_tokens":1,"stream":false,"messages":[{"role":"user","content":"hi"}]})
    }

    fn stream<'a>(
        &'a self,
        http: &'a reqwest::Client,
        call: UpstreamCall<'a>,
    ) -> BoxFuture<'a, Result<EventStream, UpstreamError>> {
        Box::pin(async move {
            let request = anthropic_translate::build_request(
                call.request,
                call.provider,
                call.upstream_model,
            )
            .map_err(UpstreamError::BadRequest)?;
            let headers = anthropic::client::build_headers(
                &call.provider.api_key,
                &call.provider.extra,
                false,
            );
            let stream =
                anthropic::client::post_messages(http, &call.provider.base_url, headers, &request)
                    .await
                    .map_err(UpstreamError::Upstream)?;
            Ok(Box::pin(anthropic::sse::translate_sse(stream)) as EventStream)
        })
    }
}

struct OpenAiChatUpstream;

impl UpstreamClient for OpenAiChatUpstream {
    fn endpoint(&self, base_url: &str) -> String {
        format!("{}/chat/completions", trimmed(base_url))
    }

    fn native_headers(&self, provider: &Provider, _body: &Value) -> HeaderMap {
        openai_chat::client::build_headers(&provider.api_key, &provider.extra)
    }

    fn probe_body(&self, model: &str) -> Value {
        json!({"model":model,"max_completion_tokens":1,"messages":[{"role":"user","content":"hi"}]})
    }

    fn stream<'a>(
        &'a self,
        http: &'a reqwest::Client,
        call: UpstreamCall<'a>,
    ) -> BoxFuture<'a, Result<EventStream, UpstreamError>> {
        Box::pin(async move {
            let request =
                chat_translate::build_request(call.request, call.provider, call.upstream_model)
                    .map_err(UpstreamError::BadRequest)?;
            let headers =
                openai_chat::client::build_headers(&call.provider.api_key, &call.provider.extra);
            let stream =
                openai_chat::client::post_chat(http, &call.provider.base_url, headers, &request)
                    .await
                    .map_err(UpstreamError::Upstream)?;
            Ok(Box::pin(openai_chat::sse::translate_sse(stream)) as EventStream)
        })
    }
}

struct ResponsesUpstream;

impl UpstreamClient for ResponsesUpstream {
    fn endpoint(&self, base_url: &str) -> String {
        format!("{}/responses", trimmed(base_url))
    }

    fn native_headers(&self, provider: &Provider, _body: &Value) -> HeaderMap {
        openai_chat::client::build_headers(&provider.api_key, &provider.extra)
    }

    fn probe_body(&self, model: &str) -> Value {
        json!({"model":model,"input":"hi","max_output_tokens":1,"stream":false})
    }

    fn stream<'a>(
        &'a self,
        http: &'a reqwest::Client,
        call: UpstreamCall<'a>,
    ) -> BoxFuture<'a, Result<EventStream, UpstreamError>> {
        Box::pin(async move {
            let mut forwarded = call.normalized_body.clone();
            forwarded["stream"] = Value::Bool(true);
            forwarded["store"] = Value::Bool(false);
            // `parallel_tool_calls` is an internal canonical extension (OpenAI Chat
            // Completions / Anthropic tool_choice), not an OpenAI Responses API field.
            if let Some(object) = forwarded.as_object_mut() {
                object.remove("parallel_tool_calls");
            }
            let stream = responses::post_responses(
                http,
                &call.provider.base_url,
                &call.provider.api_key,
                &call.provider.extra,
                &mut forwarded,
                call.upstream_model,
            )
            .await
            .map_err(UpstreamError::Upstream)?;
            Ok(Box::pin(responses_sse::translate_sse(stream)) as EventStream)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_by_provider_type() {
        assert!(upstream_client("anthropic").is_some());
        assert!(upstream_client("openai_chat").is_some());
        assert!(upstream_client("responses").is_some());
        assert!(upstream_client("other").is_none());
    }

    #[test]
    fn endpoints_ignore_trailing_slashes() {
        let endpoint = |kind: &str| {
            upstream_client(kind)
                .unwrap()
                .endpoint("https://x.test/v1/")
        };
        assert_eq!(endpoint("anthropic"), "https://x.test/v1/v1/messages");
        assert_eq!(
            endpoint("openai_chat"),
            "https://x.test/v1/chat/completions"
        );
        assert_eq!(endpoint("responses"), "https://x.test/v1/responses");
    }
}
