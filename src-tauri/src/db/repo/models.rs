use crate::{domain::provider::ModelMapping, error::Result};
use rusqlite::{params, Connection, OptionalExtension};

const SELECT: &str =
    "SELECT m.id, m.provider_id, m.exposed_name, m.upstream_model, m.standard_model_id, m.non_standard, \
     COALESCE(s.capabilities, m.capabilities) AS capabilities \
     FROM models m LEFT JOIN standard_models s ON s.id = m.standard_model_id";

fn row_to_model(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelMapping> {
    let raw: String = row.get("capabilities")?;
    let capabilities = serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ModelMapping {
        id: row.get("id")?,
        provider_id: row.get("provider_id")?,
        exposed_name: row.get("exposed_name")?,
        upstream_model: row.get("upstream_model")?,
        standard_model_id: row.get("standard_model_id")?,
        capabilities,
        non_standard: row.get::<_, i64>("non_standard")? != 0,
    })
}

pub fn list_for_provider(conn: &Connection, provider_id: &str) -> Result<Vec<ModelMapping>> {
    let mut stmt = conn.prepare(&format!("{SELECT} WHERE m.provider_id = ?1 ORDER BY m.id"))?;
    let rows = stmt.query_map(params![provider_id], row_to_model)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Every model in one query, ordered so mappings of one provider are contiguous.
pub fn list_all(conn: &Connection) -> Result<Vec<ModelMapping>> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY m.provider_id, m.id"))?;
    let rows = stmt.query_map([], row_to_model)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Finds a provider's model by exposed name, falling back to its upstream name.
pub fn find_for_provider(
    conn: &Connection,
    provider_id: &str,
    name: &str,
) -> Result<Option<ModelMapping>> {
    Ok(conn
        .query_row(
            &format!(
                "{SELECT} WHERE m.provider_id=?1 AND (m.exposed_name=?2 OR m.upstream_model=?2) \
                 ORDER BY (m.exposed_name=?2) DESC LIMIT 1"
            ),
            params![provider_id, name],
            row_to_model,
        )
        .optional()?)
}
