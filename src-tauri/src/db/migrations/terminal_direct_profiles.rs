use crate::error::Result;
use rusqlite::Connection;

/// Direct terminal configurations are stored independently from proxy model profiles so
/// applying either mode never erases the other mode's settings.
pub(super) fn apply(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS terminal_direct_profiles (
            client TEXT PRIMARY KEY,
            profile TEXT NOT NULL
        );",
    )?;
    Ok(())
}
