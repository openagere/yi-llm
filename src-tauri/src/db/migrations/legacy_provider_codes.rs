use super::has_column;
use crate::{
    db::repo::providers::{rename_references, RouteRename},
    domain::provider_names::{model_name, short_code_base},
    error::Result,
};
use rusqlite::{params, Connection};
use std::collections::HashSet;

/// Assigns unique short codes to providers created before codes existed and renames
/// their routes, terminal profiles and usage rows to the `name(code)` form.
pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    let has_short_code = has_column(conn, "providers", "short_code")?;
    let tx = conn.transaction()?;
    if !has_short_code {
        tx.execute(
            "ALTER TABLE providers ADD COLUMN short_code TEXT NOT NULL DEFAULT ''",
            [],
        )?;
    }
    let providers = {
        let mut stmt =
            tx.prepare("SELECT id,name,short_code FROM providers ORDER BY sort_order,id")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut used: HashSet<String> = providers
        .iter()
        .filter(|(_, _, code)| !code.is_empty())
        .map(|(_, _, code)| code.clone())
        .collect();
    let mut renames = Vec::new();
    for (id, name, code) in providers {
        if !code.is_empty() {
            continue;
        }
        let base = short_code_base(&name);
        let mut code = base.clone();
        let mut suffix = 2;
        while !used.insert(code.clone()) {
            code = format!("{base}-{suffix}");
            suffix += 1;
        }
        tx.execute(
            "UPDATE providers SET short_code=?1 WHERE id=?2",
            params![code, id],
        )?;
        // Read the v1 columns directly; the current model repository may depend on
        // columns introduced only by later migrations.
        let mappings = {
            let mut stmt = tx.prepare(
                "SELECT id, exposed_name, upstream_model FROM models WHERE provider_id=?1 ORDER BY id",
            )?;
            let rows = stmt.query_map(params![id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        for (model_id, exposed_name, upstream_model) in mappings {
            renames.push((
                model_id,
                RouteRename {
                    provider_id: id.clone(),
                    old: exposed_name,
                    new: model_name(&upstream_model, &code),
                },
            ));
        }
    }
    if !renames.is_empty() {
        tx.execute("DROP INDEX IF EXISTS idx_models_exposed", [])?;
        for (id, route) in &renames {
            tx.execute(
                "UPDATE models SET exposed_name=?1 WHERE id=?2",
                params![route.new, id],
            )?;
        }
        let routes: Vec<_> = renames.into_iter().map(|(_, route)| route).collect();
        rename_references(&tx, &routes)?;
        tx.execute(
            "CREATE UNIQUE INDEX idx_models_exposed ON models(exposed_name)",
            [],
        )?;
    }
    tx.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_providers_short_code ON providers(short_code COLLATE NOCASE)", [])?;
    tx.commit()?;
    Ok(())
}
