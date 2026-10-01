use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerSettings {
    pub host: String,
    pub port: u16,
    pub log_level: String,
}
