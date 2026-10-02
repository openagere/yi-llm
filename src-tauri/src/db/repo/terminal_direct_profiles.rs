use crate::{
    domain::terminal::DirectProfile,
    error::{AppError, Result},
};
use rusqlite::{Connection, OptionalExtension};

fn parse(raw: &str) -> Result<DirectProfile> {
    serde_json::from_str(raw)
        .map_err(|error| AppError::internal(format!("终端直连配置损坏: {error}")))
}

pub fn get(conn: &Connection, client: &str) -> Result<Option<DirectProfile>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT profile FROM terminal_direct_profiles WHERE client=?1",
            [client],
            |row| row.get(0),
        )
        .optional()?;
    raw.as_deref().map(parse).transpose()
}

pub fn upsert(conn: &Connection, profile: &DirectProfile) -> Result<()> {
    let raw = serde_json::to_string(profile)?;
    conn.execute(
        "INSERT INTO terminal_direct_profiles(client,profile) VALUES(?1,?2) ON CONFLICT(client) DO UPDATE SET profile=excluded.profile",
        (&profile.client, raw),
    )?;
    Ok(())
}
