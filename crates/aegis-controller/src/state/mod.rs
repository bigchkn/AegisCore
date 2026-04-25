use crate::registry::agents::AgentStore;
use crate::registry::tasks::TaskStore;
use aegis_core::agent::AgentStatus;
use aegis_core::error::{AegisError, Result};
use aegis_core::storage::StorageBackend;
use aegis_core::task::TaskStatus;
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, info, warn};

pub struct StateManager {
    pub storage: Arc<dyn StorageBackend>,
}

pub struct RecoveryResult {
    pub registry_restored: bool,
    pub snapshot_used: Option<PathBuf>,
    pub agents_recovered: usize,
    pub agents_marked_failed: usize,
}

impl StateManager {
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        Self { storage }
    }

    pub fn snapshot_now(&self) -> Result<PathBuf> {
        let registry_path = self.storage.registry_path();
        if !registry_path.exists() {
            return Err(AegisError::StorageIo {
                path: registry_path,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "registry.json not found",
                ),
            });
        }

        let timestamp = Utc::now().to_rfc3339();
        let snapshot_name = format!("registry_{}.json", timestamp);
        let snapshot_path = self.storage.snapshots_dir().join(snapshot_name);

        fs::copy(&registry_path, &snapshot_path).map_err(|e| AegisError::StorageIo {
            path: snapshot_path.clone(),
            source: e,
        })?;

        info!(path = %snapshot_path.display(), "State snapshot created");
        Ok(snapshot_path)
    }

    pub fn prune_snapshots(&self, retention_count: usize) -> Result<()> {
        let snapshots_dir = self.storage.snapshots_dir();
        let mut entries = fs::read_dir(&snapshots_dir)
            .map_err(|e| AegisError::StorageIo {
                path: snapshots_dir.clone(),
                source: e,
            })?
            .filter_map(|res| res.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
            .collect::<Vec<_>>();

        if entries.len() <= retention_count {
            return Ok(());
        }

        // Sort by modification time (oldest first)
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());

        let to_remove = entries.len() - retention_count;
        for entry in entries.iter().take(to_remove) {
            fs::remove_file(entry.path()).map_err(|e| AegisError::StorageIo {
                path: entry.path(),
                source: e,
            })?;
            debug!(path = %entry.path().display(), "Pruned old snapshot");
        }

        Ok(())
    }

    pub fn recover(&self) -> Result<RecoveryResult> {
        let registry_path = self.storage.registry_path();
        let mut result = RecoveryResult {
            registry_restored: false,
            snapshot_used: None,
            agents_recovered: 0,
            agents_marked_failed: 0,
        };

        let mut needs_restore = false;
        if !registry_path.exists() {
            needs_restore = true;
            warn!("registry.json missing, attempting recovery from snapshot");
        } else {
            // Try to parse it
            let content =
                fs::read_to_string(&registry_path).map_err(|e| AegisError::StorageIo {
                    path: registry_path.clone(),
                    source: e,
                })?;
            if serde_json::from_str::<AgentStore>(&content).is_err() {
                needs_restore = true;
                warn!("registry.json corrupted, attempting recovery from snapshot");
            }
        }

        if needs_restore {
            let snapshots_dir = self.storage.snapshots_dir();
            let mut snapshots = fs::read_dir(&snapshots_dir)
                .map_err(|e| AegisError::StorageIo {
                    path: snapshots_dir.clone(),
                    source: e,
                })?
                .filter_map(|res| res.ok())
                .collect::<Vec<_>>();

            // Sort by modification time (newest first)
            snapshots
                .sort_by_key(|e| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).ok()));

            if let Some(latest) = snapshots.first() {
                fs::copy(latest.path(), &registry_path).map_err(|e| AegisError::StorageIo {
                    path: registry_path.clone(),
                    source: e,
                })?;
                result.registry_restored = true;
                result.snapshot_used = Some(latest.path());
                info!(snapshot = %latest.path().display(), "Registry restored from snapshot");
            } else {
                warn!("No snapshots found for recovery");
                return Ok(result);
            }
        }

        // 2. Post-recovery status cleanup
        let content = fs::read_to_string(&registry_path).map_err(|e| AegisError::StorageIo {
            path: registry_path.clone(),
            source: e,
        })?;
        let mut store: AgentStore =
            serde_json::from_str(&content).map_err(|e| AegisError::RegistryCorrupted {
                path: registry_path.clone(),
                source: e,
            })?;

        for agent in &mut store.agents {
            match agent.status {
                AgentStatus::Starting
                | AgentStatus::Active
                | AgentStatus::Cooling
                | AgentStatus::Reporting => {
                    agent.status = AgentStatus::Failed;
                    agent.updated_at = Utc::now();
                    result.agents_marked_failed += 1;
                }
                AgentStatus::Queued | AgentStatus::Paused => {
                    result.agents_recovered += 1;
                }
                AgentStatus::Terminated | AgentStatus::Failed => {
                    // No change
                }
            }
        }

        // Write back cleaned up store
        let json =
            serde_json::to_string_pretty(&store).map_err(|e| AegisError::RegistryCorrupted {
                path: registry_path.clone(),
                source: e,
            })?;
        fs::write(&registry_path, json).map_err(|e| AegisError::StorageIo {
            path: registry_path.clone(),
            source: e,
        })?;

        // 3. Reset orphaned Active tasks: any task still Active whose assigned agent
        //    is no longer live gets returned to Queued so the drain loop can retry it.
        let tasks_path = self.storage.tasks_path();
        if tasks_path.exists() {
            let live_agent_ids: HashSet<_> = store
                .agents
                .iter()
                .filter(|a| {
                    matches!(
                        a.status,
                        AgentStatus::Queued | AgentStatus::Starting | AgentStatus::Active
                    )
                })
                .map(|a| a.agent_id)
                .collect();

            let content = fs::read_to_string(&tasks_path).map_err(|e| AegisError::StorageIo {
                path: tasks_path.clone(),
                source: e,
            })?;
            if let Ok(mut task_store) = serde_json::from_str::<TaskStore>(&content) {
                let mut tasks_reset = 0usize;
                for task in &mut task_store.tasks {
                    if task.status == TaskStatus::Active {
                        let orphaned = task
                            .assigned_agent_id
                            .map_or(true, |id| !live_agent_ids.contains(&id));
                        if orphaned {
                            task.status = TaskStatus::Queued;
                            task.assigned_agent_id = None;
                            tasks_reset += 1;
                        }
                    }
                }
                if tasks_reset > 0 {
                    let json = serde_json::to_string_pretty(&task_store).map_err(|e| {
                        AegisError::RegistryCorrupted {
                            path: tasks_path.clone(),
                            source: e,
                        }
                    })?;
                    fs::write(&tasks_path, json).map_err(|e| AegisError::StorageIo {
                        path: tasks_path.clone(),
                        source: e,
                    })?;
                    info!(count = tasks_reset, "Reset orphaned Active tasks to Queued");
                }
            } else {
                warn!("tasks.json could not be parsed during recovery — skipping task reset");
            }
        }

        Ok(result)
    }
}
