use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

pub const CLIENTS: [&str; 3] = ["codex", "claude-code", "opencode"];

pub fn protocol(client: &str) -> Result<&'static str> {
    match client {
        "codex" => Ok("responses"),
        "claude-code" => Ok("anthropic"),
        "opencode" => Ok("openai_chat"),
        _ => Err(AppError::validation("未知终端类型")),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Selection {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub client: String,
    pub models: Vec<Selection>,
    pub default_model: String,
}
