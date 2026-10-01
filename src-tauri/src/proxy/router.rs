use crate::{
    app::AppState,
    proxy::{
        handlers::{chat, health, messages, models, responses},
        request_log::log_request,
    },
};
use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

pub fn router(state: AppState) -> Router {
    // Long histories and inline images can exceed both Axum's default 2 MiB
    // and the former 64 MiB cap. Keep the opt-out scoped to Codex Responses,
    // including its unversioned alias; model and protocol validation still run.
    let codex_response_handler =
        post(responses::codex_responses).layer(DefaultBodyLimit::disable());
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        codex_responses_body_limit = "unlimited",
        "proxy router configured"
    );
    Router::new()
        .route("/health", get(health::health))
        .route("/v1/models", get(models::models))
        .route("/v1/responses", post(responses::responses))
        .route("/responses", post(responses::responses))
        .route("/v1/chat/completions", post(chat::chat_completions))
        .route("/v1/messages", post(messages::anthropic_messages))
        .route("/clients/{client}/v1/models", get(models::client_models))
        .route(
            "/clients/codex/v1/responses",
            codex_response_handler.clone(),
        )
        .route("/clients/codex/responses", codex_response_handler)
        .route(
            "/clients/{client}/v1/responses",
            post(responses::client_responses),
        )
        .route(
            "/clients/{client}/responses",
            post(responses::client_responses),
        )
        .route(
            "/clients/{client}/v1/messages",
            post(messages::client_messages),
        )
        .route(
            "/clients/{client}/v1/chat/completions",
            post(chat::client_chat),
        )
        .layer(middleware::from_fn_with_state(state.clone(), log_request))
        .with_state(state)
}

#[cfg(test)]
mod body_limit_tests {
    use super::*;
    use crate::db::Db;
    use axum::http::StatusCode;

    struct TestProxy {
        root: String,
        _dir: tempfile::TempDir,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for TestProxy {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn start_proxy() -> TestProxy {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.sqlite")).unwrap();
        let state = AppState::new(db, reqwest::Client::new(), dir.path().to_path_buf(), None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = router(state);
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        TestProxy {
            root: format!("http://{address}"),
            _dir: dir,
            task,
        }
    }

    fn large_body(mebibytes: usize) -> bytes::Bytes {
        serde_json::to_vec(&serde_json::json!({
            "model": "unknown-model",
            "input": "x".repeat(mebibytes * 1024 * 1024),
        }))
        .unwrap()
        .into()
    }

    async fn assert_codex_body_accepted(mebibytes: usize, chunked: bool) {
        let proxy = start_proxy().await;
        let body = large_body(mebibytes);
        let client = reqwest::Client::new();
        for endpoint in ["/clients/codex/v1/responses", "/clients/codex/responses"] {
            let request_body = if chunked {
                let chunks = (0..body.len())
                    .step_by(64 * 1024)
                    .map(|start| {
                        Ok::<_, std::io::Error>(
                            body.slice(start..(start + 64 * 1024).min(body.len())),
                        )
                    })
                    .collect::<Vec<_>>();
                reqwest::Body::wrap_stream(futures::stream::iter(chunks))
            } else {
                reqwest::Body::from(body.clone())
            };
            let response = client
                .post(format!("{}{endpoint}", proxy.root))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(request_body)
                .send()
                .await
                .unwrap();
            // A structured model-selection error proves extraction completed,
            // without sending the synthetic history to an actual provider.
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{endpoint}");
            let error: serde_json::Value = response.json().await.unwrap();
            assert_eq!(
                error["error"]["type"], "invalid_request_error",
                "{endpoint}"
            );
        }
    }

    #[tokio::test]
    async fn codex_responses_aliases_accept_bodies_above_default_limit() {
        assert_codex_body_accepted(3, false).await;
    }

    #[tokio::test]
    async fn codex_responses_accept_bodies_above_previous_64_mib_limit() {
        assert_codex_body_accepted(65, false).await;
    }

    #[tokio::test]
    async fn codex_responses_accept_large_chunked_bodies_without_content_length() {
        assert_codex_body_accepted(65, true).await;
    }

    #[tokio::test]
    async fn other_routes_retain_the_default_body_limit() {
        let proxy = start_proxy().await;
        // The JSON envelope takes this just over 2 MiB. Keeping the unread
        // tail small avoids an HTTP/1 connection reset on early rejection.
        let body = large_body(2);
        assert!(body.len() > 2 * 1024 * 1024);
        let client = reqwest::Client::new();
        for endpoint in [
            "/v1/responses",
            "/responses",
            "/v1/messages",
            "/v1/chat/completions",
            "/clients/opencode/v1/responses",
            "/clients/opencode/responses",
            "/clients/claude-code/v1/responses",
            "/clients/codex/v1/messages",
            "/clients/codex/v1/chat/completions",
        ] {
            let response = client
                .post(format!("{}{endpoint}", proxy.root))
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body.clone())
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::PAYLOAD_TOO_LARGE,
                "{endpoint}"
            );
        }
    }

    #[tokio::test]
    async fn codex_responses_still_reject_invalid_json_and_missing_models() {
        let proxy = start_proxy().await;
        let client = reqwest::Client::new();
        for endpoint in ["/clients/codex/v1/responses", "/clients/codex/responses"] {
            for body in ["{", r#"{"input":"hello"}"#] {
                let response = client
                    .post(format!("{}{endpoint}", proxy.root))
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body)
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{endpoint}");
            }
        }
    }
}
