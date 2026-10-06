mod common;

use common::*;
use wiremock::{
    matchers::{body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};
use yi_llm::{
    app::AppState,
    db::repo::providers,
    db::Db,
    domain::provider::{ModelMapping, Provider, ProviderView},
};

#[tokio::test]
async fn terminal_collection_routes_multiple_providers_and_never_falls_back() {
    let upstream = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("test.db")).unwrap();
    let mut selections = Vec::new();
    for id in ["one", "two"] {
        providers::save(
            &mut db.conn().unwrap(),
            &ProviderView {
                provider: Provider {
                    id: id.into(),
                    name: id.into(),
                    short_code: id.into(),
                    provider_type: "responses".into(),
                    base_url: upstream.uri(),
                    api_key: "test".into(),
                    enabled: true,
                    thinking: "minimal".into(),
                    extra: serde_json::json!({}),
                    is_default: id == "one",
                    protocol_support: yi_llm::domain::provider::ProtocolSupport::native(
                        "responses",
                    ),
                },
                models: vec![ModelMapping {
                    id: 0,
                    provider_id: id.into(),
                    exposed_name: format!("real-{id}({id})"),
                    upstream_model: format!("real-{id}"),
                    standard_model_id: None,
                    capabilities: Default::default(),
                    non_standard: false,
                }],
            },
        )
        .unwrap();
        selections.push(yi_llm::terminal::Selection {
            provider_id: id.into(),
            model: format!("real-{id}({id})"),
        });
        Mock::given(method("POST")).and(path("/responses")).and(body_string_contains(format!("real-{id}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id":format!("resp-{id}"),"model":format!("real-{id}"),"output":[]}))).expect(1).mount(&upstream).await;
    }
    yi_llm::terminal::save(
        &db.conn().unwrap(),
        &yi_llm::terminal::Profile {
            client: "codex".into(),
            models: selections,
            default_model: "real-one(one)".into(),
            protocol: None,
        },
    )
    .unwrap();
    let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None);
    let (root, task) = serve(&state).await;
    let client = reqwest::Client::new();
    let list: serde_json::Value = client
        .get(format!("{root}/clients/codex/v1/models"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["data"].as_array().unwrap().len(), 2);
    assert_eq!(list["data"][0]["id"], "real-one(one)");
    assert_eq!(list["data"][1]["id"], "real-two(two)");
    for id in ["one", "two"] {
        let response: serde_json::Value = client
            .post(format!("{root}/clients/codex/v1/responses"))
            .json(&serde_json::json!({"model":format!("real-{id}({id})"),"input":"hello","stream":false}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["id"], format!("resp-{id}"));
    }
    let excluded = client
        .post(format!("{root}/clients/codex/v1/responses"))
        .json(&serde_json::json!({"model":"not-selected","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(excluded.status(), 400);
    let wrong_protocol = client
        .post(format!("{root}/clients/codex/v1/messages"))
        .json(&serde_json::json!({"model":"real-one(one)","messages":[]}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_protocol.status(), 400);
    let error: serde_json::Value = wrong_protocol.json().await.unwrap();
    assert_eq!(error["type"], "error");
    sql(
        &state,
        "UPDATE models SET provider_id='one' WHERE exposed_name='real-two(two)'",
    );
    let reassigned = client
        .post(format!("{root}/clients/codex/v1/responses"))
        .json(&serde_json::json!({"model":"real-two(two)","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(reassigned.status(), 400);
    sql(&state, "UPDATE providers SET enabled=0 WHERE id='one'");
    let disabled = client
        .post(format!("{root}/clients/codex/v1/responses"))
        .json(&serde_json::json!({"model":"real-one(one)","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(disabled.status(), 400);
    let list: serde_json::Value = client
        .get(format!("{root}/clients/codex/v1/models"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list["data"], serde_json::json!([]));
    task.abort();
}

#[tokio::test]
async fn shared_catalog_file_changes_apply_to_running_requests_and_model_lists() {
    use yi_llm::{
        catalog::CatalogService,
        db::repo::catalog,
        domain::capabilities::Capabilities,
        domain::standard_model::StandardModel,
        terminal::{self, Profile, Selection},
    };
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"id":"resp-test","object":"response","output":[]}),
            ),
        )
        .mount(&upstream)
        .await;
    let (_unused_proxy, dir) = start_proxy("responses", &upstream).await;
    let mut conn = dir.conn();
    catalog::save(
        &mut conn,
        &StandardModel {
            id: "shared-file".into(),
            name: "File standard".into(),
            protocol: "responses".into(),
            brand: "openai".into(),
            provider_count: 0,
            capabilities: Capabilities {
                max_output_tokens: Some(2048),
                ..Default::default()
            },
        },
    )
    .unwrap();
    conn.execute(
        "UPDATE models SET standard_model_id='shared-file' WHERE provider_id='primary'",
        [],
    )
    .unwrap();
    terminal::save(
        &conn,
        &Profile {
            client: "codex".into(),
            models: vec![Selection {
                provider_id: "primary".into(),
                model: "public-model".into(),
            }],
            default_model: "public-model".into(),
            protocol: None,
        },
    )
    .unwrap();
    let catalog_path = dir.path().join("models/catalog.json");
    let store = CatalogService::open(catalog_path.clone(), &conn).unwrap();
    let state = AppState::new(
        Db::open(&dir.db_path()).unwrap(),
        reqwest::Client::new(),
        dir.path().join("logs"),
        None,
    )
    .with_catalog(store);
    let (proxy, task) = serve(&state).await;
    let client = reqwest::Client::new();
    for (index, limit) in [2048, 1024].into_iter().enumerate() {
        if index == 1 {
            let mut document: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
            document["models"][0]["capabilities"]["max_output_tokens"] = limit.into();
            std::fs::write(&catalog_path, serde_json::to_vec(&document).unwrap()).unwrap();
        }
        let response = client
            .post(format!("{proxy}/clients/codex/v1/responses"))
            .json(&serde_json::json!({"model":"public-model","input":"hello"}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let requests = upstream.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&requests[index].body).unwrap();
        assert_eq!(body["max_output_tokens"], limit);
    }
    let response = client
        .post(format!("{proxy}/v1/responses"))
        .json(&serde_json::json!({"model":"public-model","input":"hello","max_output_tokens":1025}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let valid = std::fs::read(&catalog_path).unwrap();
    std::fs::write(&catalog_path, "{invalid").unwrap();
    // The first read may observe the invalid revision and report its parse error, or
    // the file stamp may be unchanged at the platform's timestamp resolution. Either
    // way, FileCatalog remembers the revision and subsequent reads use the last valid cache.
    let first_invalid_status = client
        .get(format!("{proxy}/v1/models"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(first_invalid_status.is_success() || first_invalid_status.is_server_error());
    for endpoint in ["/v1/models", "/clients/codex/v1/models"] {
        assert_eq!(
            client
                .get(format!("{proxy}{endpoint}"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(
        client
            .post(format!("{proxy}/v1/responses"))
            .json(&serde_json::json!({"model":"public-model","input":"hello"}))
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(upstream.received_requests().await.unwrap().len(), 3);
    std::fs::write(&catalog_path, valid).unwrap();
    for endpoint in ["/v1/models", "/clients/codex/v1/models"] {
        assert_eq!(
            client
                .get(format!("{proxy}{endpoint}"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
    }
    task.abort();
}

#[tokio::test]
async fn codex_readable_model_uses_bound_capabilities_and_internal_usage_identity() {
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .and(body_string_contains("\"model\":\"real-model\""))
        .and(body_string_contains("\"max_output_tokens\":64"))
        .and(body_string_contains("\"effort\":\"low\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id":"resp-test","model":"real-model","output":[],
            "usage":{"input_tokens":2,"output_tokens":3}
        })))
        .expect(1)
        .mount(&upstream)
        .await;
    let (proxy, dir) = start_proxy("responses", &upstream).await;
    let db = dir.conn();
    let caps = serde_json::json!({"input_modalities":["text"],"output_modalities":["text"],"context_window":1024,"max_output_tokens":64,"effort":{"support":"supported","levels":["low"],"default":"low"}});
    db.execute(
        "UPDATE models SET exposed_name='real-model(primary)' WHERE provider_id='primary'",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO standard_models(id,name,protocol,brand,capabilities) VALUES('readable','Readable','responses','',?1)",
        [caps.to_string()],
    )
    .unwrap();
    db.execute(
        "UPDATE models SET standard_model_id='readable' WHERE provider_id='primary'",
        [],
    )
    .unwrap();
    yi_llm::terminal::save(
        &db,
        &yi_llm::terminal::Profile {
            client: "codex".into(),
            models: vec![yi_llm::terminal::Selection {
                provider_id: "primary".into(),
                model: "real-model(primary)".into(),
            }],
            default_model: "real-model(primary)".into(),
            protocol: None,
        },
    )
    .unwrap();
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{proxy}/clients/codex/v1/responses"))
        .json(&serde_json::json!({"model":"real-model(primary)","input":"hello"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.json::<serde_json::Value>().await.unwrap()["model"],
        "real-model"
    );
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if yi_llm::db::repo::usage::snapshot_for_days(&db, 1, None)
                .unwrap()
                .total_requests
                == 1
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("native usage was not recorded");
    let usage = yi_llm::db::repo::usage::snapshot_for_days(&db, 1, None).unwrap();
    assert_eq!(usage.recent[0].model, "real-model(primary)");
    assert_eq!(usage.output_tokens, 3);
    for body in [
        serde_json::json!({"model":"real-model(primary)","input":"hello","max_output_tokens":65}),
        serde_json::json!({"model":"real-model(primary)","input":"hello","reasoning":{"effort":"high"}}),
        serde_json::json!({"model":"real-model(primary)","input":[{"role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,aA=="}]}]}),
    ] {
        assert_eq!(
            client
                .post(format!("{proxy}/clients/codex/v1/responses"))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }
}

#[tokio::test]
async fn provider_changes_take_effect_without_restart() {
    let upstream_a = MockServer::start().await;
    let upstream_b = MockServer::start().await;
    let fixture = concat!(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"}}],\"usage\":null}\n\n",
        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
        "data: [DONE]\n\n"
    );
    for upstream in [&upstream_a, &upstream_b] {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(fixture),
            )
            .mount(upstream)
            .await;
    }
    let (proxy, dir) = start_proxy("openai_chat", &upstream_a).await;
    let client = reqwest::Client::new();
    let post = |model: &str| {
        let client = client.clone();
        let proxy = proxy.clone();
        let model = model.to_owned();
        async move {
            client
                .post(format!("{proxy}/v1/responses"))
                .json(
                    &serde_json::json!({"model": model, "input": "hi", "stream": true, "store": false}),
                )
                .send()
                .await
                .unwrap()
        }
    };

    // Baseline: the initial provider routes the initial model.
    assert_eq!(post("public-model").await.status(), 200);

    // Add a provider while the proxy keeps running; no restart is signalled.
    let provider_b = ProviderView {
        provider: Provider {
            id: "secondary".into(),
            provider_type: "openai_chat".into(),
            name: "Secondary".into(),
            short_code: "secondary".into(),
            base_url: upstream_b.uri(),
            api_key: "test-key".into(),
            enabled: true,
            thinking: "minimal".into(),
            extra: serde_json::json!({}),
            is_default: false,
            protocol_support: {
                let mut support = yi_llm::domain::provider::ProtocolSupport::native("openai_chat");
                support.responses = true;
                support
            },
        },
        models: vec![ModelMapping {
            id: 0,
            provider_id: "secondary".into(),
            exposed_name: "second-model".into(),
            upstream_model: "real-b".into(),
            standard_model_id: None,
            capabilities: Default::default(),
            non_standard: false,
        }],
    };
    {
        let mut conn = dir.conn();
        providers::save(&mut conn, &provider_b).unwrap();
    }

    // The new provider routes immediately.
    assert_eq!(post("second-model").await.status(), 200);
    let requests_b = upstream_b.received_requests().await.unwrap();
    assert_eq!(requests_b.len(), 1);
    let body: serde_json::Value = serde_json::from_slice(&requests_b[0].body).unwrap();
    assert_eq!(body["model"], "real-b");

    // The original provider still routes its own mapping.
    assert_eq!(post("public-model").await.status(), 200);

    // Disabling the provider takes effect just as immediately.
    {
        let mut disabled = provider_b.clone();
        disabled.provider.enabled = false;
        let mut conn = dir.conn();
        providers::save(&mut conn, &disabled).unwrap();
    }
    let rejected = post("second-model").await;
    assert_eq!(rejected.status(), 400);
}
