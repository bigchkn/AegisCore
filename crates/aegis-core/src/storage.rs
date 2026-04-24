use std::path::{Path, PathBuf};
use uuid::Uuid;

pub trait StorageBackend: Send + Sync {
    fn project_root(&self) -> &Path;

    fn aegis_dir(&self) -> PathBuf {
        self.project_root().join(".aegis")
    }
    fn logs_dir(&self) -> PathBuf {
        self.aegis_dir().join("logs").join("sessions")
    }
    fn archive_dir(&self) -> PathBuf {
        self.aegis_dir().join("logs").join("archive")
    }
    fn state_dir(&self) -> PathBuf {
        self.aegis_dir().join("state")
    }
    fn snapshots_dir(&self) -> PathBuf {
        self.state_dir().join("snapshots")
    }
    fn channels_dir(&self) -> PathBuf {
        self.aegis_dir().join("channels")
    }
    fn profiles_dir(&self) -> PathBuf {
        self.aegis_dir().join("profiles")
    }
    fn worktrees_dir(&self) -> PathBuf {
        self.aegis_dir().join("worktrees")
    }
    fn handoff_dir(&self) -> PathBuf {
        self.aegis_dir().join("handoff")
    }
    fn prompts_dir(&self) -> PathBuf {
        self.aegis_dir().join("prompts")
    }
    fn designs_dir(&self) -> PathBuf {
        self.aegis_dir().join("designs")
    }

    // ── Derived paths ────────────────────────────────────────────────

    fn registry_path(&self) -> PathBuf {
        self.state_dir().join("registry.json")
    }
    fn tasks_path(&self) -> PathBuf {
        self.state_dir().join("tasks.json")
    }
    fn channels_state_path(&self) -> PathBuf {
        self.state_dir().join("channels.json")
    }
    fn taskflow_path(&self) -> PathBuf {
        self.state_dir().join("taskflow.json")
    }
    fn agent_log_path(&self, agent_id: Uuid) -> PathBuf {
        self.logs_dir().join(format!("{}.log", agent_id))
    }
    fn sandbox_profile_path(&self, agent_id: Uuid) -> PathBuf {
        self.profiles_dir().join(format!("{}.sb", agent_id))
    }
    fn agent_worktree_path(&self, agent_id: Uuid) -> PathBuf {
        self.worktrees_dir().join(agent_id.to_string())
    }
    fn agent_inbox_path(&self, agent_id: Uuid) -> PathBuf {
        self.channels_dir().join(agent_id.to_string()).join("inbox")
    }
}
