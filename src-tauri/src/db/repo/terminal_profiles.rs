use crate::{
    domain::terminal::Profile,
    error::{AppError, Result},
};
use rusqlite::{Connection, OptionalExtension};

fn parse(raw: &str) -> Result<Profile> {
    serde_json::from_str(raw)
        .map_err(|error| AppError::internal(format!("终端模型配置损坏: {error}")))
}

pub fn get(conn: &Connection, client: &str) -> Result<Option<Profile>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT profile FROM terminal_profiles WHERE client=?1",
            [client],
            |row| row.get(0),
        )
        .optional()?;
    raw.as_deref().map(parse).transpose()
}

/// Lists stored profiles, skipping (and logging) any that fail to parse so one corrupt
/// record cannot take down routing for the other clients.
pub fn list_lenient(conn: &Connection) -> Result<Vec<Profile>> {
    let mut stmt = conn.prepare("SELECT profile FROM terminal_profiles ORDER BY client")?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .filter_map(|raw| match parse(raw) {
            Ok(profile) => Some(profile),
            Err(error) => {
                tracing::warn!(%error, "skipping unreadable terminal profile");
                None
            }
        })
        .collect())
}

pub fn upsert(conn: &Connection, profile: &Profile) -> Result<()> {
    let raw = serde_json::to_string(profile)?;
    conn.execute(
        "INSERT INTO terminal_profiles(client,profile) VALUES(?1,?2) ON CONFLICT(client) DO UPDATE SET profile=excluded.profile",
        (&profile.client, raw),
    )?;
    Ok(())
}
