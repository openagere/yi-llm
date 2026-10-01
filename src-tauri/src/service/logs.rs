use crate::{app::AppState, error::Result};
use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
};

const MAX_TAIL_BYTES: u64 = 512 * 1024;
const MAX_TAIL_LINES: usize = 1000;

/// The most recent lines of the newest `yi-llm.log*` file.
pub async fn tail(state: &AppState) -> Result<String> {
    let dir = state.logs_dir.clone();
    tokio::task::spawn_blocking(move || {
        let mut files = std::fs::read_dir(&dir)?
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("yi-llm.log")
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
        match files.pop() {
            Some(file) => Ok(read_tail(&file.path())?),
            None => Ok(String::new()),
        }
    })
    .await?
}

fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    if start > 0 {
        reader.read_until(b'\n', &mut line)?;
    }
    let mut lines = VecDeque::with_capacity(MAX_TAIL_LINES);
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        if lines.len() == MAX_TAIL_LINES {
            lines.pop_front();
        }
        lines.push_back(
            String::from_utf8_lossy(&line)
                .trim_end_matches(['\r', '\n'])
                .to_owned(),
        );
    }
    Ok(lines.into_iter().collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::read_tail;

    #[test]
    fn log_tail_limits_lines_and_preserves_utf8_at_byte_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("yi-llm.log");
        let prefix = "中".repeat(180_000);
        let lines = (0..1200)
            .map(|index| format!("line {index}: 日志\r\n"))
            .collect::<String>();
        std::fs::write(&path, format!("{prefix}\n{lines}")).unwrap();
        let tail = read_tail(&path).unwrap();
        assert_eq!(tail.lines().count(), 1000);
        assert!(tail.starts_with("line 200: 日志\n"));
        assert!(tail.ends_with("line 1199: 日志"));
        assert!(!tail.contains('\u{fffd}'));
    }
}
