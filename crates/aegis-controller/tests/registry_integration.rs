use aegis_controller::registry::FileRegistry;
use aegis_controller::state::StateManager;
use aegis_core::agent::{Agent, AgentKind, AgentRegistry, AgentStatus};
use aegis_core::storage::StorageBackend;
use aegis_core::task::{TaskCreator, TaskQueue};
use chrono::Utc;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;

struct MockStorage {
    root: PathBuf,
}

impl StorageBackend for MockStorage {
    fn project_root(&self) -> &Path {
        &self.root
    }
}

fn setup_registry() -> (FileRegistry, Arc<MockStorage>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let storage = Arc::new(MockStorage {
        root: dir.path().to_path_buf(),
    });

    FileRegistry::init(storage.as_ref()).unwrap();

    let registry = FileRegistry::new(storage.clone());
    (registry, storage, dir)
}

#[test]
fn test_agent_roundtrip() {
    let (registry, _, _dir) = setup_registry();
    let agent_id = Uuid::new_v4();
    let agent = Agent {
        agent_id,
        name: "test-agent".to_string(),
        kind: AgentKind::Bastion,
        status: AgentStatus::Starting,
        role: "tester".to_string(),
        parent_id: None,
        task_id: None,
        tmux_session: "aegis".to_string(),
        tmux_window: 1,
        tmux_pane: "%1".to_string(),
        worktree_path: PathBuf::from("/tmp"),
        cli_provider: "claude-code".to_string(),
        fallback_cascade: vec![],
        sandbox_profile: PathBuf::from("/tmp/test.sb"),
        log_path: PathBuf::from("/tmp/test.log"),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        terminated_at: None,
    };

    AgentRegistry::insert(&registry, &agent).unwrap();
    let fetched = AgentRegistry::get(&registry, agent_id)
        .unwrap()
        .expect("Agent not found");
    assert_eq!(fetched.name, "test-agent");
    assert_eq!(fetched.status, AgentStatus::Starting);

    AgentRegistry::update_status(&registry, agent_id, AgentStatus::Active).unwrap();
    let updated = AgentRegistry::get(&registry, agent_id).unwrap().unwrap();
    assert_eq!(updated.status, AgentStatus::Active);
}

#[test]
fn test_task_claiming() {
    let (registry, _, _dir) = setup_registry();
    let agent_id = Uuid::new_v4();

    let t1 = TaskQueue::enqueue(&registry, "Task 1", TaskCreator::System).unwrap();
    let t2 = TaskQueue::enqueue(&registry, "Task 2", TaskCreator::System).unwrap();

    assert_eq!(TaskQueue::pending_count(&registry).unwrap(), 2);

    let claimed = TaskQueue::claim_next(&registry, agent_id)
        .unwrap()
        .expect("Should claim t1");
    assert_eq!(claimed.task_id, t1);
    assert_eq!(claimed.description, "Task 1");

    assert_eq!(TaskQueue::pending_count(&registry).unwrap(), 1);

    let claimed2 = TaskQueue::claim_next(&registry, agent_id)
        .unwrap()
        .expect("Should claim t2");
    assert_eq!(claimed2.task_id, t2);
}

#[test]
fn test_snapshot_and_recovery() {
    let (registry, storage, _dir) = setup_registry();
    let state_manager = StateManager::new(storage.clone());

    // 1. Add some state
    let agent_id = Uuid::new_v4();
    let agent = Agent {
        agent_id,
        name: "active-agent".to_string(),
        kind: AgentKind::Bastion,
        status: AgentStatus::Active,
        role: "tester".to_string(),
        parent_id: None,
        task_id: None,
        tmux_session: "aegis".to_string(),
        tmux_window: 1,
        tmux_pane: "%1".to_string(),
        worktree_path: PathBuf::from("/tmp"),
        cli_provider: "claude-code".to_string(),
        fallback_cascade: vec![],
        sandbox_profile: PathBuf::from("/tmp/test.sb"),
        log_path: PathBuf::from("/tmp/test.log"),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        terminated_at: None,
    };
    AgentRegistry::insert(&registry, &agent).unwrap();

    // 2. Snapshot
    state_manager.snapshot_now().unwrap();

    // 3. Corrupt registry
    std::fs::write(storage.registry_path(), "CORRUPT JSON").unwrap();

    // 4. Recover
    let result = state_manager.recover().unwrap();
    assert!(result.registry_restored);
    assert_eq!(result.agents_marked_failed, 1);

    // 5. Verify agent status changed to Failed
    let recovered = AgentRegistry::get(&registry, agent_id).unwrap().unwrap();
    assert_eq!(recovered.status, AgentStatus::Failed);
}
