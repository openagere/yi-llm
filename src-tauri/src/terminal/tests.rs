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
            protocol: None,
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
            match client {
                "codex" => "config.toml",
                "pi" => "models.json",
                "deepseek-harness" => "profiles/web/cordis.patch.yml",
                _ => "config.jsonc",
            }
        ));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let original = match client {
            "codex" => "# preserve\n[projects.example]\ntrust_level = 'trusted'\n",
            "deepseek-harness" => {
                "# preserve\n- id: session-persistence-jsonl\n  config:\n    root: './.sessions'\n"
            }
            _ => "{ // preserve in backup\n\"custom\": {\"keep\": true},\n}",
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
        } else if client == "deepseek-harness" {
            let patch: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
            assert_eq!(patch[0]["id"].as_str(), Some("session-persistence-jsonl"));
            assert_eq!(patch[0]["config"]["root"].as_str(), Some("./.sessions"));
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
        let path = dir.path().join(match client {
            "codex" => "codex/config.toml",
            "pi" => "pi/models.json",
            "deepseek-harness" => "dsh/profiles/web/cordis.patch.yml",
            _ => "settings.json",
        });
        let original = if client == "codex" {
            "[model_providers.other]\nenv_key = 'private-credential'\n[model_providers.llm-man]\nexperimental_bearer_token = 'private-credential'\n"
        } else if client == "deepseek-harness" {
            "- id: llm-pi-ai\n  config:\n    providers:\n      foreign:\n        apiKeyEnv: PRIVATE_CREDENTIAL\n        baseURL: https://private-credential.example/v1\n"
        } else {
            "{\"env\":{\"OTHER_TOKEN\":\"private-credential\"},\"hooks\":{\"private\":\"private-credential\"},\"provider\":{\"llm-man\":{\"options\":{\"private\":\"private-credential\"}}}}"
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
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
        let path = output.join(client).join(match client {
            "codex" => "config.toml",
            "claude-code" => "settings.json",
            "pi" => "models.json",
            _ => "opencode.json",
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

#[test]
fn codex_direct_native_mode_uses_provider_url_without_replacing_native_models_or_proxy_profile() {
    let (dir, conn, proxy_profile) = fixture();
    save(&conn, &proxy_profile).unwrap();
    let profile = DirectProfile {
        client: "codex".into(),
        provider_id: "one".into(),
        model_source: "native".into(),
        model: None,
    };
    let path = dir.path().join("codex/config.toml");
    let result = apply_direct_at(&conn, &profile, &path).unwrap();
    assert_eq!(result.model_count, 1);
    let source = fs::read_to_string(&path).unwrap();
    let doc = source.parse::<DocumentMut>().unwrap();
    assert_eq!(doc["model_provider"].as_str(), Some("yi_direct_one"));
    assert_eq!(
        doc["model_providers"]["yi_direct_one"]["base_url"].as_str(),
        Some("http://localhost")
    );
    assert_eq!(
        doc["model_providers"]["yi_direct_one"]["experimental_bearer_token"].as_str(),
        Some("secret")
    );
    assert_eq!(
        doc["model_providers"]["yi_direct_one"]["requires_openai_auth"].as_bool(),
        Some(false)
    );
    assert!(doc.get("model").is_none());
    assert!(doc.get("model_catalog_json").is_none());
    let saved_proxy = load(&conn, "codex").unwrap().unwrap();
    assert_eq!(saved_proxy.default_model, proxy_profile.default_model);
    assert_eq!(saved_proxy.models, proxy_profile.models);
    assert_eq!(
        crate::db::repo::terminal_direct_profiles::get(&conn, "codex").unwrap(),
        Some(profile.clone())
    );
    let preview = preview_direct_at(&conn, &profile, &path).unwrap();
    let content = &preview
        .files
        .iter()
        .find(|file| file.path.ends_with("config.toml"))
        .unwrap()
        .content;
    assert!(!content.contains("secret"));
    assert!(content.contains("••••••••"));
    assert!(direct_active(&conn, &profile, &path).unwrap());
    conn.execute("UPDATE providers SET api_key='rotated' WHERE id='one'", [])
        .unwrap();
    assert!(!direct_active(&conn, &profile, &path).unwrap());

    apply_at(&conn, &proxy_profile, &path).unwrap();
    let proxy_source = fs::read_to_string(&path).unwrap();
    let proxy_doc = proxy_source.parse::<DocumentMut>().unwrap();
    assert_eq!(proxy_doc["model_provider"].as_str(), Some("yi"));
    assert!(proxy_doc["model_providers"].get("yi_direct_one").is_none());
    assert!(!proxy_source.contains("secret"));
    assert_eq!(
        crate::db::repo::terminal_direct_profiles::get(&conn, "codex").unwrap(),
        Some(profile)
    );
}

#[test]
fn codex_direct_provider_models_write_upstream_ids_to_a_separate_catalog() {
    let (dir, conn, _) = fixture();
    let profile = DirectProfile {
        client: "codex".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: Some("real-one".into()),
    };
    let path = dir.path().join("config.toml");
    apply_direct_at(&conn, &profile, &path).unwrap();
    let doc = fs::read_to_string(&path)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(doc["model"].as_str(), Some("real-one"));
    assert_eq!(
        doc["model_catalog_json"].as_str(),
        Some(
            path.with_file_name("yi-codex-direct-models.json")
                .to_string_lossy()
                .as_ref()
        )
    );
    let catalog: Value = serde_json::from_slice(
        &fs::read(path.with_file_name("yi-codex-direct-models.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(catalog["models"][0]["slug"], "real-one");
    assert!(!catalog.to_string().contains("real-one(one)"));
}

#[test]
fn direct_provider_models_default_to_first_upstream_model_when_unspecified() {
    let (dir, conn, _) = fixture();
    let profile = DirectProfile {
        client: "codex".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: None,
    };
    let path = dir.path().join("config.toml");
    apply_direct_at(&conn, &profile, &path).unwrap();
    let doc = fs::read_to_string(&path)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert_eq!(doc["model"].as_str(), Some("real-one"));
    let catalog: Value = serde_json::from_slice(
        &fs::read(path.with_file_name("yi-codex-direct-models.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(catalog["models"][0]["slug"], "real-one");
    assert_eq!(
        crate::db::repo::terminal_direct_profiles::get(&conn, "codex").unwrap(),
        Some(profile)
    );
}

#[test]
fn direct_provider_models_reject_providers_without_maintained_models() {
    let (dir, conn, _) = fixture();
    conn.execute("DELETE FROM models WHERE provider_id='one'", [])
        .unwrap();
    let profile = DirectProfile {
        client: "codex".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: None,
    };
    let path = dir.path().join("config.toml");
    assert!(preview_direct_at(&conn, &profile, &path).is_err());
    assert!(!path.exists());
}
#[test]
fn direct_preview_endpoint_redacts_userinfo_and_query_parameters() {
    assert_eq!(
        crate::terminal::clients::redact_endpoint(
            "https://user:password@example.test/v1?api_key=secret&region=1#token"
        ),
        "https://example.test/v1?…"
    );
}

#[test]
fn direct_modes_require_the_clients_native_upstream_protocol() {
    let (dir, conn, _) = fixture();
    conn.execute("UPDATE providers SET type='openai_chat' WHERE id='one'", [])
        .unwrap();
    // This provider advertises converted Responses support, but that is not native direct compatibility.
    let profile = DirectProfile {
        client: "codex".into(),
        provider_id: "one".into(),
        model_source: "native".into(),
        model: None,
    };
    assert!(preview_direct_at(&conn, &profile, &dir.path().join("config.toml")).is_err());
}

#[test]
fn claude_and_opencode_direct_configs_use_their_native_provider_shapes_and_redact_keys() {
    let (dir, conn, mut proxy_profile) = fixture();
    conn.execute("UPDATE providers SET type='anthropic' WHERE id='one'", [])
        .unwrap();
    let claude = DirectProfile {
        client: "claude-code".into(),
        provider_id: "one".into(),
        model_source: "native".into(),
        model: None,
    };
    let claude_path = dir.path().join("claude/settings.json");
    apply_direct_at(&conn, &claude, &claude_path).unwrap();
    let claude_doc: Value = serde_json::from_slice(&fs::read(&claude_path).unwrap()).unwrap();
    assert_eq!(
        claude_doc
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str),
        Some("http://localhost")
    );
    assert_eq!(
        claude_doc
            .pointer("/env/ANTHROPIC_AUTH_TOKEN")
            .and_then(Value::as_str),
        Some("secret")
    );
    assert!(!preview_direct_at(&conn, &claude, &claude_path)
        .unwrap()
        .files[0]
        .content
        .contains("secret"));

    conn.execute("UPDATE providers SET type='responses' WHERE id='one'", [])
        .unwrap();
    let opencode = DirectProfile {
        client: "opencode".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: Some("real-one".into()),
    };
    let opencode_path = dir.path().join("opencode/opencode.json");
    apply_direct_at(&conn, &opencode, &opencode_path).unwrap();
    let opencode_doc: Value = serde_json::from_slice(&fs::read(&opencode_path).unwrap()).unwrap();
    assert_eq!(
        opencode_doc["provider"]["yi_direct_one"]["npm"],
        "@ai-sdk/openai"
    );
    assert_eq!(
        opencode_doc["provider"]["yi_direct_one"]["options"]["baseURL"],
        "http://localhost"
    );
    assert_eq!(opencode_doc["model"], "yi_direct_one/real-one");
    assert!(!preview_direct_at(&conn, &opencode, &opencode_path)
        .unwrap()
        .files[0]
        .content
        .contains("secret"));
    assert!(direct_active(&conn, &opencode, &opencode_path).unwrap());
    conn.execute("UPDATE providers SET api_key='rotated' WHERE id='one'", [])
        .unwrap();
    assert!(!direct_active(&conn, &opencode, &opencode_path).unwrap());

    proxy_profile.client = "opencode".into();
    apply_at(&conn, &proxy_profile, &opencode_path).unwrap();
    let proxy_source = fs::read_to_string(&opencode_path).unwrap();
    let proxy_doc: Value = serde_json::from_str(&proxy_source).unwrap();
    assert_eq!(proxy_doc["model"], "yi/real-one(one)");
    assert!(proxy_doc["provider"].get("yi_direct_one").is_none());
    assert!(!proxy_source.contains("secret"));
    assert_eq!(
        crate::db::repo::terminal_direct_profiles::get(&conn, "opencode").unwrap(),
        Some(opencode)
    );
}

#[test]
fn pi_proxy_config_writes_models_settings_and_thinking_levels() {
    let (dir, conn, mut profile) = fixture();
    profile.client = "pi".into();
    // Model "one" declares effort levels; Pi maps its thinking levels onto them.
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [json!({
            "input_modalities":["text","image"],
            "output_modalities":["text"],
            "context_window":200000,
            "max_output_tokens":32000,
            "effort":{"support":"supported","levels":["low","high","max"],"default":"high"}
        })
        .to_string()],
    )
    .unwrap();
    let path = dir.path().join("pi/models.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        r#"{"providers":{"other":{"api":"openai-completions","baseUrl":"https://example.test/v1","apiKey":"other-secret"}}}"#,
    )
    .unwrap();
    let settings_path = dir.path().join("pi/settings.json");
    fs::write(
        &settings_path,
        r#"{"theme":"dark","defaultProvider":"other"}"#,
    )
    .unwrap();

    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(result.model_count, 2);
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"]["yi"]["api"], "openai-completions");
    assert_eq!(
        doc["providers"]["yi"]["baseUrl"],
        "http://127.0.0.1:11435/clients/pi/v1"
    );
    assert_eq!(doc["providers"]["yi"]["apiKey"], "yi");
    assert_eq!(doc["providers"]["other"]["apiKey"], "other-secret");
    let models = doc["providers"]["yi"]["models"].as_array().unwrap();
    let default = models
        .iter()
        .find(|entry| entry["id"] == "real-one(one)")
        .unwrap();
    assert_eq!(default["reasoning"], true);
    assert_eq!(default["contextWindow"], 200000);
    assert_eq!(default["maxTokens"], 32000);
    assert_eq!(default["input"], json!(["text", "image"]));
    assert_eq!(default["thinkingLevelMap"]["minimal"], Value::Null);
    assert_eq!(default["thinkingLevelMap"]["low"], "low");
    assert_eq!(default["thinkingLevelMap"]["medium"], Value::Null);
    assert_eq!(default["thinkingLevelMap"]["high"], "high");
    assert_eq!(default["thinkingLevelMap"]["max"], "max");
    assert!(models
        .iter()
        .find(|entry| entry["id"] == "real-two(two)")
        .unwrap()
        .get("reasoning")
        .is_none());
    let settings: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
    assert_eq!(settings["theme"], "dark");
    assert_eq!(settings["defaultProvider"], "yi");
    assert_eq!(settings["defaultModel"], "real-one(one)");
    let source = fs::read_to_string(&path).unwrap();
    assert!(crate::terminal::clients::configurator("pi")
        .unwrap()
        .is_active(
            &path,
            &source,
            &endpoint(&conn, "pi").unwrap(),
            protocol("pi").unwrap()
        )
        .unwrap());
    // The preview shows owned settings only; the foreign provider key stays hidden.
    let preview = preview_at(&conn, &profile, &path).unwrap();
    let joined = preview
        .files
        .iter()
        .map(|file| file.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!joined.contains("other-secret"));
    assert!(!joined.contains("secret"));
    assert!(joined.contains("thinkingLevelMap"));
    assert!(joined.contains("\"defaultProvider\": \"yi\""));
}

#[test]
fn pi_direct_configs_use_native_apis_remove_stale_providers_and_redact_keys() {
    let (dir, conn, _) = fixture();
    // Pi always syncs maintained upstream models; native model following is not offered.
    let native = DirectProfile {
        client: "pi".into(),
        provider_id: "one".into(),
        model_source: "native".into(),
        model: None,
    };
    assert!(preview_direct_at(&conn, &native, &dir.path().join("pi/models.json")).is_err());

    // Provider "one" speaks Responses; provider "two" speaks Anthropic.
    conn.execute("UPDATE providers SET type='anthropic' WHERE id='two'", [])
        .unwrap();
    let mut profile = DirectProfile {
        client: "pi".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: None,
    };
    let path = dir.path().join("pi/models.json");
    apply_direct_at(&conn, &profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"]["yi_direct_one"]["api"], "openai-responses");
    assert_eq!(
        doc["providers"]["yi_direct_one"]["baseUrl"],
        "http://localhost"
    );
    assert_eq!(doc["providers"]["yi_direct_one"]["apiKey"], "secret");
    let direct_models = doc["providers"]["yi_direct_one"]["models"]
        .as_array()
        .unwrap();
    assert_eq!(direct_models.len(), 1);
    assert_eq!(direct_models[0]["id"], "real-one");
    assert_eq!(direct_models[0]["name"], "real-one");
    let settings: Value =
        serde_json::from_slice(&fs::read(dir.path().join("pi/settings.json")).unwrap()).unwrap();
    assert_eq!(settings["defaultProvider"], "yi_direct_one");
    assert_eq!(settings["defaultModel"], "real-one");
    assert!(direct_active(&conn, &profile, &path).unwrap());
    let preview = preview_direct_at(&conn, &profile, &path).unwrap();
    for file in &preview.files {
        assert!(!file.content.contains("secret"));
    }
    assert!(preview
        .files
        .iter()
        .any(|file| file.content.contains("openai-responses")));

    // Switching the direct Provider replaces the previous owned entry and uses Messages.
    profile.provider_id = "two".into();
    apply_direct_at(&conn, &profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(doc["providers"].get("yi_direct_one").is_none());
    assert_eq!(
        doc["providers"]["yi_direct_two"]["api"],
        "anthropic-messages"
    );
    assert!(direct_active(&conn, &profile, &path).unwrap());

    // A proxy apply removes direct entries and restores startup defaults.
    let proxy_profile = Profile {
        client: "pi".into(),
        models: vec![Selection {
            provider_id: "one".into(),
            model: "real-one(one)".into(),
        }],
        default_model: "real-one(one)".into(),
        protocol: None,
    };
    apply_at(&conn, &proxy_profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(doc["providers"].get("yi_direct_two").is_none());
    assert_eq!(doc["providers"]["yi"]["api"], "openai-completions");
}

#[test]
fn pi_protocol_selection_drives_config_endpoint_and_routing() {
    let (dir, conn, mut profile) = fixture();
    profile.client = "pi".into();
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [json!({
            "input_modalities":["text"],
            "output_modalities":["text"],
            "context_window":200000,
            "max_output_tokens":32000,
            "effort":{"support":"supported","levels":["low","medium","high"],"default":"medium"}
        })
        .to_string()],
    )
    .unwrap();
    let path = dir.path().join("pi/models.json");

    // Anthropic Messages is served from the bare client root and pins the generated api.
    profile.protocol = Some("anthropic".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(result.endpoint, "http://127.0.0.1:11435/clients/pi");
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"]["yi"]["api"], "anthropic-messages");
    assert_eq!(
        doc["providers"]["yi"]["baseUrl"],
        "http://127.0.0.1:11435/clients/pi"
    );
    assert!(resolve(&conn, "pi", "real-one(one)", "anthropic").is_ok());
    assert!(resolve(&conn, "pi", "real-one(one)", "openai_chat").is_err());
    assert!(resolve(&conn, "pi", "real-one(one)", "responses").is_err());

    // Responses hides the off level: pi would otherwise send a literal `none` effort.
    profile.protocol = Some("responses".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(result.endpoint, "http://127.0.0.1:11435/clients/pi/v1");
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"]["yi"]["api"], "openai-responses");
    let model = doc["providers"]["yi"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "real-one(one)")
        .unwrap();
    assert!(matches!(
        model["thinkingLevelMap"].get("off"),
        Some(Value::Null)
    ));
    assert_eq!(model["thinkingLevelMap"]["medium"], "medium");
    assert!(resolve(&conn, "pi", "real-one(one)", "responses").is_ok());
    assert!(resolve(&conn, "pi", "real-one(one)", "anthropic").is_err());

    // Without a stored protocol, chat completions remains the primary entrypoint.
    profile.protocol = None;
    apply_at(&conn, &profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["providers"]["yi"]["api"], "openai-completions");
    let model = doc["providers"]["yi"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "real-one(one)")
        .unwrap();
    assert!(model["thinkingLevelMap"].get("off").is_none());
    assert!(resolve(&conn, "pi", "real-one(one)", "openai_chat").is_ok());
    assert!(resolve(&conn, "pi", "real-one(one)", "responses").is_err());
}

#[test]
fn stored_protocols_are_validated_against_the_client() {
    let (_dir, conn, mut profile) = fixture();
    profile.client = "codex".into();
    profile.protocol = Some("anthropic".into());
    assert!(validate(&conn, &profile)
        .unwrap_err()
        .to_string()
        .contains("不支持所选协议"));
    profile.client = "pi".into();
    assert!(validate(&conn, &profile).is_ok());
}

#[test]
fn opencode_protocol_selection_drives_package_endpoint_and_routing() {
    let (dir, conn, mut profile) = fixture();
    profile.client = "opencode".into();
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [json!({
            "input_modalities":["text"],
            "output_modalities":["text"],
            "context_window":200000,
            "max_output_tokens":32000,
            "effort":{"support":"supported","levels":["low","medium","high"],"default":"high"}
        })
        .to_string()],
    )
    .unwrap();
    let path = dir.path().join("opencode/opencode.json");

    // Responses loads the openai package; variants carry effort, summary, encrypted
    // reasoning and `forceReasoning` (OpenCode's AI SDK cannot infer reasoning from yi
    // route ids). The `reasoning` flag stays off so OpenCode adds no undeclared levels.
    profile.protocol = Some("responses".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(
        result.endpoint,
        "http://127.0.0.1:11435/clients/opencode/v1"
    );
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["provider"]["yi"]["npm"], "@ai-sdk/openai");
    let model = doc["provider"]["yi"]["models"]["real-one(one)"].clone();
    assert_eq!(model["variants"]["high"]["reasoningEffort"], "high");
    assert_eq!(model["variants"]["high"]["reasoningSummary"], "auto");
    assert_eq!(
        model["variants"]["high"]["include"],
        json!(["reasoning.encrypted_content"])
    );
    assert_eq!(model["variants"]["high"]["forceReasoning"], true);
    assert_eq!(model["options"]["reasoningEffort"], "high");
    assert_eq!(model["options"]["forceReasoning"], true);
    assert!(model.get("reasoning").is_none());
    assert!(resolve(&conn, "opencode", "real-one(one)", "responses").is_ok());
    assert!(resolve(&conn, "opencode", "real-one(one)", "openai_chat").is_err());

    // Anthropic keeps the `/v1` entrypoint (its AI SDK only appends `/messages`) and maps
    // reasoning levels to effort instead of reasoning_effort.
    profile.protocol = Some("anthropic".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(
        result.endpoint,
        "http://127.0.0.1:11435/clients/opencode/v1"
    );
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["provider"]["yi"]["npm"], "@ai-sdk/anthropic");
    let model = doc["provider"]["yi"]["models"]["real-one(one)"].clone();
    assert_eq!(model["variants"]["low"]["effort"], "low");
    assert!(model["variants"]["low"].get("reasoningEffort").is_none());
    assert_eq!(model["options"]["effort"], "high");
    // The reasoning flag would make OpenCode inject its own budget-based thinking, which
    // raises `max_tokens` past the declared output limit.
    assert!(model.get("reasoning").is_none());
    assert!(resolve(&conn, "opencode", "real-one(one)", "anthropic").is_ok());
    assert!(resolve(&conn, "opencode", "real-one(one)", "responses").is_err());

    // Chat completions stays the default entrypoint with the openai-compatible package.
    profile.protocol = None;
    apply_at(&conn, &profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["provider"]["yi"]["npm"], "@ai-sdk/openai-compatible");
    assert_eq!(
        doc["provider"]["yi"]["models"]["real-one(one)"]["variants"]["high"]["reasoningEffort"],
        "high"
    );
    assert!(doc["provider"]["yi"]["models"]["real-one(one)"]
        .get("reasoning")
        .is_none());
    assert!(resolve(&conn, "opencode", "real-one(one)", "openai_chat").is_ok());
    assert!(resolve(&conn, "opencode", "real-one(one)", "anthropic").is_err());
}

#[test]
fn opencode_direct_anthropic_uses_messages_package_and_v1_base() {
    let (dir, conn, _) = fixture();
    conn.execute("UPDATE providers SET type='anthropic' WHERE id='one'", [])
        .unwrap();
    let profile = DirectProfile {
        client: "opencode".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: Some("real-one".into()),
    };
    let path = dir.path().join("opencode/opencode.json");
    apply_direct_at(&conn, &profile, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(doc["provider"]["yi_direct_one"]["npm"], "@ai-sdk/anthropic");
    assert_eq!(
        doc["provider"]["yi_direct_one"]["options"]["baseURL"],
        "http://localhost/v1"
    );
    assert_eq!(
        doc["provider"]["yi_direct_one"]["options"]["apiKey"],
        "secret"
    );
    assert!(direct_active(&conn, &profile, &path).unwrap());
    conn.execute("UPDATE providers SET api_key='rotated' WHERE id='one'", [])
        .unwrap();
    assert!(!direct_active(&conn, &profile, &path).unwrap());
    conn.execute("UPDATE providers SET api_key='secret' WHERE id='one'", [])
        .unwrap();

    // A proxy apply for anthropic removes the direct entry and keeps the `/v1` endpoint.
    let proxy = Profile {
        client: "opencode".into(),
        models: vec![Selection {
            provider_id: "one".into(),
            model: "real-one(one)".into(),
        }],
        default_model: "real-one(one)".into(),
        protocol: Some("anthropic".into()),
    };
    apply_at(&conn, &proxy, &path).unwrap();
    let doc: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(doc["provider"].get("yi_direct_one").is_none());
    assert_eq!(doc["provider"]["yi"]["npm"], "@ai-sdk/anthropic");
    assert_eq!(
        doc["provider"]["yi"]["options"]["baseURL"],
        "http://127.0.0.1:11435/clients/opencode/v1"
    );
}

fn deepseek_doc(source: &str) -> Value {
    serde_json::to_value(serde_yaml::from_str::<serde_yaml::Value>(source).unwrap()).unwrap()
}

fn deepseek_providers(source: &str) -> Value {
    deepseek_doc(source)
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "llm-pi-ai")
        .unwrap()["config"]["providers"]
        .clone()
}

#[test]
fn deepseek_harness_merges_patch_updates_defaults_and_routes() {
    let (dir, conn, mut profile) = fixture();
    profile.client = "deepseek-harness".into();
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='one'",
        [json!({
            "input_modalities":["text"],
            "output_modalities":["text"],
            "context_window":200000,
            "max_output_tokens":32000,
            "effort":{"support":"supported","levels":["low","medium","high"],"default":"medium"}
        })
        .to_string()],
    )
    .unwrap();
    conn.execute(
        "UPDATE models SET capabilities=?1 WHERE provider_id='two'",
        [json!({"context_window":1048576}).to_string()],
    )
    .unwrap();
    let path = dir.path().join("dsh/profiles/web/cordis.patch.yml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let original = "# preserve\n- id: llm-pi-ai\n  config:\n    providers:\n      custom-gateway:\n        api: openai-completions\n        baseURL: https://gateway.example/v1\n- id: agent-loop\n  config:\n    agents:\n      - id: main\n        provider: deepseek\n        model: deepseek-flash\n";
    fs::write(&path, original).unwrap();
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(result.model_count, 2);
    assert_eq!(
        fs::read_to_string(&result.backup_paths[0]).unwrap(),
        original
    );
    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains("secret"));
    let doc = deepseek_doc(&content);
    let providers = deepseek_providers(&content);
    // The user's other pi-ai routes survive the merge untouched.
    assert_eq!(
        providers["custom-gateway"]["baseURL"],
        json!("https://gateway.example/v1")
    );
    let yi = &providers["yi"];
    assert_eq!(yi["api"], json!("openai-completions"));
    assert_eq!(
        yi["baseURL"],
        json!("http://127.0.0.1:11435/clients/deepseek-harness/v1")
    );
    assert_eq!(yi["apiKeyEnv"], json!("YI_LLM_PROXY"));
    assert!(yi.get("apiKey").is_none());
    let models = yi["models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    let one = models
        .iter()
        .find(|model| model["id"] == "real-one(one)")
        .unwrap();
    assert_eq!(one["input"], json!(["text"]));
    assert_eq!(one["contextWindow"], json!(200000));
    assert_eq!(one["maxTokens"], json!(32000));
    assert_eq!(one["reasoningEfforts"]["off"], Value::Null);
    assert_eq!(one["reasoningEfforts"]["medium"], json!("medium"));
    assert!(one["reasoningEfforts"].get("xhigh").is_none());
    let two = models
        .iter()
        .find(|model| model["id"] == "real-two(two)")
        .unwrap();
    // A model with a context window but no output cap still publishes its context.
    assert_eq!(two["contextWindow"], json!(1048576));
    assert!(two.get("maxTokens").is_none());
    // The default startup model follows the profile default inside an existing agent.
    let agent = doc
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == "agent-loop")
        .unwrap();
    assert_eq!(agent["config"]["agents"][0]["provider"], json!("yi"));
    assert_eq!(
        agent["config"]["agents"][0]["model"],
        json!("real-one(one)")
    );
    // The merged file still reads back as an active proxy configuration.
    let source = fs::read_to_string(&path).unwrap();
    assert!(crate::terminal::clients::configurator("deepseek-harness")
        .unwrap()
        .is_active(
            &path,
            &source,
            &result.endpoint,
            protocol("deepseek-harness").unwrap()
        )
        .unwrap());
    assert!(resolve(
        &conn,
        "deepseek-harness",
        "real-two(two)",
        protocol("deepseek-harness").unwrap()
    )
    .is_ok());
    // The proxy credential lands in dsh's own hot-reloaded credential store.
    let store = fs::read_to_string(dir.path().join("dsh/.credentials.yaml")).unwrap();
    assert!(store.contains("YI_LLM_PROXY"));
    assert!(store.contains("yi"));
}

#[test]
fn deepseek_harness_protocol_selection_drives_api_and_endpoint() {
    let (dir, conn, mut profile) = fixture();
    profile.client = "deepseek-harness".into();
    let path = dir.path().join("dsh/profiles/web/cordis.patch.yml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();

    // Anthropic Messages is served from the bare client root and pins the generated api.
    profile.protocol = Some("anthropic".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(
        result.endpoint,
        "http://127.0.0.1:11435/clients/deepseek-harness"
    );
    let yi = deepseek_providers(&fs::read_to_string(&path).unwrap())["yi"].clone();
    assert_eq!(yi["api"], json!("anthropic-messages"));
    assert_eq!(
        yi["baseURL"],
        json!("http://127.0.0.1:11435/clients/deepseek-harness")
    );
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "anthropic").is_ok());
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "openai_chat").is_err());

    // Responses keeps the `/v1` entrypoint like every other OpenAI-compatible route.
    profile.protocol = Some("responses".into());
    let result = apply_at(&conn, &profile, &path).unwrap();
    assert_eq!(
        result.endpoint,
        "http://127.0.0.1:11435/clients/deepseek-harness/v1"
    );
    let yi = deepseek_providers(&fs::read_to_string(&path).unwrap())["yi"].clone();
    assert_eq!(yi["api"], json!("openai-responses"));
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "responses").is_ok());
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "anthropic").is_err());

    // Chat completions stays the primary entrypoint without a stored protocol.
    profile.protocol = None;
    apply_at(&conn, &profile, &path).unwrap();
    let yi = deepseek_providers(&fs::read_to_string(&path).unwrap())["yi"].clone();
    assert_eq!(yi["api"], json!("openai-completions"));
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "openai_chat").is_ok());
    assert!(resolve(&conn, "deepseek-harness", "real-one(one)", "responses").is_err());
}

#[test]
fn deepseek_harness_direct_uses_credential_reference_and_cleanup() {
    let (dir, conn, _) = fixture();
    let mut profile = DirectProfile {
        client: "deepseek-harness".into(),
        provider_id: "one".into(),
        model_source: "provider".into(),
        model: None,
    };
    let path = dir.path().join("dsh/profiles/web/cordis.patch.yml");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let store_path = dir.path().join("dsh/.credentials.yaml");
    fs::write(&store_path, "version: 1\nrefs:\n  OTHER_KEY: keep\n").unwrap();
    apply_direct_at(&conn, &profile, &path).unwrap();
    // dsh patch files never carry plaintext keys; the route references a credential.
    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains("secret"));
    let providers = deepseek_providers(&content);
    let direct = &providers["yi_direct_one"];
    assert_eq!(direct["api"], json!("openai-responses"));
    assert_eq!(direct["baseURL"], json!("http://localhost"));
    assert_eq!(direct["name"], json!("yi-llm direct:one"));
    assert_eq!(direct["apiKeyEnv"], json!("YI_LLM_DIRECT_ONE"));
    let models = direct["models"].as_array().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0]["id"], json!("real-one"));
    assert!(direct_active(&conn, &profile, &path).unwrap());
    let preview = preview_direct_at(&conn, &profile, &path).unwrap();
    for file in &preview.files {
        assert!(!file.content.contains("secret"));
    }
    // The provider's real key is provisioned into dsh's credential store.
    let store = fs::read_to_string(&store_path).unwrap();
    assert!(store.contains("OTHER_KEY"));
    assert!(store.contains("YI_LLM_DIRECT_ONE"));

    // Switching the direct Provider replaces the previous owned entry and uses Messages.
    conn.execute("UPDATE providers SET type='anthropic' WHERE id='two'", [])
        .unwrap();
    profile.provider_id = "two".into();
    apply_direct_at(&conn, &profile, &path).unwrap();
    let store = fs::read_to_string(&store_path).unwrap();
    assert!(store.contains("YI_LLM_DIRECT_TWO"));
    assert!(!store.contains("YI_LLM_DIRECT_ONE"));
    let providers = deepseek_providers(&fs::read_to_string(&path).unwrap());
    assert!(providers.get("yi_direct_one").is_none());
    assert_eq!(
        providers["yi_direct_two"]["api"],
        json!("anthropic-messages")
    );
    assert!(direct_active(&conn, &profile, &path).unwrap());

    // A proxy apply removes direct entries and restores the keyless proxy route.
    let proxy = Profile {
        client: "deepseek-harness".into(),
        models: vec![Selection {
            provider_id: "one".into(),
            model: "real-one(one)".into(),
        }],
        default_model: "real-one(one)".into(),
        protocol: None,
    };
    apply_at(&conn, &proxy, &path).unwrap();
    let providers = deepseek_providers(&fs::read_to_string(&path).unwrap());
    assert!(providers.get("yi_direct_two").is_none());
    assert_eq!(providers["yi"]["api"], json!("openai-completions"));
    // The proxy route provisions its placeholder and prunes direct references.
    let store = fs::read_to_string(&store_path).unwrap();
    assert!(store.contains("YI_LLM_PROXY"));
    assert!(!store.contains("YI_LLM_DIRECT_TWO"));
    assert!(store.contains("OTHER_KEY"));
}
