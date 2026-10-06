mod common;

use common::*;
use wiremock::{
    matchers::{body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn responses_passthrough_rewrites_model_and_preserves_raw_sse() {
    let upstream = MockServer::start().await;
    let fixture = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_x\"}}\n\n";
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(body_string_contains("\"model\":\"real-model\""))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(fixture),
        )
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("responses", &upstream).await;
    let response = reqwest::Client::new().post(format!("{proxy}/responses")).json(&serde_json::json!({"model":"public-model","input":"hello","stream":true,"store":false,"custom_field":{"kept":true}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), fixture);
}

#[tokio::test]
async fn large_codex_responses_preserve_history_images_and_tools_upstream() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "resp-large", "model": "real-model", "output": []
        })))
        .expect(2)
        .mount(&upstream)
        .await;
    let (proxy, dir) = start_proxy("responses", &upstream).await;
    let db = dir.conn();
    yi_llm::terminal::save(
        &db,
        &yi_llm::terminal::Profile {
            client: "codex".into(),
            models: vec![yi_llm::terminal::Selection {
                provider_id: "primary".into(),
                model: "public-model".into(),
            }],
            default_model: "public-model".into(),
            protocol: None,
        },
    )
    .unwrap();
    drop(db);
    let mut body = serde_json::json!({
        "model": "public-model",
        "input": [
            {"role": "user", "content": [
                {"type": "input_text", "text": "x".repeat(1024 * 1024)},
                {"type": "input_image", "image_url": format!("data:image/png;base64,{}", "A".repeat(2 * 1024 * 1024))}
            ]},
            {"type": "function_call_output", "call_id": "call-large", "output": "z".repeat(512 * 1024)}
        ],
        "tools": [{"type": "function", "name": "inspect", "parameters": {"type": "object", "properties": {}}}],
        "stream": false,
        "store": false,
        "custom_field": {"kept": true}
    });
    let client = reqwest::Client::new();
    for endpoint in ["/clients/codex/v1/responses", "/clients/codex/responses"] {
        let response = client
            .post(format!("{proxy}{endpoint}"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "{endpoint}");
        let response: serde_json::Value = response.json().await.unwrap();
        assert_eq!(response["id"], "resp-large");
    }
    body["model"] = serde_json::json!("real-model");
    let requests = upstream.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert!(request.body.len() > 3 * 1024 * 1024);
        let forwarded: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(forwarded, body);
    }
}

#[tokio::test]
async fn native_terminal_routes_preserve_client_headers_and_upstream_model_case() {
    use wiremock::matchers::header;
    for (protocol, terminal, endpoint, upstream_path, user_agent) in [
        (
            "responses",
            "codex",
            "responses",
            "/responses",
            "codex_cli_rs/0.158.0",
        ),
        (
            "anthropic",
            "claude-code",
            "messages",
            "/v1/messages",
            "claude-cli/2.2.0",
        ),
        (
            "openai_chat",
            "opencode",
            "chat/completions",
            "/chat/completions",
            "opencode/1.2.0",
        ),
    ] {
        let upstream = MockServer::start().await;
        let fixture = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\"}}\n\n";
        Mock::given(method("POST"))
            .and(path(upstream_path))
            .and(header("user-agent", user_agent))
            .and(header("originator", "test-terminal"))
            .and(header("session_id", "session-test"))
            .and(header("x-codex-turn-id", "turn-test"))
            .and(header("x-client-request-id", "client-request-test"))
            .and(header("accept", "text/event-stream"))
            .and(header("authorization", "Bearer test-key"))
            .and(body_string_contains("\"model\":\"Real-Model-v1\""))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .insert_header("x-request-id", "upstream-request-test")
                    .set_body_string(fixture),
            )
            .expect(2)
            .mount(&upstream)
            .await;
        let (proxy, dir) =
            start_proxy_with(protocol, &upstream.uri(), "test-key", "Real-Model-v1").await;
        let db = dir.conn();
        yi_llm::terminal::save(
            &db,
            &yi_llm::terminal::Profile {
                client: terminal.into(),
                models: vec![yi_llm::terminal::Selection {
                    provider_id: "primary".into(),
                    model: "public-model".into(),
                }],
                default_model: "public-model".into(),
                protocol: None,
            },
        )
        .unwrap();
        let client = reqwest::Client::new();
        for prefix in ["/v1".to_owned(), format!("/clients/{terminal}/v1")] {
            let response = client.post(format!("{proxy}{prefix}/{endpoint}"))
                .header("user-agent", user_agent)
                .header("originator", "test-terminal")
                .header("session_id", "session-test")
                .header("x-codex-turn-id", "turn-test")
                .header("x-client-request-id", "client-request-test")
                .header("accept", "text/event-stream")
                .header("authorization", "Bearer client-secret")
                .header("x-api-key", "client-secret")
                .header("cookie", "client-secret")
                .json(&serde_json::json!({"model":"public-model","input":"hello","messages":[{"role":"user","content":"hello"}],"stream":true,"store":false,"custom_field":{"kept":true}}))
                .send().await.unwrap();
            assert_eq!(response.status(), 200, "{protocol} {prefix}");
            assert_eq!(response.headers()["x-request-id"], "upstream-request-test");
            assert_eq!(response.text().await.unwrap(), fixture);
        }
        for request in upstream.received_requests().await.unwrap() {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["custom_field"]["kept"], true);
            assert_eq!(body["model"], "Real-Model-v1");
            assert!(!request.headers.contains_key("cookie"));
            if protocol != "anthropic" {
                assert!(!request.headers.contains_key("x-api-key"));
            }
        }
    }
}

#[tokio::test]
async fn native_errors_preserve_status_body_and_retry_diagnostics() {
    let upstream = MockServer::start().await;
    let fixture =
        "{\"error\":{\"message\":\"Service temporarily unavailable\",\"type\":\"api_error\"}}";
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(503)
                .insert_header("content-type", "application/json")
                .insert_header("retry-after", "12")
                .insert_header("x-request-id", "upstream-failure-test")
                .insert_header("x-ratelimit-remaining-requests", "0")
                .set_body_string(fixture),
        )
        .expect(1)
        .mount(&upstream)
        .await;
    let (proxy, _dir) = start_proxy("responses", &upstream).await;
    let response = reqwest::Client::new()
        .post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello","stream":true}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert_eq!(response.headers()["retry-after"], "12");
    assert_eq!(response.headers()["x-request-id"], "upstream-failure-test");
    assert_eq!(response.headers()["x-ratelimit-remaining-requests"], "0");
    assert_eq!(response.text().await.unwrap(), fixture);
}

#[tokio::test]
async fn non_standard_model_adapts_native_and_codex_requests_without_changing_standard_models() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "resp_compat", "output": []
        })))
        .expect(2)
        .mount(&upstream)
        .await;
    let (proxy, dir) = start_proxy("responses", &upstream).await;
    let body = serde_json::json!({
        "model": "public-model", "input": "hello", "stream": false, "store": false,
        "reasoning": {"effort": "max"},
        "tools": [
            {"type":"function","name":"shell","parameters":{"type":"object"}},
            {"type":"namespace","name":"mcp","tools":[]},
            {"type":"web_search","external_web_access":true}
        ],
        "tool_choice": "auto", "include": ["reasoning.encrypted_content"]
    });
    let client = reqwest::Client::new();
    let standard = client
        .post(format!("{proxy}/v1/responses"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(standard.status(), 200);

    // Simulate saving the mapping's checkbox; routes are invalidated just as the service does.
    sql(
        &dir.state,
        "UPDATE models SET non_standard=1 WHERE exposed_name='public-model'",
    );
    yi_llm::terminal::save(
        &dir.conn(),
        &yi_llm::terminal::Profile {
            client: "codex".into(),
            models: vec![yi_llm::terminal::Selection {
                provider_id: "primary".into(),
                model: "public-model".into(),
            }],
            default_model: "public-model".into(),
            protocol: None,
        },
    )
    .unwrap();
    let adapted = client
        .post(format!("{proxy}/clients/codex/v1/responses"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(adapted.status(), 200);
    let requests = upstream.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let before: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(before["tools"].as_array().unwrap().len(), 3);
    assert_eq!(after["tools"].as_array().unwrap().len(), 1);
    assert_eq!(after["tools"][0]["name"], "shell");
    assert_eq!(after["tool_choice"], "auto");
    assert_eq!(after["reasoning"]["effort"], "max");
    assert_eq!(after["include"][0], "reasoning.encrypted_content");
    assert_eq!(after["input"], "hello");
}
