use crate::error::AppError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Error rendered in the wire format of the protocol the client spoke.
#[derive(Debug)]
pub struct ProxyError {
    pub status: StatusCode,
    pub message: String,
}

impl ProxyError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }

    pub fn upstream(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::new(upstream_status(&message), message)
    }

    pub fn into_response_for(self, protocol: &str) -> Response {
        protocol_error(protocol, self.status, &self.message)
    }
}

impl From<AppError> for ProxyError {
    fn from(error: AppError) -> Self {
        Self::internal(error.to_string())
    }
}

pub fn protocol_error(protocol: &str, status: StatusCode, message: &str) -> Response {
    if protocol == "anthropic" {
        let error_type = match status {
            StatusCode::BAD_REQUEST => "invalid_request_error",
            StatusCode::UNAUTHORIZED => "authentication_error",
            StatusCode::FORBIDDEN => "permission_error",
            StatusCode::NOT_FOUND => "not_found_error",
            StatusCode::TOO_MANY_REQUESTS => "rate_limit_error",
            _ => "api_error",
        };
        return (
            status,
            Json(json!({
                "type":"error",
                "error":{"type":error_type,"message":message}
            })),
        )
            .into_response();
    }
    let error_type = if status.is_client_error() {
        "invalid_request_error"
    } else {
        "server_error"
    };
    (
        status,
        Json(json!({"error":{"message":message,"type":error_type}})),
    )
        .into_response()
}

pub fn upstream_status(message: &str) -> StatusCode {
    if message.contains("上游请求超时") {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::BAD_GATEWAY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeouts_map_to_gateway_timeout_and_others_to_bad_gateway() {
        assert_eq!(
            upstream_status("上游请求超时: x"),
            StatusCode::GATEWAY_TIMEOUT
        );
        assert_eq!(upstream_status("连接上游失败"), StatusCode::BAD_GATEWAY);
        assert_eq!(
            ProxyError::upstream("上游请求超时").status,
            StatusCode::GATEWAY_TIMEOUT
        );
    }

    #[tokio::test]
    async fn anthropic_errors_use_the_anthropic_envelope() {
        let response = protocol_error("anthropic", StatusCode::BAD_REQUEST, "bad");
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["error"]["type"], "invalid_request_error");
    }
}
