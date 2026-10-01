use super::has_column;
use crate::{
    domain::{capabilities::Capabilities, terminal::Profile},
    error::{AppError, Result},
};
use rusqlite::{params, Connection, OptionalExtension};

/// Upgrades pre-catalog databases: models gain a `standard_model_id`, routes become
/// `provider/name`, and stored terminal profiles are rewritten to the new route names.
pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS standard_models(id TEXT PRIMARY KEY,name TEXT NOT NULL,protocol TEXT NOT NULL,capabilities TEXT NOT NULL DEFAULT '{}',brand TEXT NOT NULL DEFAULT '',UNIQUE(name,protocol));")?;
    if !has_column(conn, "standard_models", "brand")? {
        conn.execute(
            "ALTER TABLE standard_models ADD COLUMN brand TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    let new_catalog = !has_column(conn, "models", "standard_model_id")?;
    let tx = conn.transaction()?;
    if new_catalog {
        tx.execute("ALTER TABLE models ADD COLUMN standard_model_id TEXT REFERENCES standard_models(id) ON DELETE RESTRICT", [])?;
    }
    let legacy = {
        let mut stmt = tx.prepare("SELECT m.id,m.upstream_model,p.type,m.capabilities FROM models m JOIN providers p ON p.id=m.provider_id WHERE m.standard_model_id IS NULL ORDER BY m.id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (id, name, protocol, raw) in legacy {
        let caps: Capabilities = serde_json::from_str(&raw)?;
        let existing = tx
            .query_row(
                "SELECT id,capabilities FROM standard_models WHERE name=?1 AND protocol=?2",
                params![name, protocol],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let standard_id = if let Some((existing_id, existing_raw)) = &existing {
            let existing_caps: Capabilities = serde_json::from_str(existing_raw)?;
            (existing_caps == caps).then(|| existing_id.clone())
        } else {
            None
        };
        let standard_id = if let Some(id) = standard_id {
            id
        } else {
            let standard_id = format!("imported-{id}");
            let name = if existing.is_some() {
                format!("{name} (imported-{id})")
            } else {
                name
            };
            tx.execute(
                "INSERT INTO standard_models(id,name,protocol,capabilities) VALUES(?1,?2,?3,?4)",
                params![standard_id, name, protocol, raw],
            )?;
            standard_id
        };
        tx.execute(
            "UPDATE models SET standard_model_id=?1 WHERE id=?2",
            params![standard_id, id],
        )?;
    }
    if new_catalog {
        rewrite_routes(&tx)?;
    }
    tx.commit()?;
    Ok(())
}

fn rewrite_routes(tx: &rusqlite::Transaction<'_>) -> Result<()> {
    let routes = {
        let mut stmt =
            tx.prepare("SELECT id,exposed_name,provider_id,upstream_model FROM models")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    tx.execute("DROP INDEX IF EXISTS idx_models_exposed", [])?;
    let profiles = {
        let mut stmt = tx.prepare("SELECT client,profile FROM terminal_profiles")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    for (id, _, provider, name) in &routes {
        tx.execute(
            "UPDATE models SET exposed_name=?1 WHERE id=?2",
            params![format!("{provider}/{name}"), id],
        )?;
    }
    tx.execute(
        "CREATE UNIQUE INDEX idx_models_exposed ON models(exposed_name)",
        [],
    )?;
    for (client, raw) in profiles {
        let mut profile: Profile = serde_json::from_str(&raw)
            .map_err(|error| AppError::internal(format!("终端模型配置损坏: {error}")))?;
        for selection in &mut profile.models {
            if let Some((_, old, provider, name)) = routes.iter().find(|(_, old, provider, _)| {
                old == &selection.model && provider == &selection.provider_id
            }) {
                let route = format!("{provider}/{name}");
                if profile.default_model == *old {
                    profile.default_model = route.clone();
                }
                selection.model = route;
            }
        }
        tx.execute(
            "UPDATE terminal_profiles SET profile=?1 WHERE client=?2",
            params![serde_json::to_string(&profile)?, client],
        )?;
    }
    Ok(())
}
