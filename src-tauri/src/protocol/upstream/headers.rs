use reqwest::header::{HeaderMap, HeaderName};

fn hop_by_hop(name: &HeaderName, headers: &HeaderMap) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "proxy-connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    ) || headers.get_all("connection").iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case(name.as_str()))
        })
    })
}

pub fn response(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for (name, value) in headers {
        // The wrapped body must reach EOF to finish monitoring and usage recording.
        if !hop_by_hop(name, headers) && name.as_str() != "content-length" {
            forwarded.append(name.clone(), value.clone());
        }
    }
    forwarded
}

pub fn request(incoming: &HeaderMap, provider: HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for (name, value) in incoming {
        // The upstream has its own credentials, host and reserialized JSON body.
        if !hop_by_hop(name, incoming)
            && !matches!(
                name.as_str(),
                "host"
                    | "authorization"
                    | "x-api-key"
                    | "api-key"
                    | "x-goog-api-key"
                    | "cookie"
                    | "content-length"
                    | "content-encoding"
                    | "content-type"
                    | "expect"
            )
        {
            forwarded.append(name.clone(), value.clone());
        }
    }
    // Explicit Provider headers override client headers, including User-Agent.
    forwarded.extend(provider);
    forwarded
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn request_keeps_client_identity_and_uses_provider_credentials_and_overrides() {
        let incoming: HeaderMap = [
            ("user-agent", "codex_cli_rs/0.158.0"),
            ("originator", "codex_cli_rs"),
            ("session_id", "session-test"),
            ("x-codex-turn-id", "turn-test"),
            ("authorization", "Bearer client-secret"),
            ("x-api-key", "client-secret"),
            ("cookie", "client-secret"),
            ("host", "127.0.0.1:11435"),
            ("content-length", "99999"),
            ("content-encoding", "gzip"),
            ("connection", "keep-alive, X-Local-Hop"),
            ("x-local-hop", "local-only"),
            ("transfer-encoding", "chunked"),
        ]
        .into_iter()
        .map(|(name, value)| {
            (
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            )
        })
        .collect();
        let provider = crate::protocol::upstream::openai_chat::client::build_headers(
            "provider-secret",
            &serde_json::json!({"originator":"provider-originator"}),
        );
        let forwarded = request(&incoming, provider);
        assert_eq!(forwarded["user-agent"], "codex_cli_rs/0.158.0");
        assert_eq!(forwarded["originator"], "provider-originator");
        assert_eq!(forwarded["session_id"], "session-test");
        assert_eq!(forwarded["x-codex-turn-id"], "turn-test");
        assert_eq!(forwarded["authorization"], "Bearer provider-secret");
        assert_eq!(forwarded["content-type"], "application/json");
        for name in [
            "x-api-key",
            "cookie",
            "host",
            "content-length",
            "content-encoding",
            "connection",
            "x-local-hop",
            "transfer-encoding",
        ] {
            assert!(!forwarded.contains_key(name), "unexpected header: {name}");
        }
        assert!(!request(&incoming, HeaderMap::new()).contains_key("authorization"));
    }

    #[test]
    fn response_preserves_diagnostics_and_duplicate_headers_but_removes_hop_headers() {
        let mut incoming = HeaderMap::new();
        incoming.insert("retry-after", HeaderValue::from_static("12"));
        incoming.insert("x-request-id", HeaderValue::from_static("upstream-test"));
        incoming.insert("connection", HeaderValue::from_static("X-Internal-Hop"));
        incoming.insert("x-internal-hop", HeaderValue::from_static("local-only"));
        incoming.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        incoming.insert("content-length", HeaderValue::from_static("128"));
        incoming.append("www-authenticate", HeaderValue::from_static("Bearer"));
        incoming.append("www-authenticate", HeaderValue::from_static("Basic"));
        let forwarded = response(&incoming);
        assert_eq!(forwarded["retry-after"], "12");
        assert_eq!(forwarded["x-request-id"], "upstream-test");
        assert_eq!(forwarded.get_all("www-authenticate").iter().count(), 2);
        for name in [
            "connection",
            "x-internal-hop",
            "transfer-encoding",
            "content-length",
        ] {
            assert!(!forwarded.contains_key(name));
        }
    }
}
