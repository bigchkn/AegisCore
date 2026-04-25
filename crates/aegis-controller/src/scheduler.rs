use std::sync::Arc;

use aegis_core::{Result, TaskCreator, TaskQueue, TaskRegistry, TaskStatus};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{dispatcher::Dispatcher, registry::FileRegistry};

const DRAIN_POLL_MS: u64 = 500;

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

        if let Err(e) = self
            .dispatcher
            .spawn_splinter_with_id(agent_id, role, &task, None)
            .await
        {
            tracing::error!(
                task_id = %task.task_id,
                agent_id = %agent_id,
                role = %role,
                error = %e,
                "splinter spawn failed — releasing task back to queued"
            );
            if let Err(re) = TaskRegistry::update_status(
                self.registry.as_ref(),
                task.task_id,
                TaskStatus::Queued,
            ) {
                tracing::error!(
                    task_id = %task.task_id,
                    rollback_error = %re,
                    "failed to roll back task claim — task stuck in active"
                );
            }
            drop(permit);
            return Err(e);
        }

        drop(permit);
        Ok(Some(agent_id))
    }

    /// Runs forever, draining queued tasks as semaphore permits become available.
    /// Spawn this as a background task from `AegisRuntime::start`.
    pub async fn run_drain_loop(self: Arc<Self>, role: &str) {
        loop {
            match self.dispatch_once(role).await {
                Ok(Some(agent_id)) => {
                    tracing::info!(%agent_id, "drain loop dispatched queued task");
                    // immediately try the next one — there may be more queued tasks
                    // and a permit available
                    continue;
                }
                Ok(None) => {
                    // Either no queued tasks or semaphore is full. Sleep briefly
                    // before checking again so we pick up newly enqueued tasks.
                    tokio::time::sleep(tokio::time::Duration::from_millis(DRAIN_POLL_MS)).await;
                }
                Err(e) => {
                    tracing::error!(error = %e, "drain loop dispatch error");
                    tokio::time::sleep(tokio::time::Duration::from_millis(DRAIN_POLL_MS)).await;
                }
            }
        }
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
            None,
            None,
            None,
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
