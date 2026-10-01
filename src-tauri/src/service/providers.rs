use crate::{
    app::AppState,
    db::repo::{models, providers},
    domain::{
        provider::{ModelMapping, Provider, ProviderView},
        provider_names::model_name,
        terminal::Profile,
    },
    error::{AppError, Result},
    protocol::upstream::upstream_client,
    terminal,
};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Serialize)]
pub struct SaveProviderResult {
    pub terminal_config_warnings: Vec<String>,
}

pub async fn list(state: &AppState) -> Result<Vec<ProviderView>> {
    state.refresh_catalog().await?;
    state.db.run(|conn| providers::list_views(conn)).await
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
                status.active
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

    #[test]
    fn model_related_rejections_get_a_hint() {
        let status = reqwest::StatusCode::BAD_REQUEST;
        assert!(describe_failure(status, "m", "The model `m` does not exist").contains("不可用"));
        assert_eq!(
            describe_failure(status, "m", "bad key"),
            "HTTP 400 Bad Request: bad key"
        );
    }
}
