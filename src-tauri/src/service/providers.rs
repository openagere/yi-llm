use crate::{
    app::AppState,
    backup::ProviderConfigBundle,
    db::repo::{catalog, models, providers},
    domain::{
        provider::{ModelMapping, Provider, ProviderView},
        provider_names::model_name,
        standard_model::StandardModel,
        terminal::Profile,
    },
    error::{AppError, Result},
    protocol::upstream::upstream_client,
    terminal,
};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Serialize)]
pub struct SaveProviderResult {
    pub terminal_config_warnings: Vec<String>,
}

pub async fn list(state: &AppState) -> Result<Vec<ProviderView>> {
    state.refresh_catalog().await?;
    state.db.run(|conn| providers::list_views(conn)).await
}

/// Collects every Provider (with its model mappings) plus the whole model catalog, so an
/// exported file can be applied to another installation without extra steps.
pub async fn export_bundle(state: &AppState) -> Result<ProviderConfigBundle> {
    state.refresh_catalog().await?;
    let (views, standard_models) = state
        .db
        .run(|conn| Ok((providers::list_views(conn)?, catalog::list(conn)?)))
        .await?;
    Ok(ProviderConfigBundle::export(views, standard_models))
}

/// What an applied import changed.
#[derive(Debug, Serialize)]
pub struct ImportSummary {
    pub providers: usize,
    pub models: usize,
    /// Standard models merged into the local catalog.
    pub standard_models: usize,
    pub terminal_config_warnings: Vec<String>,
}

/// Applies an exported bundle.
///
/// Standard models are merged into the catalog first so the imported references resolve,
/// then every Provider is written in a single transaction. Local standard models are never
/// overwritten: entries already present (by id, or by name within the same protocol) win and
/// references are remapped onto them, which keeps an import from another installation
/// consistent. Providers that are not part of the file are left untouched.
pub async fn import_bundle(
    state: &AppState,
    bundle: ProviderConfigBundle,
) -> Result<ImportSummary> {
    state.refresh_catalog().await?;
    bundle.validate()?;
    let catalog_service = state.catalog.clone();
    let ProviderConfigBundle {
        standard_models,
        providers: mut views,
        ..
    } = bundle;
    let summary = ImportSummary {
        providers: views.len(),
        models: views.iter().map(|view| view.models.len()).sum(),
        standard_models: 0,
        terminal_config_warnings: Vec::new(),
    };
    let (added, terminal_config_warnings) = state
        .db
        .run(move |conn| {
            let (pending, remap) = plan_standard_models(&catalog::list(conn)?, &standard_models)?;
            for view in &mut views {
                normalize_imported_models(view);
            }
            apply_remap(&mut views, &remap);
            validate_import(conn, &views, &pending)?;
            // An unrelated local default remains the default after merging foreign providers.
            let local_default = providers::list(conn)?.into_iter().find(|provider| {
                provider.is_default && !views.iter().any(|view| view.provider.id == provider.id)
            });
            if local_default.is_some() {
                for view in &mut views {
                    view.provider.is_default = false;
                }
            }
            let mut clients = BTreeSet::new();
            let mut direct_clients = BTreeMap::new();
            for view in &views {
                let previous_provider = providers::get(conn, &view.provider.id)?;
                let previous = models::list_for_provider(conn, &view.provider.id)?;
                let routes_changed = previous.iter().any(|old| {
                    !view
                        .models
                        .iter()
                        .any(|new| new.exposed_name == old.exposed_name)
                });
                if routes_changed {
                    clients.extend(active_clients_using(conn, &view.provider.id));
                }
                let upstreams_changed = previous.len() != view.models.len()
                    || previous.iter().any(|old| {
                        !view
                            .models
                            .iter()
                            .any(|new| new.upstream_model == old.upstream_model)
                    });
                if previous_provider
                    .as_ref()
                    .is_some_and(|old| old != &view.provider)
                    || upstreams_changed
                {
                    for profile in active_direct_clients_using(conn, &view.provider.id) {
                        direct_clients.insert(profile.client.clone(), profile);
                    }
                }
            }
            if pending.is_empty() {
                providers::save_many(conn, &views)?;
            } else {
                let catalog = catalog_service
                    .as_ref()
                    .ok_or_else(|| AppError::internal("模型目录未初始化，无法导入标准模型"))?;
                catalog
                    .save_many_with(conn, &pending, |tx| providers::save_many_in_tx(tx, &views))?;
            }
            let mut warnings = Vec::new();
            for client in clients {
                let result = terminal::load(conn, client).and_then(|profile| {
                    let profile: Profile = profile.ok_or("终端模型集合不存在")?;
                    terminal::apply(conn, &profile).map(|_| ())
                });
                if let Err(error) = result {
                    warnings.push(format!(
                        "{client} 配置未更新：{error}。请在终端接入中更新配置"
                    ));
                }
            }
            for (client, profile) in direct_clients {
                if let Err(error) = terminal::apply_direct(conn, &profile) {
                    warnings.push(format!(
                        "{client} 直连配置未更新：{error}。请在终端接入中更新配置"
                    ));
                }
            }
            Ok((pending.len(), warnings))
        })
        .await?;
    state.invalidate_routes();
    Ok(ImportSummary {
        standard_models: added,
        terminal_config_warnings,
        ..summary
    })
}

/// Decides which standard models must be added locally and how incoming references map onto
/// existing local entries. Local definitions win, so an import never rewrites a model that
/// another Provider already depends on.
fn plan_standard_models(
    local: &[StandardModel],
    incoming: &[StandardModel],
) -> Result<(Vec<StandardModel>, HashMap<String, String>)> {
    let mut pending = Vec::new();
    let mut remap = HashMap::new();
    for model in incoming {
        remap.insert(model.id.clone(), model.id.clone());
        if let Some(existing) = local.iter().find(|candidate| candidate.id == model.id) {
            if existing.protocol != model.protocol {
                let (name, protocol) = (model.name.trim(), model.protocol.as_str());
                return Err(AppError::conflict(format!(
                    "标准模型「{name}」与本地同标识模型的协议不一致（{protocol}）"
                )));
            }
            remap.insert(model.id.clone(), existing.id.clone());
            continue;
        }
        let name = model.name.trim();
        if let Some(existing) = local
            .iter()
            .find(|candidate| candidate.name.trim() == name && candidate.protocol == model.protocol)
        {
            remap.insert(model.id.clone(), existing.id.clone());
            continue;
        }
        pending.push(model.clone());
    }
    Ok((pending, remap))
}

/// Rewrites standard model references according to [plan_standard_models].
fn apply_remap(views: &mut [ProviderView], remap: &HashMap<String, String>) {
    for view in views {
        for model in &mut view.models {
            let Some(id) = model.standard_model_id.clone() else {
                continue;
            };
            if let Some(mapped) = remap.get(&id) {
                model.standard_model_id = Some(mapped.clone());
            }
        }
    }
}

/// Imported mappings are normalized exactly like editor saves, so importing the same file
/// twice is idempotent. Capabilities come from the bound standard model, as everywhere else.
fn normalize_imported_models(view: &mut ProviderView) {
    for model in &mut view.models {
        model.upstream_model = model.upstream_model.trim().to_owned();
        model.provider_id = view.provider.id.clone();
        model.exposed_name = model_name(&model.upstream_model, &view.provider.short_code);
    }
}

/// Rejects the import before anything is written: occupied short codes and standard model
/// references that neither the file nor the local catalog can provide.
fn validate_import(
    conn: &Connection,
    views: &[ProviderView],
    pending: &[StandardModel],
) -> Result<()> {
    let existing = providers::list(conn)?;
    for view in views {
        let provider = &view.provider;
        let occupied = existing.iter().any(|candidate| {
            candidate.id != provider.id
                && candidate
                    .short_code
                    .eq_ignore_ascii_case(&provider.short_code)
        });
        if occupied {
            let (name, code) = (provider.name.trim(), provider.short_code.as_str());
            return Err(AppError::conflict(format!(
                "Provider「{name}」的简写 {code} 已被本地其他连接占用"
            )));
        }
        for model in &view.models {
            let Some(id) = &model.standard_model_id else {
                continue;
            };
            let standard = match pending.iter().find(|candidate| &candidate.id == id) {
                Some(standard) => standard.clone(),
                None => catalog::get(conn, id)?.ok_or_else(|| {
                    let (name, upstream) = (provider.name.trim(), model.upstream_model.trim());
                    AppError::validation(format!(
                        "Provider「{name}」的模型「{upstream}」引用的标准模型不存在：{id}"
                    ))
                })?,
            };
            if standard.protocol != provider.provider_type {
                let (name, upstream, standard_name) = (
                    provider.name.trim(),
                    model.upstream_model.trim(),
                    standard.name.trim(),
                );
                return Err(AppError::validation(format!(
                    "Provider「{name}」的模型「{upstream}」与标准模型「{standard_name}」的协议不一致"
                )));
            }
        }
    }
    Ok(())
}

pub async fn save(
    state: &AppState,
    provider: Provider,
    models: Vec<ModelMapping>,
) -> Result<SaveProviderResult> {
    state.refresh_catalog().await?;
    let result = state
        .db
        .run(move |conn| save_and_refresh_terminals(conn, provider, models))
        .await;
    state.invalidate_routes();
    result
}

pub async fn delete(state: &AppState, id: String) -> Result<()> {
    let result = state.db.run(move |conn| providers::delete(conn, &id)).await;
    state.invalidate_routes();
    result
}

fn save_and_refresh_terminals(
    conn: &mut Connection,
    provider: Provider,
    mut models: Vec<ModelMapping>,
) -> Result<SaveProviderResult> {
    if models.iter().any(|model| model.standard_model_id.is_none()) {
        return Err("请为每个模型选择标准模型".into());
    }
    let mut names = HashSet::new();
    for model in &mut models {
        model.upstream_model = model.upstream_model.trim().to_owned();
        if !names.insert(model.upstream_model.clone()) {
            return Err("同一 Provider 的模型名称不能重复".into());
        }
        model.provider_id = provider.id.clone();
        model.exposed_name = model_name(&model.upstream_model, &provider.short_code);
        model.capabilities = Default::default();
    }
    let previous = models::list_for_provider(conn, &provider.id)?;
    let routes_changed = previous.iter().any(|old| {
        !models
            .iter()
            .any(|new| new.exposed_name == old.exposed_name)
    });
    let clients = if routes_changed {
        active_clients_using(conn, &provider.id)
    } else {
        Vec::new()
    };
    providers::save(conn, &ProviderView { provider, models })?;
    let mut terminal_config_warnings = Vec::new();
    for client in clients {
        let result = terminal::load(conn, client).and_then(|profile| {
            let profile: Profile = profile.ok_or("终端模型集合不存在")?;
            terminal::apply(conn, &profile).map(|_| ())
        });
        if let Err(error) = result {
            terminal_config_warnings.push(format!(
                "{client} 配置未更新：{error}。请在终端接入中更新配置"
            ));
        }
    }
    Ok(SaveProviderResult {
        terminal_config_warnings,
    })
}

/// Terminal clients whose applied config currently depends on `provider_id`.
fn active_clients_using(conn: &Connection, provider_id: &str) -> Vec<&'static str> {
    terminal::CLIENTS
        .into_iter()
        .filter(|client| {
            terminal::status(conn, client).ok().is_some_and(|status| {
                status.active_mode == "proxy"
                    && status.profile.as_ref().is_some_and(|profile| {
                        profile
                            .models
                            .iter()
                            .any(|selection| selection.provider_id == provider_id)
                    })
            })
        })
        .collect()
}

/// Direct-mode files embed provider settings and must be refreshed after an import changes them.
fn active_direct_clients_using(
    conn: &Connection,
    provider_id: &str,
) -> Vec<crate::domain::terminal::DirectProfile> {
    terminal::CLIENTS
        .into_iter()
        .filter_map(|client| terminal::status(conn, client).ok())
        .filter(|status| status.active_mode == "direct")
        .filter_map(|status| status.direct_profile)
        .filter(|profile| profile.provider_id == provider_id)
        .collect()
}

/// Sends a one-token request to verify credentials and the model name.
pub async fn test(
    state: &AppState,
    provider: Provider,
    models: Vec<ModelMapping>,
    model: String,
) -> Result<String> {
    let model = if !model.trim().is_empty() {
        model.trim().to_owned()
    } else {
        models
            .iter()
            .find(|m| !m.upstream_model.trim().is_empty())
            .map(|m| m.upstream_model.trim().to_owned())
            .ok_or("请填写测试模型，或先配置模型映射")?
    };
    let upstream = upstream_client(&provider.provider_type).ok_or_else(|| {
        AppError::validation(format!("未知 provider 类型: {}", provider.provider_type))
    })?;
    let body = upstream.probe_body(&model);
    let response = state
        .http
        .post(upstream.endpoint(&provider.base_url))
        .headers(upstream.native_headers(&provider, &body))
        .json(&body)
        .send()
        .await
        .map_err(|error| AppError::upstream(format!("连接失败: {error}")))?;
    let status = response.status();
    if status.is_success() {
        return Ok(format!("HTTP {status}"));
    }
    let body = response.text().await.unwrap_or_default();
    Err(AppError::upstream(describe_failure(status, &model, &body)))
}

fn describe_failure(status: reqwest::StatusCode, model: &str, body: &str) -> String {
    let detail = body.to_lowercase();
    let model_problem = detail.contains("model")
        && [
            "unsupported",
            "not found",
            "does not exist",
            "not available",
            "invalid",
        ]
        .iter()
        .any(|hint| detail.contains(hint));
    if model_problem {
        format!("HTTP {status}: 上游模型“{model}”不可用，请核对该 provider 支持的模型名称。上游信息：{body}")
    } else {
        format!("HTTP {status}: {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::CatalogService,
        db::Db,
        domain::provider::{ModelMapping, ProtocolSupport, Provider},
    };

    #[test]
    fn model_related_rejections_get_a_hint() {
        let status = reqwest::StatusCode::BAD_REQUEST;
        assert!(describe_failure(status, "m", "The model `m` does not exist").contains("不可用"));
        assert_eq!(
            describe_failure(status, "m", "bad key"),
            "HTTP 400 Bad Request: bad key"
        );
    }

    /// App state over a temporary database plus a real catalog file.
    struct Fixture {
        _dir: tempfile::TempDir,
        state: AppState,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("test.db")).unwrap();
        let catalog = {
            let conn = db.conn().unwrap();
            CatalogService::open(dir.path().join("models/catalog.json"), &conn).unwrap()
        };
        let state = AppState::new(db, reqwest::Client::new(), dir.path().join("logs"), None)
            .with_catalog(catalog);
        Fixture { _dir: dir, state }
    }

    /// First standard model of the built-in catalog, used as a valid binding target.
    async fn catalog_model(state: &AppState) -> StandardModel {
        state.refresh_catalog().await.unwrap();
        state
            .db
            .run(|conn| Ok(catalog::list(conn)?.into_iter().next().unwrap()))
            .await
            .unwrap()
    }

    fn view(id: &str, short_code: &str, standard: &StandardModel, upstream: &str) -> ProviderView {
        ProviderView {
            provider: Provider {
                id: id.into(),
                provider_type: standard.protocol.clone(),
                name: format!("Provider {id}"),
                short_code: short_code.into(),
                base_url: "https://example.com/v1".into(),
                api_key: "secret".into(),
                enabled: true,
                thinking: "medium".into(),
                extra: serde_json::json!({}),
                is_default: false,
                protocol_support: ProtocolSupport::native(&standard.protocol),
            },
            models: vec![ModelMapping {
                id: 0,
                provider_id: id.into(),
                exposed_name: model_name(upstream, short_code),
                upstream_model: upstream.into(),
                standard_model_id: Some(standard.id.clone()),
                capabilities: Default::default(),
                non_standard: false,
            }],
        }
    }

    fn save(state: &AppState, view: &ProviderView) {
        let mut conn = state.db.conn().unwrap();
        providers::save(&mut conn, view).unwrap();
    }

    #[tokio::test]
    async fn export_covers_every_provider_and_the_whole_catalog() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        save(state, &view("work", "work", &standard, "one"));

        let bundle = export_bundle(state).await.unwrap();
        assert_eq!(bundle.providers.len(), 1);
        assert_eq!(bundle.providers[0].models.len(), 1);
        assert_eq!(bundle.app_version, env!("CARGO_PKG_VERSION"));
        assert!(bundle
            .standard_models
            .iter()
            .any(|model| model.id == standard.id));
        assert!(bundle.validate().is_ok());
    }

    #[tokio::test]
    async fn import_is_idempotent_and_never_removes_local_providers() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        save(state, &view("work", "work", &standard, "one"));

        let bundle = export_bundle(state).await.unwrap();
        let repeated = import_bundle(state, bundle.clone()).await.unwrap();
        assert_eq!((repeated.providers, repeated.models), (1, 1));
        assert_eq!(
            repeated.standard_models, 0,
            "existing models are not re-added"
        );

        let mut foreign = bundle.clone();
        foreign.providers = vec![view("extra", "extra", &standard, "two")];
        let added = import_bundle(state, foreign).await.unwrap();
        assert_eq!((added.providers, added.models), (1, 1));
        assert_eq!(added.standard_models, 0);

        let conn = state.db.conn().unwrap();
        let ids: Vec<String> = providers::list(&conn)
            .unwrap()
            .into_iter()
            .map(|provider| provider.id)
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"work".to_owned()) && ids.contains(&"extra".to_owned()));
    }

    #[tokio::test]
    async fn import_rejects_an_occupied_short_code_without_writing() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        save(state, &view("work", "work", &standard, "one"));

        let mut bundle = export_bundle(state).await.unwrap();
        bundle.providers = vec![view("other", "work", &standard, "two")];
        let error = import_bundle(state, bundle).await.unwrap_err();
        assert!(
            error.to_string().contains("已被本地其他连接占用"),
            "{error}"
        );

        let conn = state.db.conn().unwrap();
        let list = providers::list(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "work");
    }

    #[tokio::test]
    async fn import_preserves_an_unrelated_local_default() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        let mut local = view("local", "local", &standard, "one");
        local.provider.is_default = true;
        save(state, &local);

        let mut bundle = export_bundle(state).await.unwrap();
        let mut imported = view("foreign", "foreign", &standard, "two");
        imported.provider.is_default = true;
        bundle.providers = vec![imported];
        import_bundle(state, bundle).await.unwrap();

        let conn = state.db.conn().unwrap();
        assert!(providers::get(&conn, "local").unwrap().unwrap().is_default);
        assert!(
            !providers::get(&conn, "foreign")
                .unwrap()
                .unwrap()
                .is_default
        );
    }

    #[tokio::test]
    async fn failed_provider_import_does_not_add_models_to_file_or_cache() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        save(state, &view("work", "work", &standard, "one"));
        let mut bundle = export_bundle(state).await.unwrap();
        let mut added = standard.clone();
        added.id = "new-import-model".into();
        added.name = "New import model".into();
        bundle.standard_models.push(added);
        bundle.providers[0].provider.short_code = "renamed".into();
        // The profile is not part of the backup. A corrupt local profile makes the
        // provider write fail after validation, exercising the combined rollback.
        {
            let conn = state.db.conn().unwrap();
            conn.execute(
                "INSERT INTO terminal_profiles(client,profile) VALUES('codex','invalid')",
                [],
            )
            .unwrap();
        }
        let path = fixture._dir.path().join("models/catalog.json");
        let before = std::fs::read(&path).unwrap();
        assert!(import_bundle(state, bundle).await.is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let conn = state.db.conn().unwrap();
        assert!(catalog::get(&conn, "new-import-model").unwrap().is_none());
        assert_eq!(
            providers::get(&conn, "work").unwrap().unwrap().short_code,
            "work"
        );
    }

    #[tokio::test]
    async fn import_remaps_renamed_standard_models_onto_local_ones() {
        let fixture = fixture();
        let state = &fixture.state;
        let standard = catalog_model(state).await;
        let mut bundle = export_bundle(state).await.unwrap();
        let mut renamed = standard.clone();
        renamed.id = "other-install-id".into();
        bundle.standard_models = bundle
            .standard_models
            .iter()
            .map(|model| {
                if model.id == standard.id {
                    renamed.clone()
                } else {
                    model.clone()
                }
            })
            .collect();
        bundle.providers = vec![view("work", "work", &renamed, "one")];

        let summary = import_bundle(state, bundle).await.unwrap();
        assert_eq!((summary.providers, summary.standard_models), (1, 0));

        let conn = state.db.conn().unwrap();
        let saved = providers::list_views(&conn).unwrap();
        assert_eq!(
            saved[0].models[0].standard_model_id.as_deref(),
            Some(standard.id.as_str())
        );
        assert!(catalog::get(&conn, "other-install-id").unwrap().is_none());
    }
}
