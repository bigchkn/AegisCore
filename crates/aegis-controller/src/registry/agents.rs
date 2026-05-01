use crate::registry::LockedFile;
use aegis_core::agent::{Agent, AgentRegistry, AgentStatus};
use aegis_core::error::{AegisError, Result};
use aegis_core::storage::StorageBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AgentStore {
    pub version: u32,
    pub agents: Vec<Agent>,
    pub archived: Vec<Agent>,
}

pub struct FileRegistry {
    pub storage: Arc<dyn StorageBackend>,
}

impl FileRegistry {
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        Self { storage }
    }

    pub fn init(storage: &dyn StorageBackend) -> Result<()> {
        std::fs::create_dir_all(storage.state_dir()).map_err(|e| AegisError::StorageIo {
            path: storage.state_dir(),
            source: e,
        })?;
        std::fs::create_dir_all(storage.snapshots_dir()).map_err(|e| AegisError::StorageIo {
            path: storage.snapshots_dir(),
            source: e,
        })?;

        Self::write_if_absent(
            &storage.registry_path(),
            &AgentStore {
                version: 1,
                ..Default::default()
            },
        )?;
        Self::write_if_absent(
            &storage.tasks_path(),
            &crate::registry::tasks::TaskStore {
                version: 1,
                ..Default::default()
            },
        )?;
        Self::write_if_absent(
            &storage.channels_state_path(),
            &crate::registry::channels::ChannelStore {
                version: 1,
                ..Default::default()
            },
        )?;

        Ok(())
    }

    fn write_if_absent<T: Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
        if !path.exists() {
            let json =
                serde_json::to_string_pretty(value).map_err(|e| AegisError::RegistryCorrupted {
                    path: path.to_path_buf(),
                    source: e,
                })?;
            std::fs::write(path, json).map_err(|e| AegisError::StorageIo {
                path: path.to_path_buf(),
                source: e,
            })?;
        }
        Ok(())
    }
}

impl AgentRegistry for FileRegistry {
    fn insert(&self, agent: &Agent) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;
        store.agents.push(agent.clone());
        file.write_json_atomic(&store)
    }

    fn get(&self, agent_id: Uuid) -> Result<Option<Agent>> {
        let mut file = LockedFile::open_shared(&self.storage.registry_path())?;
        let store: AgentStore = file.read_json()?;

        let found = store
            .agents
            .iter()
            .chain(store.archived.iter())
            .find(|a| a.agent_id == agent_id)
            .cloned();

        Ok(found)
    }

    fn update(&self, agent: &Agent) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        if let Some(idx) = store
            .agents
            .iter()
            .position(|a| a.agent_id == agent.agent_id)
        {
            store.agents[idx] = agent.clone();
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::AgentNotFound {
                agent_id: agent.agent_id,
            })
        }
    }

    fn update_status(&self, agent_id: Uuid, status: AgentStatus) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        if let Some(agent) = store.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.status = status;
            agent.updated_at = Utc::now();
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::AgentNotFound { agent_id })
        }
    }

    fn update_provider(&self, agent_id: Uuid, provider: &str) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        if let Some(agent) = store.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.cli_provider = provider.to_string();
            agent.updated_at = Utc::now();
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::AgentNotFound { agent_id })
        }
    }

    fn list_active(&self) -> Result<Vec<Agent>> {
        let mut file = LockedFile::open_shared(&self.storage.registry_path())?;
        let store: AgentStore = file.read_json()?;
        Ok(store
            .agents
            .iter()
            .filter(|a| !a.status.is_terminal())
            .cloned()
            .collect())
    }

    fn list_by_role(&self, role: &str) -> Result<Vec<Agent>> {
        let mut file = LockedFile::open_shared(&self.storage.registry_path())?;
        let store: AgentStore = file.read_json()?;
        Ok(store
            .agents
            .iter()
            .filter(|a| !a.status.is_terminal() && a.role == role)
            .cloned()
            .collect())
    }

    fn list_all(&self) -> Result<Vec<Agent>> {
        let mut file = LockedFile::open_shared(&self.storage.registry_path())?;
        let store: AgentStore = file.read_json()?;
        let mut all = store.agents;
        all.extend(store.archived);
        Ok(all)
    }

    fn archive(&self, agent_id: Uuid) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        if let Some(pos) = store.agents.iter().position(|a| a.agent_id == agent_id) {
            let mut agent = store.agents.remove(pos);
            agent.terminated_at = Some(Utc::now());
            agent.updated_at = Utc::now();
            store.archived.push(agent);
            file.write_json_atomic(&store)
        } else {
            Err(AegisError::AgentNotFound { agent_id })
        }
    }

    fn find_or_insert_starting_bastion(&self, role: &str, agent: &Agent) -> Result<(Agent, bool)> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        if let Some(existing) = store
            .agents
            .iter()
            .find(|a| a.kind == AgentKind::Bastion && a.role == role)
        {
            return Ok((existing.clone(), false));
        }

        store.agents.push(agent.clone());
        file.write_json_atomic(&store)?;
        Ok((agent.clone(), true))
    }

    fn remove(&self, agent_id: Uuid) -> Result<()> {
        let mut file = LockedFile::open_exclusive(&self.storage.registry_path())?;
        let mut store: AgentStore = file.read_json()?;

        store.agents.retain(|a| a.agent_id != agent_id);
        file.write_json_atomic(&store)
    }
}
