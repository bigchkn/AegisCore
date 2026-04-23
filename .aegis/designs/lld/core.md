# LLD: `aegis-core`

**Milestone:** M0  
**Status:** done  
**HLD ref:** §2.2, §3, §4, §6.1, §7, §8, §9, §10, §14.5  
**Implements:** Zero subsystem logic — traits, types, errors only.

---

## 1. Purpose

`aegis-core` is the contract crate. It defines every trait and shared type that subsystem crates implement or consume. Nothing in `aegis-core` knows about tmux, files, HTTP, or any specific CLI tool. It must compile with zero platform-specific dependencies.

**Rule:** If a type or trait references a concrete external system (tmux, sandbox-exec, Telegram API), it does not belong in `aegis-core`.

---

## 2. Module Structure

```
crates/aegis-core/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-exports all public items; no logic
    ├── agent.rs        ← Agent, AgentKind, AgentStatus, AgentHandle trait, AgentRegistry trait
    ├── task.rs         ← Task, TaskStatus, TaskCreator, TaskQueue trait, TaskRegistry trait
    ├── channel.rs      ← ChannelKind, MessageType, MessageSource, Message, Channel trait, ChannelRegistry trait
    ├── provider.rs     ← ProviderConfig, SessionRef, FailoverContext, Provider trait
    ├── sandbox.rs      ← SandboxNetworkPolicy, SandboxPolicy, SandboxProfile trait
    ├── recorder.rs     ← LogQuery, Recorder trait
    ├── watchdog.rs     ← DetectedEvent, WatchdogAction, WatchdogSink trait
    ├── storage.rs      ← StorageBackend trait
    └── error.rs        ← AegisError, Result
```

---

## 3. Dependencies (`Cargo.toml`)

```toml
[dependencies]
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"

[dev-dependencies]
# none required — core has no I/O to test directly
```

No async runtime dependency. Traits are defined as sync; async variants are left to implementing crates using `async-trait` if needed.

---

## 4. `agent.rs`

### 4.1 Types

```rust
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Bastion,
    Splinter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Queued,
    Starting,
    Active,
    Paused,
    Cooling,
    Reporting,
    Terminated,
    Failed,
}

impl AgentStatus {
    /// Returns true if this is a terminal state — no further transitions expected.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Terminated | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: Uuid,
    pub name: String,
    pub kind: AgentKind,
    pub status: AgentStatus,
    pub role: String,
    pub parent_id: Option<Uuid>,       // Bastion that spawned this Splinter
    pub task_id: Option<Uuid>,
    pub tmux_session: String,
    pub tmux_window: u32,
    pub tmux_pane: String,             // tmux pane ID e.g. "%3"
    pub worktree_path: PathBuf,
    pub cli_provider: String,          // current active provider name
    pub fallback_cascade: Vec<String>, // ordered list of fallback provider names
    pub sandbox_profile: PathBuf,
    pub log_path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub terminated_at: Option<DateTime<Utc>>,
}

impl Agent {
    /// Returns the tmux target string used in send-keys / capture-pane calls.
    pub fn tmux_target(&self) -> String {
        format!("{}:{}.{}", self.tmux_session, self.tmux_window, self.tmux_pane)
    }
}
```

### 4.2 `AgentHandle` Trait

A live reference to a running agent process. Implemented by `aegis-controller`'s `RunningAgent`.

```rust
pub trait AgentHandle: Send + Sync {
    fn agent_id(&self) -> Uuid;
    fn tmux_target(&self) -> String;
    fn worktree_path(&self) -> &std::path::Path;
    /// Returns false if the tmux pane no longer exists or the process has exited.
    fn is_alive(&self) -> bool;
}
```

### 4.3 `AgentRegistry` Trait

Persistent store for agent records. Implemented by `FileRegistry` in `aegis-controller`.

```rust
use crate::error::Result;

pub trait AgentRegistry: Send + Sync {
    fn insert(&self, agent: &Agent) -> Result<()>;
    fn get(&self, agent_id: Uuid) -> Result<Option<Agent>>;
    fn update(&self, agent: &Agent) -> Result<()>;
    fn update_status(&self, agent_id: Uuid, status: AgentStatus) -> Result<()>;
    fn update_provider(&self, agent_id: Uuid, provider: &str) -> Result<()>;
    fn list_active(&self) -> Result<Vec<Agent>>;
    fn list_by_role(&self, role: &str) -> Result<Vec<Agent>>;
    fn list_all(&self) -> Result<Vec<Agent>>;
    /// Move agent to archived state; removes from active query results.
    fn archive(&self, agent_id: Uuid) -> Result<()>;
}
```

---

## 5. `task.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Active,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCreator {
    Agent(Uuid),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: Uuid,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_agent_id: Option<Uuid>,
    pub created_by: TaskCreator,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub receipt_path: Option<PathBuf>,
}

pub trait TaskQueue: Send + Sync {
    /// Enqueue a new task; assigns a UUID. Returns the assigned task_id.
    fn enqueue(&self, description: &str, created_by: TaskCreator) -> Result<Uuid>;
    /// Claim the next queued task for an agent. Returns None if queue is empty.
    fn claim_next(&self, agent_id: Uuid) -> Result<Option<Task>>;
    fn pending_count(&self) -> Result<usize>;
}

pub trait TaskRegistry: Send + Sync {
    fn insert(&self, task: &Task) -> Result<()>;
    fn get(&self, task_id: Uuid) -> Result<Option<Task>>;
    fn update_status(&self, task_id: Uuid, status: TaskStatus) -> Result<()>;
    fn assign(&self, task_id: Uuid, agent_id: Uuid) -> Result<()>;
    fn complete(&self, task_id: Uuid, receipt_path: Option<PathBuf>) -> Result<()>;
    fn list_pending(&self) -> Result<Vec<Task>>;
    fn list_all(&self) -> Result<Vec<Task>>;
}
```

---

## 6. `channel.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    Injection,   // implicit — tmux send-keys
    Mailbox,     // explicit — filesystem drop-box
    Observation, // implicit — tmux capture-pane
    Broadcast,   // explicit — fan-out via Mailbox
    Telegram,    // explicit — Telegram Bot API
}

impl ChannelKind {
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::Injection | Self::Observation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Task,
    Handoff,
    Notification,
    Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    Agent(Uuid),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub from: MessageSource,
    pub to_agent_id: Uuid,
    pub kind: MessageType,
    pub priority: i32, // higher = more urgent; default 0
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

impl Message {
    pub fn new(
        from: MessageSource,
        to_agent_id: Uuid,
        kind: MessageType,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            from,
            to_agent_id,
            kind,
            priority: 0,
            payload,
            created_at: Utc::now(),
        }
    }
}

pub trait Channel: Send + Sync {
    fn kind(&self) -> ChannelKind;
    fn name(&self) -> &str;
    fn is_active(&self) -> bool;
    fn send(&self, message: &Message) -> Result<()>;
}

/// Tracks explicitly-registered channel instances.
pub trait ChannelRegistry: Send + Sync {
    fn register(&self, name: &str, kind: ChannelKind) -> Result<()>;
    fn deregister(&self, name: &str) -> Result<()>;
    fn get(&self, name: &str) -> Result<Option<ChannelRecord>>;
    fn list(&self) -> Result<Vec<ChannelRecord>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRecord {
    pub name: String,
    pub kind: ChannelKind,
    pub active: bool,
    pub registered_at: DateTime<Utc>,
    pub config: serde_json::Value, // channel-specific config blob
}
```

---

## 7. `provider.rs`

```rust
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub binary: String,
    pub extra_args: Vec<String>,
    pub resume_flag: Option<String>,
    pub model: Option<String>, // for ollama-style providers
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRef {
    pub provider: String,
    pub session_id: String,
    pub checkpoint: Option<String>, // e.g. gemini checkpoint name
}

#[derive(Debug, Clone)]
pub struct FailoverContext {
    pub agent_id: Uuid,
    pub task_id: Option<Uuid>,
    pub previous_provider: String,
    /// Last N lines from the Flight Recorder log.
    pub terminal_context: String,
    pub task_description: Option<String>,
    pub worktree_path: PathBuf,
    pub role: String,
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    fn config(&self) -> &ProviderConfig;

    /// Build the Command to launch this provider in the given worktree.
    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command;

    /// Arguments to append to spawn_command for resuming a session.
    fn resume_args(&self, session: &SessionRef) -> Vec<String>;

    /// Shell command to inject into the pane to trigger a context export.
    /// Returns None if the provider has no export mechanism.
    fn export_context_command(&self) -> Option<&str>;

    // ── Pattern detection (called against captured pane lines) ──────────

    fn is_rate_limit_error(&self, line: &str) -> bool;
    fn is_auth_error(&self, line: &str) -> bool;
    fn is_task_complete(&self, line: &str) -> bool; // user-defined patterns via config

    // ── Handoff ─────────────────────────────────────────────────────────

    /// Generate the prompt to inject into the receiving provider at failover time.
    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String;
}
```

---

## 8. `sandbox.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkPolicy {
    None,
    #[default]
    OutboundOnly,
    Any,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SandboxPolicy {
    pub network: SandboxNetworkPolicy,
    pub extra_reads: Vec<PathBuf>,
    pub extra_writes: Vec<PathBuf>,
    /// Paths explicitly denied even if a parent path is allowed.
    pub hard_deny_reads: Vec<PathBuf>,
}

pub trait SandboxProfile: Send + Sync {
    /// Render the `.sb` profile content as a string.
    fn render(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy) -> Result<String>;

    /// Render and write the profile to `dest`. Returns the path written.
    fn write(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy, dest: &Path) -> Result<()>;

    /// Returns the arguments to prefix any command with for sandbox execution.
    /// e.g. `["sandbox-exec", "-f", "/path/to/profile.sb"]`
    fn exec_prefix(&self, profile_path: &Path) -> Vec<String>;
}
```

---

## 9. `recorder.rs`

```rust
#[derive(Debug, Clone)]
pub struct LogQuery {
    pub agent_id: Uuid,
    /// Number of trailing lines to return. None = entire log.
    pub last_n_lines: Option<usize>,
    /// Only return lines after this timestamp (best-effort; log lines are raw terminal output).
    pub since: Option<DateTime<Utc>>,
    /// If true, the caller intends to stream; implementation may return a reader.
    pub follow: bool,
}

pub trait Recorder: Send + Sync {
    /// Attach a flight recorder to the agent's tmux pane. Called at spawn time.
    fn attach(&self, agent: &Agent) -> Result<()>;

    /// Detach the recorder. Called before pane is closed.
    fn detach(&self, agent_id: Uuid) -> Result<()>;

    /// Move the log file to archive; returns the archive path.
    fn archive(&self, agent_id: Uuid) -> Result<PathBuf>;

    /// Return lines from the agent's log matching the query.
    fn query(&self, query: &LogQuery) -> Result<Vec<String>>;

    /// Canonical path for a live session log.
    fn log_path(&self, agent_id: Uuid) -> PathBuf;
}
```

---

## 10. `watchdog.rs`

```rust
#[derive(Debug, Clone)]
pub enum DetectedEvent {
    RateLimit {
        agent_id: Uuid,
        matched_pattern: String,
    },
    AuthFailure {
        agent_id: Uuid,
        matched_pattern: String,
    },
    CliCrash {
        agent_id: Uuid,
        exit_code: Option<i32>,
    },
    SandboxViolation {
        agent_id: Uuid,
        matched_pattern: String,
    },
    TaskComplete {
        agent_id: Uuid,
        matched_pattern: String,
    },
}

impl DetectedEvent {
    pub fn agent_id(&self) -> Uuid {
        match self {
            Self::RateLimit { agent_id, .. }
            | Self::AuthFailure { agent_id, .. }
            | Self::CliCrash { agent_id, .. }
            | Self::SandboxViolation { agent_id, .. }
            | Self::TaskComplete { agent_id, .. } => *agent_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchdogAction {
    InitiateFailover,
    PauseAndNotify,
    CaptureAndMarkFailed,
    LogAndContinue,
    TriggerReceiptProcessing,
}

/// Receives detected events from the Watchdog monitor.
/// Implemented by the Controller, which decides and executes the action.
pub trait WatchdogSink: Send + Sync {
    fn on_event(&self, event: DetectedEvent) -> WatchdogAction;
}
```

---

## 11. `storage.rs`

```rust
pub trait StorageBackend: Send + Sync {
    fn project_root(&self) -> &Path;

    fn aegis_dir(&self) -> PathBuf {
        self.project_root().join(".aegis")
    }
    fn logs_dir(&self) -> PathBuf {
        self.aegis_dir().join("logs").join("sessions")
    }
    fn archive_dir(&self) -> PathBuf {
        self.aegis_dir().join("logs").join("archive")
    }
    fn state_dir(&self) -> PathBuf {
        self.aegis_dir().join("state")
    }
    fn snapshots_dir(&self) -> PathBuf {
        self.state_dir().join("snapshots")
    }
    fn channels_dir(&self) -> PathBuf {
        self.aegis_dir().join("channels")
    }
    fn profiles_dir(&self) -> PathBuf {
        self.aegis_dir().join("profiles")
    }
    fn worktrees_dir(&self) -> PathBuf {
        self.aegis_dir().join("worktrees")
    }
    fn handoff_dir(&self) -> PathBuf {
        self.aegis_dir().join("handoff")
    }
    fn prompts_dir(&self) -> PathBuf {
        self.aegis_dir().join("prompts")
    }
    fn designs_dir(&self) -> PathBuf {
        self.aegis_dir().join("designs")
    }

    // ── Derived paths ────────────────────────────────────────────────

    fn registry_path(&self) -> PathBuf {
        self.state_dir().join("registry.json")
    }
    fn tasks_path(&self) -> PathBuf {
        self.state_dir().join("tasks.json")
    }
    fn channels_state_path(&self) -> PathBuf {
        self.state_dir().join("channels.json")
    }
    fn agent_log_path(&self, agent_id: Uuid) -> PathBuf {
        self.logs_dir().join(format!("{}.log", agent_id))
    }
    fn sandbox_profile_path(&self, agent_id: Uuid) -> PathBuf {
        self.profiles_dir().join(format!("{}.sb", agent_id))
    }
    fn agent_worktree_path(&self, agent_id: Uuid) -> PathBuf {
        self.worktrees_dir().join(agent_id.to_string())
    }
    fn agent_inbox_path(&self, agent_id: Uuid) -> PathBuf {
        self.channels_dir().join(agent_id.to_string()).join("inbox")
    }
}
```

All path methods have default implementations derived from `project_root()`. Implementors only need to provide `project_root()`.

---

## 12. `error.rs`

```rust
use std::{io, path::PathBuf};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, AegisError>;

#[derive(Debug, thiserror::Error)]
pub enum AegisError {
    // ── Config ───────────────────────────────────────────────────────
    #[error("config file not found: {path}")]
    ConfigNotFound { path: PathBuf },

    #[error("config parse error at {path}: {source}")]
    ConfigParseError { path: PathBuf, #[source] source: toml::de::Error },

    #[error("config validation error — field `{field}`: {reason}")]
    ConfigValidation { field: String, reason: String },

    // ── Registry ─────────────────────────────────────────────────────
    #[error("registry lock error: {source}")]
    RegistryLock { #[source] source: io::Error },

    #[error("registry file corrupted at {path}: {source}")]
    RegistryCorrupted { path: PathBuf, #[source] source: serde_json::Error },

    #[error("agent not found: {agent_id}")]
    AgentNotFound { agent_id: Uuid },

    #[error("task not found: {task_id}")]
    TaskNotFound { task_id: Uuid },

    // ── Tmux ─────────────────────────────────────────────────────────
    #[error("tmux command `{command}` failed: {stderr}")]
    TmuxCommand { command: String, stderr: String },

    #[error("tmux session not found: {target}")]
    TmuxSessionNotFound { target: String },

    #[error("tmux pane not found: {target}")]
    TmuxPaneNotFound { target: String },

    // ── Sandbox ──────────────────────────────────────────────────────
    #[error("sandbox profile render failed: {reason}")]
    SandboxProfileRender { reason: String },

    #[error("sandbox-exec failed to start: {source}")]
    SandboxExec { #[source] source: io::Error },

    // ── Provider ─────────────────────────────────────────────────────
    #[error("provider not found: `{name}`")]
    ProviderNotFound { name: String },

    #[error("provider `{provider}` failed to spawn: {source}")]
    ProviderSpawn { provider: String, #[source] source: io::Error },

    // ── Channel ──────────────────────────────────────────────────────
    #[error("channel not found: `{name}`")]
    ChannelNotFound { name: String },

    #[error("channel send failed on `{kind}`: {reason}")]
    ChannelSend { kind: String, reason: String },

    // ── Recorder ─────────────────────────────────────────────────────
    #[error("recorder attach failed for agent {agent_id}: {source}")]
    RecorderAttach { agent_id: Uuid, #[source] source: io::Error },

    #[error("log file not found for agent {agent_id} at {path}")]
    LogFileNotFound { agent_id: Uuid, path: PathBuf },

    // ── IPC / Daemon ─────────────────────────────────────────────────
    #[error("aegisd is not running (socket: {socket_path})")]
    DaemonNotRunning { socket_path: PathBuf },

    #[error("IPC connection failed: {source}")]
    IpcConnection { #[source] source: io::Error },

    #[error("IPC protocol error: {reason}")]
    IpcProtocol { reason: String },

    // ── Storage ──────────────────────────────────────────────────────
    #[error("storage I/O error at {path}: {source}")]
    StorageIo { path: PathBuf, #[source] source: io::Error },

    #[error("not an AegisCore project (no .aegis/ found from {path})")]
    ProjectNotInitialized { path: PathBuf },

    #[error("project already initialized at {path}")]
    ProjectAlreadyInitialized { path: PathBuf },

    // ── Git ──────────────────────────────────────────────────────────
    #[error("git worktree add failed at {path}: {reason}")]
    GitWorktreeAdd { path: PathBuf, reason: String },

    #[error("git worktree prune failed: {reason}")]
    GitWorktreePrune { reason: String },

    // ── General ──────────────────────────────────────────────────────
    #[error(transparent)]
    Unexpected(#[from] Box<dyn std::error::Error + Send + Sync>),
}
```

---

## 13. `lib.rs` (re-exports)

```rust
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
```

---

## 14. Trait Object Safety

All traits must be object-safe (`dyn Trait` must compile) to allow the controller builder to hold boxed implementations.

Constraints that ensure this:
- No generic methods on any trait (use concrete types instead)
- `spawn_command` on `Provider` returns `std::process::Command` (concrete) — this is acceptable since `Command` is `!Send`; the provider constructs it, the controller consumes it immediately
- All traits are `Send + Sync` bounded — required for use in tokio tasks

---

## 15. Serialization Rules

- All public types that cross a persistence or IPC boundary derive `Serialize, Deserialize`
- `Uuid` serialized as hyphenated string
- `DateTime<Utc>` serialized as ISO 8601 string
- Enums use `serde(rename_all = "snake_case")` throughout for JSON/TOML consistency
- `PathBuf` serializes as a UTF-8 string; non-UTF-8 paths are rejected at validation time
