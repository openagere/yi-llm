use super::{
    direct_provider_id, direct_startup_model, redact_endpoint, user_home, Built,
    ClientConfigurator, DirectPlanInput, PlanInput,
};
use crate::{
    domain::{
        capabilities::{Capabilities, Modality},
        provider::ProviderView,
        terminal::{selected_protocol, DirectProfile},
    },
    error::{AppError, Result},
    terminal::changes::{read_optional, FileChange},
};
use serde_yaml::{Mapping, Value as Yaml};
use std::{
    env,
    path::{Path, PathBuf},
};

pub struct DeepSeekHarness;

/// Patch entry id for the pi-ai multi-provider adapter inside `cordis.patch.yml`.
const PLUGIN_ID: &str = "llm-pi-ai";
/// Fully qualified plugin name some hand-written patches use instead of the short id.
const PLUGIN_NAME: &str = "@deepseek-ai/dsh-llm-pi-ai";
/// Patch entry whose `config.agents[].provider/model` records the default startup model.
const AGENT_LOOP_ID: &str = "agent-loop";
const PROVIDER_ID: &str = "yi";
const DIRECT_ID_PREFIX: &str = "yi_direct_";
const DIRECT_NAME_PREFIX: &str = "yi-llm direct:";
/// dsh (pi-ai) thinking levels shared with yi-llm efforts; `ultra`/`ultracode` have no dsh
/// vocabulary, and `off` is the only level allowed to stay empty in `reasoningEfforts`.
const DSH_EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Profile directories dsh manages under `$DSH_HOME/profiles`: the Electron
/// desktop app owns `desktop`, while the CLI surface defaults to `web`.
const DESKTOP_PROFILE: &str = "desktop";
const WEB_PROFILE: &str = "web";

/// dsh's own credential store: routes name a credential reference (`apiKeyEnv`)
/// that the seam resolves per request from this hot-reloaded document, so
/// provisioning here makes a fresh apply work without any manual step.
const CREDENTIALS_FILENAME: &str = ".credentials.yaml";
/// Reference the proxy route names; the local proxy performs no authentication,
/// so the stored value is the documented `yi` placeholder.
const PROXY_KEY_REFERENCE: &str = "YI_LLM_PROXY";
const PROXY_KEY_PLACEHOLDER: &str = "yi";
/// Reference prefix yi-llm owns in the store; stale entries are pruned on apply.
const DIRECT_KEY_REFERENCE_PREFIX: &str = "YI_LLM_DIRECT_";

/// Resolve `$DSH_HOME`, expanding a leading `~` the way dsh does; the default is `~/.dsh`.
fn config_dir() -> Result<PathBuf> {
    let Some(raw) = env::var_os("DSH_HOME") else {
        return Ok(user_home()?.join(".dsh"));
    };
    let path = PathBuf::from(raw);
    // dsh expands a leading `~`; mirror that so both sides read the same file.
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

/// The profile's own Cordis patch: every `dsh` surface composes its bundle
/// layers under `profiles/<name>/cordis.patch.yml`, so one file covers the CLI
/// and the desktop app alike.
///
/// Prefer the desktop profile once its directory exists — the Electron app
/// creates it on first launch and owns it exclusively (the CLI even rejects
/// `--profile desktop`) — and fall back to `web`, the documented default for
/// `dsh web`.
fn select_profile(home: &Path) -> &'static str {
    if home.join("profiles").join(DESKTOP_PROFILE).is_dir() {
        DESKTOP_PROFILE
    } else {
        WEB_PROFILE
    }
}

/// Parse a profile patch into its top-level entry list; an empty file is an empty list.
fn parse_patch(source: &str) -> Result<Vec<Yaml>> {
    if source.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_yaml::from_str(source).map_err(|error| {
        AppError::validation(format!(
            "DeepSeek Harness cordis.patch.yml 无法解析: {error}"
        ))
    })
}

fn to_yaml_string(value: &impl serde::Serialize) -> Result<String> {
    serde_yaml::to_string(value)
        .map_err(|error| AppError::internal(format!("DeepSeek Harness YAML 序列化失败: {error}")))
}

fn to_yaml_bytes(value: &impl serde::Serialize) -> Result<Vec<u8>> {
    to_yaml_string(value).map(String::into_bytes)
}

fn is_pi_ai_entry(entry: &Yaml) -> bool {
    entry.get("id").and_then(Yaml::as_str) == Some(PLUGIN_ID)
        || entry.get("name").and_then(Yaml::as_str) == Some(PLUGIN_NAME)
        || entry.get("name").and_then(Yaml::as_str) == Some(PLUGIN_ID)
}

/// The patch entry this configurator owns, appended in the documented `id:` form when the
/// file does not carry one yet.
fn plugin_entry(doc: &mut Vec<Yaml>) -> Result<&mut Yaml> {
    if let Some(index) = doc.iter().position(is_pi_ai_entry) {
        return Ok(&mut doc[index]);
    }
    let mut entry = Mapping::new();
    entry.insert(Yaml::String("id".into()), Yaml::String(PLUGIN_ID.into()));
    let mut config = Mapping::new();
    config.insert(
        Yaml::String("providers".into()),
        Yaml::Mapping(Mapping::new()),
    );
    entry.insert(Yaml::String("config".into()), Yaml::Mapping(config));
    doc.push(Yaml::Mapping(entry));
    Ok(doc.last_mut().unwrap())
}

/// Merge-style access that creates missing mapping keys but refuses non-object values.
fn object_yaml<'a>(value: &'a mut Yaml, key: &str) -> Result<&'a mut Yaml> {
    let map = value
        .as_mapping_mut()
        .ok_or_else(|| AppError::validation(format!("配置 {key} 不是对象")))?;
    let entry = map
        .entry(Yaml::String(key.into()))
        .or_insert_with(|| Yaml::Mapping(Mapping::new()));
    if !entry.is_mapping() {
        return Err(AppError::validation(format!(
            "配置 {key} 不是对象，未修改文件"
        )));
    }
    Ok(entry)
}

fn providers_value(doc: &mut Vec<Yaml>) -> Result<&mut Yaml> {
    let entry = plugin_entry(doc)?;
    object_yaml(object_yaml(entry, "config")?, "providers")
}

fn providers_map(providers: &mut Yaml) -> Result<&mut Mapping> {
    providers
        .as_mapping_mut()
        .ok_or_else(|| AppError::validation("配置 providers 不是对象"))
}

/// pi-ai API id for one wire protocol, used by the proxy provider and direct providers alike.
fn protocol_api(protocol: &str) -> Result<&'static str> {
    match protocol {
        "anthropic" => Ok("anthropic-messages"),
        "responses" => Ok("openai-responses"),
        "openai_chat" => Ok("openai-completions"),
        _ => Err(AppError::validation(format!(
            "DeepSeek Harness 不支持 {protocol} 协议"
        ))),
    }
}

/// dsh `reasoningEfforts` map: menu level -> wire value. Only `off` may stay empty because
/// "no thinking" simply sends no reasoning field; undeclared levels fall back to pi-ai's
/// own detection, so levels outside the dsh vocabulary are omitted.
fn reasoning_efforts(caps: &Capabilities, protocol: &str, upstream: &str) -> Option<Yaml> {
    let supported = caps.efforts_for(protocol, upstream);
    if supported.is_empty() {
        return None;
    }
    let mut map = Mapping::new();
    map.insert(Yaml::String("off".into()), Yaml::Null);
    for level in DSH_EFFORT_LEVELS {
        if supported.iter().any(|supported| supported == level) {
            map.insert(Yaml::String(level.into()), Yaml::String(level.into()));
        }
    }
    Some(Yaml::Mapping(map))
}

/// One pi-ai model entry from declared capabilities. `protocol` is the client protocol the
/// route speaks: the protocol selected for the proxy, or the provider's native protocol
/// for direct access.
fn model_entry(id: &str, caps: &Capabilities, protocol: &str, upstream: &str) -> Yaml {
    let input: Vec<Modality> = caps
        .input_for(protocol, upstream)
        .into_iter()
        .filter(|modality| matches!(modality, Modality::Text | Modality::Image))
        .collect();
    let mut entry = Mapping::new();
    entry.insert(Yaml::String("id".into()), Yaml::String(id.into()));
    entry.insert(Yaml::String("name".into()), Yaml::String(id.into()));
    entry.insert(
        Yaml::String("input".into()),
        serde_yaml::to_value(input).unwrap_or(Yaml::Null),
    );
    if let Some(context) = caps.context_window {
        entry.insert(
            Yaml::String("contextWindow".into()),
            serde_yaml::to_value(context).unwrap_or(Yaml::Null),
        );
    }
    if let Some(max) = caps.max_output_tokens {
        entry.insert(
            Yaml::String("maxTokens".into()),
            serde_yaml::to_value(max).unwrap_or(Yaml::Null),
        );
    }
    if let Some(efforts) = reasoning_efforts(caps, protocol, upstream) {
        entry.insert(Yaml::String("reasoningEfforts".into()), efforts);
    }
    Yaml::Mapping(entry)
}

fn owned_direct(id: &str, entry: &Yaml) -> bool {
    id.starts_with(DIRECT_ID_PREFIX)
        && entry
            .get("name")
            .and_then(Yaml::as_str)
            .unwrap_or_default()
            .starts_with(DIRECT_NAME_PREFIX)
}

fn remove_owned_direct(providers: &mut Mapping) {
    let stale: Vec<Yaml> = providers
        .iter()
        .filter(|(id, entry)| id.as_str().is_some_and(|id| owned_direct(id, entry)))
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        providers.shift_remove(&id);
    }
}

/// dsh custom route ids are lowercase; sanitize like the shared helper and downcase.
fn direct_id(provider_id: &str) -> String {
    direct_provider_id(provider_id).to_lowercase()
}

fn direct_name(provider_id: &str) -> String {
    format!("{DIRECT_NAME_PREFIX}{provider_id}")
}

/// Credential reference a direct route points at. dsh patch files never carry plaintext
/// keys: the reference resolves per request through the environment or dsh's own
/// credential store, so the value stays owned by the yi-llm provider record.
fn direct_key_env(provider_id: &str) -> String {
    let safe: String = provider_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("YI_LLM_DIRECT_{safe}")
}

/// `$DSH_HOME` derived from the patch path: `<home>/profiles/<profile>/cordis.patch.yml`.
/// Callers pass the path `config_path()` produced (or an apply-time override of the
/// same shape), so the credential store lands beside the profile; anything else
/// falls back to the path itself.
fn harness_home(path: &Path) -> &Path {
    path.ancestors().nth(3).unwrap_or(path)
}

/// POSIX-identifier reference names — the shape dsh's `credentialRef` accepts.
fn is_credential_ref(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// Merge one credential reference into dsh's credential store, pruning stale
/// yi-llm-owned direct references. Returns the file change to apply, or `None`
/// when the store already holds the reference — a value the user stored wins.
fn provision_credential(home: &Path, reference: &str, value: &str) -> Result<Option<FileChange>> {
    let path = home.join(CREDENTIALS_FILENAME);
    let (mut fields, text) = match read_optional(&path)? {
        None => (Mapping::new(), String::new()),
        Some(bytes) => {
            let text = bytes;
            if text.trim().is_empty() {
                (Mapping::new(), text)
            } else {
                match serde_yaml::from_str::<Yaml>(&text) {
                    Ok(Yaml::Mapping(fields)) => (fields, text),
                    Ok(Yaml::Null) => (Mapping::new(), text),
                    Ok(_) => {
                        return Err(AppError::validation(format!(
                            "dsh 凭据存储 {CREDENTIALS_FILENAME} 必须是映射，请先修复后重新应用"
                        )))
                    }
                    Err(_) => {
                        return Err(AppError::validation(format!(
                            "dsh 凭据存储 {CREDENTIALS_FILENAME} 无法解析，请先修复或删除该文件后重新应用"
                        )))
                    }
                }
            }
        }
    };
    let mut records: Option<Yaml> = None;
    let mut refs = if fields.get("version").is_some() {
        let version = fields
            .get("version")
            .and_then(Yaml::as_i64)
            .ok_or_else(|| AppError::validation("dsh 凭据存储 version 必须是整数"))?;
        if version != 1 {
            return Err(AppError::validation(format!(
                "dsh 凭据存储声明版本 {version}，本版本只支持 1"
            )));
        }
        for key in fields.keys() {
            let Some(name) = key.as_str() else {
                return Err(AppError::validation("dsh 凭据存储的键必须是文本"));
            };
            if name != "version" && name != "refs" && name != "records" {
                return Err(AppError::validation(format!(
                    "dsh 凭据存储包含未知键 {name}，请先修复后重新应用"
                )));
            }
        }
        records = fields.remove(Yaml::String("records".into()));
        match fields.remove(Yaml::String("refs".into())) {
            None | Some(Yaml::Null) => Mapping::new(),
            Some(Yaml::Mapping(map)) => {
                for (key, value) in &map {
                    let Some(name) = key.as_str() else {
                        return Err(AppError::validation("dsh 凭据存储 refs 的键必须是文本"));
                    };
                    if !is_credential_ref(name) {
                        return Err(AppError::validation(format!(
                            "dsh 凭据存储引用名 {name} 不是有效标识符"
                        )));
                    }
                    if value.as_str().is_none_or(|value| value.is_empty()) {
                        return Err(AppError::validation(format!(
                            "dsh 凭据存储引用 {name} 的值必须是非空文本"
                        )));
                    }
                }
                map
            }
            Some(_) => return Err(AppError::validation("dsh 凭据存储 refs 必须是映射")),
        }
    } else {
        // Pre-release flat layout: dsh migrates it on load, so mirror its
        // recognizer exactly — directives or non-ref entries stay a loud error.
        if text
            .lines()
            .any(|line| line.starts_with('%') || line.starts_with("---") || line.starts_with("..."))
        {
            return Err(AppError::validation(format!(
                "dsh 凭据存储 {CREDENTIALS_FILENAME} 使用旧版扁平格式且包含文档指令，请按 `version: 1` + `refs:` 迁移后重新应用"
            )));
        }
        for (key, value) in &fields {
            let Some(name) = key.as_str() else {
                return Err(AppError::validation("dsh 凭据存储 refs 的键必须是文本"));
            };
            if !is_credential_ref(name) {
                return Err(AppError::validation(format!(
                    "dsh 凭据存储引用名 {name} 不是有效标识符"
                )));
            }
            if value.as_str().is_none_or(|value| value.is_empty()) {
                return Err(AppError::validation(format!(
                    "dsh 凭据存储引用 {name} 的值必须是非空文本"
                )));
            }
        }
        fields
    };
    let mut changed = false;
    let stale: Vec<Yaml> = refs
        .keys()
        .filter(|key| {
            key.as_str().is_some_and(|name| {
                name.starts_with(DIRECT_KEY_REFERENCE_PREFIX) && name != reference
            })
        })
        .cloned()
        .collect();
    for key in stale {
        refs.remove(&key);
        changed = true;
    }
    let provisioned = matches!(
        refs.get(reference),
        Some(Yaml::String(stored)) if !stored.is_empty()
    );
    if !provisioned {
        refs.insert(Yaml::String(reference.into()), Yaml::String(value.into()));
        changed = true;
    }
    if !changed {
        return Ok(None);
    }
    let mut document = Mapping::new();
    document.insert(Yaml::String("version".into()), Yaml::Number(1.into()));
    document.insert(Yaml::String("refs".into()), Yaml::Mapping(refs));
    if let Some(records) = records {
        document.insert(Yaml::String("records".into()), records);
    }
    Ok(Some(FileChange::new(
        path,
        to_yaml_string(&Yaml::Mapping(document))?.into_bytes(),
    )?))
}

/// The agent-loop patch entry records the default startup provider/model for new sessions.
/// Patch `config` replaces the whole entry config, so only an existing `main` agent is
/// updated in place — a missing entry stays untouched instead of clobbering bundled agents.
fn update_default_agent(doc: &mut [Yaml], provider: &str, model: &str) {
    for entry in doc.iter_mut() {
        let is_agent_loop = entry.get("id").and_then(Yaml::as_str) == Some(AGENT_LOOP_ID)
            || entry
                .get("name")
                .and_then(Yaml::as_str)
                .is_some_and(|name| name.ends_with(AGENT_LOOP_ID));
        if !is_agent_loop {
            continue;
        }
        let Some(agents) = entry
            .get_mut("config")
            .and_then(|config| config.get_mut("agents"))
            .and_then(Yaml::as_sequence_mut)
        else {
            continue;
        };
        for agent in agents {
            if agent.get("id").and_then(Yaml::as_str) == Some("main") {
                agent["provider"] = Yaml::String(provider.into());
                agent["model"] = Yaml::String(model.into());
            }
        }
    }
}

fn find_entry(doc: &[Yaml]) -> Option<&Yaml> {
    doc.iter().find(|entry| is_pi_ai_entry(entry))
}

fn find_providers(doc: &[Yaml]) -> Option<&Mapping> {
    find_entry(doc)?
        .get("config")?
        .get("providers")?
        .as_mapping()
}

fn find_provider<'a>(doc: &'a [Yaml], id: &str) -> Option<&'a Yaml> {
    find_providers(doc).and_then(|providers| providers.get(id))
}

/// Standalone single-entry patch shape used by the owned previews.
fn preview_patch(providers: Mapping) -> Vec<Yaml> {
    let mut config = Mapping::new();
    config.insert(Yaml::String("providers".into()), Yaml::Mapping(providers));
    let mut entry = Mapping::new();
    entry.insert(Yaml::String("id".into()), Yaml::String(PLUGIN_ID.into()));
    entry.insert(Yaml::String("config".into()), Yaml::Mapping(config));
    vec![Yaml::Mapping(entry)]
}

impl ClientConfigurator for DeepSeekHarness {
    fn config_path(&self) -> Result<PathBuf> {
        let home = config_dir()?;
        Ok(home
            .join("profiles")
            .join(select_profile(&home))
            .join("cordis.patch.yml"))
    }

    fn is_active(
        &self,
        _path: &Path,
        source: &str,
        endpoint: &str,
        protocol: &str,
    ) -> Result<bool> {
        let api = protocol_api(protocol)?;
        let doc = parse_patch(source)?;
        let provider = find_provider(&doc, PROVIDER_ID);
        let key_matches = provider
            .and_then(|p| p.get("apiKeyEnv"))
            .and_then(Yaml::as_str)
            == Some(PROXY_KEY_REFERENCE);
        Ok(provider
            .and_then(|p| p.get("baseURL"))
            .and_then(Yaml::as_str)
            == Some(endpoint)
            && provider.and_then(|p| p.get("api")).and_then(Yaml::as_str) == Some(api)
            && key_matches)
    }

    fn build(&self, input: &PlanInput<'_>, path: &Path, source: &str) -> Result<Built> {
        let protocol = selected_protocol("deepseek-harness", input.profile.protocol.as_deref())?;
        let api = protocol_api(protocol)?;
        let mut doc = parse_patch(source)?;
        let providers = providers_value(&mut doc)?;
        {
            let map = providers_map(providers)?;
            if let Some(existing) = map.get(PROVIDER_ID) {
                if existing.get("name").and_then(Yaml::as_str).is_some() {
                    return Err(AppError::conflict(format!(
                        "DeepSeek Harness Provider 标识 {PROVIDER_ID} 已被其他配置占用"
                    )));
                }
            }
            remove_owned_direct(map);
        }
        let provider = object_yaml(providers, PROVIDER_ID)?;
        let map = provider
            .as_mapping_mut()
            .ok_or_else(|| AppError::validation("配置 yi 不是对象"))?;
        // pi-ai refuses keyless custom routes ("No API key for provider"), so the
        // proxy route names its credential reference; the local proxy ignores the
        // value, which is provisioned as the documented `yi` placeholder.
        map.insert(
            Yaml::String("apiKeyEnv".into()),
            Yaml::String(PROXY_KEY_REFERENCE.into()),
        );
        map.insert(Yaml::String("api".into()), Yaml::String(api.into()));
        map.insert(
            Yaml::String("baseURL".into()),
            Yaml::String(input.endpoint.to_string()),
        );
        let models: Vec<Yaml> = input
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
            .collect();
        map.insert(Yaml::String("models".into()), Yaml::Sequence(models));
        update_default_agent(&mut doc, PROVIDER_ID, &input.profile.default_model);
        let mut extra = Vec::new();
        if let Some(change) = provision_credential(
            harness_home(path),
            PROXY_KEY_REFERENCE,
            PROXY_KEY_PLACEHOLDER,
        )? {
            extra.push(change);
        }
        Ok(Built {
            extra,
            main: to_yaml_bytes(&doc)?,
        })
    }

    fn build_direct(
        &self,
        input: &DirectPlanInput<'_>,
        path: &Path,
        source: &str,
    ) -> Result<Built> {
        let record = &input.provider.provider;
        let id = direct_id(&record.id);
        let name = direct_name(&record.id);
        let api = protocol_api(&record.provider_type)?;
        let mut doc = parse_patch(source)?;
        let providers = providers_value(&mut doc)?;
        {
            let map = providers_map(providers)?;
            if let Some(existing) = map.get(id.as_str()) {
                if existing.get("name").and_then(Yaml::as_str) != Some(name.as_str()) {
                    return Err(AppError::conflict(format!(
                        "DeepSeek Harness Provider 标识 {id} 已被其他配置占用"
                    )));
                }
            }
            remove_owned_direct(map);
        }
        let provider = object_yaml(providers, &id)?;
        let direct = provider
            .as_mapping_mut()
            .ok_or_else(|| AppError::validation(format!("配置 {id} 不是对象")))?;
        direct.insert(Yaml::String("api".into()), Yaml::String(api.into()));
        direct.insert(Yaml::String("name".into()), Yaml::String(name));
        direct.insert(
            Yaml::String("baseURL".into()),
            Yaml::String(record.base_url.trim_end_matches('/').to_string()),
        );
        let key_reference = direct_key_env(&record.id);
        if record.api_key.trim().is_empty() {
            direct.shift_remove(Yaml::String("apiKeyEnv".into()));
        } else {
            direct.insert(
                Yaml::String("apiKeyEnv".into()),
                Yaml::String(key_reference.clone()),
            );
        }
        let mut models: Vec<Yaml> = input
            .provider
            .models
            .iter()
            .map(|model| {
                model_entry(
                    &model.upstream_model,
                    &model.capabilities,
                    &record.provider_type,
                    &record.provider_type,
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
            let mut fallback = Mapping::new();
            fallback.insert(Yaml::String("id".into()), Yaml::String(selected.into()));
            fallback.insert(Yaml::String("name".into()), Yaml::String(selected.into()));
            models.push(Yaml::Mapping(fallback));
        }
        direct.insert(Yaml::String("models".into()), Yaml::Sequence(models));
        update_default_agent(&mut doc, &id, selected);
        let mut extra = Vec::new();
        if !record.api_key.trim().is_empty() {
            if let Some(change) =
                provision_credential(harness_home(path), &key_reference, record.api_key.trim())?
            {
                extra.push(change);
            }
        }
        Ok(Built {
            extra,
            main: to_yaml_bytes(&doc)?,
        })
    }

    fn owned_direct_preview(&self, content: &[u8]) -> Result<String> {
        let doc = parse_patch(&String::from_utf8_lossy(content))?;
        let mut providers = Mapping::new();
        if let Some(map) = find_providers(&doc) {
            for (id, provider) in map {
                if !id.as_str().is_some_and(|id| owned_direct(id, provider)) {
                    continue;
                }
                let mut safe = Mapping::new();
                for key in ["name", "api", "apiKeyEnv"] {
                    if let Some(value) = provider.get(key) {
                        safe.insert(Yaml::String(key.into()), value.clone());
                    }
                }
                if let Some(base_url) = provider.get("baseURL").and_then(Yaml::as_str) {
                    safe.insert(
                        Yaml::String("baseURL".into()),
                        Yaml::String(redact_endpoint(base_url)),
                    );
                }
                if let Some(models) = provider.get("models") {
                    safe.insert(Yaml::String("models".into()), models.clone());
                }
                providers.insert(id.clone(), Yaml::Mapping(safe));
            }
        }
        to_yaml_string(&preview_patch(providers))
    }

    fn is_direct_active(
        &self,
        _path: &Path,
        source: &str,
        profile: &DirectProfile,
        provider: &ProviderView,
    ) -> Result<bool> {
        let api = protocol_api(&provider.provider.provider_type)?;
        let doc = parse_patch(source)?;
        let id = direct_id(&profile.provider_id);
        let direct = find_provider(&doc, &id);
        let key_matches = provider.provider.api_key.trim().is_empty()
            || direct
                .and_then(|entry| entry.get("apiKeyEnv"))
                .and_then(Yaml::as_str)
                == Some(direct_key_env(&profile.provider_id).as_str());
        Ok(direct
            .and_then(|entry| entry.get("baseURL"))
            .and_then(Yaml::as_str)
            == Some(provider.provider.base_url.trim_end_matches('/'))
            && direct
                .and_then(|entry| entry.get("api"))
                .and_then(Yaml::as_str)
                == Some(api)
            && key_matches)
    }

    fn owned_preview(&self, content: &[u8]) -> Result<String> {
        let doc = parse_patch(&String::from_utf8_lossy(content))?;
        let mut safe = Mapping::new();
        if let Some(provider) = find_provider(&doc, PROVIDER_ID) {
            for key in ["api", "apiKeyEnv", "models"] {
                if let Some(value) = provider.get(key) {
                    safe.insert(Yaml::String(key.into()), value.clone());
                }
            }
            if let Some(base_url) = provider.get("baseURL").and_then(Yaml::as_str) {
                safe.insert(
                    Yaml::String("baseURL".into()),
                    Yaml::String(redact_endpoint(base_url)),
                );
            }
        }
        let mut providers = Mapping::new();
        providers.insert(Yaml::String(PROVIDER_ID.into()), Yaml::Mapping(safe));
        to_yaml_string(&preview_patch(providers))
    }

    fn extra_preview(&self, path: &Path, content: &[u8]) -> Result<Option<String>> {
        if path
            .file_name()
            .is_none_or(|name| name != CREDENTIALS_FILENAME)
        {
            return Ok(None);
        }
        let document: Yaml = serde_yaml::from_slice(content)
            .map_err(|error| AppError::validation(format!("dsh 凭据存储无法解析: {error}")))?;
        let mut safe = Mapping::new();
        if let Some(version) = document.get("version") {
            safe.insert(Yaml::String("version".into()), version.clone());
        }
        if let Some(Yaml::Mapping(refs)) = document.get("refs") {
            let mut masked = Mapping::new();
            for key in refs.keys() {
                masked.insert(key.clone(), Yaml::String("••••••••".into()));
            }
            safe.insert(Yaml::String("refs".into()), Yaml::Mapping(masked));
        }
        Ok(Some(to_yaml_string(&Yaml::Mapping(safe))?))
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn prefers_the_desktop_profile_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(select_profile(dir.path()), "web");
        std::fs::create_dir_all(dir.path().join("profiles/desktop")).unwrap();
        assert_eq!(select_profile(dir.path()), "desktop");
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;

    #[test]
    fn fresh_store_writes_the_versioned_layout() {
        let dir = tempfile::tempdir().unwrap();
        let change = provision_credential(dir.path(), PROXY_KEY_REFERENCE, PROXY_KEY_PLACEHOLDER)
            .unwrap()
            .unwrap();
        let store = String::from_utf8(change.content.unwrap()).unwrap();
        assert_eq!(store, "version: 1\nrefs:\n  YI_LLM_PROXY: yi\n");
    }

    #[test]
    fn flat_stores_are_migrated_with_entries_intact() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CREDENTIALS_FILENAME), "OLD_REF: keep\n").unwrap();
        let change = provision_credential(dir.path(), PROXY_KEY_REFERENCE, "yi")
            .unwrap()
            .unwrap();
        let store = String::from_utf8(change.content.unwrap()).unwrap();
        assert!(store.contains("version: 1"));
        assert!(store.contains("OLD_REF: keep"));
        assert!(store.contains("YI_LLM_PROXY: yi"));
    }

    #[test]
    fn existing_references_are_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(CREDENTIALS_FILENAME),
            "version: 1\nrefs:\n  YI_LLM_PROXY: user-value\n",
        )
        .unwrap();
        assert!(provision_credential(dir.path(), PROXY_KEY_REFERENCE, "yi")
            .unwrap()
            .is_none());
    }
}
