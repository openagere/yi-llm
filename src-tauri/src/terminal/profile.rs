use crate::{
    db::repo::terminal_profiles,
    domain::terminal::{protocol, Profile},
    error::Result,
    terminal::resolve::validate,
};
use rusqlite::Connection;

pub fn load(conn: &Connection, client: &str) -> Result<Option<Profile>> {
    protocol(client)?;
    terminal_profiles::get(conn, client)
}

pub fn save(conn: &Connection, profile: &Profile) -> Result<()> {
    validate(conn, profile)?;
    terminal_profiles::upsert(conn, profile)
}
