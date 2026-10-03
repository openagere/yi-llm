use crate::{
    db::repo::{catalog, models},
    domain::{
        provider::{ModelMapping, ProtocolSupport, Provider, ProviderView},
        provider_names::validate_short_code,
        terminal::Profile,
    },
    error::{AppError, Result},
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::collections::HashMap;

pub const COLUMNS: &str = "id, type, name, short_code, base_url, api_key, enabled, thinking, extra, is_default, protocol_support";

pub fn row_to_provider(row: &rusqlite::Row<'_>) -> rusqlite::Result<Provider> {
    let provider_type: String = row.get("type")?;
    let raw_support: String = row.get("protocol_support")?;
    let protocol_support = serde_json::from_str::<ProtocolSupport>(&raw_support)
        .unwrap_or_default()
        .normalized(&provider_type);
    Ok(Provider {
        id: row.get("id")?,
        provider_type,
        name: row.get("name")?,
        short_code: row.get("short_code")?,
        base_url: row.get("base_url")?,
        api_key: row.get("api_key")?,
        enabled: row.get::<_, i64>("enabled")? != 0,
        thinking: row.get("thinking")?,
        extra: serde_json::from_str(&row.get::<_, String>("extra")?)
            .unwrap_or(serde_json::json!({})),
        is_default: row.get::<_, i64>("is_default")? != 0,
        protocol_support,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<Provider>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM providers ORDER BY sort_order, id"
    ))?;
    let rows = stmt.query_map([], row_to_provider)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Provider>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLUMNS} FROM providers WHERE id = ?1"),
            params![id],
            row_to_provider,
        )
        .optional()?)
}

/// Providers with their models using two queries in total, regardless of provider count.
pub fn list_views(conn: &Connection) -> Result<Vec<ProviderView>> {
    let mut grouped: HashMap<String, Vec<ModelMapping>> = HashMap::new();
    for model in models::list_all(conn)? {
        grouped
            .entry(model.provider_id.clone())
            .or_default()
            .push(model);
    }
    Ok(list(conn)?
        .into_iter()
        .map(|provider| {
            let models = grouped.remove(&provider.id).unwrap_or_default();
            ProviderView { provider, models }
        })
        .collect())
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM providers WHERE id = ?1", params![id])?;
    Ok(())
}

/// Insert or replace a provider and its model mappings (mappings are replaced wholesale).
pub fn save(conn: &mut Connection, view: &ProviderView) -> Result<()> {
    save_many(conn, std::slice::from_ref(view))
}

/// Insert or replace several providers atomically; used by encrypted configuration import.
pub fn save_many(conn: &mut Connection, views: &[ProviderView]) -> Result<()> {
    let tx = conn.transaction()?;
    save_many_in_tx(&tx, views)?;
    tx.commit()?;
    Ok(())
}

/// Writes imported providers inside the catalog transaction when new standard models are added.
pub fn save_many_in_tx(tx: &Transaction<'_>, views: &[ProviderView]) -> Result<()> {
    for view in views {
        save_in_tx(tx, view)?;
    }
    Ok(())
}

/// Validation and writes for a single provider, shared by the public save functions.
fn save_in_tx(tx: &Transaction<'_>, view: &ProviderView) -> Result<()> {
    validate_short_code(&view.provider.short_code)?;
    let duplicate: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM providers WHERE short_code=?1 COLLATE NOCASE AND id!=?2)",
        params![view.provider.short_code, view.provider.id],
        |row| row.get(0),
    )?;
    if duplicate {
        return Err(AppError::conflict("Provider 简写已被其他连接使用"));
    }
    let renames: Vec<_> = models::list_for_provider(tx, &view.provider.id)?
        .into_iter()
        .filter_map(|old| {
            view.models
                .iter()
                .find(|new| {
                    new.upstream_model == old.upstream_model && new.exposed_name != old.exposed_name
                })
                .map(|new| RouteRename {
                    provider_id: view.provider.id.clone(),
                    old: old.exposed_name,
                    new: new.exposed_name.clone(),
                })
        })
        .collect();
    for model in &view.models {
        if model.upstream_model.trim().is_empty() || model.exposed_name.trim().is_empty() {
            return Err(AppError::validation("模型名称不能为空"));
        }
        if let Some(id) = &model.standard_model_id {
            let standard = catalog::get(tx, id)?
                .ok_or_else(|| AppError::validation("关联的标准模型不存在"))?;
            if standard.protocol != view.provider.provider_type {
                return Err(AppError::validation("标准模型与 Provider 上游协议不匹配"));
            }
        } else {
            model
                .capabilities
                .validate(&view.provider.provider_type)
                .map_err(|error| {
                    AppError::validation(format!("模型 {}: {error}", model.exposed_name))
                })?;
        }
    }
    let protocol_support = view
        .provider
        .protocol_support
        .clone()
        .normalized(&view.provider.provider_type);
    tx.execute(
        "INSERT INTO providers (id, type, name, base_url, api_key, enabled, thinking, extra, is_default, protocol_support, short_code, sort_order)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,
                 COALESCE((SELECT sort_order FROM providers WHERE id = ?1), 0))
         ON CONFLICT(id) DO UPDATE SET
           type=excluded.type, name=excluded.name, base_url=excluded.base_url,
           api_key=excluded.api_key, enabled=excluded.enabled, thinking=excluded.thinking,
           extra=excluded.extra, is_default=excluded.is_default,
           protocol_support=excluded.protocol_support, short_code=excluded.short_code",
        params![
            view.provider.id,
            view.provider.provider_type,
            view.provider.name,
            view.provider.base_url,
            view.provider.api_key,
            i64::from(view.provider.enabled),
            view.provider.thinking,
            view.provider.extra.to_string(),
            i64::from(view.provider.is_default),
            serde_json::to_string(&protocol_support)?,
            view.provider.short_code,
        ],
    )?;
    tx.execute(
        "DELETE FROM models WHERE provider_id = ?1",
        params![view.provider.id],
    )?;
    for m in &view.models {
        tx.execute(
            "INSERT INTO models (provider_id, exposed_name, upstream_model, capabilities, standard_model_id, non_standard) VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                view.provider.id,
                m.exposed_name,
                m.upstream_model,
                serde_json::to_string(&m.capabilities)?,
                m.standard_model_id,
                i64::from(m.non_standard)
            ],
        )?;
    }
    if view.provider.is_default {
        tx.execute(
            "UPDATE providers SET is_default = 0 WHERE id != ?1",
            params![view.provider.id],
        )?;
    }
    rename_references(tx, &renames)?;
    Ok(())
}

pub struct RouteRename {
    pub provider_id: String,
    pub old: String,
    pub new: String,
}

/// Rewrites terminal profiles and usage history after a route name changed.
pub fn rename_references(conn: &Connection, routes: &[RouteRename]) -> Result<()> {
    if routes.is_empty() {
        return Ok(());
    }
    let profiles = {
        let mut stmt = conn.prepare("SELECT client,profile FROM terminal_profiles")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (client, raw) in profiles {
        let mut profile: Profile = serde_json::from_str(&raw)
            .map_err(|error| AppError::internal(format!("终端模型配置损坏: {error}")))?;
        let mut changed = false;
        for selection in &mut profile.models {
            if let Some(route) = routes.iter().find(|route| {
                route.provider_id == selection.provider_id && route.old == selection.model
            }) {
                if profile.default_model == selection.model {
                    profile.default_model = route.new.clone();
                }
                selection.model = route.new.clone();
                changed = true;
            }
        }
        if changed {
            conn.execute(
                "UPDATE terminal_profiles SET profile=?1 WHERE client=?2",
                params![serde_json::to_string(&profile)?, client],
            )?;
        }
    }
    for route in routes {
        conn.execute(
            "UPDATE usage SET model=?1 WHERE model=?2",
            params![route.new, route.old],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Db, domain::capabilities::Capabilities, domain::provider_names::model_name};

    fn sample(id: &str, code: &str, is_default: bool) -> ProviderView {
        ProviderView {
            provider: Provider {
                id: id.into(),
                provider_type: "anthropic".into(),
                name: "Work account".into(),
                short_code: code.into(),
                base_url: "https://api.anthropic.com".into(),
                api_key: "sk-ant".into(),
                enabled: true,
                thinking: "high".into(),
                extra: serde_json::json!({}),
                is_default,
                protocol_support: ProtocolSupport::native("anthropic"),
            },
            models: vec![ModelMapping {
                id: 0,
                provider_id: id.into(),
                exposed_name: model_name("claude-sonnet-4-5", code),
                upstream_model: "claude-sonnet-4-5".into(),
                standard_model_id: None,
                capabilities: Capabilities::default(),
                non_standard: false,
            }],
        }
    }

    fn open() -> (tempfile::TempDir, crate::db::Conn) {
        let dir = tempfile::tempdir().unwrap();
        let conn = Db::open(&dir.path().join("test.db"))
            .unwrap()
            .conn()
            .unwrap();
        (dir, conn)
    }

    #[test]
    fn save_list_delete_provider() {
        let (_dir, mut conn) = open();
        save(&mut conn, &sample("p1", "p1", true)).unwrap();
        let list = list_views(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].provider.is_default);
        assert_eq!(list[0].models[0].upstream_model, "claude-sonnet-4-5");
        delete(&conn, "p1").unwrap();
        assert!(list_views(&conn).unwrap().is_empty());
    }

    #[test]
    fn non_standard_flag_survives_provider_save_and_reload() {
        let (_dir, mut conn) = open();
        let mut view = sample("p1", "p1", false);
        view.models[0].non_standard = true;
        save(&mut conn, &view).unwrap();
        assert!(list_views(&conn).unwrap()[0].models[0].non_standard);
        view.models[0].non_standard = false;
        save(&mut conn, &view).unwrap();
        assert!(!list_views(&conn).unwrap()[0].models[0].non_standard);
    }

    #[test]
    fn only_one_provider_is_default() {
        let (_dir, mut conn) = open();
        save(&mut conn, &sample("p1", "p1", true)).unwrap();
        save(&mut conn, &sample("p2", "p2", true)).unwrap();
        let providers = list(&conn).unwrap();
        assert_eq!(providers.iter().filter(|p| p.is_default).count(), 1);
        assert!(providers.iter().find(|p| p.id == "p2").unwrap().is_default);
    }

    #[test]
    fn list_views_groups_models_by_provider() {
        let (_dir, mut conn) = open();
        for id in ["a", "b", "c"] {
            save(&mut conn, &sample(id, id, false)).unwrap();
        }
        let views = list_views(&conn).unwrap();
        assert_eq!(views.len(), 3);
        for view in views {
            assert_eq!(view.models.len(), 1);
            assert_eq!(view.models[0].provider_id, view.provider.id);
        }
    }

    #[test]
    fn provider_codes_are_required_and_unique() {
        let (_dir, mut conn) = open();
        save(&mut conn, &sample("one", "kk", false)).unwrap();
        for code in [
            "",
            "KK",
            "has space",
            "bad/code",
            "has(parentheses)",
            "abcdefghijklmnopqrstuvwxyz",
        ] {
            assert!(
                save(&mut conn, &sample("two", code, false)).is_err(),
                "{code}"
            );
        }
        assert!(save(&mut conn, &sample("two", "kk", false)).is_err());
        assert_eq!(list(&conn).unwrap().len(), 1);
    }

    #[test]
    fn code_changes_rewrite_usage_atomically() {
        let (_dir, mut conn) = open();
        let mut view = sample("one", "kk", false);
        save(&mut conn, &view).unwrap();
        conn.execute(
            "INSERT INTO usage(provider_name,protocol,model,input_tokens,output_tokens,cached_tokens,reasoning_tokens,requested_at) VALUES('Work account','anthropic','claude-sonnet-4-5(kk)',2,3,0,0,strftime('%s','now'))",
            [],
        )
        .unwrap();
        view.provider.short_code = "work".into();
        view.models[0].exposed_name = model_name("claude-sonnet-4-5", "work");
        save(&mut conn, &view).unwrap();
        let models: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM usage WHERE model='claude-sonnet-4-5(work)'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(models, 1);
        save(&mut conn, &sample("two", "other", false)).unwrap();
        view.provider.short_code = "other".into();
        assert!(save(&mut conn, &view).is_err());
        assert_eq!(get(&conn, "one").unwrap().unwrap().short_code, "work");
    }
}
