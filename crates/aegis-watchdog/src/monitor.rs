use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use aegis_core::{
    config::{RecorderConfig, WatchdogConfig},
    Agent, AgentRegistry, AgentStatus, DetectedEvent, Recorder, Result, TaskRegistry,
    WatchdogAction, WatchdogSink,
};
use aegis_providers::ProviderRegistry;
use aegis_tmux::{TmuxClient, TmuxTarget};
use async_trait::async_trait;
use tokio::sync::watch;
use tracing::warn;
use uuid::Uuid;

use crate::{FailoverCoordinator, FailoverExecutor, PatternMatcher};

pub struct Watchdog {
    observer: Arc<dyn PaneObserver>,
    agents: Arc<dyn AgentRegistry>,
    executor: Arc<dyn FailoverExecutor>,
    failover: FailoverCoordinator,
    providers: Arc<ProviderRegistry>,
    sink: Arc<dyn WatchdogSink>,
    matcher: PatternMatcher,
    config: WatchdogConfig,
    recent_events: Mutex<HashMap<(Uuid, &'static str, String), Instant>>,
}

impl Watchdog {
    pub fn new(
        tmux: Arc<TmuxClient>,
        agents: Arc<dyn AgentRegistry>,
        tasks: Arc<dyn TaskRegistry>,
        recorder: Arc<dyn Recorder>,
        providers: Arc<ProviderRegistry>,
        sink: Arc<dyn WatchdogSink>,
        config: WatchdogConfig,
        recorder_config: RecorderConfig,
        executor: Arc<dyn FailoverExecutor>,
    ) -> Result<Self> {
        Self::with_observer(
            Arc::new(TmuxPaneObserver { tmux }),
            agents,
            tasks,
            recorder,
            providers,
            sink,
            config,
            recorder_config,
            executor,
        )
    }

    fn with_observer(
        observer: Arc<dyn PaneObserver>,
        agents: Arc<dyn AgentRegistry>,
        tasks: Arc<dyn TaskRegistry>,
        recorder: Arc<dyn Recorder>,
        providers: Arc<ProviderRegistry>,
        sink: Arc<dyn WatchdogSink>,
        config: WatchdogConfig,
        recorder_config: RecorderConfig,
        executor: Arc<dyn FailoverExecutor>,
    ) -> Result<Self> {
        let matcher = PatternMatcher::new(&config.patterns)?;
        let failover = FailoverCoordinator::new(
            agents.clone(),
            tasks,
            recorder,
            providers.clone(),
            recorder_config,
            executor.clone(),
        );

        Ok(Self {
            observer,
            agents,
            executor,
            failover,
            providers,
            sink,
            matcher,
            config,
            recent_events: Mutex::new(HashMap::new()),
        })
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(self.config.poll_interval_ms));

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    for event in self.sweep_once().await? {
                        self.handle_event(event).await?;
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn sweep_once(&self) -> Result<Vec<DetectedEvent>> {
        let agents = self.agents.list_active()?;
        let mut events = Vec::new();

        for agent in agents.into_iter().filter(is_monitor_eligible) {
            let target = TmuxTarget::parse(&agent.tmux_target())?;
            match self.observer.pane_exit_status(&target).await {
                Ok(Some(exit_code)) => {
                    let event = DetectedEvent::CliCrash {
                        agent_id: agent.agent_id,
                        exit_code: Some(exit_code),
                    };
                    if self.should_emit(&event) {
                        events.push(event);
                    }
                    continue;
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(
                        agent_id = %agent.agent_id,
                        error = %error,
                        "watchdog pane status check failed; treating pane as crashed"
                    );
                    let event = DetectedEvent::CliCrash {
                        agent_id: agent.agent_id,
                        exit_code: None,
                    };
                    if self.should_emit(&event) {
                        events.push(event);
                    }
                    continue;
                }
            }

            let provider = self.providers.get(&agent.cli_provider)?;
            let capture = match self
                .observer
                .capture_pane_plain(&target, self.config.scan_lines)
                .await
            {
                Ok(capture) => capture,
                Err(error) => {
                    warn!(
                        agent_id = %agent.agent_id,
                        provider = %agent.cli_provider,
                        error = %error,
                        "watchdog pane capture failed"
                    );
                    continue;
                }
            };

            if let Some(event) = self.matcher.detect(agent.agent_id, provider, &capture) {
                if self.should_emit(&event) {
                    events.push(event);
                }
            }
        }

        Ok(events)
    }

    async fn handle_event(&self, event: DetectedEvent) -> Result<()> {
        let action = self.sink.on_event(event.clone());
        match action {
            WatchdogAction::InitiateFailover if self.config.failover_enabled => {
                self.failover.initiate(event).await
            }
            WatchdogAction::PauseAndNotify => {
                if let Some(agent) = self.agents.get(event.agent_id())? {
                    self.executor.pause_current(&agent).await?;
                }
                Ok(())
            }
            WatchdogAction::CaptureAndMarkFailed => {
                let reason = event_reason(&event);
                self.executor.mark_failed(event.agent_id(), &reason).await
            }
            WatchdogAction::TriggerReceiptProcessing => {
                self.executor.process_receipt(event.agent_id()).await
            }
            WatchdogAction::LogAndContinue | WatchdogAction::InitiateFailover => Ok(()),
        }
    }

    fn should_emit(&self, event: &DetectedEvent) -> bool {
        let now = Instant::now();
        let suppression_window = Duration::from_millis(self.config.poll_interval_ms.saturating_mul(2))
            .max(Duration::from_secs(5));
        let key = suppression_key(event);

        let mut recent = self
            .recent_events
            .lock()
            .expect("watchdog recent event cache poisoned");
        recent.retain(|_, observed_at| now.duration_since(*observed_at) < suppression_window);

        match recent.get(&key) {
            Some(previous) if now.duration_since(*previous) < suppression_window => false,
            _ => {
                recent.insert(key, now);
                true
            }
        }
    }
}

fn is_monitor_eligible(agent: &Agent) -> bool {
    !matches!(
        agent.status,
        AgentStatus::Paused
            | AgentStatus::Cooling
            | AgentStatus::Reporting
            | AgentStatus::Terminated
            | AgentStatus::Failed
    )
}

fn suppression_key(event: &DetectedEvent) -> (Uuid, &'static str, String) {
    match event {
        DetectedEvent::RateLimit {
            agent_id,
            matched_pattern,
        } => (*agent_id, "rate_limit", matched_pattern.clone()),
        DetectedEvent::AuthFailure {
            agent_id,
            matched_pattern,
        } => (*agent_id, "auth_failure", matched_pattern.clone()),
        DetectedEvent::CliCrash {
            agent_id,
            exit_code,
        } => (*agent_id, "cli_crash", format!("{exit_code:?}")),
        DetectedEvent::SandboxViolation {
            agent_id,
            matched_pattern,
        } => (*agent_id, "sandbox_violation", matched_pattern.clone()),
        DetectedEvent::TaskComplete {
            agent_id,
            matched_pattern,
        } => (*agent_id, "task_complete", matched_pattern.clone()),
    }
}

fn event_reason(event: &DetectedEvent) -> String {
    match event {
        DetectedEvent::RateLimit { matched_pattern, .. } => {
            format!("rate limit detected: {matched_pattern}")
        }
        DetectedEvent::AuthFailure {
            matched_pattern, ..
        } => {
            format!("authentication failure detected: {matched_pattern}")
        }
        DetectedEvent::CliCrash { exit_code, .. } => match exit_code {
            Some(code) => format!("cli crashed with exit code {code}"),
            None => "cli pane disappeared".to_string(),
        },
        DetectedEvent::SandboxViolation {
            matched_pattern, ..
        } => {
            format!("sandbox violation detected: {matched_pattern}")
        }
        DetectedEvent::TaskComplete {
            matched_pattern, ..
        } => {
            format!("task completion detected: {matched_pattern}")
        }
    }
}

#[async_trait]
trait PaneObserver: Send + Sync {
    async fn pane_exit_status(&self, target: &TmuxTarget) -> Result<Option<i32>>;
    async fn capture_pane_plain(&self, target: &TmuxTarget, lines: usize) -> Result<String>;
}

struct TmuxPaneObserver {
    tmux: Arc<TmuxClient>,
}

#[async_trait]
impl PaneObserver for TmuxPaneObserver {
    async fn pane_exit_status(&self, target: &TmuxTarget) -> Result<Option<i32>> {
        self.tmux.pane_exit_status(target).await.map_err(Into::into)
    }

    async fn capture_pane_plain(&self, target: &TmuxTarget, lines: usize) -> Result<String> {
        self.tmux.capture_pane_plain(target, lines).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{
        config::WatchdogPatterns, AegisError, AgentKind, LogQuery, Task, TaskStatus,
    };
    use chrono::Utc;
    use std::path::PathBuf;

    fn recorder_config() -> RecorderConfig {
        RecorderConfig {
            failover_context_lines: 3,
            log_rotation_max_mb: 32,
            log_retention_count: 8,
        }
    }

    fn watchdog_config(task_complete: &[&str]) -> WatchdogConfig {
        WatchdogConfig {
            poll_interval_ms: 10,
            scan_lines: 50,
            failover_enabled: true,
            patterns: WatchdogPatterns {
                rate_limit: Vec::new(),
                auth_failure: Vec::new(),
                task_complete: task_complete.iter().map(|value| value.to_string()).collect(),
                sandbox_violation: Vec::new(),
            },
        }
    }

    fn default_registry() -> Arc<ProviderRegistry> {
        let cfg = aegis_core::config::EffectiveConfig::resolve(
            &aegis_core::config::RawConfig::default(),
            &aegis_core::config::RawConfig::default(),
        )
        .unwrap();
        Arc::new(ProviderRegistry::from_config(&cfg).unwrap())
    }

    fn agent_with_status(status: AgentStatus) -> Agent {
        let now = Utc::now();
        Agent {
            agent_id: Uuid::new_v4(),
            name: "worker".to_string(),
            kind: AgentKind::Splinter,
            status,
            role: "worker".to_string(),
            parent_id: None,
            task_id: Some(Uuid::new_v4()),
            tmux_session: "aegis".to_string(),
            tmux_window: 1,
            tmux_pane: "%1".to_string(),
            worktree_path: PathBuf::from("/tmp/worktree"),
            cli_provider: "codex".to_string(),
            fallback_cascade: vec!["gemini-cli".to_string()],
            sandbox_profile: PathBuf::from("/tmp/profile.sb"),
            log_path: PathBuf::from("/tmp/session.log"),
            created_at: now,
            updated_at: now,
            terminated_at: None,
        }
    }

    struct FakeAgentRegistry {
        agents: Vec<Agent>,
    }

    impl AgentRegistry for FakeAgentRegistry {
        fn insert(&self, _agent: &Agent) -> Result<()> {
            unimplemented!()
        }

        fn get(&self, agent_id: Uuid) -> Result<Option<Agent>> {
            Ok(self.agents.iter().find(|agent| agent.agent_id == agent_id).cloned())
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
            Ok(self.agents.clone())
        }

        fn list_by_role(&self, _role: &str) -> Result<Vec<Agent>> {
            unimplemented!()
        }

        fn list_all(&self) -> Result<Vec<Agent>> {
            Ok(self.agents.clone())
        }

        fn archive(&self, _agent_id: Uuid) -> Result<()> {
            unimplemented!()
        }
    }

    struct FakeObserver {
        capture: String,
        exit_status: Option<i32>,
        captures: Mutex<usize>,
    }

    #[async_trait]
    impl PaneObserver for FakeObserver {
        async fn pane_exit_status(&self, _target: &TmuxTarget) -> Result<Option<i32>> {
            Ok(self.exit_status)
        }

        async fn capture_pane_plain(&self, _target: &TmuxTarget, _lines: usize) -> Result<String> {
            *self.captures.lock().unwrap() += 1;
            Ok(self.capture.clone())
        }
    }

    struct NoopTaskRegistry;

    impl TaskRegistry for NoopTaskRegistry {
        fn insert(&self, _task: &Task) -> Result<()> {
            unimplemented!()
        }

        fn get(&self, _task_id: Uuid) -> Result<Option<Task>> {
            Ok(None)
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
            Ok(Vec::new())
        }
    }

    struct NoopRecorder;

    impl Recorder for NoopRecorder {
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
            Ok(Vec::new())
        }

        fn log_path(&self, agent_id: Uuid) -> PathBuf {
            std::env::temp_dir().join(format!("{agent_id}.log"))
        }
    }

    struct RecordingSink {
        action: WatchdogAction,
        events: Mutex<Vec<DetectedEvent>>,
    }

    impl Default for RecordingSink {
        fn default() -> Self {
            Self {
                action: WatchdogAction::LogAndContinue,
                events: Mutex::new(Vec::new()),
            }
        }
    }

    impl WatchdogSink for RecordingSink {
        fn on_event(&self, event: DetectedEvent) -> WatchdogAction {
            self.events.lock().unwrap().push(event);
            self.action.clone()
        }
    }

    #[derive(Default)]
    struct RecordingExecutor {
        paused: Mutex<Vec<Uuid>>,
        failed: Mutex<Vec<(Uuid, String)>>,
        receipts: Mutex<Vec<Uuid>>,
    }

    #[async_trait]
    impl FailoverExecutor for RecordingExecutor {
        async fn pause_current(&self, agent: &Agent) -> Result<()> {
            self.paused.lock().unwrap().push(agent.agent_id);
            Ok(())
        }

        async fn relaunch_with_provider(
            &self,
            agent: &Agent,
            provider_name: &str,
        ) -> Result<Agent> {
            let mut updated = agent.clone();
            updated.cli_provider = provider_name.to_string();
            Ok(updated)
        }

        async fn inject_recovery(&self, _agent: &Agent, _prompt: &str) -> Result<()> {
            Ok(())
        }

        async fn mark_failed(&self, agent_id: Uuid, reason: &str) -> Result<()> {
            self.failed
                .lock()
                .unwrap()
                .push((agent_id, reason.to_string()));
            Ok(())
        }

        async fn mark_cooling(&self, _agent_id: Uuid) -> Result<()> {
            Ok(())
        }

        async fn mark_active(&self, _agent_id: Uuid, _provider_name: &str) -> Result<()> {
            Ok(())
        }

        async fn process_receipt(&self, agent_id: Uuid) -> Result<()> {
            self.receipts.lock().unwrap().push(agent_id);
            Ok(())
        }
    }

    fn test_watchdog(
        observer: Arc<dyn PaneObserver>,
        agents: Arc<dyn AgentRegistry>,
        sink: Arc<dyn WatchdogSink>,
        executor: Arc<dyn FailoverExecutor>,
        config: WatchdogConfig,
    ) -> Watchdog {
        Watchdog::with_observer(
            observer,
            agents,
            Arc::new(NoopTaskRegistry),
            Arc::new(NoopRecorder),
            default_registry(),
            sink,
            config,
            recorder_config(),
            executor,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sweep_detects_task_complete() {
        let observer = Arc::new(FakeObserver {
            capture: "work finished [AEGIS:DONE]".to_string(),
            exit_status: None,
            captures: Mutex::new(0),
        });
        let agent = agent_with_status(AgentStatus::Active);
        let sink = Arc::new(RecordingSink::default());
        let watchdog = test_watchdog(
            observer,
            Arc::new(FakeAgentRegistry {
                agents: vec![agent.clone()],
            }),
            sink,
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        let events = watchdog.sweep_once().await.unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DetectedEvent::TaskComplete { .. }));
        assert_eq!(events[0].agent_id(), agent.agent_id);
    }

    #[tokio::test]
    async fn run_dispatches_events_until_shutdown() {
        let observer = Arc::new(FakeObserver {
            capture: "work finished [AEGIS:DONE]".to_string(),
            exit_status: None,
            captures: Mutex::new(0),
        });
        let sink = Arc::new(RecordingSink::default());
        let watchdog = Arc::new(test_watchdog(
            observer,
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Active)],
            }),
            sink.clone(),
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        ));
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let task = tokio::spawn({
            let watchdog = watchdog.clone();
            async move { watchdog.run(shutdown_rx).await }
        });

        tokio::time::sleep(Duration::from_millis(30)).await;
        shutdown_tx.send(true).unwrap();

        task.await.unwrap().unwrap();

        let events = sink.events.lock().unwrap();
        assert!(!events.is_empty());
        assert!(matches!(events[0], DetectedEvent::TaskComplete { .. }));
    }

    #[tokio::test]
    async fn monitor_skips_paused_agents() {
        let observer = Arc::new(FakeObserver {
            capture: "work finished [AEGIS:DONE]".to_string(),
            exit_status: None,
            captures: Mutex::new(0),
        });
        let watchdog = test_watchdog(
            observer.clone(),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Paused)],
            }),
            Arc::new(RecordingSink::default()),
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        let events = watchdog.sweep_once().await.unwrap();

        assert!(events.is_empty());
        assert_eq!(*observer.captures.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn capture_failures_do_not_abort_sweep() {
        struct FailingObserver;

        #[async_trait]
        impl PaneObserver for FailingObserver {
            async fn pane_exit_status(&self, _target: &TmuxTarget) -> Result<Option<i32>> {
                Ok(None)
            }

            async fn capture_pane_plain(
                &self,
                _target: &TmuxTarget,
                _lines: usize,
            ) -> Result<String> {
                Err(AegisError::TmuxPaneNotFound {
                    target: "aegis:1.%1".to_string(),
                })
            }
        }

        let watchdog = test_watchdog(
            Arc::new(FailingObserver),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Active)],
            }),
            Arc::new(RecordingSink::default()),
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        let events = watchdog.sweep_once().await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn sweep_detects_dead_pane() {
        let watchdog = test_watchdog(
            Arc::new(FakeObserver {
                capture: String::new(),
                exit_status: Some(17),
                captures: Mutex::new(0),
            }),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Active)],
            }),
            Arc::new(RecordingSink::default()),
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        let events = watchdog.sweep_once().await.unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            DetectedEvent::CliCrash {
                exit_code: Some(17),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn missing_pane_is_treated_as_cli_crash() {
        struct MissingPaneObserver;

        #[async_trait]
        impl PaneObserver for MissingPaneObserver {
            async fn pane_exit_status(&self, _target: &TmuxTarget) -> Result<Option<i32>> {
                Err(AegisError::TmuxPaneNotFound {
                    target: "aegis:1.%1".to_string(),
                })
            }

            async fn capture_pane_plain(
                &self,
                _target: &TmuxTarget,
                _lines: usize,
            ) -> Result<String> {
                panic!("capture should not run when pane status lookup fails");
            }
        }

        let watchdog = test_watchdog(
            Arc::new(MissingPaneObserver),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Active)],
            }),
            Arc::new(RecordingSink::default()),
            Arc::new(RecordingExecutor::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        let events = watchdog.sweep_once().await.unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            DetectedEvent::CliCrash {
                exit_code: None,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn handle_event_pauses_agent_when_sink_requests_notify() {
        let agent = agent_with_status(AgentStatus::Active);
        let executor = Arc::new(RecordingExecutor::default());
        let watchdog = test_watchdog(
            Arc::new(FakeObserver {
                capture: String::new(),
                exit_status: None,
                captures: Mutex::new(0),
            }),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent.clone()],
            }),
            Arc::new(RecordingSink {
                action: WatchdogAction::PauseAndNotify,
                events: Mutex::new(Vec::new()),
            }),
            executor.clone(),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        watchdog
            .handle_event(DetectedEvent::SandboxViolation {
                agent_id: agent.agent_id,
                matched_pattern: "Operation not permitted".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(executor.paused.lock().unwrap().as_slice(), &[agent.agent_id]);
    }

    #[tokio::test]
    async fn handle_event_marks_failed_when_sink_requests_failure() {
        let agent = agent_with_status(AgentStatus::Active);
        let executor = Arc::new(RecordingExecutor::default());
        let watchdog = test_watchdog(
            Arc::new(FakeObserver {
                capture: String::new(),
                exit_status: None,
                captures: Mutex::new(0),
            }),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent.clone()],
            }),
            Arc::new(RecordingSink {
                action: WatchdogAction::CaptureAndMarkFailed,
                events: Mutex::new(Vec::new()),
            }),
            executor.clone(),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        watchdog
            .handle_event(DetectedEvent::CliCrash {
                agent_id: agent.agent_id,
                exit_code: Some(9),
            })
            .await
            .unwrap();

        let failed = executor.failed.lock().unwrap();
        assert_eq!(failed[0].0, agent.agent_id);
        assert!(failed[0].1.contains("exit code 9"));
    }

    #[tokio::test]
    async fn handle_event_triggers_receipt_processing() {
        let agent = agent_with_status(AgentStatus::Active);
        let executor = Arc::new(RecordingExecutor::default());
        let watchdog = test_watchdog(
            Arc::new(FakeObserver {
                capture: String::new(),
                exit_status: None,
                captures: Mutex::new(0),
            }),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent.clone()],
            }),
            Arc::new(RecordingSink {
                action: WatchdogAction::TriggerReceiptProcessing,
                events: Mutex::new(Vec::new()),
            }),
            executor.clone(),
            watchdog_config(&["[AEGIS:DONE]"]),
        );

        watchdog
            .handle_event(DetectedEvent::TaskComplete {
                agent_id: agent.agent_id,
                matched_pattern: "[AEGIS:DONE]".to_string(),
            })
            .await
            .unwrap();

        assert_eq!(executor.receipts.lock().unwrap().as_slice(), &[agent.agent_id]);
    }
}
