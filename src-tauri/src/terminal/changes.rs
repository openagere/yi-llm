use crate::error::{AppError, Result};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

/// One planned file operation: write `content`, or delete the file when it is `None`.
pub struct FileChange {
    pub path: PathBuf,
    pub original: Option<Vec<u8>>,
    pub content: Option<Vec<u8>>,
}

fn read_original(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

impl FileChange {
    pub fn new(path: PathBuf, content: Vec<u8>) -> Result<Self> {
        Ok(Self {
            original: read_original(&path)?,
            path,
            content: Some(content),
        })
    }

    pub fn delete(path: PathBuf) -> Result<Self> {
        Ok(Self {
            original: read_original(&path)?,
            path,
            content: None,
        })
    }
}

pub fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::validation("配置路径无父目录"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    if let Ok(metadata) = fs::metadata(path) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())?;
    }
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn remove_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
