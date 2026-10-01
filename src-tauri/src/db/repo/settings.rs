use crate::{domain::settings::ServerSettings, error::Result};
use rusqlite::{params, Connection};

pub fn get(conn: &Connection) -> Result<ServerSettings> {
    Ok(conn.query_row(
        "SELECT host, port, log_level FROM settings WHERE id = 1",
        [],
        |row| {
            Ok(ServerSettings {
                host: row.get(0)?,
                port: row.get::<_, u16>(1)?,
                log_level: row.get(2)?,
            })
        },
    )?)
}

pub fn save(conn: &Connection, host: &str, port: u16, log_level: &str) -> Result<()> {
    conn.execute(
        "UPDATE settings SET host = ?1, port = ?2, log_level = ?3 WHERE id = 1",
        params![host, i64::from(port), log_level],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn schema_and_settings_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let conn = Db::open(&dir.path().join("test.db"))
            .unwrap()
            .conn()
            .unwrap();
        let s = get(&conn).unwrap();
        assert_eq!((s.host.as_str(), s.port), ("127.0.0.1", 11435));
        save(&conn, "0.0.0.0", 9999, "debug").unwrap();
        let s = get(&conn).unwrap();
        assert_eq!(
            (s.host.as_str(), s.port, s.log_level.as_str()),
            ("0.0.0.0", 9999, "debug")
        );
    }
}
