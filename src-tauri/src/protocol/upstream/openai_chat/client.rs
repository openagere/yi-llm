use crate::protocol::upstream::openai_chat::types::ChatRequest;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

pub fn build_headers(api_key: &str, extra: &serde_json::Value) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if !api_key.is_empty() {
        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
            headers.insert(AUTHORIZATION, value);
        }
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

pub async fn post_chat(
    client: &reqwest::Client,
    base_url: &str,
    headers: HeaderMap,
    body: &ChatRequest,
) -> Result<impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>>, String> {
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
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
