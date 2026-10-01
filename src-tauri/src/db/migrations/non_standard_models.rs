use super::has_column;
use crate::error::Result;
use rusqlite::Connection;

/// Existing mappings stay standard by default. Idempotent for databases created by
/// development builds that already carried this column.
pub(super) fn apply(conn: &mut Connection) -> Result<()> {
    if !has_column(conn, "models", "non_standard")? {
        conn.execute(
            "ALTER TABLE models ADD COLUMN non_standard INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}
