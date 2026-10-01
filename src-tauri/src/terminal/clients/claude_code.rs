use super::{object, parse_json, user_home, Built, ClientConfigurator, PlanInput};
use crate::error::{AppError, Result};
use serde_json::{json, Value};
use std::{
    env,
    path::{Path, PathBuf},
};

pub struct ClaudeCode;

impl ClientConfigurator for ClaudeCode {
    fn config_path(&self) -> Result<PathBuf> {
        Ok(env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or(user_home()?.join(".claude"))
            .join("settings.json"))
    }

    fn is_active(&self, path: &Path, source: &str, endpoint: &str) -> Result<bool> {
        let doc = parse_json(path, source)?;
        Ok(doc
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            == Some(endpoint))
    }

    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built> {
        let mut doc = if source.is_empty() {
            json!({})
        } else {
            parse_json(path, source)?
        };
        doc["model"] = json!(input.profile.default_model);
        let environment = object(&mut doc, "env")?;
        environment["ANTHROPIC_BASE_URL"] = json!(input.endpoint);
        environment["ANTHROPIC_AUTH_TOKEN"] = json!("yi");
        // A fixed picker includes arbitrary aliases; gateway discovery filters
        // non-Anthropic names in Claude Code and cannot represent this collection.
        let picker = object(&mut doc, "modelPicker")?;
        picker["replaceBuiltInOptions"] = json!(true);
        picker["options"] = Value::Array(
            input
                .models
                .iter()
                .map(|model| json!({"model":model.selection.model,"label":model.selection.model}))
                .collect(),
        );
        Ok(Built {
            extra: Vec::new(),
            main: serde_json::to_string_pretty(&doc)?.into_bytes(),
        })
    }

    fn owned_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        let owned = json!({
            "model":doc["model"],
            "env":{
                "ANTHROPIC_BASE_URL":doc["env"]["ANTHROPIC_BASE_URL"],
                "ANTHROPIC_AUTH_TOKEN":doc["env"]["ANTHROPIC_AUTH_TOKEN"]
            },
            "modelPicker":doc["modelPicker"]
        });
        serde_json::to_string_pretty(&owned).map_err(AppError::from)
    }
}
