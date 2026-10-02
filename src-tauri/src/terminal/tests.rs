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
