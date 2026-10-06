mod common;

use common::*;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};
#[tokio::test]
async fn shared_standard_capabilities_control_live_upstream_requests() {
    use yi_llm::{
        db::repo::catalog,
        domain::capabilities::{Capabilities, Effort, Modality, Support},
        domain::standard_model::StandardModel,
    };
    let upstream = MockServer::start().await;
    Mock::given(method("POST")).and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id":"resp-test","object":"response","model":"real-model","output":[]})))
        .mount(&upstream).await;
    let (proxy, dir) = start_proxy("responses", &upstream).await;
    let mut conn = dir.conn();
    let mut standard = StandardModel {
        id: "shared".into(),
        name: "Vision standard".into(),
        protocol: "responses".into(),
        brand: "openai".into(),
        provider_count: 0,
        capabilities: Capabilities {
            input_modalities: Some(vec![Modality::Text, Modality::Image]),
            output_modalities: Some(vec![Modality::Text]),
            context_window: Some(65536),
            max_output_tokens: Some(2048),
            effort: Effort {
                support: Support::Supported,
                levels: vec!["low".into(), "high".into()],
                default: Some("high".into()),
            },
        },
    };
    catalog::save(&mut conn, &standard).unwrap();
    conn.execute(
        "UPDATE models SET standard_model_id='shared' WHERE provider_id='primary'",
        [],
    )
    .unwrap();
    let client = reqwest::Client::new();
    let image = serde_json::json!({"model":"public-model","input":[{"role":"user","content":[{"type":"input_text","text":"hello"},{"type":"input_image","image_url":"data:image/png;base64,aGVsbG8="}]}]});
    let response = client
        .post(format!("{proxy}/v1/responses"))
        .json(&image)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let upstream_request: serde_json::Value =
        serde_json::from_slice(&upstream.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(upstream_request["model"], "real-model");
    assert_eq!(upstream_request["reasoning"]["effort"], "high");
    assert_eq!(upstream_request["max_output_tokens"], 2048);
    standard.capabilities.input_modalities = Some(vec![Modality::Text]);
    standard.capabilities.max_output_tokens = Some(1024);
    standard.capabilities.effort = Effort {
        support: Support::Unsupported,
        ..Default::default()
    };
    catalog::save(&mut conn, &standard).unwrap();
    for request in [
        image,
        serde_json::json!({"model":"public-model","input":"hello","max_output_tokens":1025}),
        serde_json::json!({"model":"public-model","input":"hello","reasoning":{"effort":"high"}}),
    ] {
        let response = client
            .post(format!("{proxy}/v1/responses"))
            .json(&request)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(upstream.received_requests().await.unwrap().len(), 1);
    let response = client
        .post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let requests = upstream.received_requests().await.unwrap();
    let final_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(final_body["max_output_tokens"], 1024);
    assert!(final_body.get("reasoning").is_none());
}

#[tokio::test]
async fn declared_anthropic_effort_converts_to_output_config_without_budget_heuristics() {
    use yi_llm::{
        db::repo::catalog,
        domain::capabilities::{Capabilities, Effort, Support},
        domain::standard_model::StandardModel,
    };
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
        )
        .mount(&upstream)
        .await;
    let (proxy, dir) = start_proxy("anthropic", &upstream).await;
    let mut conn = dir.conn();
    let standard = StandardModel {
        id: "effort".into(),
        name: "Anthropic".into(),
        protocol: "anthropic".into(),
        brand: "anthropic".into(),
        provider_count: 0,
        capabilities: Capabilities {
            max_output_tokens: Some(4096),
            effort: Effort {
                support: Support::Supported,
                levels: vec!["high".into(), "max".into()],
                default: Some("high".into()),
            },
            ..Default::default()
        },
    };
    catalog::save(&mut conn, &standard).unwrap();
    conn.execute(
        "UPDATE models SET standard_model_id='effort' WHERE provider_id='primary'",
        [],
    )
    .unwrap();
    let response = reqwest::Client::new().post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello","store":false,"stream":true})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&upstream.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(body["max_tokens"], 4096);
    assert!(body.get("thinking").is_none());
    let response = reqwest::Client::new().post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello","store":false,"stream":true,"reasoning":{"effort":"max"}})).send().await.unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value =
        serde_json::from_slice(&upstream.received_requests().await.unwrap()[1].body).unwrap();
    assert_eq!(body["output_config"]["effort"], "max");
    assert!(body.get("thinking").is_none());
    let response = reqwest::Client::new().post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello","reasoning":{"effort":"ultra"}})).send().await.unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(upstream.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn claude_terminal_thinking_and_effort_work_across_all_upstreams() {
    use yi_llm::{
        db::repo::catalog,
        domain::capabilities::{effort_levels, Capabilities, Effort, Support},
        domain::standard_model::StandardModel,
        terminal::{self, Profile, Selection},
    };
    for (protocol, upstream_path, fixture) in [
        ("anthropic", "/v1/messages", "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
        ("responses", "/responses", "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_test\",\"output\":[]}}\n\n"),
        ("openai_chat", "/chat/completions", "data: {\"id\":\"chat_test\",\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"),
    ] {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(upstream_path))
            .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/event-stream").set_body_string(fixture))
            .mount(&upstream).await;
        let (proxy, dir) = start_proxy(protocol, &upstream).await;
        let mut conn = dir.conn();
        conn.execute("UPDATE providers SET protocol_support='{\"responses\":true,\"anthropic\":true,\"openai_chat\":true}'", []).unwrap();
        catalog::save(&mut conn, &StandardModel {
            id: "claude-effort".into(), name: "Effort standard".into(),
            protocol: protocol.into(), brand: "".into(), provider_count: 0,
            capabilities: Capabilities {
                effort: Effort {
                    support: Support::Supported,
                    levels: effort_levels(protocol).iter().map(|level| (*level).into()).collect(),
                    default: Some("high".into()),
                },
                ..Default::default()
            },
        }).unwrap();
        conn.execute("UPDATE models SET standard_model_id='claude-effort'", []).unwrap();
        terminal::save(&conn, &Profile {
            client: "claude-code".into(),
            models: vec![Selection { provider_id: "primary".into(), model: "public-model".into() }],
            default_model: "public-model".into(),
            protocol: None,
        }).unwrap();
        let client = reqwest::Client::new();
        let endpoint = format!("{proxy}/clients/claude-code/v1/messages");
        let mut request_count = 0;
        for effort in effort_levels("anthropic").iter().filter(|level| effort_levels(protocol).contains(level)) {
            for thinking in [
                serde_json::json!({"type":"adaptive","display":"summarized"}),
                serde_json::json!({"type":"enabled","budget_tokens":1024}),
                serde_json::json!({"type":"disabled"}),
            ] {
                let mut body = serde_json::json!({"model":"public-model","max_tokens":4096,"stream":true,
                    "system":"Base instruction",
                    "messages":[{"role":"user","content":"hello"},{"role":"system","content":[{"type":"text","text":"Session instruction"}]}],"thinking":thinking,"output_config":{"effort":effort},
                    "metadata":{"user_id":"{\"session_id\":\"test-session\",\"account_uuid\":\"test-account\"}"}});
                let response = client.post(&endpoint).json(&body).send().await.unwrap();
                let status = response.status();
                let response_body = response.text().await.unwrap();
                assert_eq!(status, 200, "{protocol}/{effort}: {response_body}");
                let requests = upstream.received_requests().await.unwrap();
                assert_eq!(requests.len(), request_count + 1);
                let forwarded: serde_json::Value = serde_json::from_slice(&requests[request_count].body).unwrap();
                assert_eq!(forwarded["metadata"], body["metadata"]);
                match protocol {
                    "anthropic" => {
                        body["model"] = serde_json::json!("real-model");
                        assert_eq!(forwarded, body);
                    }
                    "responses" => {
                        assert_eq!(forwarded["instructions"], "Base instruction\n\nSession instruction");
                        assert_eq!(forwarded["reasoning"]["effort"], *effort);
                        assert!(forwarded.get("thinking").is_none());
                        assert!(forwarded.get("output_config").is_none());
                    }
                    _ => {
                        assert_eq!(forwarded["messages"][0]["role"], "system");
                        assert_eq!(forwarded["messages"][0]["content"], "Base instruction\n\nSession instruction");
                        assert_eq!(forwarded["reasoning_effort"], *effort);
                        assert!(forwarded.get("thinking").is_none());
                        assert!(forwarded.get("output_config").is_none());
                    }
                }
                request_count += 1;
            }
        }
        if protocol != "anthropic" {
            for effort in ["ultracode", "ultra"] {
                let body = serde_json::json!({"model":"public-model","max_tokens":4096,"messages":[{"role":"user","content":"hello"}],
                    "thinking":{"type":"adaptive"},"output_config":{"effort":effort}});
                let response = client.post(&endpoint).json(&body).send().await.unwrap();
                assert_eq!(response.status(), 400);
            }
            assert_eq!(upstream.received_requests().await.unwrap().len(), request_count);
        }
    }
}

#[tokio::test]
async fn translated_thinking_budget_cannot_bypass_declared_effort_policy() {
    use yi_llm::{
        db::repo::catalog,
        domain::capabilities::{Capabilities, Effort, Support},
        domain::standard_model::StandardModel,
    };
    let upstream = MockServer::start().await;
    let (proxy, dir) = start_proxy("responses", &upstream).await;
    let mut conn = dir.conn();
    conn.execute(
        "UPDATE providers SET protocol_support='{\"responses\":true,\"anthropic\":true,\"openai_chat\":true}'",
        [],
    )
    .unwrap();
    let model = StandardModel {
        id: "no-effort".into(),
        name: "No effort".into(),
        protocol: "responses".into(),
        brand: "".into(),
        provider_count: 0,
        capabilities: Capabilities {
            effort: Effort {
                support: Support::Unsupported,
                ..Default::default()
            },
            ..Default::default()
        },
    };
    catalog::save(&mut conn, &model).unwrap();
    conn.execute("UPDATE models SET standard_model_id='no-effort'", [])
        .unwrap();
    let response = reqwest::Client::new().post(format!("{proxy}/v1/messages")).json(&serde_json::json!({"model":"public-model","max_tokens":4096,"messages":[{"role":"user","content":"hello"}],"thinking":{"type":"enabled","budget_tokens":1024}})).send().await.unwrap();
    assert_eq!(response.status(), 400);
    let error = response.text().await.unwrap();
    assert!(error.contains("Effort"), "{error}");
    assert!(upstream.received_requests().await.unwrap().is_empty());
}
