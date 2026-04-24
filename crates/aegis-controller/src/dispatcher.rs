use std::{path::PathBuf, sync::Arc};

use aegis_core::{
    config::AgentEntry, AegisError, AegisEvent, Agent, AgentKind, AgentRegistry, AgentStatus,
    Recorder, Result, SandboxProfile, StorageBackend, Task, TaskRegistry, TaskStatus,
};
use aegis_providers::ProviderRegistry;
use aegis_tmux::{TmuxClient, TmuxTarget};
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    events::EventBus,
    git::GitWorktree,
    lifecycle::{sandbox_policy_from_config, validate_transition, AgentSpec, SpawnPlan},
    prompts::{PromptContext, PromptManager, PromptType},
    registry::FileRegistry,
    storage::ProjectStorage,
};

pub struct Dispatcher {
    registry: Arc<FileRegistry>,
    tmux: Option<Arc<TmuxClient>>,
    sandbox: Option<Arc<dyn SandboxProfile>>,
    recorder: Option<Arc<dyn Recorder>>,
    providers: Arc<ProviderRegistry>,
    prompts: Arc<PromptManager>,
    storage: Arc<ProjectStorage>,
    events: EventBus,
    config: aegis_core::EffectiveConfig,
}

impl Dispatcher {
    pub fn new(
        registry: Arc<FileRegistry>,
        tmux: Option<Arc<TmuxClient>>,
        sandbox: Option<Arc<dyn SandboxProfile>>,
        recorder: Option<Arc<dyn Recorder>>,
        providers: Arc<ProviderRegistry>,
        prompts: Arc<PromptManager>,
        storage: Arc<ProjectStorage>,
        events: EventBus,
        config: aegis_core::EffectiveConfig,
    ) -> Self {
        Self {
            registry,
            tmux,
            sandbox,
            recorder,
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
        let mut launch_command = Vec::new();
        if let Some(sandbox) = &self.sandbox {
            launch_command.extend(sandbox.exec_prefix(&sandbox_profile));
        }
        launch_command.extend(command_parts(&provider_command));

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
        let mut initial_prompt = self.prompts.resolve_prompt(
            prompt_type,
            &prompt_context,
            spec.system_prompt.as_deref(),
        )?;

        // Taskflow awareness injection (Task 13.7).
        let taskflow_snippet = r#"
### Project Context & Navigation
You are operating within an AegisCore autonomous environment. To understand your place in the broader project roadmap, use the following tools:
- Run `aegis taskflow status` to see the overall project health.
- Run `aegis taskflow show <M-ID>` (e.g., M13) to see the specific tasks and design goals for your current milestone.
- Read design documents directly at `.aegis/designs/` for deep technical context (Read-Only).
"#;
        initial_prompt.push_str(taskflow_snippet);

        Ok(SpawnPlan {
            agent,
            provider_command,
            launch_command,
            initial_prompt,
            sandbox_policy: spec.sandbox,
        })
    }

    pub async fn spawn_bastion(&self, name: &str) -> Result<Agent> {
        let spec = self.build_bastion_spec(name)?;
        let agent_id = Uuid::new_v4();
        let (window, pane) = self.prepare_tmux_window(agent_id, name).await?;
        let plan = self.build_spawn_plan(spec, agent_id, window, pane)?;
        self.launch_or_insert_plan(plan).await
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
        self.prepare_splinter_worktree(agent_id, role).await?;
        let window_name = format!("splinter-{role}-{}", short_id(agent_id));
        let (window, pane) = self.prepare_tmux_window(agent_id, &window_name).await?;
        let plan = self.build_spawn_plan(spec, agent_id, window, pane)?;
        TaskRegistry::assign(self.registry.as_ref(), task.task_id, agent_id)?;
        self.launch_or_insert_plan(plan).await
    }

    async fn prepare_tmux_window(
        &self,
        agent_id: Uuid,
        window_name: &str,
    ) -> Result<(u32, String)> {
        let Some(tmux) = &self.tmux else {
            return Ok((0, "%0".to_string()));
        };

        let session = &self.config.global.tmux_session_name;
        if !tmux.session_exists(session).await? {
            tmux.new_session(session).await?;
        }

        let window = tmux.new_window(session, Some(window_name)).await?;
        let target = TmuxTarget::new(session, window, "%0");
        let pane = tmux
            .list_panes(&target)
            .await?
            .into_iter()
            .next()
            .unwrap_or_else(|| "%0".to_string());

        tracing::debug!(%agent_id, session, window, pane, "prepared tmux window");
        Ok((window, pane))
    }

    async fn prepare_splinter_worktree(&self, agent_id: Uuid, role: &str) -> Result<()> {
        if self.tmux.is_some() {
            let git = GitWorktree::new(
                self.storage.project_root().to_path_buf(),
                self.storage.worktrees_dir(),
            );
            git.create_for_agent(agent_id, role).await?;
            return Ok(());
        }

        let worktree = self.storage.agent_worktree_path(agent_id);
        std::fs::create_dir_all(&worktree).map_err(|source| AegisError::StorageIo {
            path: worktree,
            source,
        })
    }

    async fn launch_or_insert_plan(&self, plan: SpawnPlan) -> Result<Agent> {
        self.write_sandbox_profile(&plan)?;

        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&plan.agent.tmux_target())?;
            tmux.send_text(&target, &shell_command(&plan.launch_command))
                .await?;
            let agent = self.insert_starting_agent(plan.agent)?;
            self.attach_recorder(&agent)?;
            let agent = self.activate_agent(agent)?;
            tmux.send_text(&target, &plan.initial_prompt).await?;
            Ok(agent)
        } else {
            let agent = self.insert_starting_agent(plan.agent)?;
            self.attach_recorder(&agent)?;
            self.activate_agent(agent)
        }
    }

    fn write_sandbox_profile(&self, plan: &SpawnPlan) -> Result<()> {
        let Some(sandbox) = &self.sandbox else {
            return Ok(());
        };
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| self.storage.project_root().to_path_buf());
        sandbox.write(
            &plan.agent.worktree_path,
            &home,
            &plan.sandbox_policy,
            &plan.agent.sandbox_profile,
        )
    }

    fn insert_starting_agent(&self, agent: Agent) -> Result<Agent> {
        AgentRegistry::insert(self.registry.as_ref(), &agent)?;
        Ok(agent)
    }

    fn attach_recorder(&self, agent: &Agent) -> Result<()> {
        if let Some(recorder) = &self.recorder {
            recorder.attach(agent)?;
        }
        Ok(())
    }

    fn activate_agent(&self, mut agent: Agent) -> Result<Agent> {
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
        let agent = self.require_agent(agent_id)?;
        self.ensure_transition(&agent.status, &AgentStatus::Paused)?;
        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&agent.tmux_target())?;
            tmux.interrupt(&target).await?;
        }
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Paused)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status: agent.status,
            new_status: AgentStatus::Paused,
        });
        Ok(())
    }

    pub async fn resume_agent(&self, agent_id: Uuid) -> Result<()> {
        let agent = self.require_agent(agent_id)?;
        self.ensure_transition(&agent.status, &AgentStatus::Active)?;
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Active)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status: agent.status,
            new_status: AgentStatus::Active,
        });
        Ok(())
    }

    pub async fn kill_agent(&self, agent_id: Uuid) -> Result<()> {
        let agent = self.require_agent(agent_id)?;
        self.detach_and_archive(agent_id)?;
        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&agent.tmux_target())?;
            let _ = tmux.kill_window(&target).await;
        }
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Terminated)?;
        AgentRegistry::archive(self.registry.as_ref(), agent_id)
    }

    pub async fn process_receipt(&self, agent_id: Uuid) -> Result<()> {
        let agent = self.require_agent(agent_id)?;
        self.ensure_transition(&agent.status, &AgentStatus::Reporting)?;
        let Some(task_id) = agent.task_id else {
            return Err(AegisError::Config {
                field: "agent.task_id".to_string(),
                reason: "receipt processing requires an assigned task".to_string(),
            });
        };

        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Reporting)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status: agent.status.clone(),
            new_status: AgentStatus::Reporting,
        });

        let receipt_path = self.receipt_path(task_id);
        let receipt_result = self.validate_receipt(task_id, &receipt_path);
        match receipt_result {
            Ok(()) => {
                TaskRegistry::complete(
                    self.registry.as_ref(),
                    task_id,
                    Some(receipt_path.clone()),
                )?;
                self.detach_and_archive(agent_id)?;
                if let Some(tmux) = &self.tmux {
                    let target = TmuxTarget::parse(&agent.tmux_target())?;
                    let _ = tmux.kill_window(&target).await;
                }
                if agent.kind == AgentKind::Splinter && self.config.splinter_defaults.auto_cleanup {
                    let git = GitWorktree::new(
                        self.storage.project_root().to_path_buf(),
                        self.storage.worktrees_dir(),
                    );
                    let _ = git.prune_for_agent(agent_id).await;
                }
                AgentRegistry::update_status(
                    self.registry.as_ref(),
                    agent_id,
                    AgentStatus::Terminated,
                )?;
                AgentRegistry::archive(self.registry.as_ref(), agent_id)?;
                self.events.publish(AegisEvent::AgentStatusChanged {
                    agent_id,
                    old_status: AgentStatus::Reporting,
                    new_status: AgentStatus::Terminated,
                });
                self.events.publish(AegisEvent::TaskComplete {
                    task_id,
                    receipt_path: receipt_path.to_string_lossy().into_owned(),
                });
                Ok(())
            }
            Err(error) => {
                let _ = TaskRegistry::update_status(
                    self.registry.as_ref(),
                    task_id,
                    TaskStatus::Failed,
                );
                let _ = AgentRegistry::update_status(
                    self.registry.as_ref(),
                    agent_id,
                    AgentStatus::Failed,
                );
                self.events.publish(AegisEvent::AgentStatusChanged {
                    agent_id,
                    old_status: AgentStatus::Reporting,
                    new_status: AgentStatus::Failed,
                });
                Err(error)
            }
        }
    }

    fn receipt_path(&self, task_id: Uuid) -> PathBuf {
        self.storage
            .handoff_dir()
            .join(task_id.to_string())
            .join("receipt.json")
    }

    fn validate_receipt(&self, task_id: Uuid, path: &std::path::Path) -> Result<()> {
        if !path.exists() {
            return Err(AegisError::ReceiptNotFound {
                task_id,
                path: path.to_path_buf(),
            });
        }

        let bytes = std::fs::read(path).map_err(|source| AegisError::StorageIo {
            path: path.to_path_buf(),
            source,
        })?;
        let receipt: CompletionReceipt =
            serde_json::from_slice(&bytes).map_err(|source| AegisError::ReceiptInvalid {
                path: path.to_path_buf(),
                reason: source.to_string(),
            })?;

        if receipt.task_id != task_id {
            return Err(AegisError::ReceiptInvalid {
                path: path.to_path_buf(),
                reason: format!(
                    "receipt task_id {} does not match expected task_id {task_id}",
                    receipt.task_id
                ),
            });
        }

        Ok(())
    }

    fn require_agent(&self, agent_id: Uuid) -> Result<Agent> {
        AgentRegistry::get(self.registry.as_ref(), agent_id)?
            .ok_or(AegisError::AgentNotFound { agent_id })
    }

    fn ensure_transition(&self, from: &AgentStatus, to: &AgentStatus) -> Result<()> {
        if validate_transition(from, to) {
            return Ok(());
        }

        Err(AegisError::Config {
            field: "agent.status".to_string(),
            reason: format!("invalid transition from {from:?} to {to:?}"),
        })
    }

    fn detach_and_archive(&self, agent_id: Uuid) -> Result<()> {
        let Some(recorder) = &self.recorder else {
            return Ok(());
        };

        recorder.detach(agent_id)?;
        match recorder.archive(agent_id) {
            Ok(_) | Err(AegisError::LogFileNotFound { .. }) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CompletionReceipt {
    task_id: Uuid,
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

fn shell_command(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'-' | b':' | b'='))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn short_id(id: Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{
        config::{
            RawAgentConfig, RawConfig, RawGlobalConfig, RawProviderConfig, RawSplinterDefaults,
        },
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
        project.splinter_defaults = Some(RawSplinterDefaults {
            cli_provider: Some("claude-code".to_string()),
            auto_cleanup: Some(false),
            ..Default::default()
        });
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
            None,
            None,
            None,
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

    #[tokio::test]
    async fn process_receipt_completes_task_and_archives_agent() {
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
        dispatcher
            .spawn_splinter_with_id(agent_id, "worker", &task, None)
            .await
            .unwrap();

        let receipt_path = dispatcher.receipt_path(task.task_id);
        std::fs::create_dir_all(receipt_path.parent().unwrap()).unwrap();
        std::fs::write(
            &receipt_path,
            serde_json::json!({ "task_id": task.task_id }).to_string(),
        )
        .unwrap();

        dispatcher.process_receipt(agent_id).await.unwrap();

        let stored_task = TaskRegistry::get(dispatcher.registry.as_ref(), task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_task.status, TaskStatus::Complete);
        assert_eq!(stored_task.receipt_path, Some(receipt_path.clone()));
        let stored_agent = AgentRegistry::get(dispatcher.registry.as_ref(), agent_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_agent.status, AgentStatus::Terminated);
        assert!(stored_agent.terminated_at.is_some());
    }

    #[tokio::test]
    async fn process_receipt_missing_receipt_marks_task_and_agent_failed() {
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

        let error = dispatcher.process_receipt(agent_id).await.unwrap_err();

        assert!(matches!(error, AegisError::ReceiptNotFound { .. }));
        let stored_task = TaskRegistry::get(dispatcher.registry.as_ref(), task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_task.status, TaskStatus::Failed);
        let stored_agent = AgentRegistry::get(dispatcher.registry.as_ref(), agent_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_agent.status, AgentStatus::Failed);
        assert!(agent.worktree_path.exists());
    }
}
