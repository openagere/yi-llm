use std::path::Path;

const LEGACY_IDENTIFIER: &str = "com.llm-man.app";

/// 把旧标识符 `com.llm-man.app` 下的数据迁移到新目录，
/// 并重命名旧的数据库/日志文件，避免改名后用户丢失已有配置和用量。
pub fn migrate_legacy_data_dir(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        return Ok(());
    }
    let Some(parent) = dir.parent() else {
        return Ok(());
    };
    let legacy = parent.join(LEGACY_IDENTIFIER);
    if !legacy.is_dir() {
        return Ok(());
    }
    std::fs::rename(&legacy, dir)?;
    let legacy_db = dir.join("llm-man.db");
    if legacy_db.exists() && !dir.join("yi-llm.db").exists() {
        std::fs::rename(&legacy_db, dir.join("yi-llm.db"))?;
    }
    let logs = dir.join("logs");
    if let Ok(entries) = std::fs::read_dir(&logs) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(suffix) = name.strip_prefix("llm-man.log") {
                let _ = std::fs::rename(entry.path(), logs.join(format!("yi-llm.log{suffix}")));
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
        std::fs::create_dir_all(root.path().join(LEGACY_IDENTIFIER)).unwrap();
        let target = root.path().join("new");
        std::fs::create_dir_all(&target).unwrap();
        migrate_legacy_data_dir(&target).unwrap();
        assert!(root.path().join(LEGACY_IDENTIFIER).exists());
    }
}
