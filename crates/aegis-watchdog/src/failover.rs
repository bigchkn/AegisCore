use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};

use aegis_core::{
    config::RecorderConfig, Agent, AgentRegistry, AgentStatus, DetectedEvent, FailoverContext,
    LogQuery, Recorder, Result, TaskRegistry,
};
use aegis_providers::ProviderRegistry;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tracing::warn;
use uuid::Uuid;

use crate::BackoffPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailoverState {
    Detected,
    PauseCurrent,
    CaptureContext,
    SelectProvider,
    Backoff { attempt: u32, delay: Duration },
    Relaunch,
    InjectRecovery,
    ResumeMonitoring,
    Exhausted,
}

#[derive(Debug, Clone)]
pub struct FailoverAttempt {
    pub agent_id: Uuid,
    pub from_provider: String,
    pub to_provider: String,
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
}

#[async_trait]
pub trait FailoverExecutor: Send + Sync {
    async fn pause_current(&self, agent: &Agent) -> Result<()>;
    async fn relaunch_with_provider(&self, agent: &Agent, provider_name: &str) -> Result<Agent>;
    async fn inject_recovery(&self, agent: &Agent, prompt: &str) -> Result<()>;
    async fn mark_failed(&self, agent_id: Uuid, reason: &str) -> Result<()>;
    async fn mark_cooling(&self, agent_id: Uuid) -> Result<()>;
    async fn mark_active(&self, agent_id: Uuid, provider_name: &str) -> Result<()>;
    async fn process_receipt(&self, agent_id: Uuid) -> Result<()>;
}

pub struct FailoverCoordinator {
    agents: Arc<dyn AgentRegistry>,
    tasks: Arc<dyn TaskRegistry>,
    recorder: Arc<dyn Recorder>,
    providers: Arc<ProviderRegistry>,
    executor: Arc<dyn FailoverExecutor>,
    recorder_config: RecorderConfig,
    backoff: BackoffPolicy,
    attempts: Mutex<HashMap<Uuid, u32>>,
    active_agents: Mutex<HashSet<Uuid>>,
}

impl FailoverCoordinator {
    pub fn new(
        agents: Arc<dyn AgentRegistry>,
        tasks: Arc<dyn TaskRegistry>,
        recorder: Arc<dyn Recorder>,
        providers: Arc<ProviderRegistry>,
        recorder_config: RecorderConfig,
        executor: Arc<dyn FailoverExecutor>,
    ) -> Self {
        Self::with_backoff(
            agents,
            tasks,
            recorder,
            providers,
            recorder_config,
            executor,
            BackoffPolicy::default(),
        )
    }

    pub fn with_backoff(
        agents: Arc<dyn AgentRegistry>,
        tasks: Arc<dyn TaskRegistry>,
        recorder: Arc<dyn Recorder>,
        providers: Arc<ProviderRegistry>,
        recorder_config: RecorderConfig,
        executor: Arc<dyn FailoverExecutor>,
        backoff: BackoffPolicy,
    ) -> Self {
        Self {
            agents,
            tasks,
            recorder,
            providers,
            executor,
            recorder_config,
            backoff,
            attempts: Mutex::new(HashMap::new()),
            active_agents: Mutex::new(HashSet::new()),
        }
    }

    pub async fn initiate(&self, event: DetectedEvent) -> Result<()> {
        let agent_id = event.agent_id();
        if !self.try_lock(agent_id) {
            return Ok(());
        }

        let result = self.initiate_locked(agent_id).await;
        self.unlock(agent_id);
        result
    }

    async fn initiate_locked(&self, agent_id: Uuid) -> Result<()> {
        let Some(agent) = self.agents.get(agent_id)? else {
            return Ok(());
        };
        if !is_failover_eligible(&agent) {
            return Ok(());
        }

        self.executor.mark_cooling(agent_id).await?;
        self.executor.pause_current(&agent).await?;

        let next_provider = match self.select_next_provider(&agent) {
            Some(provider) => provider,
            None => {
                self.clear_attempt(agent_id);
                return self
                    .executor
                    .mark_failed(agent_id, "no fallback provider available")
                    .await;
            }
        };

        let attempt = self.reserve_attempt(agent_id);
        let delay = self.backoff.delay_for_attempt(attempt);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }

        let task_description = match agent.task_id {
            Some(task_id) => self.tasks.get(task_id)?.map(|task| task.description),
            None => None,
        };
        let terminal_context = self.capture_terminal_context(agent_id)?;
        let context = FailoverContext {
            agent_id,
            task_id: agent.task_id,
            previous_provider: agent.cli_provider.clone(),
            terminal_context,
            task_description,
            worktree_path: agent.worktree_path.clone(),
            role: agent.role.clone(),
        };

        let provider = self.providers.get(&next_provider)?;
        let relaunched = self
            .executor
            .relaunch_with_provider(&agent, &next_provider)
            .await?;

        if !self.recorder.log_path(relaunched.agent_id).exists() {
            warn!(
                agent_id = %relaunched.agent_id,
                provider = %next_provider,
                "watchdog failover relaunch completed without an attached recorder log"
            );
        }

        let recovery_prompt = provider.failover_handoff_prompt(&context);
        self.executor
            .inject_recovery(&relaunched, &recovery_prompt)
            .await?;
        self.executor
            .mark_active(relaunched.agent_id, &next_provider)
            .await?;
        self.clear_attempt(agent_id);
        Ok(())
    }

    fn capture_terminal_context(&self, agent_id: Uuid) -> Result<String> {
        match self.recorder.query(&LogQuery {
            agent_id,
            last_n_lines: Some(self.recorder_config.failover_context_lines),
            since: None,
            follow: false,
        }) {
            Ok(lines) => Ok(lines.join("\n")),
            Err(aegis_core::AegisError::LogFileNotFound { .. }) => Ok(String::new()),
            Err(error) => Err(error),
        }
    }

    fn select_next_provider(&self, agent: &Agent) -> Option<String> {
        agent
            .fallback_cascade
            .iter()
            .find(|provider| *provider != &agent.cli_provider)
            .cloned()
    }

    fn reserve_attempt(&self, agent_id: Uuid) -> u32 {
        let mut attempts = self.attempts.lock().expect("failover attempts poisoned");
        let entry = attempts.entry(agent_id).or_insert(0);
        let attempt = *entry;
        *entry += 1;
        attempt
    }

    fn clear_attempt(&self, agent_id: Uuid) {
        self.attempts
            .lock()
            .expect("failover attempts poisoned")
            .remove(&agent_id);
    }

    fn try_lock(&self, agent_id: Uuid) -> bool {
        self.active_agents
            .lock()
            .expect("failover agent lock poisoned")
            .insert(agent_id)
    }

    fn unlock(&self, agent_id: Uuid) {
        self.active_agents
            .lock()
            .expect("failover agent lock poisoned")
            .remove(&agent_id);
    }
}

fn is_failover_eligible(agent: &Agent) -> bool {
    !matches!(
        agent.status,
        AgentStatus::Paused
            | AgentStatus::Cooling
            | AgentStatus::Reporting
            | AgentStatus::Terminated
            | AgentStatus::Failed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{
        config::{EffectiveConfig, RawConfig},
        AgentKind, Task, TaskCreator, TaskStatus,
    };
    use std::path::PathBuf;

    fn default_registry() -> Arc<ProviderRegistry> {
        let cfg = EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap();
        Arc::new(ProviderRegistry::from_config(&cfg).unwrap())
    }

    fn recorder_config() -> RecorderConfig {
        RecorderConfig {
            failover_context_lines: 3,
            log_rotation_max_mb: 32,
            log_retention_count: 8,
        }
    }

    fn zero_backoff() -> BackoffPolicy {
        BackoffPolicy {
            initial_delay: Duration::ZERO,
            max_delay: Duration::ZERO,
            multiplier: 1.0,
            jitter_ratio: 0.0,
        }
    }

    fn active_agent(provider: &str, fallback_cascade: Vec<&str>) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id: Uuid::new_v4(),
            name: "worker".to_string(),
            kind: AgentKind::Splinter,
            status: AgentStatus::Active,
            role: "worker".to_string(),
            parent_id: None,
            task_id: Some(Uuid::new_v4()),
            tmux_session: "aegis".to_string(),
            tmux_window: 1,
            tmux_pane: "%1".to_string(),
            worktree_path: PathBuf::from("/tmp/worktree"),
            cli_provider: provider.to_string(),
            fallback_cascade: fallback_cascade.into_iter().map(str::to_string).collect(),
            sandbox_profile: PathBuf::from("/tmp/profile.sb"),
            log_path: PathBuf::from("/tmp/session.log"),
            created_at: now,
            updated_at: now,
            terminated_at: None,
        }
    }

    struct FakeAgentRegistry {
        agent: Agent,
    }

    impl AgentRegistry for FakeAgentRegistry {
        fn insert(&self, _agent: &Agent) -> Result<()> {
            unimplemented!()
        }

        fn get(&self, agent_id: Uuid) -> Result<Option<Agent>> {
            if self.agent.agent_id == agent_id {
                Ok(Some(self.agent.clone()))
            } else {
                Ok(None)
            }
        }

        fn update(&self, _agent: &Agent) -> Result<()> {
            unimplemented!()
        }

        fn update_status(&self, _agent_id: Uuid, _status: AgentStatus) -> Result<()> {
            unimplemented!()
        }

        fn update_provider(&self, _agent_id: Uuid, _provider: &str) -> Result<()> {
            unimplemented!()
        }

        fn list_active(&self) -> Result<Vec<Agent>> {
            Ok(vec![self.agent.clone()])
        }

        fn list_by_role(&self, _role: &str) -> Result<Vec<Agent>> {
            unimplemented!()
        }

        fn list_all(&self) -> Result<Vec<Agent>> {
            Ok(vec![self.agent.clone()])
        }

        fn archive(&self, _agent_id: Uuid) -> Result<()> {
            unimplemented!()
        }
    }

    struct FakeTaskRegistry {
        task: Task,
    }

    impl TaskRegistry for FakeTaskRegistry {
        fn insert(&self, _task: &Task) -> Result<()> {
            unimplemented!()
        }

        fn get(&self, task_id: Uuid) -> Result<Option<Task>> {
            if self.task.task_id == task_id {
                Ok(Some(self.task.clone()))
            } else {
                Ok(None)
            }
        }

        fn update_status(&self, _task_id: Uuid, _status: TaskStatus) -> Result<()> {
            unimplemented!()
        }

        fn assign(&self, _task_id: Uuid, _agent_id: Uuid) -> Result<()> {
            unimplemented!()
        }

        fn complete(&self, _task_id: Uuid, _receipt_path: Option<PathBuf>) -> Result<()> {
            unimplemented!()
        }

        fn list_pending(&self) -> Result<Vec<Task>> {
            Ok(Vec::new())
        }

        fn list_all(&self) -> Result<Vec<Task>> {
            Ok(vec![self.task.clone()])
        }
    }

    struct FakeRecorder {
        lines: Vec<String>,
        log_path: PathBuf,
    }

    impl Recorder for FakeRecorder {
        fn attach(&self, _agent: &Agent) -> Result<()> {
            unimplemented!()
        }

        fn detach(&self, _agent_id: Uuid) -> Result<()> {
            unimplemented!()
        }

        fn archive(&self, _agent_id: Uuid) -> Result<PathBuf> {
            unimplemented!()
        }

        fn query(&self, _query: &LogQuery) -> Result<Vec<String>> {
            Ok(self.lines.clone())
        }

        fn log_path(&self, _agent_id: Uuid) -> PathBuf {
            self.log_path.clone()
        }
    }

    #[derive(Default)]
    struct RecordingExecutor {
        calls: Mutex<Vec<String>>,
        relaunched_provider: Mutex<Option<String>>,
        injected_prompt: Mutex<Option<String>>,
        active_provider: Mutex<Option<String>>,
        marked_failed: Mutex<Option<(Uuid, String)>>,
    }

    #[async_trait]
    impl FailoverExecutor for RecordingExecutor {
        async fn pause_current(&self, agent: &Agent) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("pause:{}", agent.agent_id));
            Ok(())
        }

        async fn relaunch_with_provider(
            &self,
            agent: &Agent,
            provider_name: &str,
        ) -> Result<Agent> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("relaunch:{provider_name}"));
            *self.relaunched_provider.lock().unwrap() = Some(provider_name.to_string());
            let mut updated = agent.clone();
            updated.cli_provider = provider_name.to_string();
            Ok(updated)
        }

        async fn inject_recovery(&self, _agent: &Agent, prompt: &str) -> Result<()> {
            self.calls.lock().unwrap().push("inject".to_string());
            *self.injected_prompt.lock().unwrap() = Some(prompt.to_string());
            Ok(())
        }

        async fn mark_failed(&self, agent_id: Uuid, reason: &str) -> Result<()> {
            self.calls.lock().unwrap().push("failed".to_string());
            *self.marked_failed.lock().unwrap() = Some((agent_id, reason.to_string()));
            Ok(())
        }

        async fn mark_cooling(&self, agent_id: Uuid) -> Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("cooling:{agent_id}"));
            Ok(())
        }

        async fn mark_active(&self, _agent_id: Uuid, provider_name: &str) -> Result<()> {
            self.calls.lock().unwrap().push("active".to_string());
            *self.active_provider.lock().unwrap() = Some(provider_name.to_string());
            Ok(())
        }

        async fn process_receipt(&self, _agent_id: Uuid) -> Result<()> {
            self.calls.lock().unwrap().push("receipt".to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn failover_selects_next_provider() {
        let agent = active_agent("claude-code", vec!["claude-code", "gemini-cli"]);
        let task = Task {
            task_id: agent.task_id.unwrap(),
            description: "Implement the missing handler".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent.agent_id),
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        let recorder = Arc::new(FakeRecorder {
            lines: vec!["first line".to_string(), "second line".to_string()],
            log_path: std::env::temp_dir().join("watchdog-failover.log"),
        });
        let executor = Arc::new(RecordingExecutor::default());
        let coordinator = FailoverCoordinator::with_backoff(
            Arc::new(FakeAgentRegistry {
                agent: agent.clone(),
            }),
            Arc::new(FakeTaskRegistry { task }),
            recorder,
            default_registry(),
            recorder_config(),
            executor.clone(),
            zero_backoff(),
        );

        coordinator
            .initiate(DetectedEvent::RateLimit {
                agent_id: agent.agent_id,
                matched_pattern: "429".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(
            executor.relaunched_provider.lock().unwrap().as_deref(),
            Some("gemini-cli")
        );
        assert_eq!(
            executor.active_provider.lock().unwrap().as_deref(),
            Some("gemini-cli")
        );
    }

    #[tokio::test]
    async fn failover_exhausted_marks_failed() {
        let agent = active_agent("codex", vec!["codex"]);
        let task = Task {
            task_id: agent.task_id.unwrap(),
            description: "Do the work".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent.agent_id),
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        let executor = Arc::new(RecordingExecutor::default());
        let coordinator = FailoverCoordinator::with_backoff(
            Arc::new(FakeAgentRegistry {
                agent: agent.clone(),
            }),
            Arc::new(FakeTaskRegistry { task }),
            Arc::new(FakeRecorder {
                lines: Vec::new(),
                log_path: std::env::temp_dir().join("watchdog-missing.log"),
            }),
            default_registry(),
            recorder_config(),
            executor.clone(),
            zero_backoff(),
        );

        coordinator
            .initiate(DetectedEvent::RateLimit {
                agent_id: agent.agent_id,
                matched_pattern: "429".to_string(),
            })
            .await
            .unwrap();

        let failed = executor.marked_failed.lock().unwrap();
        assert_eq!(failed.as_ref().map(|entry| entry.0), Some(agent.agent_id));
    }

    #[tokio::test]
    async fn failover_uses_recorder_context() {
        let agent = active_agent("claude-code", vec!["gemini-cli"]);
        let task = Task {
            task_id: agent.task_id.unwrap(),
            description: "Recover the interrupted task".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent.agent_id),
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        let executor = Arc::new(RecordingExecutor::default());
        let coordinator = FailoverCoordinator::with_backoff(
            Arc::new(FakeAgentRegistry {
                agent: agent.clone(),
            }),
            Arc::new(FakeTaskRegistry { task }),
            Arc::new(FakeRecorder {
                lines: vec![
                    "context line one".to_string(),
                    "context line two".to_string(),
                    "[AEGIS:DONE]".to_string(),
                ],
                log_path: std::env::temp_dir().join("watchdog-context.log"),
            }),
            default_registry(),
            recorder_config(),
            executor.clone(),
            zero_backoff(),
        );

        coordinator
            .initiate(DetectedEvent::RateLimit {
                agent_id: agent.agent_id,
                matched_pattern: "429".to_string(),
            })
            .await
            .unwrap();

        let prompt = executor.injected_prompt.lock().unwrap();
        let prompt = prompt.as_ref().unwrap();
        assert!(prompt.contains("Recover the interrupted task"));
        assert!(prompt.contains("context line one"));
        assert!(prompt.contains("context line two"));
    }

    #[tokio::test]
    async fn failover_skips_ineligible_agents() {
        let mut agent = active_agent("claude-code", vec!["gemini-cli"]);
        agent.status = AgentStatus::Paused;
        let task = Task {
            task_id: agent.task_id.unwrap(),
            description: "Do the work".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent.agent_id),
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        let executor = Arc::new(RecordingExecutor::default());
        let coordinator = FailoverCoordinator::with_backoff(
            Arc::new(FakeAgentRegistry {
                agent: agent.clone(),
            }),
            Arc::new(FakeTaskRegistry { task }),
            Arc::new(FakeRecorder {
                lines: Vec::new(),
                log_path: std::env::temp_dir().join("watchdog-skip.log"),
            }),
            default_registry(),
            recorder_config(),
            executor.clone(),
            zero_backoff(),
        );

        coordinator
            .initiate(DetectedEvent::RateLimit {
                agent_id: agent.agent_id,
                matched_pattern: "429".to_string(),
            })
            .await
            .unwrap();

        assert!(executor.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failover_follows_expected_transition_order() {
        let agent = active_agent("claude-code", vec!["gemini-cli"]);
        let task = Task {
            task_id: agent.task_id.unwrap(),
            description: "Recover the task".to_string(),
            status: TaskStatus::Active,
            assigned_agent_id: Some(agent.agent_id),
            created_by: TaskCreator::System,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        let executor = Arc::new(RecordingExecutor::default());
        let coordinator = FailoverCoordinator::with_backoff(
            Arc::new(FakeAgentRegistry {
                agent: agent.clone(),
            }),
            Arc::new(FakeTaskRegistry { task }),
            Arc::new(FakeRecorder {
                lines: vec!["tail line".to_string()],
                log_path: std::env::temp_dir().join("watchdog-order.log"),
            }),
            default_registry(),
            recorder_config(),
            executor.clone(),
            zero_backoff(),
        );

        coordinator
            .initiate(DetectedEvent::RateLimit {
                agent_id: agent.agent_id,
                matched_pattern: "429".to_string(),
            })
            .await
            .unwrap();

        let calls = executor.calls.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec![
                format!("cooling:{}", agent.agent_id),
                format!("pause:{}", agent.agent_id),
                "relaunch:gemini-cli".to_string(),
                "inject".to_string(),
                "active".to_string(),
            ]
        );
    }
}
