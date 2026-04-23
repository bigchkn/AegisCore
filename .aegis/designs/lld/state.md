# LLD: State & Registry (`aegis-controller` — state module)

**Milestone:** M3  
**Status:** done  
**HLD ref:** §9, §14.6  
**Implements:** `crates/aegis-controller/src/registry/` and `crates/aegis-controller/src/state/`

---

## 1. Purpose

Provides the concrete `FileRegistry` that implements `AgentRegistry`, `TaskRegistry`, and `ChannelRegistry` from `aegis-core`. State is persisted as JSON files under `.aegis/state/`. All access is protected by a file-level advisory lock so multiple CLI invocations and the daemon can coexist safely.

---

## 2. Module Structure

```
crates/aegis-controller/src/
├── registry/
│   ├── mod.rs          ← re-exports FileRegistry
│   ├── agents.rs       ← AgentRegistry impl
│   ├── tasks.rs        ← TaskRegistry impl
│   └── channels.rs     ← ChannelRegistry impl
└── state/
    ├── mod.rs          ← StateManager (snapshot writer + recovery)
    ├── snapshot.rs     ← periodic snapshot logic
    └── recovery.rs     ← boot-time recovery from last valid snapshot
```

---

## 3. Dependencies (additions to `aegis-controller/Cargo.toml`)

```toml
serde_json = "1"
fs2 = "0.4"          # advisory file locking (flock)
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tokio = { version = "1", features = ["fs", "time"] }
```

---

## 4. File Layout

All files live inside `.aegis/state/` (resolved via `StorageBackend`):

| File | Contents |
|---|---|
| `registry.json` | `{ "agents": [Agent, ...] }` |
| `tasks.json` | `{ "tasks": [Task, ...] }` |
| `channels.json` | `{ "channels": [ChannelRecord, ...] }` |
| `snapshots/registry_<iso8601>.json` | Periodic snapshots of `registry.json` |

---

## 5. Locking Strategy

All three JSON files use **advisory file locks** via `fs2::FileExt::lock_exclusive()`.

### 5.1 Lock Protocol

```
1. Open the JSON file (create if absent)
2. Acquire exclusive lock (blocking with 5s timeout → AegisError::RegistryLock)
3. Read current contents
4. Apply mutation
5. Write new contents atomically (write to <file>.tmp, rename)
6. Release lock (file closed / lock dropped)
```

Read-only operations use `lock_shared()` (allows concurrent reads; blocks writers).

### 5.2 Implementation Helper

```rust
struct LockedFile {
    file: std::fs::File,
    path: PathBuf,
}

impl LockedFile {
    fn open_exclusive(path: &Path) -> Result<Self>;
    fn open_shared(path: &Path) -> Result<Self>;
    fn read_json<T: DeserializeOwned>(&self) -> Result<T>;
    fn write_json_atomic<T: Serialize>(&self, value: &T) -> Result<()>;
}
// Drop impl releases the lock automatically.
```

---

## 6. `FileRegistry` — Agent Registry

### 6.1 On-Disk Format

```json
{
  "version": 1,
  "agents": [ /* Vec<Agent> */ ],
  "archived": [ /* Vec<Agent> terminated/failed agents */ ]
}
```

`list_active()` reads from `"agents"` only. `archive()` moves the agent to `"archived"`.

### 6.2 Impl

```rust
pub struct FileRegistry {
    storage: Arc<dyn StorageBackend>,
}

impl AgentRegistry for FileRegistry {
    fn insert(&self, agent: &Agent) -> Result<()> {
        // lock, deserialize, push to agents vec, write
    }
    fn get(&self, agent_id: Uuid) -> Result<Option<Agent>> {
        // shared lock, find by agent_id in agents + archived
    }
    fn update(&self, agent: &Agent) -> Result<()> {
        // exclusive lock, find by agent_id, replace in-place
    }
    fn update_status(&self, agent_id: Uuid, status: AgentStatus) -> Result<()> {
        // exclusive lock, find, update status + updated_at
    }
    fn update_provider(&self, agent_id: Uuid, provider: &str) -> Result<()> {
        // exclusive lock, find, update cli_provider + updated_at
    }
    fn list_active(&self) -> Result<Vec<Agent>> {
        // shared lock, return agents where !status.is_terminal()
    }
    fn list_by_role(&self, role: &str) -> Result<Vec<Agent>> {
        // shared lock, filter active by role
    }
    fn list_all(&self) -> Result<Vec<Agent>> {
        // shared lock, return all agents (active + archived)
    }
    fn archive(&self, agent_id: Uuid) -> Result<()> {
        // exclusive lock, move from agents → archived, set terminated_at
    }
}
```

---

## 7. Task Registry

### 7.1 On-Disk Format

```json
{
  "version": 1,
  "tasks": [ /* Vec<Task> */ ]
}
```

### 7.2 `TaskQueue` Impl

`claim_next()` is atomic: lock → find first `Queued` task → set status to `Active` + assign agent → write → unlock. Returns the claimed task or `None`.

```rust
impl TaskQueue for FileRegistry {
    fn enqueue(&self, description: &str, created_by: TaskCreator) -> Result<Uuid> {
        let task = Task {
            task_id: Uuid::new_v4(),
            description: description.to_string(),
            status: TaskStatus::Queued,
            assigned_agent_id: None,
            created_by,
            created_at: Utc::now(),
            completed_at: None,
            receipt_path: None,
        };
        // exclusive lock, push, write
        Ok(task.task_id)
    }
    fn claim_next(&self, agent_id: Uuid) -> Result<Option<Task>> {
        // exclusive lock, find first Queued, transition to Active, assign agent_id
    }
    fn pending_count(&self) -> Result<usize> {
        // shared lock, count Queued tasks
    }
}
```

---

## 8. Channel Registry

### 8.1 On-Disk Format

```json
{
  "version": 1,
  "channels": [ /* Vec<ChannelRecord> */ ]
}
```

### 8.2 Impl

```rust
impl ChannelRegistry for FileRegistry {
    fn register(&self, name: &str, kind: ChannelKind) -> Result<()>;
    fn deregister(&self, name: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<ChannelRecord>>;
    fn list(&self) -> Result<Vec<ChannelRecord>>;
}
```

---

## 9. `StateManager` — Snapshots & Recovery

### 9.1 Snapshot Writer

Runs as a `tokio::spawn` task inside the daemon. Wakes on `snapshot_interval_s` and copies the current `registry.json` to:

```
.aegis/state/snapshots/registry_<RFC3339>.json
```

After writing, prunes oldest snapshots to keep `snapshot_retention_count` files.

```rust
pub struct StateManager {
    storage: Arc<dyn StorageBackend>,
    config: StateConfig,
}

impl StateManager {
    /// Spawn the snapshot background task. Returns a handle to cancel it.
    pub fn start(&self) -> tokio::task::JoinHandle<()>;

    /// Write a snapshot immediately (also called on clean shutdown).
    pub fn snapshot_now(&self) -> Result<PathBuf>;

    /// Prune oldest snapshots, keeping at most retention_count files.
    fn prune_snapshots(&self) -> Result<()>;
}
```

### 9.2 Boot Recovery

At daemon startup, before accepting any connections:

```rust
pub fn recover(storage: &dyn StorageBackend) -> Result<RecoveryResult>;

pub struct RecoveryResult {
    pub registry_restored: bool,
    pub snapshot_used: Option<PathBuf>,
    pub agents_recovered: usize,
    pub agents_marked_failed: usize,
}
```

Recovery sequence:

1. Try to parse `registry.json`. If valid → no action needed.
2. If corrupt/absent → find the most recent valid snapshot in `snapshots/`.
3. Copy the snapshot to `registry.json`.
4. All agents in status `Starting`, `Active`, `Cooling`, or `Reporting` are transitioned to `Failed` (their tmux panes no longer exist after a daemon restart).
5. Agents in `Queued` remain `Queued` (they can be re-dispatched).
6. Log a `WARN` entry per recovered agent.

---

## 10. Initialization (`aegis init` path)

When the state directory does not exist, `FileRegistry::init()` creates it:

```rust
impl FileRegistry {
    pub fn init(storage: &dyn StorageBackend) -> Result<()> {
        fs::create_dir_all(storage.state_dir())?;
        fs::create_dir_all(storage.snapshots_dir())?;
        // Write empty registry and tasks files if absent
        Self::write_if_absent(&storage.registry_path(), &AgentStore::default())?;
        Self::write_if_absent(&storage.tasks_path(), &TaskStore::default())?;
        Self::write_if_absent(&storage.channels_state_path(), &ChannelStore::default())?;
        Ok(())
    }
}
```

---

## 11. Test Strategy

| Test | Asserts |
|---|---|
| `test_insert_and_get_agent` | Round-trip: insert → get returns same agent |
| `test_update_status` | Status change reflected in subsequent get |
| `test_archive_agent` | Archived agent absent from `list_active`, present in `list_all` |
| `test_claim_next_atomic` | Two concurrent `claim_next` calls each get a different task |
| `test_registry_survives_concurrent_writes` | 10 threads writing simultaneously; no corruption |
| `test_snapshot_written_and_pruned` | Snapshot created; count does not exceed retention limit |
| `test_recovery_from_snapshot` | Corrupt `registry.json` → boot reads snapshot; active agents → failed |
| `test_lock_timeout` | Lock held by one process → second times out with `RegistryLock` error |
