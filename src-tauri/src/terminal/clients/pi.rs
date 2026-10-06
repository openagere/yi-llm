use super::{
    direct_provider_id, direct_startup_model, object, parse_json, redact_endpoint, user_home,
    Built, ClientConfigurator, DirectPlanInput, PlanInput,
};
use crate::{
    domain::{
        capabilities::{Capabilities, Modality},
        terminal::selected_protocol,
    },
    error::{AppError, Result},
    terminal::changes::{read_optional, FileChange},
};
use serde_json::{json, Map, Value};
use std::{
    env,
    path::{Path, PathBuf},
};

pub struct Pi;

const PROVIDER_ID: &str = "yi";
const DIRECT_ID_PREFIX: &str = "yi_direct_";
const DIRECT_NAME_PREFIX: &str = "yi-llm direct:";
/// Pi thinking levels shared with yi-llm efforts; `minimal` has no equivalent and is hidden.
const PI_THINKING_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

fn config_dir() -> Result<PathBuf> {
    let Some(raw) = env::var_os("PI_CODING_AGENT_DIR") else {
        return Ok(user_home()?.join(".pi").join("agent"));
    };
    let path = PathBuf::from(raw);
    // Pi expands a leading `~`; mirror that so both sides read the same file.
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix('~') else {
        return Ok(path);
    };
    let rest = rest.trim_start_matches(['/', '\\']);
    if rest.is_empty() {
        user_home()
    } else {
        Ok(user_home()?.join(rest))
    }
}

fn settings_path(path: &Path) -> PathBuf {
    path.with_file_name("settings.json")
}

fn parse_models(source: &str) -> Result<Value> {
    if source.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = json5::from_str(source)
        .map_err(|error| AppError::validation(format!("Pi models.json 无法解析: {error}")))?;
    if !value.is_object() {
        return Err(AppError::validation("Pi models.json 必须是 JSON 对象"));
    }
    Ok(value)
}

fn load_settings(path: &Path) -> Result<Value> {
    let settings = settings_path(path);
    let Some(source) = read_optional(&settings)? else {
        return Ok(json!({}));
    };
    if source.trim().is_empty() {
        return Ok(json!({}));
    }
    parse_json(&settings, &source)
}

fn save_settings(path: &Path, settings: &Value) -> Result<FileChange> {
    FileChange::new(settings_path(path), serde_json::to_vec_pretty(settings)?)
}

fn owned_direct(id: &str, name: &str) -> bool {
    id.starts_with(DIRECT_ID_PREFIX) && name.starts_with(DIRECT_NAME_PREFIX)
}

fn direct_name(provider_id: &str) -> String {
    format!("{DIRECT_NAME_PREFIX}{provider_id}")
}

/// Pi API id for one wire protocol, used by the proxy provider and direct providers alike.
fn protocol_api(protocol: &str) -> Result<&'static str> {
    match protocol {
        "anthropic" => Ok("anthropic-messages"),
        "responses" => Ok("openai-responses"),
        "openai_chat" => Ok("openai-completions"),
        _ => Err(AppError::validation(format!("Pi 不支持 {protocol} 协议"))),
    }
}

/// One Pi model entry from declared capabilities. `protocol` is the client protocol Pi
/// speaks to the model: the protocol selected for the proxy, or the provider's native
/// protocol for direct access.
fn model_entry(id: &str, caps: &Capabilities, protocol: &str, upstream: &str) -> Value {
    let input: Vec<Modality> = caps
        .input_for(protocol, upstream)
        .into_iter()
        .filter(|modality| matches!(modality, Modality::Text | Modality::Image))
        .collect();
    let mut entry = json!({"id":id,"name":id,"input":input});
    if let Some(context) = caps.context_window {
        entry["contextWindow"] = json!(context);
    }
    if let Some(max) = caps.max_output_tokens {
        entry["maxTokens"] = json!(max);
    }
    let supported = caps.efforts_for(protocol, upstream);
    let levels: Vec<&str> = PI_THINKING_LEVELS
        .iter()
        .copied()
        .filter(|level| supported.iter().any(|supported| supported == level))
        .collect();
    if !levels.is_empty() {
        entry["reasoning"] = json!(true);
        let mut map = Map::new();
        map.insert("minimal".into(), Value::Null);
        if protocol == "responses" {
            // Pi sends a literal `none` effort when thinking is off, which yi-llm's effort
            // levels do not carry; hide the level instead of failing every off request.
            map.insert("off".into(), Value::Null);
        }
        for level in PI_THINKING_LEVELS {
            map.insert(
                level.into(),
                if levels.contains(&level) {
                    json!(level)
                } else {
                    Value::Null
                },
            );
        }
        entry["thinkingLevelMap"] = Value::Object(map);
    }
    entry
}

fn owned_models(doc: &Value, predicate: impl Fn(&str, &Value) -> bool) -> Value {
    let mut providers = Map::new();
    if let Some(entries) = doc.get("providers").and_then(Value::as_object) {
        for (id, entry) in entries {
            if !predicate(id, entry) {
                continue;
            }
            let mut safe = Map::new();
            for key in ["name", "api"] {
                if let Some(value) = entry.get(key) {
                    safe.insert(key.to_owned(), value.clone());
                }
            }
            if let Some(base_url) = entry.get("baseUrl").and_then(Value::as_str) {
                safe.insert("baseUrl".into(), json!(redact_endpoint(base_url)));
            }
            safe.insert(
                "apiKey".into(),
                if entry.get("apiKey").is_some() {
                    json!("••••••••")
                } else {
                    Value::Null
                },
            );
            if let Some(models) = entry.get("models") {
                safe.insert("models".into(), models.clone());
            }
            providers.insert(id.clone(), Value::Object(safe));
        }
    }
    let mut root = Map::new();
    root.insert("providers".into(), Value::Object(providers));
    Value::Object(root)
}

fn update_settings(path: &Path, update: impl FnOnce(&mut Value)) -> Result<FileChange> {
    let mut settings = load_settings(path)?;
    update(&mut settings);
    save_settings(path, &settings)
}

impl ClientConfigurator for Pi {
    fn config_path(&self) -> Result<PathBuf> {
        Ok(config_dir()?.join("models.json"))
    }

    fn is_active(&self, path: &Path, source: &str, endpoint: &str, protocol: &str) -> Result<bool> {
        let api = protocol_api(protocol)?;
        let doc = parse_models(source)?;
        let settings = load_settings(path)?;
        let provider = doc
            .get("providers")
            .and_then(|providers| providers.get(PROVIDER_ID));
        Ok(provider
            .and_then(|entry| entry.get("baseUrl"))
            .and_then(Value::as_str)
            == Some(endpoint)
            && provider
                .and_then(|entry| entry.get("api"))
                .and_then(Value::as_str)
                == Some(api)
            && settings.get("defaultProvider").and_then(Value::as_str) == Some(PROVIDER_ID))
    }

    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built> {
        let protocol = selected_protocol("pi", input.profile.protocol.as_deref())?;
        let api = protocol_api(protocol)?;
        let mut doc = parse_models(source)?;
        let providers = object(&mut doc, "providers")?;
        if let Some(map) = providers.as_object_mut() {
            map.retain(|id, entry| {
                !owned_direct(
                    id,
                    entry
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            });
        }
        let entry = object(providers, PROVIDER_ID)?;
        entry["baseUrl"] = json!(input.endpoint);
        entry["api"] = json!(api);
        entry["apiKey"] = json!(PROVIDER_ID);
        entry["models"] = Value::Array(
            input
                .models
                .iter()
                .map(|model| {
                    model_entry(
                        &model.selection.model,
                        &model.mapping.capabilities,
                        protocol,
                        &model.provider.provider_type,
                    )
                })
                .collect(),
        );
        let change = update_settings(path, |settings| {
            settings["defaultProvider"] = json!(PROVIDER_ID);
            settings["defaultModel"] = json!(input.profile.default_model);
        })?;
        Ok(Built {
            extra: vec![change],
            main: serde_json::to_vec_pretty(&doc)?,
        })
    }

    fn build_direct(
        &self,
        input: &DirectPlanInput<'_>,
        path: &Path,
        source: &str,
    ) -> Result<Built> {
        let mut doc = parse_models(source)?;
        let provider = &input.provider.provider;
        let id = direct_provider_id(&provider.id);
        let name = direct_name(&provider.id);
        let api = protocol_api(&provider.provider_type)?;
        let providers = object(&mut doc, "providers")?;
        if let Some(existing) = providers.get(&id) {
            if existing.get("name").and_then(Value::as_str) != Some(name.as_str()) {
                return Err(AppError::conflict(format!(
                    "Pi Provider 标识 {id} 已被其他配置占用"
                )));
            }
        }
        if let Some(map) = providers.as_object_mut() {
            map.retain(|key, entry| {
                key == &id
                    || !owned_direct(
                        key,
                        entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                    )
            });
        }
        let entry = object(providers, &id)?;
        entry["api"] = json!(api);
        entry["name"] = json!(name);
        entry["baseUrl"] = json!(provider.base_url.trim_end_matches('/'));
        if provider.api_key.trim().is_empty() {
            entry.as_object_mut().unwrap().remove("apiKey");
        } else {
            entry["apiKey"] = json!(provider.api_key);
        }
        let mut models: Vec<Value> = input
            .provider
            .models
            .iter()
            .map(|model| {
                model_entry(
                    &model.upstream_model,
                    &model.capabilities,
                    &provider.provider_type,
                    &provider.provider_type,
                )
            })
            .collect();
        let selected = direct_startup_model(input.profile, input.provider)?;
        if !input
            .provider
            .models
            .iter()
            .any(|model| model.upstream_model == selected)
        {
            models.push(json!({"id":selected,"name":selected}));
        }
        entry["models"] = Value::Array(models);
        let change = update_settings(path, |settings| {
            settings["defaultProvider"] = json!(id);
            settings["defaultModel"] = json!(selected);
        })?;
        Ok(Built {
            extra: vec![change],
            main: serde_json::to_vec_pretty(&doc)?,
        })
    }

    fn owned_direct_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        serde_json::to_string_pretty(&owned_models(&doc, |id, entry| {
            owned_direct(
                id,
                entry
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        }))
        .map_err(AppError::from)
    }

    fn is_direct_active(
        &self,
        path: &Path,
        source: &str,
        profile: &crate::domain::terminal::DirectProfile,
        provider: &crate::domain::provider::ProviderView,
    ) -> Result<bool> {
        let doc = parse_models(source)?;
        let api = protocol_api(&provider.provider.provider_type)?;
        let id = direct_provider_id(&profile.provider_id);
        let entry = doc
            .get("providers")
            .and_then(|providers| providers.get(&id));
        let key_matches = provider.provider.api_key.trim().is_empty()
            || entry
                .and_then(|entry| entry.get("apiKey"))
                .and_then(Value::as_str)
                == Some(provider.provider.api_key.as_str());
        let settings = load_settings(path)?;
        Ok(entry
            .and_then(|entry| entry.get("baseUrl"))
            .and_then(Value::as_str)
            == Some(provider.provider.base_url.trim_end_matches('/'))
            && entry
                .and_then(|entry| entry.get("api"))
                .and_then(Value::as_str)
                == Some(api)
            && key_matches
            && settings.get("defaultProvider").and_then(Value::as_str) == Some(id.as_str()))
    }

    fn owned_preview(&self, content: &[u8]) -> Result<String> {
        let doc: Value = serde_json::from_slice(content)?;
        let provider = doc
            .get("providers")
            .and_then(|providers| providers.get(PROVIDER_ID))
            .cloned()
            .unwrap_or(Value::Null);
        let mut safe = Map::new();
        for key in ["api", "models"] {
            if let Some(value) = provider.get(key) {
                safe.insert(key.to_owned(), value.clone());
            }
        }
        if let Some(base_url) = provider.get("baseUrl").and_then(Value::as_str) {
            safe.insert("baseUrl".into(), json!(redact_endpoint(base_url)));
        }
        safe.insert("apiKey".into(), json!(PROVIDER_ID));
        let mut providers = Map::new();
        providers.insert(PROVIDER_ID.to_owned(), Value::Object(safe));
        let mut root = Map::new();
        root.insert("providers".into(), Value::Object(providers));
        serde_json::to_string_pretty(&Value::Object(root)).map_err(AppError::from)
    }

    fn extra_preview(&self, path: &Path, content: &[u8]) -> Result<Option<String>> {
        if path.file_name().is_none_or(|name| name != "settings.json") {
            return Ok(None);
        }
        let settings: Value = serde_json::from_slice(content)?;
        let mut owned = Map::new();
        for key in ["defaultProvider", "defaultModel"] {
            if let Some(value) = settings.get(key) {
                owned.insert(key.to_owned(), value.clone());
            }
        }
        Ok(Some(serde_json::to_string_pretty(&Value::Object(owned))?))
    }
}
