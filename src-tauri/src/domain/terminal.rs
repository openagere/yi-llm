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

/// A terminal-native provider configuration. The API key remains owned by the provider
/// record and is resolved only while rendering the terminal config.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectProfile {
    pub client: String,
    pub provider_id: String,
    /// `native` lets Codex/Claude keep their built-in model choices; `provider` uses
    /// upstream model IDs maintained by the selected provider.
    pub model_source: String,
    /// Startup upstream model for the `provider` model source, never a yi-llm route alias.
    /// `None` (or empty) syncs every maintained upstream model and defaults the
    /// startup model to the provider's first one.
    #[serde(default)]
    pub model: Option<String>,
}
