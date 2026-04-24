use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use aegis_core::{AegisEvent, AgentRegistry, DetectedEvent, WatchdogAction, WatchdogSink};
use uuid::Uuid;

use crate::{events::EventBus, registry::FileRegistry};

const SANDBOX_VIOLATION_PAUSE_THRESHOLD: usize = 3;

pub struct ControllerWatchdogSink {
    registry: Arc<FileRegistry>,
    events: EventBus,
    failover_enabled: bool,
    sandbox_violations: Mutex<HashMap<Uuid, usize>>,
}

impl ControllerWatchdogSink {
    pub fn new(registry: Arc<FileRegistry>, events: EventBus, failover_enabled: bool) -> Self {
        Self {
            registry,
            events,
            failover_enabled,
            sandbox_violations: Mutex::new(HashMap::new()),
        }
    }

    fn rate_limit_action(&self, agent_id: Uuid) -> WatchdogAction {
        if !self.failover_enabled {
            return WatchdogAction::PauseAndNotify;
        }

        let Ok(Some(agent)) = AgentRegistry::get(self.registry.as_ref(), agent_id) else {
            return WatchdogAction::PauseAndNotify;
        };

        let has_next_provider = agent
            .fallback_cascade
            .iter()
            .any(|provider| provider != &agent.cli_provider);

        if has_next_provider {
            WatchdogAction::InitiateFailover
        } else {
            WatchdogAction::PauseAndNotify
        }
    }

    fn sandbox_violation_action(&self, agent_id: Uuid) -> WatchdogAction {
        let mut violations = self
            .sandbox_violations
            .lock()
            .expect("sandbox violation counter poisoned");
        let count = violations.entry(agent_id).or_insert(0);
        *count += 1;

        if *count >= SANDBOX_VIOLATION_PAUSE_THRESHOLD {
            WatchdogAction::PauseAndNotify
        } else {
            WatchdogAction::LogAndContinue
        }
    }
}

impl WatchdogSink for ControllerWatchdogSink {
    fn on_event(&self, event: DetectedEvent) -> WatchdogAction {
        let action = match &event {
            DetectedEvent::RateLimit { agent_id, .. } => self.rate_limit_action(*agent_id),
            DetectedEvent::AuthFailure { .. } => WatchdogAction::PauseAndNotify,
            DetectedEvent::CliCrash { .. } => WatchdogAction::CaptureAndMarkFailed,
            DetectedEvent::SandboxViolation { agent_id, .. } => {
                self.sandbox_violation_action(*agent_id)
            }
            DetectedEvent::TaskComplete { .. } => WatchdogAction::TriggerReceiptProcessing,
        };

        self.events.publish(AegisEvent::WatchdogAlert {
            event,
            action: action.clone(),
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::{Agent, AgentKind, AgentStatus, StorageBackend};
    use chrono::Utc;

    use crate::{registry::FileRegistry, storage::ProjectStorage};

    fn sink_with_agent(
        fallback_cascade: Vec<String>,
    ) -> (ControllerWatchdogSink, Uuid, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(ProjectStorage::new(dir.path().to_path_buf()));
        storage.ensure_layout().unwrap();
        FileRegistry::init(storage.as_ref()).unwrap();
        let registry = Arc::new(FileRegistry::new(storage.clone()));
        let agent_id = Uuid::new_v4();
        let now = Utc::now();
        AgentRegistry::insert(
            registry.as_ref(),
            &Agent {
                agent_id,
                name: "worker".to_string(),
                kind: AgentKind::Splinter,
                status: AgentStatus::Active,
                role: "worker".to_string(),
                parent_id: None,
                task_id: None,
                tmux_session: "aegis".to_string(),
                tmux_window: 1,
                tmux_pane: "%0".to_string(),
                worktree_path: storage.agent_worktree_path(agent_id),
                cli_provider: "claude-code".to_string(),
                fallback_cascade,
                sandbox_profile: storage.sandbox_profile_path(agent_id),
                log_path: storage.agent_log_path(agent_id),
                created_at: now,
                updated_at: now,
                terminated_at: None,
            },
        )
        .unwrap();

        (
            ControllerWatchdogSink::new(registry, EventBus::default(), true),
            agent_id,
            dir,
        )
    }

    #[test]
    fn rate_limit_initiates_failover_when_next_provider_exists() {
        let (sink, agent_id, _dir) =
            sink_with_agent(vec!["claude-code".to_string(), "gemini-cli".to_string()]);

        let action = sink.on_event(DetectedEvent::RateLimit {
            agent_id,
            matched_pattern: "429".to_string(),
        });

        assert_eq!(action, WatchdogAction::InitiateFailover);
    }

    #[test]
    fn rate_limit_pauses_when_no_next_provider_exists() {
        let (sink, agent_id, _dir) = sink_with_agent(vec!["claude-code".to_string()]);

        let action = sink.on_event(DetectedEvent::RateLimit {
            agent_id,
            matched_pattern: "429".to_string(),
        });

        assert_eq!(action, WatchdogAction::PauseAndNotify);
    }

    #[test]
    fn non_rate_limit_events_map_to_lld_actions() {
        let (sink, agent_id, _dir) = sink_with_agent(vec!["gemini-cli".to_string()]);

        assert_eq!(
            sink.on_event(DetectedEvent::AuthFailure {
                agent_id,
                matched_pattern: "401".to_string(),
            }),
            WatchdogAction::PauseAndNotify
        );
        assert_eq!(
            sink.on_event(DetectedEvent::CliCrash {
                agent_id,
                exit_code: Some(1),
            }),
            WatchdogAction::CaptureAndMarkFailed
        );
        assert_eq!(
            sink.on_event(DetectedEvent::TaskComplete {
                agent_id,
                matched_pattern: "[AEGIS:DONE]".to_string(),
            }),
            WatchdogAction::TriggerReceiptProcessing
        );
    }

    #[test]
    fn sandbox_violations_pause_after_repeated_events() {
        let (sink, agent_id, _dir) = sink_with_agent(vec!["gemini-cli".to_string()]);

        let event = || DetectedEvent::SandboxViolation {
            agent_id,
            matched_pattern: "Operation not permitted".to_string(),
        };

        assert_eq!(sink.on_event(event()), WatchdogAction::LogAndContinue);
        assert_eq!(sink.on_event(event()), WatchdogAction::LogAndContinue);
        assert_eq!(sink.on_event(event()), WatchdogAction::PauseAndNotify);
    }
}
