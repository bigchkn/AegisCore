pub mod model;
pub mod parser;
pub mod state;
pub mod sync;
pub mod view;

use crate::state::TaskflowLinkRegistry;
use aegis_core::{StorageBackend, TaskRegistry};
use std::sync::Arc;
pub use sync::{NextMilestoneOutcome, SyncReport};

pub struct TaskflowEngine {
    storage: Arc<dyn StorageBackend>,
    registry: Arc<dyn TaskRegistry>,
    links: TaskflowLinkRegistry,
}

impl TaskflowEngine {
    pub fn new(storage: Arc<dyn StorageBackend>, registry: Arc<dyn TaskRegistry>) -> Self {
        let links = TaskflowLinkRegistry::new(storage.clone());
        Self {
            storage,
            registry,
            links,
        }
    }

    pub fn storage(&self) -> &dyn StorageBackend {
        self.storage.as_ref()
    }

    pub fn registry(&self) -> &dyn TaskRegistry {
        self.registry.as_ref()
    }

    pub fn links(&self) -> &TaskflowLinkRegistry {
        &self.links
    }
}
