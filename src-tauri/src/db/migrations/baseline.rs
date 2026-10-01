use super::{has_column, legacy_catalog, legacy_provider_codes};
use crate::error::Result;
use rusqlite::Connection;

pub(super) const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS providers (
  id          TEXT PRIMARY KEY,
  type        TEXT NOT NULL,
  name        TEXT NOT NULL,
  short_code  TEXT NOT NULL DEFAULT '',
  base_url    TEXT NOT NULL,
  api_key     TEXT NOT NULL DEFAULT '',
  enabled     INTEGER NOT NULL DEFAULT 1,
  thinking    TEXT NOT NULL DEFAULT 'medium',
  extra       TEXT NOT NULL DEFAULT '{}',
  is_default  INTEGER NOT NULL DEFAULT 0,
  protocol_support TEXT NOT NULL DEFAULT '{}',
  sort_order  INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS standard_models (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  protocol TEXT NOT NULL,
  capabilities TEXT NOT NULL DEFAULT '{}',
  brand TEXT NOT NULL DEFAULT '',
  UNIQUE(name,protocol)
);
CREATE TABLE IF NOT EXISTS models (
  id             INTEGER PRIMARY KEY AUTOINCREMENT,
  provider_id    TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
  exposed_name   TEXT NOT NULL,
  upstream_model TEXT NOT NULL,
  capabilities   TEXT NOT NULL DEFAULT '{}',
  standard_model_id TEXT REFERENCES standard_models(id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_models_exposed ON models(exposed_name);
CREATE TABLE IF NOT EXISTS terminal_profiles (
  client TEXT PRIMARY KEY,
  profile TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS settings (
  id   INTEGER PRIMARY KEY CHECK (id = 1),
  host TEXT NOT NULL DEFAULT '127.0.0.1',
  port INTEGER NOT NULL DEFAULT 11435,
  log_level TEXT NOT NULL DEFAULT 'info'
);
INSERT OR IGNORE INTO settings (id, host, port) VALUES (1, '127.0.0.1', 11435);
CREATE TABLE IF NOT EXISTS usage (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  requested_at INTEGER NOT NULL,
  provider_name TEXT NOT NULL,
  protocol TEXT NOT NULL,
  model TEXT NOT NULL,
  input_tokens INTEGER NOT NULL DEFAULT 0,
  output_tokens INTEGER NOT NULL DEFAULT 0,
  cached_tokens INTEGER NOT NULL DEFAULT 0,
  reasoning_tokens INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_usage_requested_at ON usage(requested_at);
"#;

const COLUMN_UPGRADES: &[(&str, &str, &str)] = &[
    (
        "models",
        "capabilities",
        "ALTER TABLE models ADD COLUMN capabilities TEXT NOT NULL DEFAULT '{}'",
    ),
    (
        "providers",
        "protocol_support",
        "ALTER TABLE providers ADD COLUMN protocol_support TEXT NOT NULL DEFAULT '{}'",
    ),
    (
        "settings",
        "log_level",
        "ALTER TABLE settings ADD COLUMN log_level TEXT NOT NULL DEFAULT 'info'",
    ),
];

pub(super) fn apply(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    for (table, column, statement) in COLUMN_UPGRADES {
        if !has_column(conn, table, column)? {
            conn.execute(statement, [])?;
        }
    }
    legacy_catalog::migrate(conn)?;
    legacy_provider_codes::migrate(conn)?;
    Ok(())
}
