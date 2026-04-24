use std::{fs, path::PathBuf, time::SystemTime};

use aegis_core::{config::RecorderConfig, AegisError, Result, StorageBackend};
use tracing::debug;

pub fn prune_archive(storage: &dyn StorageBackend, config: &RecorderConfig) -> Result<()> {
    let archive_dir = storage.archive_dir();
    if !archive_dir.exists() {
        return Ok(());
    }

    let mut entries = archive_entries(&archive_dir)?;
    entries.sort_by_key(|entry| entry.modified);

    while entries.len() > config.log_retention_count {
        let entry = entries.remove(0);
        remove_archive(&entry.path)?;
    }

    let max_bytes = config.log_rotation_max_mb.saturating_mul(1024 * 1024);
    if max_bytes > 0 {
        let mut total_size: u64 = entries.iter().map(|entry| entry.size).sum();
        while total_size > max_bytes && !entries.is_empty() {
            let entry = entries.remove(0);
            total_size = total_size.saturating_sub(entry.size);
            remove_archive(&entry.path)?;
        }
    }

    Ok(())
}

#[derive(Debug)]
struct ArchiveEntry {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

fn archive_entries(archive_dir: &std::path::Path) -> Result<Vec<ArchiveEntry>> {
    let read_dir = fs::read_dir(archive_dir).map_err(|source| AegisError::StorageIo {
        path: archive_dir.to_owned(),
        source,
    })?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|source| AegisError::StorageIo {
            path: archive_dir.to_owned(),
            source,
        })?;
        let path = entry.path();
        let metadata = entry.metadata().map_err(|source| AegisError::StorageIo {
            path: path.clone(),
            source,
        })?;
        if !metadata.is_file() {
            continue;
        }
        entries.push(ArchiveEntry {
            path,
            modified: metadata
                .modified()
                .map_err(|source| AegisError::StorageIo {
                    path: entry.path(),
                    source,
                })?,
            size: metadata.len(),
        });
    }

    Ok(entries)
}

fn remove_archive(path: &std::path::Path) -> Result<()> {
    fs::remove_file(path).map_err(|source| AegisError::StorageIo {
        path: path.to_owned(),
        source,
    })?;
    debug!(removed = %path.display(), "pruned old log archive");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::Path, thread, time::Duration};

    struct TestStorage {
        root: PathBuf,
    }

    impl StorageBackend for TestStorage {
        fn project_root(&self) -> &Path {
            &self.root
        }
    }

    fn config(retention: usize, max_mb: u64) -> RecorderConfig {
        RecorderConfig {
            failover_context_lines: 100,
            log_rotation_max_mb: max_mb,
            log_retention_count: retention,
        }
    }

    #[test]
    fn prune_keeps_retention_count() {
        let dir = tempfile::tempdir().unwrap();
        let storage = TestStorage {
            root: dir.path().to_owned(),
        };
        let archive_dir = storage.archive_dir();
        std::fs::create_dir_all(&archive_dir).unwrap();
        for i in 0..5 {
            std::fs::write(archive_dir.join(format!("agent_{i}.log")), "log").unwrap();
            thread::sleep(Duration::from_millis(2));
        }

        prune_archive(&storage, &config(3, 50)).unwrap();

        let remaining = std::fs::read_dir(&archive_dir).unwrap().count();
        assert_eq!(remaining, 3);
    }

    #[test]
    fn prune_removes_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let storage = TestStorage {
            root: dir.path().to_owned(),
        };
        let archive_dir = storage.archive_dir();
        std::fs::create_dir_all(&archive_dir).unwrap();
        for name in ["old.log", "middle.log", "new.log"] {
            std::fs::write(archive_dir.join(name), "log").unwrap();
            thread::sleep(Duration::from_millis(2));
        }

        prune_archive(&storage, &config(2, 50)).unwrap();

        assert!(!archive_dir.join("old.log").exists());
        assert!(archive_dir.join("middle.log").exists());
        assert!(archive_dir.join("new.log").exists());
    }
}
