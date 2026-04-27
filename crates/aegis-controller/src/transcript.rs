use std::{
    fs::OpenOptions,
    io::Write,
    path::Path,
};

use aegis_core::{AegisError, Result};

const SEND_PREFIX: &str = "[tmux send]";
const INPUT_PREFIX: &str = "[tmux input]";

pub fn append_tmux_send(log_path: &Path, text: &str) -> Result<()> {
    append_lines(log_path, SEND_PREFIX, text.as_bytes())
}

pub fn append_tmux_input(log_path: &Path, data: &[u8]) -> Result<()> {
    append_lines(log_path, INPUT_PREFIX, data)
}

fn append_lines(log_path: &Path, prefix: &str, data: &[u8]) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| AegisError::StorageIo {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|source| AegisError::StorageIo {
            path: log_path.to_path_buf(),
            source,
        })?;

    let text = String::from_utf8_lossy(data);
    if text.is_empty() {
        writeln!(file, "{prefix}").map_err(|source| AegisError::StorageIo {
            path: log_path.to_path_buf(),
            source,
        })?;
        return Ok(());
    }

    let mut wrote_any = false;
    for line in text.split_inclusive('\n') {
        wrote_any = true;
        let line = line.strip_suffix('\n').unwrap_or(line);
        if line.is_empty() {
            writeln!(file, "{prefix}").map_err(|source| AegisError::StorageIo {
                path: log_path.to_path_buf(),
                source,
            })?;
        } else {
            writeln!(file, "{prefix} {line}").map_err(|source| AegisError::StorageIo {
                path: log_path.to_path_buf(),
                source,
            })?;
        }
    }

    if !wrote_any {
        writeln!(file, "{prefix} {text}").map_err(|source| AegisError::StorageIo {
            path: log_path.to_path_buf(),
            source,
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_tmux_send_writes_prefixed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("agent.log");

        append_tmux_send(&log_path, "alpha\nbeta").unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();

        assert!(content.contains("[tmux send] alpha"));
        assert!(content.contains("[tmux send] beta"));
    }

    #[test]
    fn append_tmux_input_writes_prefixed_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("agent.log");

        append_tmux_input(&log_path, b"cmd one\ncmd two\n").unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();

        assert!(content.contains("[tmux input] cmd one"));
        assert!(content.contains("[tmux input] cmd two"));
    }
}
