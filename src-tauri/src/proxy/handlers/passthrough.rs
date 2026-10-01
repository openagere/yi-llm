use crate::{
    app::AppState,
    domain::provider::Provider,
    protocol::upstream::{headers, upstream_client},
    proxy::{diagnostics, errors::ProxyError, usage_scan::UsageScanner},
};
use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use futures::{Stream, StreamExt};
use serde_json::{json, Value};

/// Forwards a request to an upstream that speaks the client's own protocol.
///
/// The body is sent and streamed back untouched; token usage is scanned on the side.
pub async fn forward(
    state: &AppState,
    provider: &Provider,
    protocol: &str,
    public_model: &str,
    upstream_model: &str,
    mut body: Value,
    incoming_headers: &HeaderMap,
) -> Result<Response, ProxyError> {
    let streaming = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    body["model"] = Value::String(upstream_model.to_owned());
    if protocol == "openai_chat" && streaming {
        request_stream_usage(&mut body);
    }
    let upstream = upstream_client(protocol)
        .ok_or_else(|| ProxyError::bad_request(format!("未知 provider 类型: {protocol}")))?;
    let request = state
        .http
        .post(upstream.endpoint(&provider.base_url))
        .headers(headers::request(
            incoming_headers,
            upstream.native_headers(provider, &body),
        ));
    let response = request.json(&body).send().await.map_err(|error| {
        let message = format!("连接上游失败: {error}");
        if error.is_timeout() {
            ProxyError::new(StatusCode::GATEWAY_TIMEOUT, message)
        } else {
            ProxyError::upstream(message)
        }
    })?;
    let status =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let diagnostic_headers = diagnostics::response_headers(response.headers());
    let mut response_headers = headers::response(response.headers());
    response_headers
        .entry(CONTENT_TYPE)
        .or_insert(HeaderValue::from_static("application/json"));
    if !status.is_success() {
        return Ok(relay_rejection(
            response,
            status,
            response_headers,
            provider.name.clone(),
            upstream_model.to_owned(),
            protocol.to_owned(),
            diagnostics::request_summary(&body),
            diagnostic_headers,
        ));
    }
    Ok(relay(
        response.bytes_stream(),
        status,
        response_headers,
        state,
        provider.name.clone(),
        UsageScanner::new(protocol, streaming),
        protocol.to_owned(),
        public_model.to_owned(),
    ))
}

/// Observe the upstream's error stream without changing status, headers or response bytes.
/// The bounded preview is used only for the application log; the client receives every chunk.
#[allow(clippy::too_many_arguments)]
fn relay_rejection(
    upstream: reqwest::Response,
    status: StatusCode,
    headers: HeaderMap,
    provider_name: String,
    upstream_model: String,
    protocol: String,
    request_summary: Value,
    diagnostic_headers: Value,
) -> Response {
    let body = async_stream::stream! {
        let mut source = Box::pin(upstream.bytes_stream());
        let mut preview = Vec::new();
        let mut total_bytes = 0usize;
        let mut read_error = None;
        while let Some(chunk) = source.next().await {
            match chunk {
                Ok(bytes) => {
                    total_bytes += bytes.len();
                    let remaining = diagnostics::RESPONSE_PREVIEW_LIMIT.saturating_sub(preview.len());
                    preview.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
                    yield Ok::<_, std::io::Error>(bytes);
                }
                Err(error) => {
                    read_error = Some(error.to_string());
                    yield Err(std::io::Error::other(error));
                    break;
                }
            }
        }
        tracing::warn!(
            provider = %provider_name,
            upstream_model = %upstream_model,
            protocol = %protocol,
            status = status.as_u16(),
            response_bytes = total_bytes,
            response_headers = %diagnostic_headers,
            request = %request_summary,
            upstream_error = %diagnostics::response_summary(&preview, total_bytes > preview.len()),
            read_error = ?read_error,
            "upstream rejected native request"
        );
    };
    let mut response = Body::from_stream(body).into_response();
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn request_stream_usage(body: &mut Value) {
    if !body.get("stream_options").is_some_and(Value::is_object) {
        body["stream_options"] = json!({});
    }
    body["stream_options"]["include_usage"] = Value::Bool(true);
}

#[allow(clippy::too_many_arguments)]
fn relay<S>(
    stream: S,
    status: StatusCode,
    headers: HeaderMap,
    state: &AppState,
    provider_name: String,
    mut scanner: UsageScanner,
    protocol: String,
    model: String,
) -> Response
where
    S: Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + Unpin + 'static,
{
    let usage = state.usage.clone();
    let body = async_stream::stream! {
        let mut source = stream;
        let mut recorded = false;
        while let Some(item) = source.next().await {
            match item {
                Ok(chunk) => {
                    for counts in scanner.feed(&chunk) {
                        tracing::debug!(
                            provider = %provider_name,
                            model = %model,
                            protocol = %protocol,
                            input_tokens = counts.input,
                            output_tokens = counts.output,
                            cached_tokens = counts.cached,
                            reasoning_tokens = counts.reasoning,
                            "native response usage extracted"
                        );
                        usage.record_counts(&provider_name, &protocol, &model, counts.input, counts.output, counts.cached, counts.reasoning);
                        recorded |= counts.input > 0 || counts.output > 0;
                    }
                    yield Ok::<_, std::io::Error>(chunk);
                }
                Err(error) => {
                    tracing::warn!(
                        provider = %provider_name,
                        model = %model,
                        protocol = %protocol,
                        error = %error,
                        observed_events = ?scanner.observed_events(),
                        "native response stream failed before usage scan completed"
                    );
                    yield Err(std::io::Error::other(error));
                    return;
                }
            }
        }
        if !recorded {
            if let Some(counts) = scanner.finish() {
                tracing::debug!(
                    provider = %provider_name,
                    model = %model,
                    protocol = %protocol,
                    input_tokens = counts.input,
                    output_tokens = counts.output,
                    cached_tokens = counts.cached,
                    reasoning_tokens = counts.reasoning,
                    "native response usage extracted"
                );
                usage.record_counts(&provider_name, &protocol, &model, counts.input, counts.output, counts.cached, counts.reasoning);
                recorded = counts.input > 0 || counts.output > 0;
            }
        }
        if !recorded {
            tracing::warn!(
                provider = %provider_name,
                model = %model,
                protocol = %protocol,
                streaming = scanner.is_streaming(),
                observed_events = ?scanner.observed_events(),
                "native response completed without extractable token usage"
            );
        }
    };
    let mut response = Body::from_stream(body).into_response();
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}
