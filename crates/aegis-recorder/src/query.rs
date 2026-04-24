use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use aegis_core::{AegisError, Result};

const CHUNK_SIZE: u64 = 4096;

pub fn read_all_lines(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path).map_err(|source| AegisError::StorageIo {
        path: path.to_owned(),
        source,
    })?;
    Ok(content.lines().map(str::to_owned).collect())
}

pub fn tail_lines(path: &Path, n: usize) -> Result<Vec<String>> {
    if n == 0 {
        return Ok(Vec::new());
    }

    let mut file = File::open(path).map_err(|source| AegisError::StorageIo {
        path: path.to_owned(),
        source,
    })?;
    let len = file
        .metadata()
        .map_err(|source| AegisError::StorageIo {
            path: path.to_owned(),
            source,
        })?
        .len();

    if len == 0 {
        return Ok(Vec::new());
    }

    let mut end = len;
    let mut chunks: Vec<u8> = Vec::new();
    let mut newline_count = 0usize;
    let needed_newlines = n + 1;

    while end > 0 && newline_count < needed_newlines {
        let start = end.saturating_sub(CHUNK_SIZE);
        let size = (end - start) as usize;
        let mut buf = vec![0_u8; size];

        file.seek(SeekFrom::Start(start))
            .map_err(|source| AegisError::StorageIo {
                path: path.to_owned(),
                source,
            })?;
        file.read_exact(&mut buf)
            .map_err(|source| AegisError::StorageIo {
                path: path.to_owned(),
                source,
            })?;

        newline_count += buf.iter().filter(|byte| **byte == b'\n').count();
        buf.extend_from_slice(&chunks);
        chunks = buf;
        end = start;
    }

    let content = String::from_utf8_lossy(&chunks);
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    if lines.len() > n {
        lines = lines.split_off(lines.len() - n);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_returns_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.log");
        let content = (0..30)
            .map(|n| format!("line-{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, content).unwrap();

        let lines = tail_lines(&path, 10).unwrap();

        assert_eq!(lines.len(), 10);
        assert_eq!(lines.first().unwrap(), "line-20");
        assert_eq!(lines.last().unwrap(), "line-29");
    }

    #[test]
    fn tail_zero_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.log");
        std::fs::write(&path, "a\nb\n").unwrap();

        assert!(tail_lines(&path, 0).unwrap().is_empty());
    }

    #[test]
    fn read_all_strips_newlines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.log");
        std::fs::write(&path, "a\nb\n").unwrap();

        assert_eq!(read_all_lines(&path).unwrap(), vec!["a", "b"]);
    }
}
