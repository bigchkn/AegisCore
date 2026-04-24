use std::{path::PathBuf, sync::Arc};

use aegis_core::{EffectiveConfig, Result};
use aegis_providers::ProviderRegistry;
use aegis_tmux::TmuxClient;
use uuid::Uuid;

use crate::{
    commands::ControllerCommands, dispatcher::Dispatcher, events::EventBus, prompts::PromptManager,
    registry::FileRegistry, scheduler::Scheduler, state::StateManager, storage::ProjectStorage,
};

pub struct AegisRuntime {
    pub project_id: Uuid,
    pub root_path: PathBuf,
    pub config: EffectiveConfig,
    pub storage: Arc<ProjectStorage>,
    pub registry: Arc<FileRegistry>,
    pub tmux: Arc<TmuxClient>,
    pub providers: Arc<ProviderRegistry>,
    pub prompts: Arc<PromptManager>,
    pub dispatcher: Arc<Dispatcher>,
    pub scheduler: Arc<Scheduler>,
    pub state: Arc<StateManager>,
    pub events: EventBus,
}

impl AegisRuntime {
    pub async fn build(root_path: PathBuf) -> Result<Self> {
        let global = EffectiveConfig::load_global()?;
        let project = EffectiveConfig::load_project(&root_path)?;
        let config = EffectiveConfig::resolve(&global, &project)?;
        let validation_errors = config.validate();
        if let Some(error) = validation_errors.first() {
            return Err(aegis_core::AegisError::ConfigValidation {
                field: error.field.clone(),
                reason: error.reason.clone(),
            });
        }

        Self::from_config(root_path, config).await
    }

    pub async fn from_config(root_path: PathBuf, config: EffectiveConfig) -> Result<Self> {
        let storage = Arc::new(ProjectStorage::new(root_path.clone()));
        storage.ensure_layout()?;
        FileRegistry::init(storage.as_ref())?;

        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let tmux = Arc::new(TmuxClient::new());
        let providers = Arc::new(ProviderRegistry::from_config(&config)?);
        let prompts = Arc::new(PromptManager::new(root_path.clone()));
        let state = Arc::new(StateManager::new(storage.clone()));
        let events = EventBus::default();
        let dispatcher = Arc::new(Dispatcher::new(
            registry.clone(),
            Some(tmux.clone()),
            providers.clone(),
            prompts.clone(),
            storage.clone(),
            events.clone(),
            config.clone(),
        ));
        let scheduler = Arc::new(Scheduler::new(
            registry.clone(),
            dispatcher.clone(),
            config.global.max_splinters as usize,
        ));

        Ok(Self {
            project_id: Uuid::new_v4(),
            root_path,
            config,
            storage,
            registry,
            tmux,
            providers,
            prompts,
            dispatcher,
            scheduler,
            state,
            events,
        })
    }

    pub async fn load(root_path: PathBuf) -> Result<Self> {
        Self::build(root_path).await
    }

    pub async fn recover(&self) -> Result<()> {
        self.state.recover()?;
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        for (name, entry) in &self.config.agents {
            if entry.kind == aegis_core::AgentKind::Bastion {
                self.dispatcher.spawn_bastion(name).await?;
            }
        }
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.state.snapshot_now()?;
        Ok(())
    }

    pub fn commands(&self) -> ControllerCommands {
        ControllerCommands::new(
            self.registry.clone(),
            self.dispatcher.clone(),
            self.scheduler.clone(),
            None,
        )
    }

    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<aegis_core::AegisEvent> {
        self.events.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::config::{RawConfig, RawProviderConfig};
    use aegis_core::StorageBackend;
    use std::collections::HashMap;

    #[tokio::test]
    async fn runtime_from_config_initializes_storage_and_commands() {
        let dir = tempfile::tempdir().unwrap();
        let mut project = RawConfig::default();
        project.providers = HashMap::from([(
            "claude-code".to_string(),
            RawProviderConfig {
                binary: Some("claude".to_string()),
                ..Default::default()
            },
        )]);
        let config = EffectiveConfig::resolve(&RawConfig::default(), &project).unwrap();

        let runtime = AegisRuntime::from_config(dir.path().to_path_buf(), config)
            .await
            .unwrap();
        let status = runtime.commands().status().unwrap();

        assert_eq!(status.active_agents, 0);
        assert_eq!(status.pending_tasks, 0);
        assert!(runtime.storage.state_dir().is_dir());
    }
}
