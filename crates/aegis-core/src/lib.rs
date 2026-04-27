pub mod agent;
pub mod channel;
pub mod config;
pub mod error;
pub mod lock;
pub mod provider;
pub mod recorder;
pub mod sandbox;
pub mod storage;
pub mod task;
pub mod watchdog;

pub use agent::{Agent, AgentHandle, AgentKind, AgentRegistry, AgentStatus};
pub use channel::{
    Channel, ChannelKind, ChannelRecord, ChannelRegistry, Message, MessageSource, MessageType,
};
pub use config::{ConfigError, EffectiveConfig, RawConfig};
pub use error::{AegisError, Result};
pub use lock::LockedFile;
pub use provider::{FailoverContext, Provider, ProviderConfig, SessionRef, SystemPromptMechanism};
pub use recorder::{LogQuery, Recorder};
pub use sandbox::{SandboxNetworkPolicy, SandboxPolicy, SandboxProfile};
pub use storage::StorageBackend;
pub use task::{Task, TaskCreator, TaskQueue, TaskRegistry, TaskStatus};
pub use watchdog::{DetectedEvent, WatchdogAction, WatchdogSink};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum AegisEvent {
    AgentSpawned {
        agent_id: uuid::Uuid,
        role: String,
    },
    AgentStatusChanged {
        agent_id: uuid::Uuid,
        old_status: AgentStatus,
        new_status: AgentStatus,
    },
    TaskComplete {
        task_id: uuid::Uuid,
        receipt_path: String,
    },
    WatchdogAlert {
        event: DetectedEvent,
        action: WatchdogAction,
    },
    SystemNotification {
        message: String,
    },
    AgentTerminated {
        agent_id: uuid::Uuid,
        reason: String,
    },
    FailoverInitiated {
        agent_id: uuid::Uuid,
        from_provider: String,
        to_provider: String,
    },
    TaskAssigned {
        task_id: uuid::Uuid,
        agent_id: uuid::Uuid,
    },
    ChannelAdded {
        channel_name: String,
        channel_type: ChannelKind,
    },
    ChannelRemoved {
        channel_name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Compile-time object-safety proofs. These functions never run;
    // if any trait is not object-safe, this module will not compile.
    #[allow(dead_code)]
    fn _assert_object_safe(
        _agent_registry: &dyn AgentRegistry,
        _task_registry: &dyn TaskRegistry,
        _task_queue: &dyn TaskQueue,
        _channel: &dyn Channel,
        _channel_registry: &dyn ChannelRegistry,
        _provider: &dyn Provider,
        _sandbox_profile: &dyn SandboxProfile,
        _recorder: &dyn Recorder,
        _watchdog_sink: &dyn WatchdogSink,
        _storage_backend: &dyn StorageBackend,
        _agent_handle: &dyn AgentHandle,
    ) {
    }

    #[test]
    fn agent_status_terminal() {
        assert!(AgentStatus::Terminated.is_terminal());
        assert!(AgentStatus::Failed.is_terminal());
        assert!(!AgentStatus::Active.is_terminal());
        assert!(!AgentStatus::Cooling.is_terminal());
    }

    #[test]
    fn channel_kind_implicit() {
        assert!(ChannelKind::Injection.is_implicit());
        assert!(ChannelKind::Observation.is_implicit());
        assert!(!ChannelKind::Mailbox.is_implicit());
        assert!(!ChannelKind::Telegram.is_implicit());
        assert!(!ChannelKind::Broadcast.is_implicit());
    }

    #[test]
    fn message_new_sets_defaults() {
        use uuid::Uuid;
        let to = Uuid::new_v4();
        let msg = Message::new(
            MessageSource::System,
            to,
            MessageType::Task,
            serde_json::Value::Null,
        );
        assert_eq!(msg.to_agent_id, to);
        assert_eq!(msg.priority, 0);
        assert!(matches!(msg.from, MessageSource::System));
    }

    #[test]
    fn detected_event_agent_id() {
        use uuid::Uuid;
        let id = Uuid::new_v4();
        let e = DetectedEvent::RateLimit {
            agent_id: id,
            matched_pattern: "429".into(),
        };
        assert_eq!(e.agent_id(), id);

        let e2 = DetectedEvent::CliCrash {
            agent_id: id,
            exit_code: Some(1),
        };
        assert_eq!(e2.agent_id(), id);
    }

    #[test]
    fn sandbox_policy_default_is_outbound_only() {
        let policy = SandboxPolicy::default();
        assert_eq!(policy.network, SandboxNetworkPolicy::OutboundOnly);
        assert!(policy.extra_reads.is_empty());
        assert!(policy.extra_writes.is_empty());
    }

    #[test]
    fn storage_backend_paths_derive_from_root() {
        struct TestStorage;
        impl StorageBackend for TestStorage {
            fn project_root(&self) -> &Path {
                Path::new("/tmp/test-project")
            }
        }
        let s = TestStorage;
        assert_eq!(s.aegis_dir(), Path::new("/tmp/test-project/.aegis"));
        assert_eq!(s.state_dir(), Path::new("/tmp/test-project/.aegis/state"));
        assert_eq!(
            s.logs_dir(),
            Path::new("/tmp/test-project/.aegis/logs/sessions")
        );
        assert_eq!(
            s.registry_path(),
            Path::new("/tmp/test-project/.aegis/state/registry.json")
        );
        assert_eq!(
            s.tasks_path(),
            Path::new("/tmp/test-project/.aegis/state/tasks.json")
        );
        assert_eq!(
            s.clarifications_path(),
            Path::new("/tmp/test-project/.aegis/state/clarifications.json")
        );
        assert_eq!(
            s.human_inbox_path(),
            Path::new("/tmp/test-project/.aegis/channels/human/inbox")
        );

        use uuid::Uuid;
        let id = Uuid::nil();
        let log = s.agent_log_path(id);
        assert!(log.starts_with(s.logs_dir()));
        assert!(log.to_string_lossy().ends_with(".log"));
        assert_eq!(
            s.clarification_inbox_path(id),
            Path::new("/tmp/test-project/.aegis/channels/human/inbox/00000000-0000-0000-0000-000000000000.json")
        );
    }

    #[test]
    fn agent_tmux_target_format() {
        use chrono::Utc;
        use std::path::PathBuf;
        use uuid::Uuid;

        let agent = Agent {
            agent_id: Uuid::new_v4(),
            name: "test".into(),
            kind: AgentKind::Bastion,
            status: AgentStatus::Active,
            role: "architect".into(),
            parent_id: None,
            task_id: None,
            tmux_session: "aegis".into(),
            tmux_window: 2,
            tmux_pane: "%5".into(),
            worktree_path: PathBuf::from("/tmp/wt"),
            cli_provider: "claude-code".into(),
            fallback_cascade: vec![],
            sandbox_profile: PathBuf::from("/tmp/p.sb"),
            log_path: PathBuf::from("/tmp/a.log"),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            terminated_at: None,
        };
        assert_eq!(agent.tmux_target(), "aegis:2.%5");
    }
}

#[cfg(all(test, feature = "ts-export"))]
mod ts_export {
    use crate::*;
    use ts_rs::TS;

    #[test]
    fn export_ts_bindings() {
        let out = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/aegis-web/frontend/src/types"
        );
        std::fs::create_dir_all(out).unwrap();
        Agent::export_all_to(out).unwrap();
        AegisEvent::export_all_to(out).unwrap();
        Task::export_all_to(out).unwrap();
        ChannelRecord::export_all_to(out).unwrap();
    }
}
