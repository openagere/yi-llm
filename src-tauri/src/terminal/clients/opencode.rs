use super::{
    direct_provider_id, direct_startup_model, object, parse_json, user_home, Built,
    ClientConfigurator, DirectPlanInput, PlanInput,
};
use crate::error::{AppError, Result};
use serde_json::{json, Value};
use std::{
    env,
    path::{Path, PathBuf},
};

pub struct OpenCode;

impl ClientConfigurator for OpenCode {
    fn config_path(&self) -> Result<PathBuf> {
        if let Some(path) = env::var_os("OPENCODE_CONFIG") {
            return Ok(PathBuf::from(path));
        }
        let dir = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or(user_home()?.join(".config"))
            .join("opencode");
        let json = dir.join("opencode.json");
        let jsonc = dir.join("opencode.jsonc");
        Ok(if jsonc.exists() { jsonc } else { json })
    }

    fn is_active(&self, path: &Path, source: &str, endpoint: &str) -> Result<bool> {
        let doc = parse_json(path, source)?;
        Ok(doc
            .pointer("/provider/yi/options/baseURL")
            .and_then(Value::as_str)
            == Some(endpoint))
    }

    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built> {
        let mut doc = if source.is_empty() {
            json!({})
        } else {
            parse_json(path, source)?
        };
        let legacy_owned = doc["model"]
            .as_str()
            .is_some_and(|model| model.starts_with("llm-man/"))
            && doc
                .pointer("/provider/llm-man/name")
                .and_then(Value::as_str)
                == Some("LLM Man")
            && doc.pointer("/provider/llm-man/npm").and_then(Value::as_str)
                == Some("@ai-sdk/openai-compatible");
        if let Some(providers) = doc.pointer_mut("/provider").and_then(Value::as_object_mut) {
            if legacy_owned {
                providers.remove("llm-man");
            }
            providers.retain(|id, provider| {
                !(id.starts_with("yi_direct_")
                    && provider
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.starts_with("yi-llm direct:")))
            });
        }
        doc["$schema"] = json!("https://opencode.ai/config.json");
        doc["model"] = json!(format!("yi/{}", input.profile.default_model));
        let provider = object(object(&mut doc, "provider")?, "yi")?;
        provider["npm"] = json!("@ai-sdk/openai-compatible");
        provider["name"] = json!("yi");
        let options = object(provider, "options")?;
        options["baseURL"] = json!(input.endpoint);
        options["apiKey"] = json!("yi");
        provider["models"] = Value::Object(
            input
                .models
                .iter()
                .map(|model| {
                    let caps = &model.mapping.capabilities;
                    let upstream = &model.provider.provider_type;
                    let mut entry = json!({"name":model.selection.model});
                    entry["modalities"] = json!({
                        "input":caps.input_for("openai_chat", upstream),
                        "output":caps.output_for("openai_chat", upstream)
                    });
                    // OpenCode's native schema requires both limit fields.
                    if let (Some(context), Some(output)) =
                        (caps.context_window, caps.max_output_tokens)
                    {
                        entry["limit"] = json!({"context":context,"output":output});
                    }
                    let levels = caps.efforts_for("openai_chat", upstream);
                    if !levels.is_empty() {
                        entry["reasoning"] = json!(true);
                        entry["variants"] = Value::Object(
                            levels
                                .iter()
                                .map(|level| (level.clone(), json!({"reasoningEffort":level})))
                                .collect(),
                        );
                        if let Some(default) = caps
                            .effort
                            .default
                            .as_ref()
                            .filter(|level| levels.contains(level))
                        {
                            entry["options"] = json!({"reasoningEffort":default});
                        }
                    }
                    (model.selection.model.clone(), entry)
                })
                .collect(),
        );
        Ok(Built {
            extra: Vec::new(),
            main: serde_json::to_string_pretty(&doc)?.into_bytes(),
        })
    }

    fn build_direct(
        &self,
        input: &DirectPlanInput<'_>,
        path: &Path,
        source: &str,
    ) -> Result<Built> {
        let mut doc = if source.is_empty() {
            json!({})
        } else {
            parse_json(path, source)?
        };
        doc["$schema"] = json!("https://opencode.ai/config.json");
        let provider = &input.provider.provider;
        let id = direct_provider_id(&provider.id);
        let providers = object(&mut doc, "provider")?;
        if let Some(existing) = providers.get(&id) {
            if existing.get("name").and_then(Value::as_str)
                != Some(format!("yi-llm direct:{}", provider.id).as_str())
            {
                return Err(AppError::conflict(format!(
                    "OpenCode Provider 标识 {id} 已被其他配置占用"
                )));
            }
        }
        let entry = object(providers, &id)?;
        entry["npm"] = json!(if provider.provider_type == "responses" {
            "@ai-sdk/openai"
        } else {
            "@ai-sdk/openai-compatible"
        });
        entry["name"] = json!(format!("yi-llm direct:{}", provider.id));
        let options = object(entry, "options")?;
        options["baseURL"] = json!(provider.base_url.trim_end_matches('/'));
        if provider.api_key.trim().is_empty() {
            options.as_object_mut().unwrap().remove("apiKey");
        } else {
            options["apiKey"] = json!(provider.api_key);
        }
        let mut models = serde_json::Map::new();
        for model in &input.provider.models {
            let caps = &model.capabilities;
            let upstream = &provider.provider_type;
            let mut entry = json!({"name":model.upstream_model});
            entry["modalities"] = json!({
                "input":caps.input_for(upstream, upstream),
                "output":caps.output_for(upstream, upstream)
            });
            if let (Some(context), Some(output)) = (caps.context_window, caps.max_output_tokens) {
                entry["limit"] = json!({"context":context,"output":output});
            }
            let levels = caps.efforts_for(upstream, upstream);
            if !levels.is_empty() {
                entry["reasoning"] = json!(true);
                entry["variants"] = Value::Object(
                    levels
                        .iter()
                        .map(|level| (level.clone(), json!({"reasoningEffort":level})))
                        .collect(),
                );
                if let Some(default) = caps
                    .effort
                    .default
                    .as_ref()
                    .filter(|level| levels.contains(level))
                {
                    entry["options"] = json!({"reasoningEffort":default});
                }
            }
            models.insert(model.upstream_model.clone(), entry);
        }
        let selected = direct_startup_model(input.profile, input.provider)?;
        models
            .entry(selected.to_owned())
            .or_insert_with(|| json!({"name":selected}));
        entry["models"] = Value::Object(models);
        doc["model"] = json!(format!("{id}/{selected}"));
        Ok(Built {
            extra: Vec::new(),
            main: serde_json::to_vec_pretty(&doc)?,
        })
    }

    fn owned_direct_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        let mut owned = json!({"$schema":doc["$schema"],"model":doc["model"],"provider":{}});
        if let Some(providers) = doc.get("provider").and_then(Value::as_object) {
            for (id, provider) in providers {
                if id.starts_with("yi_direct_") {
                    let mut safe = json!({});
                    for key in ["name", "npm", "models"] {
                        if let Some(value) = provider.get(key) {
                            safe[key] = value.clone();
                        }
                    }
                    let endpoint = provider
                        .pointer("/options/baseURL")
                        .and_then(Value::as_str)
                        .map(super::redact_endpoint);
                    safe["options"] = json!({
                        "baseURL":endpoint,
                        "apiKey":if provider.pointer("/options/apiKey").is_some() { json!("••••••••") } else { Value::Null }
                    });
                    owned["provider"][id] = safe;
                }
            }
        }
        serde_json::to_string_pretty(&owned).map_err(AppError::from)
    }

    fn is_direct_active(
        &self,
        path: &Path,
        source: &str,
        profile: &crate::domain::terminal::DirectProfile,
        provider: &crate::domain::provider::ProviderView,
    ) -> Result<bool> {
        let doc = parse_json(path, source)?;
        let id = direct_provider_id(&profile.provider_id);
        let base_url = doc
            .pointer(&format!("/provider/{id}/options/baseURL"))
            .and_then(Value::as_str);
        let model = doc.get("model").and_then(Value::as_str);
        let key_matches = provider.provider.api_key.trim().is_empty()
            || doc
                .pointer(&format!("/provider/{id}/options/apiKey"))
                .and_then(Value::as_str)
                == Some(provider.provider.api_key.as_str());
        Ok(
            base_url == Some(provider.provider.base_url.trim_end_matches('/'))
                && model.is_some_and(|model| model.starts_with(&format!("{id}/")))
                && key_matches,
        )
    }

    fn owned_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        let provider = &doc["provider"]["yi"];
        let owned = json!({
            "$schema":doc["$schema"],
            "model":doc["model"],
            "provider":{"yi":{
                "npm":provider["npm"],
                "name":provider["name"],
                "options":{"baseURL":provider["options"]["baseURL"],"apiKey":"yi"},
                "models":provider["models"]
            }}
        });
        serde_json::to_string_pretty(&owned).map_err(AppError::from)
    }
}
