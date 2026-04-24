use aegis_core::{LockedFile, Result, StorageBackend};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskflowStore {
    pub version: u32,
    pub links: HashMap<String, Uuid>,
}

pub struct TaskflowLinkRegistry {
    storage: std::sync::Arc<dyn StorageBackend>,
}

impl TaskflowLinkRegistry {
    pub fn new(storage: std::sync::Arc<dyn StorageBackend>) -> Self {
        Self { storage }
    }

    fn lock_exclusive(&self) -> Result<LockedFile> {
        LockedFile::open_exclusive(&self.storage.taskflow_path())
    }

    fn lock_shared(&self) -> Result<LockedFile> {
        LockedFile::open_shared(&self.storage.taskflow_path())
    }

    pub fn assign(&self, roadmap_id: String, task_id: Uuid) -> Result<()> {
        let mut lock = self.lock_exclusive()?;
        let mut store: TaskflowStore = lock.read_json()?;
        store.links.insert(roadmap_id, task_id);
        if store.version == 0 {
            store.version = 1;
        }
        lock.write_json_atomic(&store)?;
        Ok(())
    }

    pub fn get(&self, roadmap_id: &str) -> Result<Option<Uuid>> {
        let mut lock = self.lock_shared()?;
        let store: TaskflowStore = lock.read_json()?;
        Ok(store.links.get(roadmap_id).cloned())
    }

    pub fn list_all(&self) -> Result<HashMap<String, Uuid>> {
        let mut lock = self.lock_shared()?;
        let store: TaskflowStore = lock.read_json()?;
        Ok(store.links)
    }
}
