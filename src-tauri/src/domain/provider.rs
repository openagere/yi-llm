use crate::domain::capabilities::Capabilities;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provider {
    pub id: String,
    pub provider_type: String, // anthropic | openai_chat | responses
    pub name: String,
    pub short_code: String,
    pub base_url: String,
    pub api_key: String,
    pub enabled: bool,
    pub thinking: String, // minimal | low | medium | high
    pub extra: serde_json::Value,
    pub is_default: bool,
    #[serde(default)]
    pub protocol_support: ProtocolSupport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProtocolSupport {
    pub anthropic: bool,
    pub openai_chat: bool,
    pub responses: bool,
}

impl ProtocolSupport {
    pub fn native(provider_type: &str) -> Self {
        Self {
            anthropic: provider_type == "anthropic",
            openai_chat: provider_type == "openai_chat",
            responses: provider_type == "responses",
        }
    }

    pub fn supports(&self, protocol: &str) -> bool {
        match protocol {
            "anthropic" => self.anthropic,
            "openai_chat" => self.openai_chat,
            "responses" => self.responses,
            _ => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.anthropic && !self.openai_chat && !self.responses
    }

    /// Always includes the provider's native protocol; falls back to it when nothing is declared.
    pub fn normalized(mut self, provider_type: &str) -> Self {
        if self.is_empty() {
            return Self::native(provider_type);
        }
        match provider_type {
            "anthropic" => self.anthropic = true,
            "openai_chat" => self.openai_chat = true,
            "responses" => self.responses = true,
            _ => {}
        }
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMapping {
    pub id: i64,
    pub provider_id: String,
    #[serde(rename = "route_id")]
    pub exposed_name: String,
    #[serde(rename = "name")]
    pub upstream_model: String,
    #[serde(default)]
    pub standard_model_id: Option<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// The mapped upstream implements only a subset of its declared wire protocol.
    #[serde(default)]
    pub non_standard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderView {
    #[serde(flatten)]
    pub provider: Provider,
    pub models: Vec<ModelMapping>,
}
