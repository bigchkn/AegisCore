use std::path::{Path, PathBuf};

use aegis_core::{AegisError, Result, StorageBackend};

#[derive(Debug, Clone)]
pub struct ProjectStorage {
    project_root: PathBuf,
}

impl ProjectStorage {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    pub fn ensure_layout(&self) -> Result<()> {
        for path in [
            self.state_dir(),
            self.snapshots_dir(),
            self.logs_dir(),
            self.archive_dir(),
            self.channels_dir(),
            self.profiles_dir(),
            self.worktrees_dir(),
            self.handoff_dir(),
            self.prompts_dir(),
        ] {
            std::fs::create_dir_all(&path)
                .map_err(|source| AegisError::StorageIo { path, source })?;
        }
        Ok(())
    }
}

impl StorageBackend for ProjectStorage {
    fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn ensure_layout_creates_controller_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let storage = ProjectStorage::new(dir.path().to_path_buf());

        storage.ensure_layout().unwrap();

        assert!(storage.state_dir().is_dir());
        assert!(storage.snapshots_dir().is_dir());
        assert!(storage.logs_dir().is_dir());
        assert!(storage.archive_dir().is_dir());
        assert!(storage.channels_dir().is_dir());
        assert!(storage.profiles_dir().is_dir());
        assert!(storage.worktrees_dir().is_dir());
        assert!(storage.handoff_dir().is_dir());
        assert!(storage.prompts_dir().is_dir());

        let agent_id = Uuid::nil();
        assert_eq!(
            storage.agent_log_path(agent_id),
            dir.path()
                .join(".aegis")
                .join("logs")
                .join("sessions")
                .join(format!("{agent_id}.log"))
        );
    }
}
