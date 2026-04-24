use std::sync::Arc;

use aegis_core::{Result, TaskCreator, TaskQueue};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{dispatcher::Dispatcher, registry::FileRegistry};

pub struct Scheduler {
    registry: Arc<FileRegistry>,
    dispatcher: Arc<Dispatcher>,
    permits: Arc<Semaphore>,
}

impl Scheduler {
    pub fn new(
        registry: Arc<FileRegistry>,
        dispatcher: Arc<Dispatcher>,
        max_splinters: usize,
    ) -> Self {
        Self {
            registry,
            dispatcher,
            permits: Arc::new(Semaphore::new(max_splinters.max(1))),
        }
    }

    pub fn enqueue_splinter_task(
        &self,
        description: &str,
        created_by: TaskCreator,
    ) -> Result<Uuid> {
        TaskQueue::enqueue(self.registry.as_ref(), description, created_by)
    }

    pub async fn dispatch_once(&self, role: &str) -> Result<Option<Uuid>> {
        let permit = match self.permits.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => return Ok(None),
        };

        let agent_id = Uuid::new_v4();
        let Some(task) = TaskQueue::claim_next(self.registry.as_ref(), agent_id)? else {
            drop(permit);
            return Ok(None);
        };

        self.dispatcher
            .spawn_splinter_with_id(agent_id, role, &task, None)
            .await?;

        // Current lifecycle is synchronous and registry-backed. Keep the permit
        // until the task is spawned, then release it for deterministic tests.
        drop(permit);
        Ok(Some(agent_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        dispatcher::Dispatcher, events::EventBus, prompts::PromptManager, storage::ProjectStorage,
    };
    use aegis_core::{
        config::{RawConfig, RawProviderConfig},
        EffectiveConfig,
    };
    use aegis_providers::ProviderRegistry;
    use std::{collections::HashMap, sync::Arc};

    #[tokio::test]
    async fn dispatch_once_claims_one_task() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(ProjectStorage::new(dir.path().to_path_buf()));
        storage.ensure_layout().unwrap();
        FileRegistry::init(storage.as_ref()).unwrap();

        let mut project = RawConfig::default();
        project.providers = HashMap::from([(
            "claude-code".to_string(),
            RawProviderConfig {
                binary: Some("claude".to_string()),
                ..Default::default()
            },
        )]);
        let config = EffectiveConfig::resolve(&RawConfig::default(), &project).unwrap();
        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let providers = Arc::new(ProviderRegistry::from_config(&config).unwrap());
        let prompts = Arc::new(PromptManager::new(dir.path().to_path_buf()));
        let dispatcher = Arc::new(Dispatcher::new(
            registry.clone(),
            providers,
            prompts,
            storage,
            EventBus::default(),
            config,
        ));
        let scheduler = Scheduler::new(registry.clone(), dispatcher, 1);

        scheduler
            .enqueue_splinter_task("do the thing", aegis_core::TaskCreator::System)
            .unwrap();
        let agent_id = scheduler.dispatch_once("worker").await.unwrap();

        assert!(agent_id.is_some());
        assert_eq!(TaskQueue::pending_count(registry.as_ref()).unwrap(), 0);
    }
}
