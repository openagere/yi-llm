use crate::{
    domain::terminal::Profile,
    error::{AppError, Result},
    terminal::{
        changes::{atomic_write, remove_if_present, FileChange},
        clients::configurator,
        plan::{config_path, plan},
        profile,
    },
};
use rusqlite::Connection;
use serde::Serialize;
use std::{io::Write, path::Path};

#[derive(Serialize)]
pub struct Preview {
    pub config_path: String,
    pub endpoint: String,
    pub files: Vec<PreviewFile>,
}

#[derive(Serialize)]
pub struct PreviewFile {
    pub path: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct ApplyResult {
    pub config_path: String,
    pub endpoint: String,
    pub backup_paths: Vec<String>,
    pub model_count: usize,
}

pub fn preview(conn: &Connection, profile: &Profile) -> Result<Preview> {
    preview_at(conn, profile, &config_path(&profile.client)?)
}

pub fn preview_at(conn: &Connection, profile: &Profile, path: &Path) -> Result<Preview> {
    let (endpoint, files) = plan(conn, profile, path)?;
    let configurator = configurator(&profile.client)?;
    let mut previews = Vec::new();
    for file in files {
        let Some(content) = &file.content else {
            continue;
        };
        if file
            .path
            .file_name()
            .is_some_and(|name| name == ".codex-global-state.json")
        {
            continue;
        }
        let content = if file.path == path {
            // Preview only owned settings; unrelated credentials and hooks stay private.
            configurator.owned_preview(content)?
        } else {
            String::from_utf8_lossy(content).into_owned()
        };
        previews.push(PreviewFile {
            path: file.path.display().to_string(),
            content,
        });
    }
    previews.sort_by_key(|file| file.path != path.display().to_string());
    Ok(Preview {
        config_path: path.display().to_string(),
        endpoint,
        files: previews,
    })
}

pub fn apply(conn: &Connection, profile: &Profile) -> Result<ApplyResult> {
    apply_at(conn, profile, &config_path(&profile.client)?)
}

pub fn apply_at(conn: &Connection, profile: &Profile, path: &Path) -> Result<ApplyResult> {
    let (endpoint, files) = plan(conn, profile, path)?;
    let mut backup_paths = Vec::new();
    for file in &files {
        if let Some(original) = &file.original {
            let parent = file
                .path
                .parent()
                .ok_or_else(|| AppError::validation("配置路径无父目录"))?;
            let mut backup = tempfile::Builder::new()
                .prefix(&format!(
                    "{}.bak-",
                    file.path.file_name().unwrap_or_default().to_string_lossy()
                ))
                .tempfile_in(parent)?;
            backup.write_all(original)?;
            backup.as_file().sync_all()?;
            let (_, backup_path) = backup.keep().map_err(|error| error.error)?;
            backup_paths.push(backup_path.display().to_string());
        }
    }
    for (index, file) in files.iter().enumerate() {
        let result = match &file.content {
            Some(content) => atomic_write(&file.path, content),
            None => remove_if_present(&file.path),
        };
        if let Err(error) = result {
            return rollback(&files[..=index], &error);
        }
    }
    if let Err(error) = profile::save(conn, profile) {
        return rollback(&files, &error);
    }
    Ok(ApplyResult {
        config_path: path.display().to_string(),
        endpoint,
        backup_paths,
        model_count: profile.models.len(),
    })
}

fn rollback<T>(files: &[FileChange], error: &AppError) -> Result<T> {
    let mut failures = Vec::new();
    for file in files.iter().rev() {
        let result = match &file.original {
            Some(bytes) => atomic_write(&file.path, bytes),
            None => remove_if_present(&file.path),
        };
        if let Err(error) = result {
            failures.push(format!("{}: {error}", file.path.display()));
        }
    }
    Err(AppError::internal(if failures.is_empty() {
        format!("{error}；已恢复原配置")
    } else {
        format!("{error}；部分配置恢复失败: {}", failures.join(", "))
    }))
}
