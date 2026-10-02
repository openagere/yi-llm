mod baseline;
mod legacy_catalog;
mod legacy_provider_codes;
mod non_standard_models;
mod terminal_direct_profiles;
#[cfg(test)]
mod tests;

use crate::error::Result;
use rusqlite::Connection;

struct Migration {
    version: i64,
    name: &'static str,
    apply: fn(&mut Connection) -> Result<()>,
}

/// Ordered by version. Never edit a released migration; append a new one.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "baseline",
        apply: baseline::apply,
    },
    Migration {
        version: 2,
        name: "non_standard_models",
        apply: non_standard_models::apply,
    },
    Migration {
        version: 3,
        name: "terminal_direct_profiles",
        apply: terminal_direct_profiles::apply,
    },
];

pub fn current_version(conn: &Connection) -> Result<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map_or(0, |migration| migration.version)
}

/// Applies every migration newer than the database's `user_version`.
///
/// Databases created before versioning existed report `user_version = 0` but already
/// contain tables; the baseline migration detects that state (all statements are
/// `IF NOT EXISTS` / column-guarded) and upgrades them in place without data loss.
pub fn run(conn: &mut Connection) -> Result<()> {
    let current = current_version(conn)?;
    for migration in MIGRATIONS.iter().filter(|m| m.version > current) {
        tracing::info!(
            version = migration.version,
            name = migration.name,
            "applying database migration"
        );
        (migration.apply)(conn)?;
        conn.pragma_update(None, "user_version", migration.version)?;
    }
    Ok(())
}

pub(crate) fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>("name"))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(names.iter().any(|name| name == column))
}
