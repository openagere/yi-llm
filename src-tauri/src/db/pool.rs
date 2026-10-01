use crate::{db::migrations, error::Result};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use std::{path::Path, time::Duration};

pub type Conn = PooledConnection<SqliteConnectionManager>;

const MAX_CONNECTIONS: u32 = 8;

/// Shared SQLite handle. Cheap to clone; every clone draws from the same pool.
#[derive(Clone)]
pub struct Db {
    pool: Pool<SqliteConnectionManager>,
}

impl Db {
    /// Opens (creating if needed) the database, enables WAL and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path).with_init(|conn| {
            conn.pragma_update(None, "foreign_keys", true)?;
            conn.pragma_update(None, "synchronous", "NORMAL")?;
            conn.busy_timeout(Duration::from_secs(5))
        });
        let pool = Pool::builder()
            .max_size(MAX_CONNECTIONS)
            .connection_timeout(Duration::from_secs(10))
            .build(manager)?;
        {
            let mut conn = pool.get()?;
            conn.pragma_update(None, "journal_mode", "WAL")?;
            migrations::run(&mut conn)?;
        }
        Ok(Self { pool })
    }

    pub fn conn(&self) -> Result<Conn> {
        Ok(self.pool.get()?)
    }

    /// Runs blocking database work on the blocking thread pool.
    pub async fn run<T, F>(&self, work: F) -> Result<T>
    where
        F: FnOnce(&mut Conn) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = db.conn()?;
            work(&mut conn)
        })
        .await?
    }
}
