pub async fn post_responses(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
    extra: &serde_json::Value,
    body: &mut serde_json::Value,
    upstream_model: &str,
) -> Result<impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>>, String> {
    body["model"] = serde_json::Value::String(upstream_model.to_owned());
    let request = client
        .post(format!("{}/responses", base_url.trim_end_matches('/')))
        .headers(crate::protocol::upstream::openai_chat::client::build_headers(api_key, extra))
        .json(body);
    let response = request.send().await.map_err(|e| {
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
