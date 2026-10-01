mod claude_code;
mod codex;
mod opencode;

use crate::{
    domain::terminal::Profile,
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

/// Files a configurator wants written: side files first, then the main config content.
pub struct Built {
    pub extra: Vec<FileChange>,
    pub main: Vec<u8>,
}

/// One terminal client (Codex, Claude Code, OpenCode). Adding a client means adding one
/// implementation and one arm in [`configurator`].
pub trait ClientConfigurator: Sync {
    fn config_path(&self) -> Result<PathBuf>;
    /// Whether `source` (the current config file content) already points at `endpoint`.
    fn is_active(&self, path: &Path, source: &str, endpoint: &str) -> Result<bool>;
    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built>;
    /// The main config reduced to the settings yi owns, safe to show to the user.
    fn owned_preview(&self, content: &[u8]) -> Result<String>;
}

pub fn configurator(client: &str) -> Result<&'static dyn ClientConfigurator> {
    match client {
        "codex" => Ok(&codex::Codex),
        "claude-code" => Ok(&claude_code::ClaudeCode),
        "opencode" => Ok(&opencode::OpenCode),
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
