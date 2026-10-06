use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};

pub const CLIENTS: [&str; 5] = ["codex", "claude-code", "opencode", "pi", "deepseek-harness"];

/// Client protocols a terminal can speak to the proxy. Pi natively supports all three;
/// every other client is pinned to the one protocol its managed configuration writes.
pub fn protocols(client: &str) -> Result<&'static [&'static str]> {
    match client {
        "codex" => Ok(&["responses"]),
        "claude-code" => Ok(&["anthropic"]),
        "opencode" => Ok(&["openai_chat", "responses", "anthropic"]),
        "pi" => Ok(&["openai_chat", "responses", "anthropic"]),
        "deepseek-harness" => Ok(&["openai_chat", "responses", "anthropic"]),
        _ => Err(AppError::validation("未知终端类型")),
    }
}

/// The terminal's primary protocol: the default for multi-protocol clients and the only
/// protocol for every other client.
pub fn protocol(client: &str) -> Result<&'static str> {
    protocols(client).map(|allowed| allowed[0])
}

/// The protocol a stored terminal profile speaks. Multi-protocol clients keep their
/// selection in the profile; single-protocol clients ignore it. Every other value is
/// rejected so an unsupported choice cannot reach configuration or routing.
pub fn selected_protocol<'a>(client: &str, selected: Option<&'a str>) -> Result<&'a str> {
    let allowed: &[&str] = protocols(client)?;
    match selected {
        Some(selected) if allowed.contains(&selected) => Ok(selected),
        Some(_) => Err(AppError::validation("该终端不支持所选协议")),
        None => Ok(allowed[0]),
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
    /// Client protocol a multi-protocol terminal speaks to the proxy. `None` means the
    /// client's primary protocol; single-protocol clients always leave it empty.
    #[serde(default)]
    pub protocol: Option<String>,
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
