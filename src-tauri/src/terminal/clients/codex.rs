use super::{table, user_home, Built, ClientConfigurator, PlanInput};
use crate::{
    domain::capabilities::{Modality, Support},
    error::{AppError, Result},
    terminal::changes::FileChange,
};
use serde_json::{json, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use toml_edit::{value, DocumentMut, Item, Table};

pub struct Codex;

const BASE_INSTRUCTIONS: &str = "You are a coding assistant. Work with the user in the shared workspace, use the available tools to inspect and edit files, and verify your changes.";

fn parse_toml(source: &str, context: &str) -> Result<DocumentMut> {
    source
        .parse::<DocumentMut>()
        .map_err(|error| AppError::validation(format!("{context}{error}")))
}

impl ClientConfigurator for Codex {
    fn config_path(&self) -> Result<PathBuf> {
        Ok(env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or(user_home()?.join(".codex"))
            .join("config.toml"))
    }

    fn is_active(&self, _path: &Path, source: &str, endpoint: &str) -> Result<bool> {
        let doc = parse_toml(source, "")?;
        Ok(
            doc.get("model_provider").and_then(Item::as_str) == Some("yi")
                && doc
                    .get("model_providers")
                    .and_then(|p| p.get("yi"))
                    .and_then(|p| p.get("base_url"))
                    .and_then(Item::as_str)
                    == Some(endpoint)
                && doc
                    .get("model_catalog_json")
                    .and_then(Item::as_str)
                    .is_some_and(|path| Path::new(path).is_file()),
        )
    }

    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built> {
        let profile = input.profile;
        let models = input.models;
        let catalog_path = path.with_file_name("yi-models.json");
        let legacy_catalog_path = path.with_file_name("llm-man-models.json");
        let mut files = Vec::new();
        let mut doc = if source.is_empty() {
            DocumentMut::new()
        } else {
            parse_toml(source, "Codex TOML 无法解析: ")?
        };
        let legacy_provider = doc
            .get("model_providers")
            .and_then(|providers| providers.get("llm-man"));
        let legacy_owned = doc.get("model_provider").and_then(Item::as_str) == Some("llm-man")
            && legacy_provider
                .and_then(|provider| provider.get("name"))
                .and_then(Item::as_str)
                == Some("LLM Man")
            && legacy_provider
                .and_then(|provider| provider.get("wire_api"))
                .and_then(Item::as_str)
                == Some("responses");
        let legacy_catalog_owned = legacy_owned
            && doc.get("model_catalog_json").and_then(Item::as_str)
                == Some(legacy_catalog_path.to_string_lossy().as_ref());
        if legacy_owned {
            if let Some(providers) = doc.get_mut("model_providers").and_then(Item::as_table_mut) {
                providers.remove("llm-man");
            }
        }
        if legacy_catalog_owned && legacy_catalog_path.exists() {
            files.push(FileChange::delete(legacy_catalog_path)?);
        }
        doc["model"] = value(&profile.default_model);
        doc["model_provider"] = value("yi");
        doc["model_catalog_json"] = value(catalog_path.display().to_string());
        let provider = table(table(doc.as_table_mut(), "model_providers")?, "yi")?;
        provider["name"] = value("yi");
        provider["base_url"] = value(input.endpoint);
        provider["wire_api"] = value("responses");
        provider["requires_openai_auth"] = value(false);
        for key in [
            "env_key",
            "env_key_instructions",
            "experimental_bearer_token",
            "http_headers",
            "env_http_headers",
        ] {
            provider.remove(key);
        }
        let default = models
            .iter()
            .find(|model| model.selection.model == profile.default_model)
            .ok_or_else(|| AppError::validation("默认模型必须在所选模型集合中"))?;
        let default_caps = &default.mapping.capabilities;
        match default_caps.effort.support {
            Support::Supported => {
                let levels = default_caps.efforts_for("responses", &default.provider.provider_type);
                match default_caps
                    .effort
                    .default
                    .as_ref()
                    .filter(|level| levels.contains(level))
                {
                    Some(level) => doc["model_reasoning_effort"] = value(level),
                    None => {
                        doc.remove("model_reasoning_effort");
                    }
                }
            }
            Support::Unsupported | Support::Unknown => {
                doc.remove("model_reasoning_effort");
            }
        }
        // Emit native ModelInfo fields from declared capabilities and the actual route.
        let catalog: Vec<_> = models
            .iter()
            .enumerate()
            .map(|(index, model)| {
                let caps = &model.mapping.capabilities;
                let upstream = &model.provider.provider_type;
                let slug = &model.selection.model;
                let levels = caps.efforts_for("responses", upstream);
                let mut entry = json!({
                    "slug":slug,"display_name":slug,
                    "description":format!("yi · {}",model.provider.name),
                    "supported_reasoning_levels":levels.iter().map(|level| json!({"effort":level,"description":format!("{level} effort")})).collect::<Vec<_>>(),
                    "default_reasoning_level":caps.effort.default.as_ref().filter(|level| levels.contains(level)),
                    "shell_type":"default","visibility":"list","supported_in_api":true,"priority":index,
                    "availability_nux":null,"upgrade":null,
                    "base_instructions":BASE_INSTRUCTIONS,
                    "supports_reasoning_summaries":false,"support_verbosity":false,"default_verbosity":null,
                    "apply_patch_tool_type":null,"truncation_policy":{"mode":"bytes","limit":10000},
                    "supports_parallel_tool_calls":false,"experimental_supported_tools":[],
                    "input_modalities":caps.input_for("responses", upstream).into_iter().filter(|modality| matches!(modality, Modality::Text | Modality::Image)).collect::<Vec<_>>()
                });
                if let Some(context) = caps.context_window {
                    entry["context_window"] = json!(context);
                    entry["max_context_window"] = json!(context);
                }
                entry
            })
            .collect();
        files.push(FileChange::new(
            catalog_path,
            serde_json::to_vec_pretty(&json!({"models":catalog}))?,
        )?);
        // Keep the desktop app's recent-model picker in step with the new default.
        let state_path = path.with_file_name(".codex-global-state.json");
        if let Ok(raw) = fs::read(&state_path) {
            if let Ok(mut state) = serde_json::from_slice::<Value>(&raw) {
                if let Some(first) = state
                    .get_mut("composer-recent-model-configurations-v1")
                    .and_then(Value::as_array_mut)
                    .and_then(|a| a.first_mut())
                    .filter(|v| v.is_object())
                {
                    first["model"] = json!(profile.default_model);
                    files.push(FileChange::new(
                        state_path,
                        serde_json::to_vec_pretty(&state)?,
                    )?);
                }
            }
        }
        Ok(Built {
            extra: files,
            main: doc.to_string().into_bytes(),
        })
    }

    fn owned_preview(&self, content: &[u8]) -> Result<String> {
        let source = String::from_utf8_lossy(content);
        let doc = parse_toml(&source, "")?;
        let mut owned = DocumentMut::new();
        for key in [
            "model",
            "model_provider",
            "model_catalog_json",
            "model_reasoning_effort",
        ] {
            if let Some(item) = doc.get(key) {
                owned[key] = item.clone();
            }
        }
        owned["model_providers"] = Item::Table(Table::new());
        let provider = &doc["model_providers"]["yi"];
        let mut yi = Table::new();
        for key in ["name", "base_url", "wire_api", "requires_openai_auth"] {
            yi[key] = provider[key].clone();
        }
        owned["model_providers"]["yi"] = Item::Table(yi);
        Ok(owned.to_string())
    }
}
