use std::path::{Path, PathBuf};

const LEGACY_IDENTIFIER: &str = "com.llm-man.app";

/// 把旧版本目录（`com.yi-llm.app` 或 `com.llm-man.app`）下的数据迁移到新目录（如 `~/.yi-llm`），
/// 并重命名旧的数据库/日志文件，避免升级、卸载或改名后用户丢失已有配置和用量。
pub fn migrate_legacy_data_dir(dir: &Path) -> std::io::Result<()> {
    migrate_from_legacy_sources(dir, None)
}

/// 从显式传入的旧 `app_data_dir`（如 `%APPDATA%\com.yi-llm.app`）或其同级的 `com.llm-man.app`
/// 迁移数据到目标目录 `dir`。
///
/// 若目标目录下已经存在 `yi-llm.db`，则视为已完成初始化，不做任何覆盖。
pub fn migrate_from_legacy_sources(
    dir: &Path,
    legacy_app_data_dir: Option<&Path>,
) -> std::io::Result<()> {
    if dir.join("yi-llm.db").exists() {
        return Ok(());
    }

    let candidates = collect_legacy_candidates(dir, legacy_app_data_dir);
    let Some(source) = candidates.into_iter().find(|path| has_legacy_db(path)) else {
        return Ok(());
    };

    if !dir.exists() {
        if let Some(parent) = dir.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if std::fs::rename(&source, dir).is_ok() {
            normalize_renamed_files(dir)?;
            return Ok(());
        }
    }

    copy_dir_missing(&source, dir)?;
    normalize_renamed_files(dir)?;
    Ok(())
}

fn collect_legacy_candidates(dir: &Path, legacy_app_data_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(app_data) = legacy_app_data_dir {
        if app_data != dir {
            candidates.push(app_data.to_path_buf());
        }
        if let Some(parent) = app_data.parent() {
            let sibling = parent.join(LEGACY_IDENTIFIER);
            if sibling != dir && !candidates.contains(&sibling) {
                candidates.push(sibling);
            }
        }
    }
    if let Some(parent) = dir.parent() {
        let sibling = parent.join(LEGACY_IDENTIFIER);
        if sibling != dir && !candidates.contains(&sibling) {
            candidates.push(sibling);
        }
    }
    candidates
}

fn has_legacy_db(dir: &Path) -> bool {
    dir.is_dir() && (dir.join("yi-llm.db").is_file() || dir.join("llm-man.db").is_file())
}

fn copy_dir_missing(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_missing(&src_path, &dst_path)?;
        } else if file_type.is_file() && !dst_path.exists() {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn normalize_renamed_files(dir: &Path) -> std::io::Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let legacy_db = dir.join(format!("llm-man.db{suffix}"));
        let target_db = dir.join(format!("yi-llm.db{suffix}"));
        if legacy_db.exists() && !target_db.exists() {
            std::fs::rename(&legacy_db, &target_db)?;
        }
    }
    let logs = dir.join("logs");
    if let Ok(entries) = std::fs::read_dir(&logs) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(suffix) = name.strip_prefix("llm-man.log") {
                let target_log = logs.join(format!("yi-llm.log{suffix}"));
                if !target_log.exists() {
                    let _ = std::fs::rename(entry.path(), target_log);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_the_legacy_directory_and_renames_database_and_logs() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(legacy.join("logs")).unwrap();
        std::fs::write(legacy.join("llm-man.db"), b"data").unwrap();
        std::fs::write(legacy.join("logs/llm-man.log.2026-01-01"), b"log").unwrap();
        let target = root.path().join("com.yi-llm.app");
        migrate_legacy_data_dir(&target).unwrap();
        assert!(!legacy.exists());
        assert_eq!(std::fs::read(target.join("yi-llm.db")).unwrap(), b"data");
        assert!(target.join("logs/yi-llm.log.2026-01-01").exists());
    }

    #[test]
    fn leaves_an_existing_target_untouched() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root.path().join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("llm-man.db"), b"old").unwrap();
        let target = root.path().join("new");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("yi-llm.db"), b"existing").unwrap();
        migrate_legacy_data_dir(&target).unwrap();
        assert!(legacy.exists());
        assert_eq!(
            std::fs::read(target.join("yi-llm.db")).unwrap(),
            b"existing"
        );
    }

    #[test]
    fn migrates_from_explicit_legacy_app_data_dir_to_home_dot_yi_llm() {
        let root = tempfile::tempdir().unwrap();
        let legacy_app_data = root.path().join("appdata").join("com.yi-llm.app");
        std::fs::create_dir_all(legacy_app_data.join("models")).unwrap();
        std::fs::create_dir_all(legacy_app_data.join("logs")).unwrap();
        std::fs::write(legacy_app_data.join("yi-llm.db"), b"db").unwrap();
        std::fs::write(legacy_app_data.join("yi-llm.db-wal"), b"wal").unwrap();
        std::fs::write(legacy_app_data.join("yi-llm.db-shm"), b"shm").unwrap();
        std::fs::write(legacy_app_data.join("models/catalog.json"), b"catalog").unwrap();
        std::fs::write(legacy_app_data.join("logs/yi-llm.log.2026-10-22"), b"log").unwrap();

        let target = root.path().join("home").join(".yi-llm");
        migrate_from_legacy_sources(&target, Some(&legacy_app_data)).unwrap();

        assert_eq!(std::fs::read(target.join("yi-llm.db")).unwrap(), b"db");
        assert_eq!(std::fs::read(target.join("yi-llm.db-wal")).unwrap(), b"wal");
        assert_eq!(std::fs::read(target.join("yi-llm.db-shm")).unwrap(), b"shm");
        assert_eq!(
            std::fs::read(target.join("models/catalog.json")).unwrap(),
            b"catalog"
        );
        assert_eq!(
            std::fs::read(target.join("logs/yi-llm.log.2026-10-22")).unwrap(),
            b"log"
        );
    }

    #[test]
    fn falls_back_to_llm_man_sibling_when_yi_llm_app_data_is_missing() {
        let root = tempfile::tempdir().unwrap();
        let appdata = root.path().join("appdata");
        let legacy_llm_man = appdata.join("com.llm-man.app");
        std::fs::create_dir_all(legacy_llm_man.join("logs")).unwrap();
        std::fs::write(legacy_llm_man.join("llm-man.db"), b"legacy-db").unwrap();
        std::fs::write(legacy_llm_man.join("llm-man.db-wal"), b"legacy-wal").unwrap();
        std::fs::write(legacy_llm_man.join("logs/llm-man.log.2026-01-01"), b"log").unwrap();

        let legacy_yi_llm = appdata.join("com.yi-llm.app");
        let target = root.path().join("home").join(".yi-llm");
        migrate_from_legacy_sources(&target, Some(&legacy_yi_llm)).unwrap();

        assert_eq!(
            std::fs::read(target.join("yi-llm.db")).unwrap(),
            b"legacy-db"
        );
        assert_eq!(
            std::fs::read(target.join("yi-llm.db-wal")).unwrap(),
            b"legacy-wal"
        );
        assert!(target.join("logs/yi-llm.log.2026-01-01").exists());
    }

    #[test]
    fn migrates_into_existing_empty_target_directory_and_preserves_copy_fallback() {
        let root = tempfile::tempdir().unwrap();
        let legacy_app_data = root.path().join("appdata").join("com.yi-llm.app");
        std::fs::create_dir_all(legacy_app_data.join("models")).unwrap();
        std::fs::write(legacy_app_data.join("yi-llm.db"), b"db-data").unwrap();
        std::fs::write(legacy_app_data.join("models/catalog.json"), b"cat").unwrap();

        let target = root.path().join("home").join(".yi-llm");
        // Even if target directory already exists (so whole-dir rename cannot replace it),
        // migration still copies/moves contents when yi-llm.db is missing.
        std::fs::create_dir_all(&target).unwrap();
        migrate_from_legacy_sources(&target, Some(&legacy_app_data)).unwrap();

        assert_eq!(std::fs::read(target.join("yi-llm.db")).unwrap(), b"db-data");
        assert_eq!(
            std::fs::read(target.join("models/catalog.json")).unwrap(),
            b"cat"
        );
    }
}
