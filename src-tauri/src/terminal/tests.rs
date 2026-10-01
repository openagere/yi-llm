use super::*;
use crate::{
    db::{repo::providers, Conn, Db},
    domain::provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
    terminal::clients::parse_json,
};
use serde_json::{json, Value};
use std::{env, fs, path::PathBuf};
use toml_edit::DocumentMut;
fn fixture() -> (tempfile::TempDir, Conn, Profile) {
    let dir = tempfile::tempdir().unwrap();
    let mut conn = Db::open(&dir.path().join("test.db"))
        .unwrap()
        .conn()
        .unwrap();
    let mut selections = Vec::new();
    for id in ["one", "two"] {
        let model = format!("real-{id}({id})");
        providers::save(
            &mut conn,
            &ProviderView {
                provider: Provider {
                    id: id.into(),
                    provider_type: "responses".into(),
                    name: id.into(),
                    short_code: id.into(),
                    base_url: "http://localhost".into(),
                    api_key: "secret".into(),
                    enabled: true,
                    thinking: "minimal".into(),
                    extra: json!({}),
                    is_default: id == "one",
                    protocol_support: ProtocolSupport {
                        responses: true,
                        anthropic: true,
                        openai_chat: true,
                    },
                },
                models: vec![ModelMapping {
                    id: 0,
                    provider_id: id.into(),
                    exposed_name: model.clone(),
                    upstream_model: format!("real-{id}"),
                    standard_model_id: None,
                    capabilities: Default::default(),
                    non_standard: false,
                }],
            },
        )
        .unwrap();
        selections.push(Selection {
            provider_id: id.into(),
            model,
        });
    }
    (
        dir,
        conn,
        Profile {
            client: "codex".into(),
            models: selections,
            default_model: "real-one(one)".into(),
        },
    )
}
#[test]
fn native_configs_merge_multiple_providers_and_back_up() {
    let (dir, conn, mut profile) = fixture();
    for client in CLIENTS {
        profile.client = client.into();
        let path = dir.path().join(format!(
            "{client}/{}",
            if client == "codex" {
                "config.toml"
            } else {
                "config.jsonc"
            }
        ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = if client == "codex" {
            "# preserve\n[projects.example]\ntrust_level = 'trusted'\n"
        } else {
            "{ // preserve in backup\n\"custom\": {\"keep\": true},\n}"
        };
        fs::write(&path, original).unwrap();
        let result = apply_at(&conn, &profile, &path).unwrap();
        assert_eq!(result.model_count, 2);
        assert_eq!(
            fs::read_to_string(&result.backup_paths[0]).unwrap(),
            original
        );
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("secret"));
        if client == "codex" {
            let doc = content.parse::<DocumentMut>().unwrap();
            assert_eq!(
                doc["projects"]["example"]["trust_level"].as_str(),
                Some("trusted")
            );
            let catalog: Value =
                serde_json::from_slice(&fs::read(path.with_file_name("yi-models.json")).unwrap())
                    .unwrap();
            assert_eq!(catalog["models"].as_array().unwrap().len(), 2);
            assert_eq!(doc["model"].as_str(), Some("real-one(one)"));
            assert_eq!(catalog["models"][0]["slug"], "real-one(one)");
            assert_eq!(catalog["models"][1]["slug"], "real-two(two)");
        } else {
            assert_eq!(parse_json(&path, &content).unwrap()["custom"]["keep"], true);
        }
        assert!(resolve(&conn, client, "real-two(two)", protocol(client).unwrap()).is_ok());
    }
}
#[test]
fn scopes_reject_fallback_wrong_protocol_and_stale_provider() {
    let (_dir, conn, profile) = fixture();
    save(&conn, &profile).unwrap();
    assert!(resolve(&conn, "codex", "unselected", "responses").is_err());
    assert!(resolve(&conn, "codex", "real-one(one)", "anthropic").is_err());
    conn.execute(
        "UPDATE models SET provider_id='two' WHERE exposed_name='real-one(one)'",
        [],
    )
    .unwrap();
    assert!(resolve(&conn, "codex", "real-one(one)", "responses").is_err());
    conn.execute("UPDATE providers SET enabled=0 WHERE id='two'", [])
        .unwrap();
    assert!(resolve(&conn, "codex", "real-two(two)", "responses").is_err());
    assert_eq!(model_list(&conn, "codex").unwrap()["data"], json!([]));
}

#[test]
fn native_model_names_use_provider_short_codes() {
    let (dir, conn, mut profile) = fixture();
    for client in CLIENTS {
        profile.client = client.into();
        let path = dir.path().join(format!("{client}.toml"));
        apply_at(&conn, &profile, &path).unwrap();
        let source = preview_at(&conn, &profile, &path)
            .unwrap()
            .files
            .into_iter()
            .map(|file| file.content)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(source.contains("real-one(one)"));
        assert!(source.contains("real-two(two)"));
        assert_eq!(
            load(&conn, client).unwrap().unwrap().default_model,
            "real-one(one)"
        );
        let route = resolve(&conn, client, "real-two(two)", protocol(client).unwrap()).unwrap();
        assert_eq!(route.0.id, "two");
        assert_eq!(route.1, "real-two");
        assert_eq!(
            model_list(&conn, client).unwrap()["data"][0]["id"],
            "real-one(one)"
        );
        assert!(resolve(&conn, client, "real-two", protocol(client).unwrap()).is_err());
    }
}

#[test]
fn same_model_names_keep_distinct_routes_when_reordered_or_disabled() {
    let (dir, conn, mut profile) = fixture();
    conn.execute("UPDATE models SET upstream_model='gpt-6-luna',exposed_name='gpt-6-luna(' || provider_id || ')'", []).unwrap();
    conn.execute("UPDATE providers SET name='Same provider'", [])
        .unwrap();
    for selection in &mut profile.models {
        selection.model = format!("gpt-6-luna({})", selection.provider_id);
    }
    profile.default_model = "gpt-6-luna(one)".into();
    let path = dir.path().join("config.toml");
    apply_at(&conn, &profile, &path).unwrap();
    profile.models.reverse();
    apply_at(&conn, &profile, &path).unwrap();
    for id in ["one", "two"] {
        assert_eq!(
            resolve(&conn, "codex", &format!("gpt-6-luna({id})"), "responses")
                .unwrap()
                .0
                .id,
            id
        );
    }
    conn.execute("UPDATE providers SET enabled=0 WHERE id='one'", [])
        .unwrap();
    assert!(resolve(&conn, "codex", "gpt-6-luna(one)", "responses").is_err());
    assert!(resolve(&conn, "codex", "gpt-6-luna", "responses").is_err());
    assert_eq!(
        model_list(&conn, "codex").unwrap()["data"][0]["id"],
        "gpt-6-luna(two)"
    );
}
#[test]
fn previews_hide_existing_credentials_without_changing_files() {
    let (dir, conn, mut profile) = fixture();
    for client in CLIENTS {
        profile.client = client.into();
        let path = dir.path().join(if client == "codex" {
            "config.toml"
        } else {
            "settings.json"
        });
        let original = if client == "codex" {
            "[model_providers.other]\nenv_key = 'private-credential'\n[model_providers.llm-man]\nexperimental_bearer_token = 'private-credential'\n"
        } else {
            "{\"env\":{\"OTHER_TOKEN\":\"private-credential\"},\"hooks\":{\"private\":\"private-credential\"},\"provider\":{\"llm-man\":{\"options\":{\"private\":\"private-credential\"}}}}"
        };
        fs::write(&path, original).unwrap();
        let preview = preview_at(&conn, &profile, &path).unwrap();
        assert_eq!(preview.files[0].path, path.display().to_string());
        for file in preview.files {
            assert!(!file.content.contains("private-credential"));
            assert!(!file.content.contains("secret"));
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        assert!(!path.with_file_name("yi-models.json").exists());
    }
}
#[test]
fn claude_reserved_aliases_are_rejected_before_configuration() {
    let (_dir, conn, mut profile) = fixture();
    profile.client = "claude-code".into();
    for alias in ["default", "opus", "sonnet", "haiku", "opusplan", "opus[1m]"] {
        conn.execute(
            "UPDATE models SET exposed_name=?1 WHERE provider_id='one'",
            [alias],
        )
        .unwrap();
        profile.models[0].model = alias.into();
        profile.default_model = alias.into();
        assert!(validate(&conn, &profile)
            .unwrap_err()
            .to_string()
            .contains("内置别名"));
    }
}
#[test]
fn malformed_configuration_does_not_write_and_database_failure_rolls_back() {
    let (dir, conn, profile) = fixture();
    let path = dir.path().join("config.toml");
    fs::write(&path, "model_providers = 12").unwrap();
    assert!(apply_at(&conn, &profile, &path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "model_providers = 12");
    fs::write(&path, "model = 'original'").unwrap();
    conn.execute_batch("CREATE TRIGGER deny_terminal BEFORE INSERT ON terminal_profiles BEGIN SELECT RAISE(FAIL, 'test failure'); END;").unwrap();
    assert!(apply_at(&conn, &profile, &path).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "model = 'original'");
    assert!(!path.with_file_name("yi-models.json").exists());
}

#[test]
fn desktop_composer_sync_preserves_state_and_backs_it_up() {
    let (dir, conn, profile) = fixture();
    let path = dir.path().join("config.toml");
    let state_path = dir.path().join(".codex-global-state.json");
    let original = json!({"keep":true,"composer-recent-model-configurations-v1":[{"model":"old","reasoningEffort":"high"},{"model":"older"}]});
    fs::write(&state_path, original.to_string()).unwrap();
    let result = apply_at(&conn, &profile, &path).unwrap();
    let state: Value = serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    assert_eq!(state["keep"], true);
    assert_eq!(
        state["composer-recent-model-configurations-v1"][0]["model"],
        "real-one(one)"
    );
    assert_eq!(
        state["composer-recent-model-configurations-v1"][0]["reasoningEffort"],
        "high"
    );
    assert_eq!(
        state["composer-recent-model-configurations-v1"][1]["model"],
        "older"
    );
    let backup: Value =
        serde_json::from_slice(&fs::read(&result.backup_paths[0]).unwrap()).unwrap();
    assert_eq!(backup, original);
}

#[test]
fn missing_or_unparseable_desktop_state_does_not_block_cli_configuration() {
    let (dir, conn, profile) = fixture();
    let path = dir.path().join("config.toml");
    let state_path = dir.path().join(".codex-global-state.json");
    for content in [
        None,
        Some("invalid JSON"),
        Some("{}"),
        Some("{\"composer-recent-model-configurations-v1\":[]}"),
    ] {
        if let Some(content) = content {
            fs::write(&state_path, content).unwrap();
        }
        apply_at(&conn, &profile, &path).unwrap();
        if let Some(content) = content {
            assert_eq!(fs::read_to_string(&state_path).unwrap(), content);
        }
    }
}

#[test]
#[ignore = "exports native CLI fixtures to an explicitly supplied test directory"]
fn export_native_fixtures() {
    let (_dir, conn, mut profile) = fixture();
    let capabilities = json!({"input_modalities":["text","image","audio"],"output_modalities":["text"],"context_window":131072,"max_output_tokens":8192,"effort":{"support":"supported","levels":["low","medium","high"],"default":"high"}});
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [capabilities.to_string()],
    )
    .unwrap();
    for selection in &mut profile.models {
        let route = format!("{}/{}", selection.provider_id, selection.model);
        conn.execute(
            "UPDATE models SET exposed_name=?1 WHERE provider_id=?2",
            (&route, &selection.provider_id),
        )
        .unwrap();
        if profile.default_model == selection.model {
            profile.default_model = route.clone();
        }
        selection.model = route;
    }
    let output = PathBuf::from(
        env::var_os("YI_LLM_TEST_CONFIG_OUTPUT").expect("set YI_LLM_TEST_CONFIG_OUTPUT"),
    );
    for client in CLIENTS {
        profile.client = client.into();
        let path = output.join(client).join(if client == "codex" {
            "config.toml"
        } else if client == "claude-code" {
            "settings.json"
        } else {
            "opencode.json"
        });
        apply_at(&conn, &profile, &path).unwrap();
    }
}

#[test]
fn native_catalogs_use_declared_limits_modalities_and_effort() {
    let (dir, conn, mut profile) = fixture();
    let caps = json!({"input_modalities":["text","image","audio"],"output_modalities":["text"],"context_window":131072,"max_output_tokens":8192,"effort":{"support":"supported","levels":["low","medium","high","max","ultra"],"default":"high"}});
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [caps.to_string()],
    )
    .unwrap();
    let preview = preview_at(&conn, &profile, &dir.path().join("config.toml")).unwrap();
    let catalog: Value = serde_json::from_str(
        &preview
            .files
            .iter()
            .find(|f| f.path.ends_with("yi-models.json"))
            .unwrap()
            .content,
    )
    .unwrap();
    let model = &catalog["models"][0];
    assert_eq!(model["context_window"], 131072);
    assert_eq!(model["input_modalities"], json!(["text", "image"]));
    assert_eq!(model["default_reasoning_level"], "high");
    assert_eq!(
        model["supported_reasoning_levels"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(preview
        .files
        .iter()
        .find(|f| f.path.ends_with("config.toml"))
        .unwrap()
        .content
        .contains("model_reasoning_effort = \"high\""));
    profile.client = "opencode".into();
    let preview = preview_at(&conn, &profile, &dir.path().join("opencode.json")).unwrap();
    let config: Value = serde_json::from_str(&preview.files[0].content).unwrap();
    let model = &config["provider"]["yi"]["models"]["real-one(one)"];
    assert_eq!(model["variants"]["max"]["reasoningEffort"], "max");
    assert_eq!(model["variants"]["ultra"]["reasoningEffort"], "ultra");
    assert_eq!(model["limit"], json!({"context":131072,"output":8192}));
    assert_eq!(model["modalities"]["input"], json!(["text", "image"]));
    assert_eq!(model["variants"]["high"]["reasoningEffort"], "high");
    assert_eq!(model["options"]["reasoningEffort"], "high");
    conn.execute("UPDATE models SET capabilities=?1 WHERE provider_id='one'",[json!({"context_window":131072,"effort":{"support":"supported","levels":["high","max"],"default":"max"}}).to_string()]).unwrap();
    conn.execute("UPDATE providers SET type='anthropic' WHERE id='one'", [])
        .unwrap();
    let preview = preview_at(&conn, &profile, &dir.path().join("opencode.json")).unwrap();
    let config: Value = serde_json::from_str(&preview.files[0].content).unwrap();
    let model = &config["provider"]["yi"]["models"]["real-one(one)"];
    assert!(model.get("limit").is_none());
    assert_eq!(model["options"]["reasoningEffort"], "max");
    assert_eq!(model["variants"]["max"]["reasoningEffort"], "max");
}

#[test]
fn provider_code_changes_rewrite_saved_terminal_collections() {
    let (_dir, mut conn, profile) = fixture();
    for client in CLIENTS {
        save(
            &conn,
            &Profile {
                client: client.into(),
                ..profile.clone()
            },
        )
        .unwrap();
    }
    let mut view = providers::list_views(&conn)
        .unwrap()
        .into_iter()
        .find(|view| view.provider.id == "one")
        .unwrap();
    view.provider.short_code = "uno".into();
    view.models[0].exposed_name = "real-one(uno)".into();
    providers::save(&mut conn, &view).unwrap();
    for client in CLIENTS {
        let saved = load(&conn, client).unwrap().unwrap();
        assert_eq!(saved.default_model, "real-one(uno)");
        assert_eq!(saved.models[0].model, "real-one(uno)");
        let route = resolve(
            &conn,
            client,
            &saved.default_model,
            protocol(client).unwrap(),
        )
        .unwrap();
        assert_eq!(route.0.id, "one");
    }
}
