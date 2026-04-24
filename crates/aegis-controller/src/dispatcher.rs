use std::sync::Arc;

use aegis_core::{
    config::AgentEntry, AegisError, AegisEvent, Agent, AgentKind, AgentRegistry, AgentStatus,
    Result, StorageBackend, Task, TaskRegistry,
};
use aegis_providers::ProviderRegistry;
use chrono::Utc;
use uuid::Uuid;

use crate::{
    events::EventBus,
    lifecycle::{sandbox_policy_from_config, AgentSpec, SpawnPlan},
    prompts::{PromptContext, PromptManager, PromptType},
    registry::FileRegistry,
    storage::ProjectStorage,
};

pub struct Dispatcher {
    registry: Arc<FileRegistry>,
    providers: Arc<ProviderRegistry>,
    prompts: Arc<PromptManager>,
    storage: Arc<ProjectStorage>,
    events: EventBus,
    config: aegis_core::EffectiveConfig,
}

impl Dispatcher {
    pub fn new(
        registry: Arc<FileRegistry>,
        providers: Arc<ProviderRegistry>,
        prompts: Arc<PromptManager>,
        storage: Arc<ProjectStorage>,
        events: EventBus,
        config: aegis_core::EffectiveConfig,
    ) -> Self {
        Self {
            registry,
            providers,
            prompts,
            storage,
            events,
            config,
        }
    }

    pub fn build_bastion_spec(&self, name: &str) -> Result<AgentSpec> {
        let entry = self
            .config
            .agents
            .get(name)
            .ok_or_else(|| AegisError::Config {
                field: format!("agent.{name}"),
                reason: "configured agent not found".to_string(),
            })?;

        if entry.kind != AgentKind::Bastion {
            return Err(AegisError::Config {
                field: format!("agent.{name}.type"),
                reason: "expected bastion".to_string(),
            });
        }

        Ok(spec_from_agent_entry(name, entry, None, None, None))
    }

    pub fn build_splinter_spec(
        &self,
        role: &str,
        task: &Task,
        parent_id: Option<Uuid>,
    ) -> AgentSpec {
        AgentSpec {
            name: format!("splinter-{role}-{}", short_id(task.task_id)),
            kind: AgentKind::Splinter,
            role: role.to_string(),
            parent_id,
            task_id: Some(task.task_id),
            task_description: Some(task.description.clone()),
            cli_provider: self.config.splinter_defaults.cli_provider.clone(),
            fallback_cascade: self.config.splinter_defaults.fallback_cascade.clone(),
            system_prompt: None,
            sandbox: sandbox_policy_from_config(&self.config.sandbox_defaults),
            auto_cleanup: self.config.splinter_defaults.auto_cleanup,
        }
    }

    pub fn build_spawn_plan(
        &self,
        spec: AgentSpec,
        agent_id: Uuid,
        tmux_window: u32,
        tmux_pane: impl Into<String>,
    ) -> Result<SpawnPlan> {
        let worktree_path = match spec.kind {
            AgentKind::Bastion => self.storage.project_root().to_path_buf(),
            AgentKind::Splinter => self.storage.agent_worktree_path(agent_id),
        };
        let sandbox_profile = self.storage.sandbox_profile_path(agent_id);
        let log_path = self.storage.agent_log_path(agent_id);

        let provider = self.providers.get(&spec.cli_provider)?;
        let provider_command = provider.spawn_command(&worktree_path, None);
        let launch_command = command_parts(&provider_command);

        let now = Utc::now();
        let agent = Agent {
            agent_id,
            name: spec.name.clone(),
            kind: spec.kind.clone(),
            status: AgentStatus::Starting,
            role: spec.role.clone(),
            parent_id: spec.parent_id,
            task_id: spec.task_id,
            tmux_session: self.config.global.tmux_session_name.clone(),
            tmux_window,
            tmux_pane: tmux_pane.into(),
            worktree_path: worktree_path.clone(),
            cli_provider: spec.cli_provider.clone(),
            fallback_cascade: spec.fallback_cascade.clone(),
            sandbox_profile,
            log_path,
            created_at: now,
            updated_at: now,
            terminated_at: None,
        };

        let prompt_context = PromptContext {
            agent_id,
            role: spec.role,
            task_id: spec.task_id,
            task_description: spec.task_description,
            context_snippet: None,
            worktree_path,
            previous_cli: None,
        };

        let prompt_type = match spec.kind {
            AgentKind::Bastion => PromptType::System,
            AgentKind::Splinter => PromptType::Task,
        };
        let initial_prompt = self.prompts.resolve_prompt(
            prompt_type,
            &prompt_context,
            spec.system_prompt.as_deref(),
        )?;

        Ok(SpawnPlan {
            agent,
            provider_command,
            launch_command,
            initial_prompt,
        })
    }

    pub async fn spawn_bastion(&self, name: &str) -> Result<Agent> {
        let spec = self.build_bastion_spec(name)?;
        let plan = self.build_spawn_plan(spec, Uuid::new_v4(), 0, "%0")?;
        self.insert_planned_agent(plan.agent)
    }

    pub async fn spawn_splinter(
        &self,
        role: &str,
        task: &Task,
        parent_id: Option<Uuid>,
    ) -> Result<Agent> {
        self.spawn_splinter_with_id(Uuid::new_v4(), role, task, parent_id)
            .await
    }

    pub async fn spawn_splinter_with_id(
        &self,
        agent_id: Uuid,
        role: &str,
        task: &Task,
        parent_id: Option<Uuid>,
    ) -> Result<Agent> {
        let spec = self.build_splinter_spec(role, task, parent_id);
        let plan = self.build_spawn_plan(spec, agent_id, 0, "%0")?;
        TaskRegistry::assign(self.registry.as_ref(), task.task_id, agent_id)?;
        self.insert_planned_agent(plan.agent)
    }

    fn insert_planned_agent(&self, mut agent: Agent) -> Result<Agent> {
        AgentRegistry::insert(self.registry.as_ref(), &agent)?;
        let old_status = agent.status.clone();
        agent.status = AgentStatus::Active;
        agent.updated_at = Utc::now();
        AgentRegistry::update(self.registry.as_ref(), &agent)?;

        self.events.publish(AegisEvent::AgentSpawned {
            agent_id: agent.agent_id,
            role: agent.role.clone(),
        });
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id: agent.agent_id,
            old_status,
            new_status: AgentStatus::Active,
        });

        Ok(agent)
    }

    pub async fn pause_agent(&self, agent_id: Uuid) -> Result<()> {
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Paused)
    }

    pub async fn resume_agent(&self, agent_id: Uuid) -> Result<()> {
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Active)
    }

    pub async fn kill_agent(&self, agent_id: Uuid) -> Result<()> {
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Terminated)?;
        AgentRegistry::archive(self.registry.as_ref(), agent_id)
    }
}

fn spec_from_agent_entry(
    name: &str,
    entry: &AgentEntry,
    parent_id: Option<Uuid>,
    task_id: Option<Uuid>,
    task_description: Option<String>,
) -> AgentSpec {
    AgentSpec {
        name: name.to_string(),
        kind: entry.kind.clone(),
        role: entry.role.clone(),
        parent_id,
        task_id,
        task_description,
        cli_provider: entry.cli_provider.clone(),
        fallback_cascade: entry.fallback_cascade.clone(),
        system_prompt: entry.system_prompt.clone(),
        sandbox: sandbox_policy_from_config(&entry.sandbox),
        auto_cleanup: entry.auto_cleanup,
    }
}

fn command_parts(command: &std::process::Command) -> Vec<String> {
    std::iter::once(command.get_program().to_string_lossy().into_owned())
        .chain(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect()
}

fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{
        config::{RawAgentConfig, RawConfig, RawGlobalConfig, RawProviderConfig},
        TaskCreator, TaskStatus,
    };
    use std::{collections::HashMap, sync::Arc};

    fn dispatcher() -> (Dispatcher, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(ProjectStorage::new(dir.path().to_path_buf()));
        storage.ensure_layout().unwrap();
        FileRegistry::init(storage.as_ref()).unwrap();

        let mut project = RawConfig::default();
        project.global = Some(RawGlobalConfig {
            tmux_session_name: Some("aegis-test".into()),
            ..Default::default()
        });
        project.providers = HashMap::from([(
            "claude-code".to_string(),
            RawProviderConfig {
                binary: Some("claude".to_string()),
                ..Default::default()
            },
        )]);
        project.agent = HashMap::from([(
            "architect".to_string(),
            RawAgentConfig {
                kind: Some("bastion".to_string()),
                role: Some("architect".to_string()),
                cli_provider: Some("claude-code".to_string()),
                ..Default::default()
            },
        )]);

        let config = aegis_core::EffectiveConfig::resolve(&RawConfig::default(), &project).unwrap();
        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let providers = Arc::new(ProviderRegistry::from_config(&config).unwrap());
        let prompts = Arc::new(PromptManager::new(dir.path().to_path_buf()));
        let dispatcher = Dispatcher::new(
            registry,
            providers,
            prompts,
            storage,
            EventBus::default(),
            config,
        );
        (dispatcher, dir)
    }

    #[test]
    fn build_spawn_plan_bastion_sets_paths_and_command() {
        let (dispatcher, dir) = dispatcher();
        let spec = dispatcher.build_bastion_spec("architect").unwrap();
        let agent_id = Uuid::nil();
        let plan = dispatcher
            .build_spawn_plan(spec, agent_id, 3, "%9")
            .unwrap();

        assert_eq!(plan.agent.kind, AgentKind::Bastion);
        assert_eq!(plan.agent.status, AgentStatus::Starting);
        assert_eq!(plan.agent.tmux_session, "aegis-test");
        assert_eq!(plan.agent.tmux_window, 3);
        assert_eq!(plan.agent.tmux_pane, "%9");
        assert_eq!(plan.agent.worktree_path, dir.path());
        assert!(plan.agent.log_path.ends_with(format!("{agent_id}.log")));
        assert!(plan.launch_command.contains(&"claude".to_string()));
        assert!(plan.initial_prompt.contains("architect"));
    }

    #[tokio::test]
    async fn spawn_splinter_assigns_task_and_inserts_active_agent() {
        let (dispatcher, _dir) = dispatcher();
        let task = Task {
            task_id: Uuid::new_v4(),
            description: "write tests".to_string(),
            status: TaskStatus::Queued,
            assigned_agent_id: None,
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        TaskRegistry::insert(dispatcher.registry.as_ref(), &task).unwrap();

        let agent_id = Uuid::new_v4();
        let agent = dispatcher
            .spawn_splinter_with_id(agent_id, "worker", &task, None)
            .await
            .unwrap();

        assert_eq!(agent.agent_id, agent_id);
        assert_eq!(agent.kind, AgentKind::Splinter);
        assert_eq!(agent.status, AgentStatus::Active);
        let stored = TaskRegistry::get(dispatcher.registry.as_ref(), task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.assigned_agent_id, Some(agent_id));
        assert_eq!(stored.status, TaskStatus::Active);
    }
}
