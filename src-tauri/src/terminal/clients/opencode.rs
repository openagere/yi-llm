use super::{object, parse_json, user_home, Built, ClientConfigurator, PlanInput};
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
        if legacy_owned {
            if let Some(providers) = doc.pointer_mut("/provider").and_then(Value::as_object_mut) {
                providers.remove("llm-man");
            }
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
