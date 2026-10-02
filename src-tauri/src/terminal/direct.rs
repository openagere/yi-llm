use crate::{
    db::repo::{providers, terminal_direct_profiles, terminal_profiles},
    domain::{provider::ProviderView, terminal::DirectProfile},
    error::{AppError, Result},
    terminal::{
        apply::{ApplyResult, Preview, PreviewFile},
        changes::{atomic_write, read_optional, remove_if_present, FileChange},
        clients::{configurator, DirectPlanInput},
    },
};
use rusqlite::Connection;
use std::{io::Write, path::Path};

fn allowed_protocols(client: &str) -> Result<&'static [&'static str]> {
    match client {
        "codex" => Ok(&["responses"]),
        "claude-code" => Ok(&["anthropic"]),
        "opencode" => Ok(&["openai_chat", "responses"]),
        _ => Err(AppError::validation("未知终端类型")),
    }
}

fn resolve(conn: &Connection, profile: &DirectProfile) -> Result<ProviderView> {
    let protocols = allowed_protocols(&profile.client)?;
    let view = providers::list_views(conn)?
        .into_iter()
        .find(|view| view.provider.id == profile.provider_id)
        .ok_or_else(|| AppError::not_found("直连 Provider 不存在，请重新选择"))?;
    if !view.provider.enabled {
        return Err(AppError::validation("直连 Provider 未启用"));
    }
    if !protocols.contains(&view.provider.provider_type.as_str()) {
        return Err(AppError::validation(format!(
            "{} 直连不支持 {} 原生协议；可改用代理接入",
            profile.client, view.provider.provider_type
        )));
    }
    match profile.model_source.as_str() {
        "native" if profile.client != "opencode" => {
            if profile
                .model
                .as_deref()
                .is_some_and(|model| !model.trim().is_empty())
            {
                return Err(AppError::validation("跟随终端模型时不应指定 Provider 模型"));
            }
        }
        // `provider` syncs every maintained upstream model; an empty model means the
        // provider's first upstream model becomes the startup default.
        "provider" => {}
        _ => return Err(AppError::validation("该终端不支持所选直连模型模式")),
    }
    Ok(view)
}

fn plan(
    conn: &Connection,
    profile: &DirectProfile,
    path: &Path,
) -> Result<(ProviderView, Vec<FileChange>)> {
    let provider = resolve(conn, profile)?;
    let source = read_optional(path)?.unwrap_or_default();
    let proxy_profile = terminal_profiles::get(conn, &profile.client)?;
    let previous_direct_profile = terminal_direct_profiles::get(conn, &profile.client)?;
    let previous_direct_provider = if let Some(previous) = previous_direct_profile.as_ref() {
        providers::list_views(conn)?
            .into_iter()
            .find(|view| view.provider.id == previous.provider_id)
    } else {
        None
    };
    let built = configurator(&profile.client)?.build_direct(
        &DirectPlanInput {
            profile,
            provider: &provider,
            proxy_profile: proxy_profile.as_ref(),
            previous_direct_profile: previous_direct_profile.as_ref(),
            previous_direct_provider: previous_direct_provider.as_ref(),
        },
        path,
        &source,
    )?;
    let mut files = built.extra;
    files.push(FileChange::new(path.to_owned(), built.main)?);
    Ok((provider, files))
}

pub fn preview(conn: &Connection, profile: &DirectProfile) -> Result<Preview> {
    preview_at(
        conn,
        profile,
        &configurator(&profile.client)?.config_path()?,
    )
}

pub fn preview_at(conn: &Connection, profile: &DirectProfile, path: &Path) -> Result<Preview> {
    let (provider, files) = plan(conn, profile, path)?;
    let client = configurator(&profile.client)?;
    let mut preview_files = Vec::new();
    for file in files {
        let Some(content) = file.content else {
            continue;
        };
        let content = if file.path == path {
            client.owned_direct_preview(&content)?
        } else {
            String::from_utf8_lossy(&content).into_owned()
        };
        preview_files.push(PreviewFile {
            path: file.path.display().to_string(),
            content,
        });
    }
    preview_files.sort_by_key(|file| file.path != path.display().to_string());
    Ok(Preview {
        config_path: path.display().to_string(),
        endpoint: super::clients::redact_endpoint(&provider.provider.base_url),
        files: preview_files,
    })
}

pub fn apply(conn: &Connection, profile: &DirectProfile) -> Result<ApplyResult> {
    apply_at(
        conn,
        profile,
        &configurator(&profile.client)?.config_path()?,
    )
}

pub fn apply_at(conn: &Connection, profile: &DirectProfile, path: &Path) -> Result<ApplyResult> {
    let (provider, files) = plan(conn, profile, path)?;
    let mut backups = Vec::new();
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
            backups.push(backup_path.display().to_string());
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
    if let Err(error) = terminal_direct_profiles::upsert(conn, profile) {
        return rollback(&files, &error);
    }
    Ok(ApplyResult {
        config_path: path.display().to_string(),
        endpoint: provider.provider.base_url,
        backup_paths: backups,
        model_count: provider.models.len(),
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

pub fn active(conn: &Connection, profile: &DirectProfile, path: &Path) -> Result<bool> {
    let Ok(provider) = resolve(conn, profile) else {
        return Ok(false);
    };
    let Some(source) = read_optional(path)? else {
        return Ok(false);
    };
    configurator(&profile.client)?.is_direct_active(path, &source, profile, &provider)
}
