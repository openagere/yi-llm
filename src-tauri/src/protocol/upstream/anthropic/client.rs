use crate::protocol::upstream::anthropic::types::MessagesRequest;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

pub const ANTHROPIC_BETA_FEATURES: &str = "interleaved-thinking-2025-05-14";
pub const ANTHROPIC_WEB_SEARCH_BETA: &str = "web-search-2025-03-05";

pub fn build_headers(api_key: &str, extra: &serde_json::Value, web_search: bool) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if !api_key.is_empty() {
        let entered_key = api_key.trim();
        let token = entered_key
            .split_once(' ')
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token.trim())
            .unwrap_or(entered_key);
        if let Ok(value) = HeaderValue::from_str(token) {
            headers.insert("x-api-key", value);
        }
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
            headers.insert(AUTHORIZATION, value);
        }
    }
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    let beta_features = if web_search {
        format!("{ANTHROPIC_BETA_FEATURES}, {ANTHROPIC_WEB_SEARCH_BETA}")
    } else {
        ANTHROPIC_BETA_FEATURES.to_owned()
    };
    if let Ok(value) = HeaderValue::from_str(&beta_features) {
        headers.insert("anthropic-beta", value);
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if let Some(values) = extra.as_object() {
        for (key, value) in values {
            if let (Ok(name), Some(value)) = (
                reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                value.as_str(),
            ) {
                if let Ok(value) = HeaderValue::from_str(value) {
                    headers.insert(name, value);
                }
            }
        }
    }
    headers
}

pub async fn post_messages(
    client: &reqwest::Client,
    base_url: &str,
    headers: HeaderMap,
    body: &MessagesRequest,
) -> Result<impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>>, String> {
    let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
    let response = client
        .post(url)
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!("上游请求超时: {e}")
            } else {
                format!("连接上游失败: {e}")
            }
        })?;
    if !response.status().is_success() {
        let status = response.status();
        return Err(format!(
            "上游返回 {status}: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    Ok(response.bytes_stream())
}
