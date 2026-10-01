use crate::{
    domain::capabilities::Capabilities,
    error::{AppError, Result},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StandardModel {
    pub id: String,
    pub name: String,
    pub protocol: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(default)]
    pub provider_count: u32,
}

pub fn validate_definition(model: &StandardModel) -> Result<()> {
    if model.id.trim().is_empty() || model.name.trim().is_empty() {
        return Err(AppError::validation("请填写标准模型名称"));
    }
    if !["anthropic", "responses", "openai_chat"].contains(&model.protocol.as_str()) {
        return Err(AppError::validation("模型协议无效"));
    }
    model
        .capabilities
        .validate(&model.protocol)
        .map_err(AppError::validation)?;
    if model.brand.len() > 64
        || !model
            .brand
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'-' || ch == b'_')
    {
        return Err(AppError::validation("品牌图标标识无效"));
    }
    if model.id.contains('\0') || model.name.contains('\0') || model.id != model.id.trim() {
        return Err(AppError::validation("模型标识或名称无效"));
    }
    Ok(())
}
