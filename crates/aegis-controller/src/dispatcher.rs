use std::{path::PathBuf, sync::Arc};

use aegis_core::{
    config::AgentEntry, AegisError, AegisEvent, Agent, AgentKind, AgentRegistry, AgentStatus,
    FailoverContext, InteractionModel, LogQuery, Recorder, Result, SandboxPolicy, SandboxProfile,
    StorageBackend, Task, TaskCreator, TaskRegistry, TaskStatus,
};
use aegis_design::{RenderedTemplate, TemplateKind};
use aegis_providers::ProviderRegistry;
use aegis_tmux::{TmuxClient, TmuxTarget};
use aegis_watchdog::FailoverExecutor;
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use crate::{
    daemon::projects::ProjectRegistry,
    events::EventBus,
    git::GitWorktree,
    lifecycle::{sandbox_policy_from_config, validate_transition, AgentSpec, SpawnPlan},
    prompts::{PromptContext, PromptManager, PromptType},
    registry::FileRegistry,
    storage::ProjectStorage,
    transcript::append_tmux_send,
};

// TODO: Fix sandbox launch/auth behavior and re-enable sandbox execution.
const DISABLE_AGENT_SANDBOX: bool = true;

pub struct Dispatcher {
    registry: Arc<FileRegistry>,
    project_registry: Option<Arc<ProjectRegistry>>,
    project_id: Option<Uuid>,
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
        project_registry: Option<Arc<ProjectRegistry>>,
        project_id: Option<Uuid>,
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
            project_registry,
            project_id,
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
            inline_prompt: None,
            worktree_override: None,
            sandbox: sandbox_policy_from_config(&self.config.sandbox_defaults),
            auto_cleanup: self.config.splinter_defaults.auto_cleanup,
            model_override: self.config.splinter_defaults.model.clone(),
            resume_session: None,
        }
    }

    pub fn build_spawn_plan(
        &self,
        mut spec: AgentSpec,
        agent_id: Uuid,
        tmux_session: String,
        tmux_window: u32,
        tmux_pane: String,
    ) -> Result<SpawnPlan> {
        let worktree_path = if let Some(override_path) = spec.worktree_override.take() {
            override_path
        } else {
            match spec.kind {
                AgentKind::Bastion => self.storage.project_root().to_path_buf(),
                AgentKind::Splinter => self.storage.agent_worktree_path(agent_id),
            }
        };
        let sandbox_profile = self.storage.sandbox_profile_path(agent_id);
        let log_path = self.storage.agent_log_path(agent_id);

        let provider = self.providers.get(&spec.cli_provider)?;
        let provider_command = provider.spawn_command(
            &worktree_path,
            spec.resume_session.as_ref(),
            spec.model_override.as_deref(),
        );

        // Auto-add the provider binary's parent directory to sandbox exec paths so the
        // binary can be launched regardless of where the user installed it.
        if let Some(exec_dir) = resolve_binary_exec_dir(&provider.config().binary) {
            if !spec.sandbox.extra_exec_paths.contains(&exec_dir) {
                spec.sandbox.extra_exec_paths.push(exec_dir);
            }
        }

        let mut launch_command = Vec::new();
        if !DISABLE_AGENT_SANDBOX {
            if let Some(sandbox) = &self.sandbox {
                launch_command.extend(sandbox.exec_prefix(&sandbox_profile));
            }
        }
        launch_command.extend(resolved_command_parts(
            &provider_command,
            &provider.config().binary,
        ));

        let now = Utc::now();
        let agent = Agent {
            agent_id,
            name: spec.name.clone(),
            kind: spec.kind.clone(),
            status: AgentStatus::Starting,
            role: spec.role.clone(),
            parent_id: spec.parent_id,
            task_id: spec.task_id,
            tmux_session,
            tmux_window,
            tmux_pane,
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

        let initial_prompt = if let Some(inline) = spec.inline_prompt.take() {
            // Template-based agent: use the fully rendered prompt as-is.
            inline
        } else {
            let prompt_type = match spec.kind {
                AgentKind::Bastion => PromptType::System,
                AgentKind::Splinter => PromptType::Task,
            };
            let mut p = self.prompts.resolve_prompt(
                prompt_type,
                &prompt_context,
                spec.system_prompt.as_deref(),
            )?;
            // Taskflow context hint for non-template agents.
            p.push_str("\n### Project Context & Navigation\nYou are operating within an AegisCore autonomous environment. To understand your place in the broader project roadmap, use the following tools:\n- Run `aegis taskflow status` to see the overall project health.\n- Run `aegis taskflow show <M-ID>` (e.g., M13) to see the specific tasks and design goals for your current milestone.\n- Read design documents directly at `.aegis/designs/` for deep technical context (Read-Only).\n");
            p
        };

        Ok(SpawnPlan {
            agent,
            provider_command,
            launch_command,
            initial_prompt,
            sandbox_policy: spec.sandbox,
            startup_delay_ms: provider.config().startup_delay_ms,
            is_resume: spec.resume_session.is_some(),
        })
    }

    pub async fn spawn_bastion(&self, name: &str) -> Result<Agent> {
        let active = self.registry.list_active()?;
        if let Some(existing) = active
            .into_iter()
            .find(|a| a.kind == AgentKind::Bastion && a.role == name)
        {
            if let Some(tmux) = &self.tmux {
                let target = TmuxTarget::parse(&existing.tmux_target())?;
                if tmux.pane_has_agent(&target).await.unwrap_or(false) {
                    tracing::info!(agent_id = %existing.agent_id, role = %name, "re-using existing active bastion");
                    return Ok(existing);
                }
            }
            // Pane is dead (daemon restarted or process exited). Restart in-place:
            // reuse the registry identity and tmux session, but seed the normal
            // bastion prompt instead of resuming a potentially broken provider session.
            tracing::info!(agent_id = %existing.agent_id, role = %name, "restarting bastion in-place with fresh prompt");
            return self.restart_bastion_in_place(name, existing).await;
        }

        let mut spec = self.build_bastion_spec(name)?;
        spec.resume_session = None;
        let agent_id = Uuid::new_v4();
        let (session, window, pane) = self
            .prepare_tmux_window(agent_id, AgentKind::Bastion, name)
            .await?;
        let plan = self.build_spawn_plan(spec, agent_id, session, window, pane)?;
        self.launch_or_insert_plan(plan).await
    }

    /// Restart an existing bastion agent in-place: kill the old tmux session, open a fresh one
    /// with the same session name, and launch it like a new bastion. The agent_id and registry
    /// record are preserved so inboxes and references keep working.
    async fn restart_bastion_in_place(&self, name: &str, agent: Agent) -> Result<Agent> {
        let Some(tmux) = &self.tmux else {
            return Ok(agent);
        };

        // Kill the stale session and open a fresh one with the same name.
        if tmux.session_exists(&agent.tmux_session).await? {
            tmux.kill_session(&agent.tmux_session).await?;
        }
        tmux.new_session(&agent.tmux_session).await?;
        let window_target = TmuxTarget::parse(&format!("{}:", agent.tmux_session))?;
        let pane = tmux
            .list_panes(&window_target)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AegisError::TmuxPaneNotFound {
                target: agent.tmux_session.clone(),
            })?;
        let pane_target = TmuxTarget::parse(&pane)?;
        tmux.harden_pane(&pane_target).await?;
        let plan = self.build_bastion_restart_plan(name, &agent, pane)?;
        self.launch_existing_plan(plan).await
    }

    fn build_bastion_restart_plan(
        &self,
        name: &str,
        existing: &Agent,
        pane: String,
    ) -> Result<SpawnPlan> {
        let mut spec = self.build_bastion_spec(name)?;
        spec.resume_session = None;
        let mut plan = self.build_spawn_plan(
            spec,
            existing.agent_id,
            existing.tmux_session.clone(),
            0,
            pane,
        )?;
        plan.agent.created_at = existing.created_at;
        Ok(plan)
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
        tracing::info!(%agent_id, task_id = %task.task_id, %role, "spawning splinter");
        let spec = self.build_splinter_spec(role, task, parent_id);

        tracing::debug!(%agent_id, "preparing worktree");
        self.prepare_splinter_worktree(agent_id, role).await?;

        let (session, window, pane) = self
            .prepare_tmux_window(agent_id, AgentKind::Splinter, role)
            .await?;

        tracing::debug!(%agent_id, session, window, pane, "building spawn plan");
        let plan = self.build_spawn_plan(spec, agent_id, session, window, pane)?;

        tracing::debug!(%agent_id, launch_cmd = ?plan.launch_command, "launching");
        TaskRegistry::assign(self.registry.as_ref(), task.task_id, agent_id)?;
        self.launch_or_insert_plan(plan).await
    }

    /// Spawn an agent directly from a rendered template.
    ///
    /// The rendered system_prompt and startup are combined into a single initial
    /// message injected after the agent CLI is launched. No PromptManager or
    /// taskflow_snippet is used — the template fully owns the prompt content.
    pub async fn spawn_from_template(&self, rendered: RenderedTemplate) -> Result<Agent> {
        let kind = match rendered.kind {
            TemplateKind::Bastion => AgentKind::Bastion,
            TemplateKind::Splinter => AgentKind::Splinter,
        };

        if kind == AgentKind::Bastion {
            let active = self.registry.list_active()?;
            if let Some(existing) = active
                .into_iter()
                .find(|a| a.kind == AgentKind::Bastion && a.role == rendered.role)
            {
                if let Some(tmux) = &self.tmux {
                    let target = TmuxTarget::parse(&existing.tmux_target())?;
                    if tmux.pane_has_agent(&target).await.unwrap_or(false) {
                        tracing::info!(agent_id = %existing.agent_id, role = %rendered.role, "re-using existing active bastion from template");
                        return Ok(existing);
                    }
                    tracing::info!(agent_id = %existing.agent_id, role = %rendered.role, "bastion pane is at shell prompt — relaunching from template");
                }
            }
        }

        let agent_id = Uuid::new_v4();

        let mut inline_prompt = rendered.system_prompt.clone();
        if let Some(startup) = &rendered.startup {
            inline_prompt.push_str("\n\n---\n\n");
            inline_prompt.push_str(startup);
        }

        let registry_task = if kind == AgentKind::Splinter {
            rendered.task_description.as_ref().map(|description| Task {
                task_id: Uuid::new_v4(),
                description: description.clone(),
                status: TaskStatus::Active,
                assigned_agent_id: Some(agent_id),
                created_by: TaskCreator::System,
                created_at: Utc::now(),
                completed_at: None,
                receipt_path: None,
            })
        } else {
            None
        };

        let spec = AgentSpec {
            name: rendered.role.clone(),
            kind: kind.clone(),
            role: rendered.role.clone(),
            parent_id: None,
            task_id: registry_task.as_ref().map(|task| task.task_id),
            task_description: rendered.task_description.clone(),
            cli_provider: rendered.cli_provider.clone(),
            fallback_cascade: rendered.fallback_cascade.clone(),
            system_prompt: None,
            inline_prompt: Some(inline_prompt),
            worktree_override: None,
            sandbox: SandboxPolicy {
                network: rendered.sandbox_network.clone(),
                ..SandboxPolicy::default()
            },
            auto_cleanup: rendered.auto_cleanup,
            model_override: rendered.model.clone(),
            resume_session: None,
        };

        if kind == AgentKind::Splinter {
            self.prepare_splinter_worktree(agent_id, &rendered.role)
                .await?;
        }

        let (session, window, pane) = self
            .prepare_tmux_window(agent_id, kind, &rendered.role)
            .await?;
        let plan = self.build_spawn_plan(spec, agent_id, session, window, pane)?;
        if let Some(task) = registry_task {
            TaskRegistry::insert(self.registry.as_ref(), &task)?;
        }
        self.launch_or_insert_plan(plan).await
    }

    async fn prepare_tmux_window(
        &self,
        agent_id: Uuid,
        kind: AgentKind,
        role: &str,
    ) -> Result<(String, u32, String)> {
        let Some(tmux) = &self.tmux else {
            return Ok(("aegis".to_string(), 0, "%0".to_string()));
        };

        let project_prefix = self
            .project_id
            .map(|id| short_id(id))
            .unwrap_or_else(|| "default".to_string());

        let session = match kind {
            AgentKind::Bastion => format!("aegis-{}-{}", project_prefix, role),
            AgentKind::Splinter => {
                format!("aegis-{}-{}-{}", project_prefix, role, short_id(agent_id))
            }
        };

        if tmux.session_exists(&session).await? {
            tmux.kill_session(&session).await?;
        }
        tmux.new_session(&session).await?;

        // In a new dedicated session, the first window is typically 0 and the first pane is %0.
        // We list panes to find the actual ID.
        let window_target = TmuxTarget::parse(&format!("{}:", session))?;
        let pane = tmux
            .list_panes(&window_target)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AegisError::TmuxPaneNotFound {
                target: session.clone(),
            })?;
        let pane_target = TmuxTarget::parse(&pane)?;
        tmux.harden_pane(&pane_target).await?;

        tracing::debug!(%agent_id, session, pane, "prepared tmux window");
        Ok((session, 0, pane))
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
            let agent = self.insert_starting_agent(plan.agent)?;
            self.attach_recorder(&agent)?;

            // Write the initial prompt to a temp file and inject it via
            // --append-system-prompt-file. This avoids the newline-as-submit problem
            // that occurs when send-keys -l delivers a multi-line string into Claude's
            // TUI: each embedded \n triggers a separate message submit. The sandbox
            // allows reads from /tmp so the sandboxed claude process can access it.
            let sys_prompt_path =
                std::env::temp_dir().join(format!("aegis_sysprompt_{}.txt", agent.agent_id));
            std::fs::write(&sys_prompt_path, &plan.initial_prompt).map_err(|source| {
                AegisError::StorageIo {
                    path: sys_prompt_path.clone(),
                    source,
                }
            })?;

            let mut launch_cmd = plan.launch_command.clone();
            let mut env_vars = vec![("AEGIS_AGENT_ID".to_string(), agent.agent_id.to_string())];
            let provider = self.providers.get(&agent.cli_provider)?;

            let trigger_text = if plan.is_resume {
                "Continue where you left off."
            } else {
                "Begin."
            };

            if let Some(i_flag) = provider.interactive_flag() {
                launch_cmd.push(i_flag.to_string());
            }

            match provider.system_prompt_mechanism() {
                aegis_core::SystemPromptMechanism::Flag { arg } => {
                    launch_cmd.push(arg);
                    launch_cmd.push(sys_prompt_path.to_string_lossy().into_owned());
                }
                aegis_core::SystemPromptMechanism::Env { var } => {
                    env_vars.push((var, sys_prompt_path.to_string_lossy().into_owned()));
                }
            }

            let launch_shell =
                launch_shell_command_with_env(&agent.worktree_path, &launch_cmd, &env_vars);
            let launch_script = write_launch_script("launch", agent.agent_id, &launch_shell)?;
            let launch_script_cmd = shell_command(&[
                "/bin/sh".to_string(),
                launch_script.to_string_lossy().into_owned(),
            ]);
            append_tmux_send(&agent.log_path, &launch_script_cmd)?;
            tmux.send_key(&target, "C-u").await?;
            let launch_script_input = format!("{launch_script_cmd}\n");
            tmux.send_raw_input(&target, launch_script_input.as_bytes())
                .await?;
            let agent = self.activate_agent(agent)?;
            if matches!(provider.interaction_model(), InteractionModel::InjectedTui) {
                self.submit_interactive_prompt(
                    &agent,
                    provider,
                    &target,
                    trigger_text,
                    plan.startup_delay_ms,
                )
                .await?;
            }

            Ok(agent)
        } else {
            let agent = self.insert_starting_agent(plan.agent)?;
            self.attach_recorder(&agent)?;
            self.activate_agent(agent)
        }
    }

    async fn launch_existing_plan(&self, plan: SpawnPlan) -> Result<Agent> {
        self.write_sandbox_profile(&plan)?;

        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&plan.agent.tmux_target())?;
            let agent = plan.agent.clone();
            AgentRegistry::update(self.registry.as_ref(), &agent)?;
            self.attach_recorder(&agent)?;

            let sys_prompt_path =
                std::env::temp_dir().join(format!("aegis_sysprompt_{}.txt", agent.agent_id));
            std::fs::write(&sys_prompt_path, &plan.initial_prompt).map_err(|source| {
                AegisError::StorageIo {
                    path: sys_prompt_path.clone(),
                    source,
                }
            })?;

            let mut launch_cmd = plan.launch_command.clone();
            let mut env_vars = vec![("AEGIS_AGENT_ID".to_string(), agent.agent_id.to_string())];
            let provider = self.providers.get(&agent.cli_provider)?;

            let trigger_text = if plan.is_resume {
                "Continue where you left off."
            } else {
                "Begin."
            };

            if let Some(i_flag) = provider.interactive_flag() {
                launch_cmd.push(i_flag.to_string());
            }

            match provider.system_prompt_mechanism() {
                aegis_core::SystemPromptMechanism::Flag { arg } => {
                    launch_cmd.push(arg);
                    launch_cmd.push(sys_prompt_path.to_string_lossy().into_owned());
                }
                aegis_core::SystemPromptMechanism::Env { var } => {
                    env_vars.push((var, sys_prompt_path.to_string_lossy().into_owned()));
                }
            }

            let launch_shell =
                launch_shell_command_with_env(&agent.worktree_path, &launch_cmd, &env_vars);
            let launch_script = write_launch_script("launch", agent.agent_id, &launch_shell)?;
            let launch_script_cmd = shell_command(&[
                "/bin/sh".to_string(),
                launch_script.to_string_lossy().into_owned(),
            ]);
            append_tmux_send(&agent.log_path, &launch_script_cmd)?;
            tmux.send_key(&target, "C-u").await?;
            let launch_script_input = format!("{launch_script_cmd}\n");
            tmux.send_raw_input(&target, launch_script_input.as_bytes())
                .await?;
            let agent = self.activate_agent(agent)?;
            if matches!(provider.interaction_model(), InteractionModel::InjectedTui) {
                self.submit_interactive_prompt(
                    &agent,
                    provider,
                    &target,
                    trigger_text,
                    plan.startup_delay_ms,
                )
                .await?;
            }

            Ok(agent)
        } else {
            let agent = plan.agent;
            AgentRegistry::update(self.registry.as_ref(), &agent)?;
            self.attach_recorder(&agent)?;
            self.activate_agent(agent)
        }
    }

    async fn submit_interactive_prompt(
        &self,
        agent: &Agent,
        provider: &dyn aegis_core::provider::Provider,
        target: &TmuxTarget,
        prompt: &str,
        startup_delay_ms: u64,
    ) -> Result<()> {
        let Some(tmux) = &self.tmux else {
            return Ok(());
        };

        if startup_delay_ms > 0 {
            sleep(Duration::from_millis(startup_delay_ms)).await;
        }

        let stable = tmux
            .wait_for_stability(target, 1000, 250, startup_delay_ms.saturating_add(20_000))
            .await?;
        if !stable {
            tracing::warn!(
                agent_id = %agent.agent_id,
                provider = %provider.name(),
                target = %target.as_str(),
                "tmux pane did not stabilize before prompt injection"
            );
        }

        let trigger = normalize_tui_prompt(prompt);
        tmux.send_interactive_text(target, &trigger).await?;
        append_tmux_send(&agent.log_path, &trigger)?;
        Ok(())
    }

    fn write_sandbox_profile(&self, plan: &SpawnPlan) -> Result<()> {
        if DISABLE_AGENT_SANDBOX {
            return Ok(());
        }
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
        self.maybe_clear_last_attached(agent_id)?;
        self.detach_and_archive(agent_id)?;
        if let Some(tmux) = &self.tmux {
            let _ = tmux.kill_session(&agent.tmux_session).await;
        }
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Terminated)?;
        AgentRegistry::archive(self.registry.as_ref(), agent_id)
    }

    fn maybe_clear_last_attached(&self, agent_id: Uuid) -> Result<()> {
        let Some(project_registry) = &self.project_registry else {
            return Ok(());
        };
        let Some(project_id) = self.project_id else {
            return Ok(());
        };

        if let Some(project) = project_registry.find_by_id(project_id)? {
            if project.last_attached_agent_id == Some(agent_id) {
                project_registry.update_last_attached(project_id, None)?;
            }
        }
        Ok(())
    }

    pub async fn failover_agent(&self, agent_id: Uuid) -> Result<Agent> {
        let agent = self.require_agent(agent_id)?;
        self.ensure_transition(&agent.status, &AgentStatus::Cooling)?;
        let next_provider = self.next_provider_name(&agent)?;
        let old_status = agent.status.clone();

        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Cooling)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status,
            new_status: AgentStatus::Cooling,
        });

        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&agent.tmux_target())?;
            tmux.interrupt(&target).await?;
        }

        let terminal_context = self.capture_failover_context(agent_id)?;
        let task_description = match agent.task_id {
            Some(task_id) => {
                TaskRegistry::get(self.registry.as_ref(), task_id)?.map(|task| task.description)
            }
            None => None,
        };

        AgentRegistry::update_provider(self.registry.as_ref(), agent_id, &next_provider)?;
        let provider = self.providers.get(&next_provider)?;
        let provider_command = provider.spawn_command(&agent.worktree_path, None, None);
        let mut launch_command = Vec::new();
        if !DISABLE_AGENT_SANDBOX {
            if let Some(sandbox) = &self.sandbox {
                launch_command.extend(sandbox.exec_prefix(&agent.sandbox_profile));
            }
        }
        launch_command.extend(resolved_command_parts(
            &provider_command,
            &provider.config().binary,
        ));

        let context = FailoverContext {
            agent_id,
            task_id: agent.task_id,
            previous_provider: agent.cli_provider.clone(),
            terminal_context,
            task_description,
            worktree_path: agent.worktree_path.clone(),
            role: agent.role.clone(),
        };
        let recovery_prompt = provider.failover_handoff_prompt(&context);

        if let Some(tmux) = &self.tmux {
            let mut launch_agent = agent.clone();
            launch_agent.cli_provider = next_provider.clone();
            let launch_agent = self.replace_tmux_session_for_relaunch(launch_agent).await?;
            let target = TmuxTarget::parse(&launch_agent.tmux_target())?;

            let sys_prompt_path =
                std::env::temp_dir().join(format!("aegis_failover_{}.txt", agent_id));
            std::fs::write(&sys_prompt_path, &recovery_prompt).map_err(|source| {
                AegisError::StorageIo {
                    path: sys_prompt_path.clone(),
                    source,
                }
            })?;

            let mut launch_cmd_with_prompt = launch_command.clone();
            let mut env_vars = vec![("AEGIS_AGENT_ID".to_string(), launch_agent.agent_id.to_string())];

            let trigger_text = "Continue.";

            if let Some(i_flag) = provider.interactive_flag() {
                launch_cmd_with_prompt.push(i_flag.to_string());
            }

            match provider.system_prompt_mechanism() {
                aegis_core::SystemPromptMechanism::Flag { arg } => {
                    launch_cmd_with_prompt.push(arg);
                    launch_cmd_with_prompt.push(sys_prompt_path.to_string_lossy().into_owned());
                }
                aegis_core::SystemPromptMechanism::Env { var } => {
                    env_vars.push((var, sys_prompt_path.to_string_lossy().into_owned()));
                }
            }

            let launch_shell =
                launch_shell_command_with_env(&agent.worktree_path, &launch_cmd_with_prompt, &env_vars);
            let launch_script =
                write_launch_script("failover", launch_agent.agent_id, &launch_shell)?;
            let launch_script_cmd = shell_command(&[
                "/bin/sh".to_string(),
                launch_script.to_string_lossy().into_owned(),
            ]);
            append_tmux_send(&launch_agent.log_path, &launch_script_cmd)?;
            tmux.send_key(&target, "C-u").await?;
            let launch_script_input = format!("{launch_script_cmd}\n");
            tmux.send_raw_input(&target, launch_script_input.as_bytes())
                .await?;
            if matches!(provider.interaction_model(), InteractionModel::InjectedTui) {
                self.submit_interactive_prompt(
                    &launch_agent,
                    provider,
                    &target,
                    trigger_text,
                    provider.config().startup_delay_ms,
                )
                .await?;
            }
        }

        let mut updated = self.require_agent(agent_id)?;
        updated.status = AgentStatus::Active;
        updated.updated_at = Utc::now();
        AgentRegistry::update(self.registry.as_ref(), &updated)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status: AgentStatus::Cooling,
            new_status: AgentStatus::Active,
        });
        Ok(updated)
    }

    fn next_provider_name(&self, agent: &Agent) -> Result<String> {
        agent
            .fallback_cascade
            .iter()
            .find(|provider| *provider != &agent.cli_provider)
            .cloned()
            .ok_or_else(|| AegisError::Config {
                field: "agent.fallback_cascade".to_string(),
                reason: format!("no fallback provider available for {}", agent.cli_provider),
            })
    }

    fn capture_failover_context(&self, agent_id: Uuid) -> Result<String> {
        let Some(recorder) = &self.recorder else {
            return Ok(String::new());
        };

        match recorder.query(&LogQuery {
            agent_id,
            last_n_lines: Some(self.config.recorder.failover_context_lines),
            since: None,
            follow: false,
        }) {
            Ok(lines) => Ok(lines.join("\n")),
            Err(AegisError::LogFileNotFound { .. }) => Ok(String::new()),
            Err(error) => Err(error),
        }
    }

    pub async fn terminate_agent(&self, agent_id: Uuid) -> Result<()> {
        let agent = self.require_agent(agent_id)?;
        self.ensure_transition(&agent.status, &AgentStatus::Reporting)?;
        self.perform_termination(agent).await
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
        if receipt_path.exists() {
            self.validate_receipt(task_id, &receipt_path)?;
            TaskRegistry::complete(
                self.registry.as_ref(),
                task_id,
                Some(receipt_path.clone()),
            )?;
            self.events.publish(AegisEvent::TaskComplete {
                task_id,
                receipt_path: receipt_path.to_string_lossy().into_owned(),
            });
        } else {
            // In M23 messaging design, agents may complete without writing a receipt.json.
            // We still mark the task complete if the agent is being explicitly terminated.
            TaskRegistry::complete(self.registry.as_ref(), task_id, None)?;
            self.events.publish(AegisEvent::TaskComplete {
                task_id: task_id,
                receipt_path: "m23-messaging".to_string(),
            });
        }

        self.perform_termination(agent).await
    }

    async fn perform_termination(&self, agent: Agent) -> Result<()> {
        let agent_id = agent.agent_id;
        self.maybe_clear_last_attached(agent_id)?;
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

        Ok(())
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

    pub async fn worktree_create(&self, milestone_id: &str) -> Result<std::path::PathBuf> {
        let git = GitWorktree::new(
            self.storage.project_root().to_path_buf(),
            self.storage.worktrees_dir(),
        );
        git.create_for_milestone(milestone_id).await
    }

    pub async fn worktree_merge(&self, milestone_id: &str) -> Result<()> {
        let git = GitWorktree::new(
            self.storage.project_root().to_path_buf(),
            self.storage.worktrees_dir(),
        );
        git.merge_milestone_into_main(milestone_id).await
    }

    pub async fn worktree_list(&self) -> Result<Vec<(String, std::path::PathBuf)>> {
        let git = GitWorktree::new(
            self.storage.project_root().to_path_buf(),
            self.storage.worktrees_dir(),
        );
        git.list_milestone_worktrees().await
    }

    async fn replace_tmux_session_for_relaunch(&self, mut agent: Agent) -> Result<Agent> {
        let Some(tmux) = &self.tmux else {
            return Ok(agent);
        };

        if let Some(current) = AgentRegistry::get(self.registry.as_ref(), agent.agent_id)? {
            agent.status = current.status;
        }

        if tmux.session_exists(&agent.tmux_session).await? {
            tmux.kill_session(&agent.tmux_session).await?;
        }
        tmux.new_session(&agent.tmux_session).await?;

        let window_target = TmuxTarget::parse(&format!("{}:", agent.tmux_session))?;
        let pane = tmux
            .list_panes(&window_target)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| AegisError::TmuxPaneNotFound {
                target: agent.tmux_session.clone(),
            })?;
        let pane_target = TmuxTarget::parse(&pane)?;
        tmux.harden_pane(&pane_target).await?;

        agent.tmux_window = 0;
        agent.tmux_pane = pane;
        agent.updated_at = Utc::now();
        AgentRegistry::update(self.registry.as_ref(), &agent)?;
        self.attach_recorder(&agent)?;
        Ok(agent)
    }
}

#[async_trait]
impl FailoverExecutor for Dispatcher {
    async fn pause_current(&self, agent: &Agent) -> Result<()> {
        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&agent.tmux_target())?;
            tmux.interrupt(&target).await?;
        }
        Ok(())
    }

    async fn relaunch_with_provider(&self, agent: &Agent, provider_name: &str) -> Result<Agent> {
        let provider = self.providers.get(provider_name)?;
        let provider_command = provider.spawn_command(&agent.worktree_path, None, None);
        let mut relaunched_agent = agent.clone();
        relaunched_agent.cli_provider = provider_name.to_string();
        let relaunched_agent = self
            .replace_tmux_session_for_relaunch(relaunched_agent)
            .await?;

        let mut launch_command = Vec::new();
        if !DISABLE_AGENT_SANDBOX {
            if let Some(sandbox) = &self.sandbox {
                launch_command.extend(sandbox.exec_prefix(&relaunched_agent.sandbox_profile));
            }
        }
        launch_command.extend(resolved_command_parts(
            &provider_command,
            &provider.config().binary,
        ));

        if let Some(tmux) = &self.tmux {
            let target = TmuxTarget::parse(&relaunched_agent.tmux_target())?;
            let launch_shell =
                launch_shell_command(&relaunched_agent.worktree_path, &launch_command);
            let launch_script =
                write_launch_script("failover", relaunched_agent.agent_id, &launch_shell)?;
            let launch_script_cmd = shell_command(&[
                "/bin/sh".to_string(),
                launch_script.to_string_lossy().into_owned(),
            ]);
            append_tmux_send(&relaunched_agent.log_path, &launch_script_cmd)?;
            tmux.send_key(&target, "C-u").await?;
            let launch_script_input = format!("{launch_script_cmd}\n");
            tmux.send_raw_input(&target, launch_script_input.as_bytes())
                .await?;
        }

        AgentRegistry::update_provider(
            self.registry.as_ref(),
            relaunched_agent.agent_id,
            provider_name,
        )?;
        let mut updated = self.require_agent(relaunched_agent.agent_id)?;
        updated.cli_provider = provider_name.to_string();
        updated.updated_at = Utc::now();
        self.events.publish(AegisEvent::FailoverInitiated {
            agent_id: relaunched_agent.agent_id,
            from_provider: agent.cli_provider.clone(),
            to_provider: provider_name.to_string(),
        });
        Ok(updated)
    }

    async fn inject_recovery(&self, agent: &Agent, prompt: &str) -> Result<()> {
        if self.tmux.is_none() {
            return Ok(());
        }

        let provider = self.providers.get(&agent.cli_provider)?;

        let target = TmuxTarget::parse(&agent.tmux_target())?;
        if matches!(provider.interaction_model(), InteractionModel::InjectedTui) {
            self.submit_interactive_prompt(
                agent,
                provider,
                &target,
                prompt,
                provider.config().startup_delay_ms,
            )
            .await?;
        }
        Ok(())
    }

    async fn mark_failed(&self, agent_id: Uuid, reason: &str) -> Result<()> {
        let old_status = self
            .require_agent(agent_id)
            .map(|agent| agent.status)
            .unwrap_or(AgentStatus::Active);
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Failed)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status,
            new_status: AgentStatus::Failed,
        });
        self.events.publish(AegisEvent::AgentTerminated {
            agent_id,
            reason: reason.to_string(),
        });
        Ok(())
    }

    async fn mark_cooling(&self, agent_id: Uuid) -> Result<()> {
        let old_status = self.require_agent(agent_id)?.status;
        AgentRegistry::update_status(self.registry.as_ref(), agent_id, AgentStatus::Cooling)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status,
            new_status: AgentStatus::Cooling,
        });
        Ok(())
    }

    async fn mark_active(&self, agent_id: Uuid, provider_name: &str) -> Result<()> {
        let mut agent = self.require_agent(agent_id)?;
        let old_status = agent.status.clone();
        agent.status = AgentStatus::Active;
        agent.cli_provider = provider_name.to_string();
        agent.updated_at = Utc::now();
        AgentRegistry::update(self.registry.as_ref(), &agent)?;
        self.events.publish(AegisEvent::AgentStatusChanged {
            agent_id,
            old_status,
            new_status: AgentStatus::Active,
        });
        Ok(())
    }

    async fn process_receipt(&self, agent_id: Uuid) -> Result<()> {
        Dispatcher::process_receipt(self, agent_id).await
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
        inline_prompt: None,
        worktree_override: None,
        sandbox: sandbox_policy_from_config(&entry.sandbox),
        auto_cleanup: entry.auto_cleanup,
        model_override: entry.model.clone(),
        resume_session: None,
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

fn resolved_command_parts(command: &std::process::Command, binary: &str) -> Vec<String> {
    let mut parts = command_parts(command);
    if let Some(full_path) = resolve_binary_full_path(binary) {
        if let Some(program) = parts.first_mut() {
            *program = full_path.to_string_lossy().to_string();
        }
    }
    parts
}

fn shell_command(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn launch_shell_command(worktree_path: &std::path::Path, parts: &[String]) -> String {
    launch_shell_command_with_env(worktree_path, parts, &[])
}

fn launch_shell_command_with_env(
    worktree_path: &std::path::Path,
    parts: &[String],
    env_vars: &[(String, String)],
) -> String {
    let env_prefix = env_vars
        .iter()
        .map(|(k, v)| format!("export {}={} && ", k, shell_quote(v)))
        .collect::<String>();

    format!(
        "cd {} && {}{}",
        shell_quote(&worktree_path.to_string_lossy()),
        env_prefix,
        shell_command(parts)
    )
}

fn write_launch_script(kind: &str, agent_id: Uuid, launch_shell: &str) -> Result<PathBuf> {
    let script_path = std::env::temp_dir().join(format!("aegis_{kind}_{}.sh", short_id(agent_id)));
    let script = format!("#!/bin/sh\n{}\n", launch_shell);
    std::fs::write(&script_path, script).map_err(|source| AegisError::StorageIo {
        path: script_path.clone(),
        source,
    })?;
    Ok(script_path)
}

fn normalize_tui_prompt(prompt: &str) -> String {
    prompt.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// Resolve a binary name to its first existing full path via `which -a`.
///
/// Uses `which -a` to enumerate all PATH matches in order, then returns the
/// first one that is an actual file. This skips stale PATH entries (e.g. an
/// old Claude install location) whose target no longer exists.
fn resolve_binary_full_path(binary: &str) -> Option<PathBuf> {
    let binary_path = std::path::Path::new(binary);
    if binary_path.is_absolute() && binary_path.is_file() {
        return std::fs::canonicalize(binary_path)
            .ok()
            .or_else(|| Some(binary_path.to_path_buf()));
    }

    let output = std::process::Command::new("which")
        .arg("-a")
        .arg(binary)
        .output()
        .ok()?;
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .map(|line| PathBuf::from(line.trim()))
        .find(|p| p.is_file())
        .and_then(|p| std::fs::canonicalize(&p).ok().or(Some(p)))
}

/// Resolve a binary name to its parent directory, using the first existing
/// PATH match (see `resolve_binary_full_path`).
fn resolve_binary_exec_dir(binary: &str) -> Option<PathBuf> {
    resolve_binary_full_path(binary)
        .as_deref()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
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
    use std::{collections::HashMap, path::Path, sync::Arc};

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
            fallback_cascade: Some(vec!["gemini-cli".to_string()]),
            auto_cleanup: Some(false),
            model: None,
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
            .build_spawn_plan(
                spec,
                agent_id,
                "aegis-test".to_string(),
                3,
                "%9".to_string(),
            )
            .unwrap();

        assert_eq!(plan.agent.kind, AgentKind::Bastion);
        assert_eq!(plan.agent.status, AgentStatus::Starting);
        assert_eq!(plan.agent.tmux_session, "aegis-test");
        assert_eq!(plan.agent.tmux_window, 3);
        assert_eq!(plan.agent.tmux_pane, "%9");
        assert_eq!(plan.agent.worktree_path, dir.path());
        assert!(plan.agent.log_path.ends_with(format!("{agent_id}.log")));
        assert!(plan
            .launch_command
            .iter()
            .any(|part| part.contains("claude")));
        assert!(plan.initial_prompt.contains("architect"));
    }

    #[test]
    fn bastion_restart_plan_reuses_identity_but_not_provider_resume() {
        let (dispatcher, _dir) = dispatcher();
        let agent_id = Uuid::new_v4();
        let existing = dispatcher
            .build_spawn_plan(
                dispatcher.build_bastion_spec("architect").unwrap(),
                agent_id,
                "aegis-test-architect".to_string(),
                0,
                "%1".to_string(),
            )
            .unwrap()
            .agent;

        let plan = dispatcher
            .build_bastion_restart_plan("architect", &existing, "%9".to_string())
            .unwrap();

        assert_eq!(plan.agent.agent_id, agent_id);
        assert_eq!(plan.agent.tmux_session, existing.tmux_session);
        assert_eq!(plan.agent.tmux_pane, "%9");
        assert_eq!(plan.agent.created_at, existing.created_at);
        assert!(!plan.is_resume);
        assert!(!plan.launch_command.iter().any(|part| part == "--resume"));
        assert!(plan.initial_prompt.contains("architect"));
        assert!(plan.initial_prompt.contains("Project Context"));
    }

    #[test]
    fn build_spawn_plan_uses_first_existing_binary_path() {
        let (dispatcher, _dir) = dispatcher();
        let temp = tempfile::tempdir().unwrap();
        let bin_dir = temp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();

        let stale = temp.path().join("stale").join("claude");
        let valid = bin_dir.join("claude");
        std::fs::write(&valid, "#!/bin/sh\nprintf 'valid\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&valid).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&valid, perms).unwrap();
        }

        let which_path = temp.path().join("which");
        std::fs::write(
            &which_path,
            format!(
                "#!/bin/sh\nprintf '%s\\n%s\\n' '{}' '{}'\n",
                stale.display(),
                valid.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&which_path).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&which_path, perms).unwrap();
        }

        static PATH_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        let _guard = PATH_LOCK
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap();
        let old_path = std::env::var_os("PATH");
        let new_path = format!("{}:/usr/bin:/bin", temp.path().display());
        unsafe {
            std::env::set_var("PATH", &new_path);
        }

        let spec = dispatcher.build_bastion_spec("architect").unwrap();
        let plan = dispatcher
            .build_spawn_plan(spec, Uuid::nil(), "aegis".to_string(), 3, "%9".to_string())
            .unwrap();
        let canonical_valid = std::fs::canonicalize(&valid).unwrap();
        assert!(plan
            .launch_command
            .iter()
            .any(|part| part == &canonical_valid.to_string_lossy()));

        if let Some(old_path) = old_path {
            unsafe {
                std::env::set_var("PATH", old_path);
            }
        }
    }

    #[test]
    fn build_spawn_plan_uses_absolute_binary_exec_path() {
        let (dispatcher, _dir) = dispatcher();
        let temp = tempfile::tempdir().unwrap();
        let bin_dir = temp.path().join("claude-home");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let binary = bin_dir.join("claude");
        std::fs::write(&binary, "#!/bin/sh\nprintf 'absolute\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&binary).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&binary, perms).unwrap();
        }

        let mut project = RawConfig::default();
        project.global = Some(RawGlobalConfig {
            tmux_session_name: Some("aegis-test".into()),
            ..Default::default()
        });
        project.providers = HashMap::from([(
            "claude-code".to_string(),
            RawProviderConfig {
                binary: Some(binary.to_string_lossy().to_string()),
                ..Default::default()
            },
        )]);
        project.splinter_defaults = Some(RawSplinterDefaults {
            cli_provider: Some("claude-code".to_string()),
            fallback_cascade: Some(vec!["gemini-cli".to_string()]),
            auto_cleanup: Some(false),
            model: None,
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
        let registry = Arc::new(FileRegistry::new(dispatcher.storage.clone()));
        let providers = Arc::new(ProviderRegistry::from_config(&config).unwrap());
        let prompts = Arc::new(PromptManager::new(temp.path().to_path_buf()));
        let dispatcher = Dispatcher::new(
            registry,
            None,
            None,
            None,
            None,
            None,
            providers,
            prompts,
            dispatcher.storage.clone(),
            EventBus::default(),
            config,
        );

        let spec = dispatcher.build_bastion_spec("architect").unwrap();
        let plan = dispatcher
            .build_spawn_plan(spec, Uuid::nil(), "aegis".to_string(), 3, "%9".to_string())
            .unwrap();

        let canonical = std::fs::canonicalize(&binary).unwrap();
        assert!(plan
            .launch_command
            .iter()
            .any(|part| part == &canonical.to_string_lossy()));
        let canonical_bin_dir = std::fs::canonicalize(&bin_dir).unwrap();
        assert!(plan
            .sandbox_policy
            .extra_exec_paths
            .contains(&canonical_bin_dir));
    }

    #[test]
    fn launch_shell_command_prefixes_worktree_cd() {
        let command = vec![
            "/opt/claude/bin/claude".to_string(),
            "--dangerously-skip-permissions".to_string(),
        ];
        let rendered = launch_shell_command(Path::new("/tmp/project root"), &command);

        assert!(rendered.starts_with("cd '/tmp/project root' && "));
        assert!(rendered.contains("/opt/claude/bin/claude"));
    }

    #[test]
    fn write_launch_script_wraps_long_launch_command() {
        let agent_id = Uuid::nil();
        let launch_shell = "cd '/tmp/project root' && sandbox-exec -f profile.sb claude";
        let script_path = write_launch_script("test-launch", agent_id, launch_shell).unwrap();

        let script = std::fs::read_to_string(&script_path).unwrap();
        assert_eq!(
            script,
            "#!/bin/sh\ncd '/tmp/project root' && sandbox-exec -f profile.sb claude\n"
        );

        let _ = std::fs::remove_file(script_path);
    }

    #[test]
    fn normalize_tui_prompt_collapses_multiline_text() {
        assert_eq!(
            normalize_tui_prompt("Begin by checking status.\nThen inspect inbox.\n\nProceed."),
            "Begin by checking status. Then inspect inbox. Proceed."
        );
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
    async fn failover_agent_switches_provider_and_returns_active() {
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

        let agent = dispatcher.failover_agent(agent_id).await.unwrap();

        assert_eq!(agent.status, AgentStatus::Active);
        assert_eq!(agent.cli_provider, "gemini-cli");
        let stored = AgentRegistry::get(dispatcher.registry.as_ref(), agent_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, AgentStatus::Active);
        assert_eq!(stored.cli_provider, "gemini-cli");
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
    async fn process_receipt_missing_receipt_marks_task_complete_and_terminates_agent() {
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

        dispatcher.process_receipt(agent_id).await.unwrap();
        let stored_task = TaskRegistry::get(dispatcher.registry.as_ref(), task.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_task.status, TaskStatus::Complete);
        assert!(stored_task.receipt_path.is_none());
        let stored_agent = AgentRegistry::get(dispatcher.registry.as_ref(), agent_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_agent.status, AgentStatus::Terminated);
        assert!(agent.worktree_path.exists());
    }

    // M22.6: bastion-splinter coordination loop integration test.
    // Verifies that the full messaging protocol between a template-spawned bastion
    // and splinter works end-to-end using the M23 messaging infrastructure.
    #[tokio::test]
    async fn bastion_splinter_coordination_loop() {
        use crate::messaging::MessageRouter;
        use aegis_core::{MessageType, SandboxNetworkPolicy};
        use aegis_design::{RenderedTemplate, TemplateKind};

        let (dispatcher, dir) = dispatcher();
        let storage = Arc::new(ProjectStorage::new(dir.path().to_path_buf()));
        let registry = Arc::clone(&dispatcher.registry);
        let router = MessageRouter::new(registry.clone(), storage, None);

        // 1. Spawn bastion from taskflow-bastion template.
        let bastion_rendered = RenderedTemplate {
            name: "taskflow-bastion".into(),
            kind: TemplateKind::Bastion,
            role: "taskflow-coordinator".into(),
            task_id: None,
            task_description: None,
            cli_provider: "claude-code".into(),
            model: None,
            auto_cleanup: false,
            fallback_cascade: vec![],
            sandbox_network: SandboxNetworkPolicy::OutboundOnly,
            system_prompt: "You are the coordinator.".into(),
            startup: Some("Begin driving the milestone.".into()),
        };
        let bastion = dispatcher
            .spawn_from_template(bastion_rendered)
            .await
            .unwrap();
        assert_eq!(bastion.kind, AgentKind::Bastion);

        // 2. Spawn splinter from taskflow-implementer template (with bastion_agent_id known).
        let splinter_rendered = RenderedTemplate {
            name: "taskflow-implementer".into(),
            kind: TemplateKind::Splinter,
            role: "taskflow-implementer".into(),
            task_id: Some("20.1".into()),
            task_description: Some("Implement task X".into()),
            cli_provider: "claude-code".into(),
            model: None,
            auto_cleanup: true,
            fallback_cascade: vec![],
            sandbox_network: SandboxNetworkPolicy::OutboundOnly,
            system_prompt: format!("You implement task X. Report to {}.", bastion.agent_id),
            startup: Some("Check inbox then implement.".into()),
        };
        let splinter = dispatcher
            .spawn_from_template(splinter_rendered)
            .await
            .unwrap();
        assert_eq!(splinter.kind, AgentKind::Splinter);
        assert!(splinter.task_id.is_some());

        // 3. Bastion sends a `task` message to the splinter.
        let receipt = router
            .send(
                Some(bastion.agent_id),
                &splinter.agent_id.to_string(),
                MessageType::Task,
                serde_json::json!({
                    "lld_path": "/tmp/project/.aegis/designs/lld/engine.md",
                    "task_id": "20.1",
                    "acceptance_criteria": "Scaffold the aegis-design crate"
                }),
            )
            .await
            .unwrap();
        assert_eq!(receipt.to_agent_id, splinter.agent_id);

        // 4. Splinter reads its inbox and sees the task message.
        let splinter_inbox = router.inbox(&splinter.agent_id.to_string()).unwrap();
        assert_eq!(splinter_inbox.messages.len(), 1);
        assert_eq!(splinter_inbox.messages[0].kind, MessageType::Task);

        // 5. Splinter sends a `notification` (done) back to the bastion.
        router
            .send(
                Some(splinter.agent_id),
                &bastion.agent_id.to_string(),
                MessageType::Notification,
                serde_json::json!({
                    "status": "done",
                    "task_id": "20.1",
                    "summary": "Scaffolded aegis-design crate with template engine"
                }),
            )
            .await
            .unwrap();

        // 6. Bastion reads its inbox and sees the completion notification.
        let bastion_inbox = router.inbox(&bastion.agent_id.to_string()).unwrap();
        assert_eq!(bastion_inbox.messages.len(), 1);
        assert_eq!(bastion_inbox.messages[0].kind, MessageType::Notification);
        let payload = &bastion_inbox.messages[0].payload;
        assert_eq!(payload["status"].as_str().unwrap(), "done");
        assert_eq!(payload["task_id"].as_str().unwrap(), "20.1");
    }

    #[tokio::test]
    async fn spawn_from_template_creates_active_bastion() {
        use aegis_core::SandboxNetworkPolicy;
        use aegis_design::{RenderedTemplate, TemplateKind};

        let (dispatcher, _dir) = dispatcher();

        let rendered = RenderedTemplate {
            name: "test-template".into(),
            kind: TemplateKind::Bastion,
            role: "test-coordinator".into(),
            task_id: None,
            task_description: None,
            cli_provider: "claude-code".into(),
            model: None,
            auto_cleanup: false,
            fallback_cascade: vec![],
            sandbox_network: SandboxNetworkPolicy::OutboundOnly,
            system_prompt: "You are a coordinator.".into(),
            startup: Some("Begin by checking status.".into()),
        };

        let agent = dispatcher.spawn_from_template(rendered).await.unwrap();

        assert_eq!(agent.kind, AgentKind::Bastion);
        assert_eq!(agent.role, "test-coordinator");
        assert_eq!(agent.status, AgentStatus::Active);
        assert_eq!(agent.cli_provider, "claude-code");
    }

    #[test]
    fn build_spawn_plan_inline_prompt_skips_prompt_manager() {
        let (dispatcher, _dir) = dispatcher();
        let spec = AgentSpec {
            name: "tpl-agent".into(),
            kind: AgentKind::Bastion,
            role: "tpl-role".into(),
            parent_id: None,
            task_id: None,
            task_description: None,
            cli_provider: "claude-code".into(),
            fallback_cascade: vec![],
            system_prompt: None,
            inline_prompt: Some("TEMPLATE PROMPT".into()),
            worktree_override: None,
            sandbox: aegis_core::SandboxPolicy::default(),
            auto_cleanup: false,
            model_override: None,
            resume_session: None,
        };
        let plan = dispatcher
            .build_spawn_plan(spec, Uuid::nil(), "aegis".to_string(), 1, "%1".to_string())
            .unwrap();
        assert_eq!(plan.initial_prompt, "TEMPLATE PROMPT");
        assert!(!plan.initial_prompt.contains("Project Context"));
    }

    #[test]
    fn normalize_tui_prompt_collapses_whitespace() {
        assert_eq!(
            normalize_tui_prompt("  Begin.\n\tContinue    here  "),
            "Begin. Continue here"
        );
    }
}
