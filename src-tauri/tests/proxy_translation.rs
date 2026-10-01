mod common;

use common::*;
use wiremock::{
    matchers::{body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn anthropic_sse_is_translated_to_responses_events() {
    let upstream = MockServer::start().await;
    let fixture = concat!(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("anthropic", &upstream).await;
    let response = reqwest::Client::new().post(format!("{proxy}/v1/responses")).json(&serde_json::json!({"model":"public-model","input":"hello","stream":true,"store":false})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("response.output_text.delta"));
    assert!(body.contains("response.completed"));
    assert!(body.contains("\"text\":\"hello\""));
}

#[tokio::test]
async fn chat_completion_sse_is_translated_and_usage_is_kept() {
    let upstream = MockServer::start().await;
    let fixture = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hello\"}}],\"usage\":null}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1,\"total_tokens\":3}}\n\n",
        "data: [DONE]\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_string_contains("\"model\":\"real-model\""))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("openai_chat", &upstream).await;
    let response = reqwest::Client::new().post(format!("{proxy}/v1/responses")).json(&serde_json::json!({"model":"public-model","input":"hello","stream":true,"store":false})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("response.output_text.delta"));
    assert!(body.contains("response.completed"));
    assert!(body.contains("\"input_tokens\":2"));
}

#[tokio::test]
async fn chat_parallel_tool_calls_false_maps_to_anthropic_disable_parallel_tool_use() {
    let upstream = MockServer::start().await;
    let fixture = concat!(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    // The raw client field must never reach the Anthropic wire; only the
    // standard tool_choice.disable_parallel_tool_use translation may.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("parallel_tool_calls"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("\"disable_parallel_tool_use\":true"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("anthropic", &upstream).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": "public-model",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "tools": [{"type": "function", "function": {"name": "shell", "parameters": {"type": "object"}}}],
            "parallel_tool_calls": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("chat.completion.chunk"));
}

#[tokio::test]
async fn chat_parallel_tool_calls_is_stripped_for_responses_upstream() {
    let upstream = MockServer::start().await;
    let fixture = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_x\"}}\n\n";
    // parallel_tool_calls is an internal canonical extension, not an OpenAI
    // Responses API field; it must not be forwarded upstream.
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(body_string_contains("parallel_tool_calls"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("responses", &upstream).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/chat/completions"))
        .json(&serde_json::json!({
            "model": "public-model",
            "messages": [{"role": "user", "content": "hi"}],
            "stream": true,
            "parallel_tool_calls": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn responses_web_search_is_omitted_for_anthropic_upstream() {
    let upstream = MockServer::start().await;
    let fixture = concat!(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    // Hosted built-in tools can never reach an Anthropic-compatible upstream:
    // function tools survive, web_search is dropped, request still succeeds.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("web_search"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&upstream)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(body_string_contains("\"name\":\"tool_0\""))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("anthropic", &upstream).await;
    // Codex-shaped tool list: ten function tools (incl. one namespace) plus
    // web_search at index 10 — the exact shape that used to 400.
    let mut tools: Vec<serde_json::Value> = (0..10)
        .map(|index| {
            serde_json::json!({"type": "function", "name": format!("tool_{index}"),
                              "parameters": {"type": "object"}})
        })
        .collect();
    tools.push(serde_json::json!({"type": "web_search"}));
    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({
            "model": "public-model",
            "input": [{"type": "message", "role": "user",
                       "content": [{"type": "input_text", "text": "hi"}]}],
            "tools": tools,
            "stream": true,
            "store": false,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("response.completed"));
}

/// Real-provider smoke test: reproduces the exact failing Codex startup
/// request (function tools + `web_search`) against the configured LongCat
/// Anthropic-compatible endpoint. Not run by default; provide credentials:
///
/// ```text
/// LONGCAT_API_KEY=... cargo test --test proxy_e2e longcat_real -- --ignored
/// ```
#[tokio::test]
#[ignore = "hits the real LongCat API; set LONGCAT_API_KEY to run"]
async fn longcat_real_web_search_request_succeeds() {
    let api_key = std::env::var("LONGCAT_API_KEY").expect("LONGCAT_API_KEY must be set");
    let base_url = std::env::var("LONGCAT_BASE_URL")
        .unwrap_or_else(|_| "https://api.longcat.chat/anthropic".into());
    let upstream_model =
        std::env::var("LONGCAT_MODEL").unwrap_or_else(|_| "LongCat-2.5-Preview".into());
    let (proxy, _dir) = start_proxy_with("anthropic", &base_url, &api_key, &upstream_model).await;

    let mut tools: Vec<serde_json::Value> = (0..10)
        .map(|index| {
            serde_json::json!({"type": "function", "name": format!("tool_{index}"),
                              "parameters": {"type": "object"}})
        })
        .collect();
    tools.push(serde_json::json!({"type": "web_search"}));

    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .unwrap()
        .post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({
            "model": "public-model",
            "input": [{"type": "message", "role": "user",
                       "content": [{"type": "input_text", "text": "Reply with the single word: OK"}]}],
            "tools": tools,
            "max_output_tokens": 64,
            "stream": false,
            "store": false,
        }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.text().await.unwrap();
    assert_eq!(
        status, 200,
        "LongCat rejected the translated request: {body}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["status"], "completed");
    assert!(parsed["output"]
        .as_array()
        .map(|items| !items.is_empty())
        .unwrap_or(false));
}

#[tokio::test]
async fn anthropic_function_call_and_result_round_trip() {
    let upstream = MockServer::start().await;
    let fixture = concat!(
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_cwd\",\"name\":\"shell\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"cmd\\\":\\\"pwd\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"input_tokens\":4,\"output_tokens\":3}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .expect(2)
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("anthropic", &upstream).await;
    let client = reqwest::Client::new();
    let tools =
        serde_json::json!([{"type":"function","name":"shell","parameters":{"type":"object"}}]);
    let first = client.post(format!("{proxy}/v1/responses")).json(&serde_json::json!({"model":"public-model","input":"find the current directory","tools":tools,"stream":true,"store":false})).send().await.unwrap();
    let first_body = first.text().await.unwrap();
    assert!(first_body.contains("response.function_call_arguments.delta"));
    assert!(first_body.contains("\"call_id\":\"call_cwd\""));
    assert!(first_body.contains("\"name\":\"shell\""));
    let history = serde_json::json!([
        {"type":"message","role":"user","content":"find the current directory"},
        {"type":"function_call","call_id":"call_cwd","name":"shell","arguments":"{\"cmd\":\"pwd\"}"},
        {"type":"function_call_output","call_id":"call_cwd","output":"D:/Github/yi-llm"}
    ]);
    let _ = client.post(format!("{proxy}/v1/responses")).json(&serde_json::json!({"model":"public-model","input":history,"tools":tools,"stream":true,"store":false})).send().await.unwrap();
    let requests = upstream.received_requests().await.unwrap();
    let round_trip: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(round_trip["messages"][1]["content"][0]["type"], "tool_use");
    assert_eq!(round_trip["messages"][1]["content"][0]["id"], "call_cwd");
    assert_eq!(
        round_trip["messages"][2]["content"][0]["type"],
        "tool_result"
    );
    assert_eq!(
        round_trip["messages"][2]["content"][0]["tool_use_id"],
        "call_cwd"
    );
}
