mod claude_code;
mod codex;
mod deepseek_harness;
mod opencode;
mod pi;

use crate::{
    domain::{
        provider::ProviderView,
        terminal::{DirectProfile, Profile},
    },
    error::{AppError, Result},
    terminal::{changes::FileChange, resolve::ResolvedModel},
};
use serde_json::{json, Value};
use std::{
    env,
    path::{Path, PathBuf},
};
use toml_edit::{Item, Table};

/// Everything a configurator needs to render a client's configuration.
pub struct PlanInput<'a> {
    pub profile: &'a Profile,
    pub models: &'a [ResolvedModel],
    pub endpoint: &'a str,
}

pub struct DirectPlanInput<'a> {
    pub profile: &'a DirectProfile,
    pub provider: &'a ProviderView,
    pub proxy_profile: Option<&'a Profile>,
    pub previous_direct_profile: Option<&'a DirectProfile>,
    pub previous_direct_provider: Option<&'a ProviderView>,
}

/// Files a configurator wants written: side files first, then the main config content.
pub struct Built {
    pub extra: Vec<FileChange>,
    pub main: Vec<u8>,
}

/// One terminal client (Codex, Claude Code, OpenCode). Adding a client means adding one
/// implementation and one arm in [`configurator`].
pub trait ClientConfigurator: Sync {
    fn config_path(&self) -> Result<PathBuf>;
    /// Whether `source` (the current config file content) already points at `endpoint`
    /// for the selected client protocol.
    fn is_active(&self, path: &Path, source: &str, endpoint: &str, protocol: &str) -> Result<bool>;
    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built>;
    fn build_direct(&self, input: &DirectPlanInput<'_>, path: &Path, source: &str)
        -> Result<Built>;
    fn owned_direct_preview(&self, content: &[u8]) -> Result<String>;
    fn is_direct_active(
        &self,
        path: &Path,
        source: &str,
        profile: &DirectProfile,
        provider: &ProviderView,
    ) -> Result<bool>;
    /// The main config reduced to the settings yi owns, safe to show to the user.
    fn owned_preview(&self, content: &[u8]) -> Result<String>;
    /// Preview for a side file this client owns; `None` falls back to the raw content.
    fn extra_preview(&self, _path: &Path, _content: &[u8]) -> Result<Option<String>> {
        Ok(None)
    }
}

pub fn redact_endpoint(value: &str) -> String {
    let query_at = value.find('?');
    let fragment_at = value.find('#');
    let end = [query_at, fragment_at]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(value.len());
    let mut safe = value[..end].to_owned();
    if let Some(scheme_end) = safe.find("://") {
        let authority_start = scheme_end + 3;
        let authority_end = safe[authority_start..]
            .find('/')
            .map_or(safe.len(), |offset| authority_start + offset);
        if let Some(userinfo_end) = safe[authority_start..authority_end].rfind('@') {
            safe.replace_range(authority_start..authority_start + userinfo_end + 1, "");
        }
    }
    if query_at.is_some() {
        safe.push_str("?…");
    }
    safe
}

/// Startup model for `model_source == "provider"`. Every maintained upstream model is
/// always written to the terminal config; an explicitly stored model ID wins, otherwise
/// the provider's first upstream model becomes the startup default.
pub fn direct_startup_model<'a>(
    profile: &'a DirectProfile,
    provider: &'a ProviderView,
) -> Result<&'a str> {
    if let Some(model) = profile
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
    {
        return Ok(model);
    }
    provider
        .models
        .first()
        .map(|model| model.upstream_model.as_str())
        .filter(|model| !model.is_empty())
        .ok_or_else(|| AppError::validation("该 Provider 尚未维护上游模型，无法使用 Provider 模型"))
}
pub fn direct_provider_id(provider_id: &str) -> String {
    let safe: String = provider_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    format!("yi_direct_{safe}")
}

pub fn configurator(client: &str) -> Result<&'static dyn ClientConfigurator> {
    match client {
        "codex" => Ok(&codex::Codex),
        "claude-code" => Ok(&claude_code::ClaudeCode),
        "opencode" => Ok(&opencode::OpenCode),
        "pi" => Ok(&pi::Pi),
        "deepseek-harness" => Ok(&deepseek_harness::DeepSeekHarness),
        _ => Err(AppError::validation("未知终端类型")),
    }
}

fn user_home() -> Result<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| AppError::validation("无法确定用户目录"))
}

pub fn parse_json(path: &Path, source: &str) -> Result<Value> {
    let value: Value = if path.extension().is_some_and(|s| s == "jsonc") {
        json5::from_str(source).map_err(|error| AppError::validation(error.to_string()))?
    } else {
        serde_json::from_str(source).map_err(|error| AppError::validation(error.to_string()))?
    };
    if !value.is_object() {
        return Err("终端配置必须是 JSON 对象".into());
    }
    Ok(value)
}

fn object<'a>(value: &'a mut Value, key: &str) -> Result<&'a mut Value> {
    let map = value.as_object_mut().ok_or("配置结构不是对象")?;
    let entry = map.entry(key).or_insert_with(|| json!({}));
    if !entry.is_object() {
        return Err(AppError::validation(format!(
            "配置 {key} 不是对象，未修改文件"
        )));
    }
    Ok(entry)
}

fn table<'a>(table: &'a mut Table, key: &str) -> Result<&'a mut Table> {
    if !table.contains_key(key) {
        table.insert(key, Item::Table(Table::new()));
    }
    table
        .get_mut(key)
        .and_then(Item::as_table_mut)
        .ok_or_else(|| AppError::validation(format!("配置 {key} 不是 TOML 表")))
}
