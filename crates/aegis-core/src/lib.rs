pub mod agent;
pub mod channel;
pub mod error;
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
pub use error::{AegisError, Result};
pub use provider::{FailoverContext, Provider, ProviderConfig, SessionRef};
pub use recorder::{LogQuery, Recorder};
pub use sandbox::{SandboxNetworkPolicy, SandboxPolicy, SandboxProfile};
pub use storage::StorageBackend;
pub use task::{Task, TaskCreator, TaskQueue, TaskRegistry, TaskStatus};
pub use watchdog::{DetectedEvent, WatchdogAction, WatchdogSink};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Compile-time object-safety proofs. These functions never run;
    // if any trait is not object-safe, this module will not compile.
    #[allow(dead_code)]
    fn _assert_object_safe(
        _agent_registry:   &dyn AgentRegistry,
        _task_registry:    &dyn TaskRegistry,
        _task_queue:       &dyn TaskQueue,
        _channel:          &dyn Channel,
        _channel_registry: &dyn ChannelRegistry,
        _provider:         &dyn Provider,
        _sandbox_profile:  &dyn SandboxProfile,
        _recorder:         &dyn Recorder,
        _watchdog_sink:    &dyn WatchdogSink,
        _storage_backend:  &dyn StorageBackend,
        _agent_handle:     &dyn AgentHandle,
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

        let e2 = DetectedEvent::CliCrash { agent_id: id, exit_code: Some(1) };
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
        assert_eq!(s.aegis_dir(),      Path::new("/tmp/test-project/.aegis"));
        assert_eq!(s.state_dir(),      Path::new("/tmp/test-project/.aegis/state"));
        assert_eq!(s.logs_dir(),       Path::new("/tmp/test-project/.aegis/logs/sessions"));
        assert_eq!(s.registry_path(),  Path::new("/tmp/test-project/.aegis/state/registry.json"));
        assert_eq!(s.tasks_path(),     Path::new("/tmp/test-project/.aegis/state/tasks.json"));

        use uuid::Uuid;
        let id = Uuid::nil();
        let log = s.agent_log_path(id);
        assert!(log.starts_with(s.logs_dir()));
        assert!(log.to_string_lossy().ends_with(".log"));
    }

    #[test]
    fn agent_tmux_target_format() {
        use uuid::Uuid;
        use chrono::Utc;
        use std::path::PathBuf;

        let agent = Agent {
            agent_id:        Uuid::new_v4(),
            name:            "test".into(),
            kind:            AgentKind::Bastion,
            status:          AgentStatus::Active,
            role:            "architect".into(),
            parent_id:       None,
            task_id:         None,
            tmux_session:    "aegis".into(),
            tmux_window:     2,
            tmux_pane:       "%5".into(),
            worktree_path:   PathBuf::from("/tmp/wt"),
            cli_provider:    "claude-code".into(),
            fallback_cascade: vec![],
            sandbox_profile: PathBuf::from("/tmp/p.sb"),
            log_path:        PathBuf::from("/tmp/a.log"),
            created_at:      Utc::now(),
            updated_at:      Utc::now(),
            terminated_at:   None,
        };
        assert_eq!(agent.tmux_target(), "aegis:2.%5");
    }
}
