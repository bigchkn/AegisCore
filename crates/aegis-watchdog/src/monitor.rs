use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use aegis_core::{
    config::WatchdogConfig, Agent, AgentRegistry, AgentStatus, DetectedEvent, Result,
    WatchdogAction, WatchdogSink,
};
use aegis_providers::ProviderRegistry;
use aegis_tmux::{TmuxClient, TmuxTarget};
use async_trait::async_trait;
use tokio::sync::watch;
use tracing::warn;
use uuid::Uuid;

use crate::PatternMatcher;

pub struct Watchdog {
    observer: Arc<dyn PaneObserver>,
    agents: Arc<dyn AgentRegistry>,
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
        providers: Arc<ProviderRegistry>,
        sink: Arc<dyn WatchdogSink>,
        config: WatchdogConfig,
    ) -> Result<Self> {
        Self::with_observer(
            Arc::new(TmuxPaneObserver { tmux }),
            agents,
            providers,
            sink,
            config,
        )
    }

    fn with_observer(
        observer: Arc<dyn PaneObserver>,
        agents: Arc<dyn AgentRegistry>,
        providers: Arc<ProviderRegistry>,
        sink: Arc<dyn WatchdogSink>,
        config: WatchdogConfig,
    ) -> Result<Self> {
        Ok(Self {
            observer,
            agents,
            providers,
            sink,
            matcher: PatternMatcher::new(&config.patterns)?,
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
            let provider = self.providers.get(&agent.cli_provider)?;
            let target = TmuxTarget::parse(&agent.tmux_target())?;
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
        match self.sink.on_event(event) {
            WatchdogAction::InitiateFailover
            | WatchdogAction::PauseAndNotify
            | WatchdogAction::CaptureAndMarkFailed
            | WatchdogAction::LogAndContinue
            | WatchdogAction::TriggerReceiptProcessing => Ok(()),
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

#[async_trait]
trait PaneObserver: Send + Sync {
    async fn capture_pane_plain(&self, target: &TmuxTarget, lines: usize) -> Result<String>;
}

struct TmuxPaneObserver {
    tmux: Arc<TmuxClient>,
}

#[async_trait]
impl PaneObserver for TmuxPaneObserver {
    async fn capture_pane_plain(&self, target: &TmuxTarget, lines: usize) -> Result<String> {
        self.tmux.capture_pane_plain(target, lines).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{
        config::WatchdogPatterns, AegisError, AgentKind,
    };
    use chrono::Utc;
    use std::path::PathBuf;
    use uuid::Uuid;

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
            task_id: None,
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
        captures: Mutex<usize>,
    }

    #[async_trait]
    impl PaneObserver for FakeObserver {
        async fn capture_pane_plain(&self, _target: &TmuxTarget, _lines: usize) -> Result<String> {
            *self.captures.lock().unwrap() += 1;
            Ok(self.capture.clone())
        }
    }

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<DetectedEvent>>,
    }

    impl WatchdogSink for RecordingSink {
        fn on_event(&self, event: DetectedEvent) -> WatchdogAction {
            self.events.lock().unwrap().push(event);
            WatchdogAction::LogAndContinue
        }
    }

    #[tokio::test]
    async fn sweep_detects_task_complete() {
        let observer = Arc::new(FakeObserver {
            capture: "work finished [AEGIS:DONE]".to_string(),
            captures: Mutex::new(0),
        });
        let agent = agent_with_status(AgentStatus::Active);
        let sink = Arc::new(RecordingSink::default());
        let watchdog = Watchdog::with_observer(
            observer,
            Arc::new(FakeAgentRegistry {
                agents: vec![agent.clone()],
            }),
            default_registry(),
            sink,
            watchdog_config(&["[AEGIS:DONE]"]),
        )
        .unwrap();

        let events = watchdog.sweep_once().await.unwrap();

        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], DetectedEvent::TaskComplete { .. }));
        assert_eq!(events[0].agent_id(), agent.agent_id);
    }

    #[tokio::test]
    async fn run_dispatches_events_until_shutdown() {
        let observer = Arc::new(FakeObserver {
            capture: "work finished [AEGIS:DONE]".to_string(),
            captures: Mutex::new(0),
        });
        let sink = Arc::new(RecordingSink::default());
        let watchdog = Arc::new(
            Watchdog::with_observer(
                observer,
                Arc::new(FakeAgentRegistry {
                    agents: vec![agent_with_status(AgentStatus::Active)],
                }),
                default_registry(),
                sink.clone(),
                watchdog_config(&["[AEGIS:DONE]"]),
            )
            .unwrap(),
        );
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
            captures: Mutex::new(0),
        });
        let watchdog = Watchdog::with_observer(
            observer.clone(),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Paused)],
            }),
            default_registry(),
            Arc::new(RecordingSink::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        )
        .unwrap();

        let events = watchdog.sweep_once().await.unwrap();

        assert!(events.is_empty());
        assert_eq!(*observer.captures.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn capture_failures_do_not_abort_sweep() {
        struct FailingObserver;

        #[async_trait]
        impl PaneObserver for FailingObserver {
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

        let watchdog = Watchdog::with_observer(
            Arc::new(FailingObserver),
            Arc::new(FakeAgentRegistry {
                agents: vec![agent_with_status(AgentStatus::Active)],
            }),
            default_registry(),
            Arc::new(RecordingSink::default()),
            watchdog_config(&["[AEGIS:DONE]"]),
        )
        .unwrap();

        let events = watchdog.sweep_once().await.unwrap();
        assert!(events.is_empty());
    }
}
