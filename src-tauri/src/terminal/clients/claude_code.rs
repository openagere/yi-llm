use super::{
    direct_startup_model, object, parse_json, user_home, Built, ClientConfigurator,
    DirectPlanInput, PlanInput,
};
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
        let environment = object(&mut doc, "env")?;
        environment["ANTHROPIC_BASE_URL"] =
            json!(input.provider.provider.base_url.trim_end_matches('/'));
        if input.provider.provider.api_key.trim().is_empty() {
            if environment["ANTHROPIC_AUTH_TOKEN"] == "yi" {
                environment
                    .as_object_mut()
                    .unwrap()
                    .remove("ANTHROPIC_AUTH_TOKEN");
            }
        } else {
            environment["ANTHROPIC_AUTH_TOKEN"] = json!(input.provider.provider.api_key);
        }
        if input.profile.model_source == "provider" {
            let selected = direct_startup_model(input.profile, input.provider)?;
            doc["model"] = json!(selected);
            let mut options: Vec<Value> = input
                .provider
                .models
                .iter()
                .map(|model| json!({"model":model.upstream_model,"label":model.upstream_model}))
                .collect();
            if !options
                .iter()
                .any(|option| option["model"].as_str() == Some(selected))
            {
                options.push(json!({"model":selected,"label":selected}));
            }
            let picker = object(&mut doc, "modelPicker")?;
            picker["replaceBuiltInOptions"] = json!(true);
            picker["options"] = Value::Array(options);
        } else {
            if let Some(proxy) = input.proxy_profile {
                if doc["model"].as_str() == Some(proxy.default_model.as_str()) {
                    doc.as_object_mut().unwrap().remove("model");
                }
                let expected: Vec<Value> = proxy
                    .models
                    .iter()
                    .map(|model| json!({"model":model.model,"label":model.model}))
                    .collect();
                if doc
                    .pointer("/modelPicker/replaceBuiltInOptions")
                    .and_then(Value::as_bool)
                    == Some(true)
                    && doc
                        .pointer("/modelPicker/options")
                        .and_then(Value::as_array)
                        == Some(&expected)
                {
                    doc.as_object_mut().unwrap().remove("modelPicker");
                }
            }
            if let Some(previous) = input
                .previous_direct_profile
                .filter(|previous| previous.model_source == "provider")
            {
                if previous
                    .model
                    .as_deref()
                    .is_some_and(|model| doc["model"].as_str() == Some(model))
                {
                    doc.as_object_mut().unwrap().remove("model");
                }
                if let Some(previous_provider) = input
                    .previous_direct_provider
                    .filter(|provider| provider.provider.id == previous.provider_id)
                {
                    let expected: Vec<Value> = previous_provider.models.iter().map(|model| json!({"model":model.upstream_model,"label":model.upstream_model})).collect();
                    if doc
                        .pointer("/modelPicker/replaceBuiltInOptions")
                        .and_then(Value::as_bool)
                        == Some(true)
                        && doc
                            .pointer("/modelPicker/options")
                            .and_then(Value::as_array)
                            == Some(&expected)
                    {
                        doc.as_object_mut().unwrap().remove("modelPicker");
                    }
                }
            }
        }
        Ok(Built {
            extra: Vec::new(),
            main: serde_json::to_vec_pretty(&doc)?,
        })
    }

    fn owned_direct_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        let mut owned = json!({});
        if let Some(model) = doc.get("model") {
            owned["model"] = model.clone();
        }
        for key in ["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN"] {
            if let Some(value) = doc.pointer(&format!("/env/{key}")) {
                owned["env"][key] = if key == "ANTHROPIC_AUTH_TOKEN" {
                    json!("••••••••")
                } else if key == "ANTHROPIC_BASE_URL" {
                    json!(super::redact_endpoint(value.as_str().unwrap_or_default()))
                } else {
                    value.clone()
                };
            }
        }
        if let Some(picker) = doc.get("modelPicker") {
            owned["modelPicker"] = picker.clone();
        }
        serde_json::to_string_pretty(&owned).map_err(AppError::from)
    }

    fn is_direct_active(
        &self,
        path: &Path,
        source: &str,
        _profile: &crate::domain::terminal::DirectProfile,
        provider: &crate::domain::provider::ProviderView,
    ) -> Result<bool> {
        let doc = parse_json(path, source)?;
        let key_matches = provider.provider.api_key.trim().is_empty()
            || doc
                .pointer("/env/ANTHROPIC_AUTH_TOKEN")
                .and_then(Value::as_str)
                == Some(provider.provider.api_key.as_str());
        Ok(doc
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str)
            == Some(provider.provider.base_url.trim_end_matches('/'))
            && key_matches)
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
