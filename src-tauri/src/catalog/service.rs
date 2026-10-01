use crate::{
    catalog::file::{CatalogInfo, FileCatalog},
    db::Db,
    domain::standard_model::StandardModel,
    error::{AppError, Result},
};
use rusqlite::Connection;
use std::{path::PathBuf, sync::Mutex};

/// Thread-safe wrapper around the shared catalog file.
///
/// Lock order: callers hold a database connection first, then this service's file lock.
pub struct CatalogService {
    file: Mutex<FileCatalog>,
}

impl CatalogService {
    pub fn open(path: PathBuf, conn: &Connection) -> Result<Self> {
        Ok(Self {
            file: Mutex::new(FileCatalog::open(path, conn)?),
        })
    }

    fn file(&self) -> Result<std::sync::MutexGuard<'_, FileCatalog>> {
        self.file
            .lock()
            .map_err(|_| AppError::internal("模型目录状态已损坏"))
    }

    pub fn info(&self) -> Result<CatalogInfo> {
        Ok(self.file()?.info())
    }

    pub fn is_stale(&self) -> bool {
        self.file().map_or(true, |file| file.is_stale())
    }

    /// Re-applies the catalog file when it changed. Returns whether cached data changed.
    pub fn refresh(&self, db: &Db) -> Result<bool> {
        if !self.is_stale() {
            return Ok(false);
        }
        let mut conn = db.conn()?;
        self.sync(&mut conn)
    }

    pub fn sync(&self, conn: &mut Connection) -> Result<bool> {
        self.file()?.sync(conn)
    }

    pub fn save(&self, conn: &mut Connection, model: &StandardModel) -> Result<()> {
        self.file()?.save(conn, model)
    }

    pub fn delete(&self, conn: &mut Connection, id: &str) -> Result<()> {
        self.file()?.delete(conn, id)
    }
}
