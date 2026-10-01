use crate::{
    domain::{
        capabilities::Modality,
        provider::{ModelMapping, Provider},
        terminal::{protocol, Profile, Selection},
    },
    error::{AppError, Result},
    proxy::routing::{ResolvedRoute, RouteTable},
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::HashSet;

const CLAUDE_CODE_RESERVED_ALIASES: [&str; 9] = [
    "default",
    "opus",
    "sonnet",
    "haiku",
    "opusplan",
    "best",
    "fable",
    "opus[1m]",
    "sonnet[1m]",
];

#[derive(Debug)]
pub struct ResolvedModel {
    pub selection: Selection,
    pub provider: Provider,
    pub mapping: ModelMapping,
}

impl ResolvedModel {
    pub fn into_route(self) -> ResolvedRoute {
        ResolvedRoute {
            provider: self.provider,
            upstream_model: self.mapping.upstream_model,
            capabilities: self.mapping.capabilities,
            non_standard: self.mapping.non_standard,
            usage_model: self.mapping.exposed_name,
        }
    }
}

fn selected_route(
    table: &RouteTable,
    selection: &Selection,
    protocol: &str,
) -> Result<ResolvedModel> {
    let provider = table.provider(&selection.provider_id).ok_or_else(|| {
        AppError::not_found(format!("模型 {} 的 Provider 已删除", selection.model))
    })?;
    if !provider.enabled || !provider.protocol_support.supports(protocol) {
        return Err(AppError::validation(format!(
            "Provider {} 未启用或不支持 {protocol}",
            provider.name
        )));
    }
    let mapping = table
        .find_model(&provider.id, &selection.model)
        .ok_or_else(|| {
            AppError::not_found(format!(
                "模型 {} 已被删除或重新映射，请更新终端配置",
                selection.model
            ))
        })?;
    Ok(ResolvedModel {
        selection: selection.clone(),
        provider: provider.clone(),
        mapping: mapping.clone(),
    })
}

pub fn validate_in(table: &RouteTable, profile: &Profile) -> Result<Vec<ResolvedModel>> {
    let protocol = protocol(&profile.client)?;
    if profile.models.is_empty() {
        return Err("请至少选择一个模型".into());
    }
    if profile.models.len() > 500 {
        return Err("最多配置 500 个模型".into());
    }
    let mut seen = HashSet::new();
    let mut resolved = Vec::new();
    for selection in &profile.models {
        if selection.model.trim().is_empty() || !seen.insert(&selection.model) {
            return Err("模型名为空或重复".into());
        }
        if profile.client == "claude-code"
            && CLAUDE_CODE_RESERVED_ALIASES.contains(&selection.model.as_str())
        {
            return Err(AppError::validation(format!(
                "Claude Code 会解析内置别名「{}」，请为该模型使用独立的客户端模型名",
                selection.model
            )));
        }
        let model = selected_route(table, selection, protocol)?;
        let capabilities = &model.mapping.capabilities;
        let upstream = &model.provider.provider_type;
        if !capabilities
            .input_for(protocol, upstream)
            .contains(&Modality::Text)
            || !capabilities
                .output_for(protocol, upstream)
                .contains(&Modality::Text)
        {
            return Err(AppError::validation(format!(
                "终端模型 {} 必须支持文字输入和输出",
                selection.model
            )));
        }
        resolved.push(model);
    }
    if !seen.contains(&profile.default_model) {
        return Err("默认模型必须在所选模型集合中".into());
    }
    Ok(resolved)
}

/// Resolves a model requested through a client entrypoint to the provider route saved in
/// that client's profile.
pub fn resolve_in(
    table: &RouteTable,
    client: &str,
    model: &str,
    requested_protocol: &str,
) -> Result<ResolvedModel> {
    if protocol(client)? != requested_protocol {
        return Err("该终端入口不支持请求的协议".into());
    }
    let profile = table.profile(client).ok_or("该终端尚未配置模型集合")?;
    let mut selections = profile.models.iter().filter(|m| m.model == model);
    let selection = selections.next().ok_or("该模型不在终端的模型集合中")?;
    if selections.next().is_some() {
        return Err("终端模型名称映射存在冲突，请重新配置终端模型集合".into());
    }
    selected_route(table, selection, requested_protocol)
}

pub fn model_list_in(table: &RouteTable, client: &str) -> Result<Value> {
    let protocol = protocol(client)?;
    let profile = table.profile(client).ok_or("该终端尚未配置模型集合")?;
    let data: Vec<_> = profile
        .models
        .iter()
        .filter_map(|selection| {
            selected_route(table, selection, protocol).ok().map(|model| {
                json!({"id":selection.model,"object":"model","created":0,"owned_by":model.provider.name})
            })
        })
        .collect();
    Ok(json!({"object":"list","data":data}))
}

pub fn validate(conn: &Connection, profile: &Profile) -> Result<Vec<ResolvedModel>> {
    validate_in(&RouteTable::load(conn)?, profile)
}

pub fn resolve(
    conn: &Connection,
    client: &str,
    model: &str,
    requested_protocol: &str,
) -> Result<(Provider, String)> {
    resolve_in(&RouteTable::load(conn)?, client, model, requested_protocol)
        .map(|model| (model.provider, model.mapping.upstream_model))
}

pub fn model_list(conn: &Connection, client: &str) -> Result<Value> {
    model_list_in(&RouteTable::load(conn)?, client)
}
