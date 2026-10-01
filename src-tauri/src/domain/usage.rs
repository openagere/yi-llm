use serde::{Deserialize, Serialize};

/// One finished request waiting to be persisted.
#[derive(Debug, Clone)]
pub struct NewUsage {
    pub provider_name: String,
    pub protocol: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
    pub reasoning_tokens: u32,
    pub requested_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: i64,
    pub requested_at: i64,
    pub provider_name: String,
    pub protocol: String,
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
    pub reasoning_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageDay {
    pub day: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageDailyModel {
    pub day: String,
    pub model: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageModel {
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSnapshot {
    pub total_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub reasoning_tokens: u64,
    pub days: Vec<UsageDay>,
    pub daily_models: Vec<UsageDailyModel>,
    pub models: Vec<UsageModel>,
    pub recent: Vec<UsageRecord>,
}
