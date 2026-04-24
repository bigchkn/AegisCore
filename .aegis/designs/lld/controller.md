# LLD: Controller & Dispatcher (`aegis-controller`)

**Milestone:** M10  
**Status:** done  
**HLD ref:** §2.2, §3, §6.4, §9, §10, §13, §14.6, §16  
**Implements:** `crates/aegis-controller/`

---

## 1. Purpose

`aegis-controller` is the runtime integration layer for a single Aegis project. It wires the implemented subsystem crates together and owns all authoritative lifecycle decisions:

- load and validate project configuration;
- initialize state, prompts, channels, logs, profiles, and worktree directories;
- spawn, pause, resume, terminate, and fail over agents;
- enforce Splinter concurrency limits and task queue dispatch;
- implement `WatchdogSink` and the Watchdog failover executor boundary;
- publish internal `AegisEvent`s for Telegram, daemon IPC, and future UI clients.

The Controller does not implement tmux command execution, sandbox profile rendering, provider-specific CLI behavior, recorder log tailing, or Telegram API calls. Those stay in their own crates. The Controller composes them and persists the result in the registry.

---

## 2. Existing Baseline

The crate already contains:

```
crates/aegis-controller/src/
├── lib.rs
├── prompts.rs
├── registry/
│   ├── agents.rs
│   ├── channels.rs
│   ├── mod.rs
│   └── tasks.rs
└── state/
    └── mod.rs
```

Implemented responsibilities:

- `FileRegistry`: agent, task, and channel JSON stores.
- `StateManager`: snapshots and crash recovery.
- `PromptManager`: prompt resolution, rendering, and scaffolding.

M10 adds the missing runtime and lifecycle modules without replacing those implementations.

---

## 3. Module Structure

```
crates/aegis-controller/src/
├── lib.rs
├── runtime.rs       ← AegisRuntime builder, startup, shutdown, subsystem handles
├── dispatcher.rs    ← agent lifecycle orchestration and tmux launch flow
├── scheduler.rs     ← max_splinters semaphore + queued task dispatch
├── commands.rs      ← typed command API used by CLI, Telegram, daemon IPC
├── events.rs        ← event bus wrapper around tokio broadcast/mpsc channels
├── git.rs           ← git worktree create/prune helpers
├── lifecycle.rs     ← AgentSpec, SpawnPlan, RunningAgent, state transitions
├── failover.rs      ← Controller-owned failover executor for Watchdog
├── registry/        ← existing FileRegistry
├── state/           ← existing StateManager
└── prompts.rs       ← existing PromptManager
```

`dispatcher.rs` owns process lifecycle. `runtime.rs` owns subsystem construction and task supervision. `commands.rs` is the stable integration surface for CLI, Telegram, and daemon IPC.

---

## 4. Dependencies

`aegis-controller` is the only crate that should depend on most subsystem crates at once.

```toml
[dependencies]
aegis-core      = { path = "../aegis-core" }
aegis-tmux      = { path = "../aegis-tmux" }
aegis-sandbox   = { path = "../aegis-sandbox", optional = true }
aegis-channels  = { path = "../aegis-channels", optional = true }
aegis-providers = { path = "../aegis-providers" }
aegis-recorder  = { path = "../aegis-recorder", optional = true }
aegis-watchdog  = { path = "../aegis-watchdog", optional = true }
aegis-telegram  = { path = "../aegis-telegram", optional = true }

tokio           = { version = "1", features = ["rt", "rt-multi-thread", "sync", "process", "time", "fs"] }
uuid            = { version = "1", features = ["v4", "serde"] }
chrono          = { version = "0.4", features = ["serde"] }
tracing         = "0.1"
thiserror       = "2"
```

Feature flags:

```toml
[features]
default  = ["channels", "sandbox", "watchdog", "recorder"]
channels = ["dep:aegis-channels"]
sandbox  = ["dep:aegis-sandbox"]
watchdog = ["dep:aegis-watchdog"]
recorder = ["dep:aegis-recorder"]
telegram = ["dep:aegis-telegram"]
```

---

## 5. Runtime Builder

`AegisRuntime` is the root object for one project.

```rust
pub struct AegisRuntime {
    project_root: PathBuf,
    config: EffectiveConfig,
    storage: Arc<ProjectStorage>,
    registry: Arc<FileRegistry>,
    tmux: Arc<TmuxClient>,
    providers: Arc<ProviderRegistry>,
    prompts: Arc<PromptManager>,
    dispatcher: Arc<Dispatcher>,
    scheduler: Arc<Scheduler>,
    state: Arc<StateManager>,
    events: EventBus,
    tasks: RuntimeTasks,
}

impl AegisRuntime {
    pub async fn build(project_root: PathBuf) -> Result<Self>;
    pub async fn start(&self) -> Result<()>;
    pub async fn shutdown(&self) -> Result<()>;
    pub fn commands(&self) -> ControllerCommands;
    pub fn subscribe_events(&self) -> broadcast::Receiver<AegisEvent>;
}
```

### 5.1 Build Sequence

1. Load `~/.aegis/config` and `<project>/aegis.toml`.
2. Resolve and validate `EffectiveConfig`.
3. Create `ProjectStorage`.
4. Initialize `FileRegistry` if missing.
5. Run `StateManager::recover()`.
6. Construct `TmuxClient`, `ProviderRegistry`, `PromptManager`, `FlightRecorder`, channel services, and optional Watchdog/Telegram bridge.
7. Construct `Dispatcher`, `Scheduler`, and `ControllerCommands`.

Build performs no agent spawning. `start()` begins background tasks and launches configured Bastions.

---

## 6. Project Storage

The Controller supplies a concrete `StorageBackend` implementation:

```rust
pub struct ProjectStorage {
    project_root: PathBuf,
}

impl StorageBackend for ProjectStorage {
    fn project_root(&self) -> &Path { &self.project_root }
}
```

`aegis init` and runtime startup ensure these directories exist:

- `.aegis/state/`
- `.aegis/logs/sessions/`
- `.aegis/logs/archive/`
- `.aegis/channels/`
- `.aegis/profiles/`
- `.aegis/worktrees/`
- `.aegis/handoff/`
- `.aegis/prompts/`

---

## 7. Agent Specs and Spawn Plans

The Dispatcher converts config and task intent into a concrete spawn plan.

```rust
pub struct AgentSpec {
    pub name: String,
    pub kind: AgentKind,
    pub role: String,
    pub parent_id: Option<Uuid>,
    pub task_id: Option<Uuid>,
    pub task_description: Option<String>,
    pub cli_provider: String,
    pub fallback_cascade: Vec<String>,
    pub system_prompt: Option<PathBuf>,
    pub sandbox: SandboxPolicy,
    pub auto_cleanup: bool,
}

pub struct SpawnPlan {
    pub agent: Agent,
    pub provider_command: std::process::Command,
    pub launch_command: Vec<String>,
    pub initial_prompt: String,
}
```

`SpawnPlan` is testable without launching tmux. It records the exact provider command, sandbox prefix, prompt, log path, profile path, and worktree path expected for the final agent.

---

## 8. Dispatcher API

```rust
pub struct Dispatcher {
    registry: Arc<FileRegistry>,
    tmux: Arc<TmuxClient>,
    providers: Arc<ProviderRegistry>,
    prompts: Arc<PromptManager>,
    storage: Arc<ProjectStorage>,
    events: EventBus,
    config: EffectiveConfig,
    recorder: Option<Arc<dyn Recorder>>,
    sandbox: Option<Arc<dyn SandboxProfile>>,
}

impl Dispatcher {
    pub async fn spawn_bastion(&self, name: &str) -> Result<Agent>;
    pub async fn spawn_splinter(&self, role: &str, task: &Task, parent_id: Option<Uuid>) -> Result<Agent>;
    pub async fn pause_agent(&self, agent_id: Uuid) -> Result<()>;
    pub async fn resume_agent(&self, agent_id: Uuid) -> Result<()>;
    pub async fn kill_agent(&self, agent_id: Uuid, archive_log: bool) -> Result<()>;
    pub async fn failover_agent(&self, agent_id: Uuid, reason: DetectedEvent) -> Result<Agent>;
}
```

### 8.1 Bastion Spawn Flow

1. Read `config.agents[name]`; require `kind = Bastion`.
2. Use project root as the Bastion worktree.
3. Render sandbox profile at `.aegis/profiles/<agent_id>.sb`.
4. Build provider command from `ProviderRegistry`.
5. Prefix with `sandbox-exec -f <profile>` when sandbox feature is enabled.
6. Ensure tmux session exists.
7. Create a named tmux window for the Bastion.
8. Start the launch command in the pane.
9. Insert `Agent { status: Starting, ... }` into the registry.
10. Attach Flight Recorder.
11. Inject rendered system prompt.
12. Mark agent `Active`.
13. Publish `AegisEvent::AgentSpawned`.

### 8.2 Splinter Spawn Flow

1. Scheduler claims or receives a queued task.
2. Create a new UUID and worktree at `.aegis/worktrees/<agent_id>`.
3. Create a branch name `aegis/<role>/<agent_id-short>`.
4. Render sandbox profile for the worktree.
5. Resolve the task prompt and system prompt.
6. Create a tmux window in the project session.
7. Launch provider command under sandbox.
8. Insert registry entry with `parent_id`, `task_id`, and log path.
9. Attach Flight Recorder.
10. Inject prompt containing task description and receipt instructions.
11. Mark task and agent active.
12. Publish `AgentSpawned`.

---

## 9. Git Worktree Helper

`git.rs` isolates process calls and keeps command construction out of the Dispatcher.

```rust
pub struct GitWorktree {
    project_root: PathBuf,
}

impl GitWorktree {
    pub async fn create_for_agent(&self, agent_id: Uuid, role: &str) -> Result<PathBuf>;
    pub async fn prune_for_agent(&self, agent_id: Uuid) -> Result<()>;
}
```

Rules:

- Bastions do not create ephemeral worktrees by default.
- Splinter worktrees are preserved on crash/failure.
- Worktrees are pruned only after clean termination and receipt processing.
- Git commands are invoked via argument arrays, never shell strings.

---

## 10. Scheduler

The Scheduler enforces `global.max_splinters`.

```rust
pub struct Scheduler {
    registry: Arc<FileRegistry>,
    dispatcher: Arc<Dispatcher>,
    permits: Arc<Semaphore>,
    events: EventBus,
}

impl Scheduler {
    pub async fn enqueue_splinter_task(&self, description: &str, created_by: TaskCreator) -> Result<Uuid>;
    pub async fn start(self: Arc<Self>, shutdown: watch::Receiver<bool>) -> JoinHandle<()>;
    pub async fn dispatch_once(&self) -> Result<Option<Uuid>>;
}
```

`dispatch_once()` is exposed for deterministic unit tests. The background loop polls queued tasks, claims one atomically, acquires a semaphore permit, and spawns a Splinter. The permit is released when the Splinter reaches a terminal status.

---

## 11. Watchdog Integration

The Controller implements `WatchdogSink`:

```rust
impl WatchdogSink for ControllerWatchdogSink {
    fn on_event(&self, event: DetectedEvent) -> WatchdogAction;
}
```

Decision table:

| Event | Action |
|---|---|
| `RateLimit` | `InitiateFailover` when cascade has a next provider, otherwise `PauseAndNotify` |
| `AuthFailure` | `PauseAndNotify` |
| `CliCrash` | `CaptureAndMarkFailed` |
| `SandboxViolation` | `LogAndContinue`, escalating to `PauseAndNotify` after repeated events |
| `TaskComplete` | `TriggerReceiptProcessing` |

The sink also publishes `AegisEvent::WatchdogAlert { event, action }`.

### 11.1 Failover Executor

Watchdog owns detection and state sequencing, but Controller owns lifecycle mutations. `aegis-controller` provides the executor called by `aegis-watchdog`:

```rust
pub trait FailoverExecutor: Send + Sync {
    async fn pause_for_failover(&self, agent_id: Uuid) -> Result<Agent>;
    async fn capture_context(&self, agent_id: Uuid, lines: usize) -> Result<String>;
    async fn switch_provider(&self, agent_id: Uuid, next_provider: &str) -> Result<Agent>;
    async fn inject_recovery_prompt(&self, agent_id: Uuid, prompt: &str) -> Result<()>;
}
```

Implementation sequence:

1. Mark agent `Cooling`.
2. Interrupt the current pane with `C-c`.
3. Query Flight Recorder for failover context.
4. Resolve next provider from `fallback_cascade`.
5. Update registry `cli_provider`.
6. Relaunch the provider in the same worktree.
7. Render recovery prompt via `PromptManager` and provider handoff template.
8. Inject prompt.
9. Mark agent `Active`.

---

## 12. Command API

`ControllerCommands` is the in-process API shared by CLI, daemon IPC, and Telegram.

```rust
pub struct ControllerCommands {
    runtime: Weak<AegisRuntime>,
}

impl ControllerCommands {
    pub async fn status(&self) -> Result<ProjectStatus>;
    pub async fn list_agents(&self) -> Result<Vec<Agent>>;
    pub async fn spawn(&self, role: &str, task: &str) -> Result<Uuid>;
    pub async fn pause(&self, agent_id: Uuid) -> Result<()>;
    pub async fn resume(&self, agent_id: Uuid) -> Result<()>;
    pub async fn kill(&self, agent_id: Uuid) -> Result<()>;
    pub async fn logs(&self, agent_id: Uuid, lines: Option<usize>) -> Result<Vec<String>>;
    pub async fn failover(&self, agent_id: Uuid) -> Result<()>;
}
```

Telegram command handlers must call this interface rather than mocking controller behavior once M10 is implemented.

---

## 13. Event Bus

```rust
#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<AegisEvent>,
}

impl EventBus {
    pub fn publish(&self, event: AegisEvent);
    pub fn subscribe(&self) -> broadcast::Receiver<AegisEvent>;
}
```

Events are best-effort notifications. State files remain the source of truth. If a receiver lags, it should resync by querying `ControllerCommands::status()`.

---

## 14. Agent Lifecycle State Machine

Allowed Controller transitions:

| From | To | Trigger |
|---|---|---|
| `Queued` | `Starting` | Scheduler begins spawn |
| `Starting` | `Active` | Provider launched and recorder attached |
| `Active` | `Paused` | Manual pause or auth failure |
| `Paused` | `Active` | Manual resume |
| `Active` | `Cooling` | Watchdog rate limit failover |
| `Cooling` | `Active` | Failover provider resumed |
| `Active` | `Reporting` | Watchdog task complete |
| `Reporting` | `Terminated` | Receipt processed and cleanup done |
| `Starting`/`Active`/`Cooling`/`Reporting` | `Failed` | Crash or unrecoverable dispatcher error |

Invalid transitions return a controller error and leave registry state unchanged.

---

## 15. Receipt Processing

When `TaskComplete` is detected:

1. Mark agent `Reporting`.
2. Locate `.aegis/handoff/<task_id>/receipt.json`.
3. Validate receipt JSON and task id.
4. Mark task `Complete` with `receipt_path`.
5. Detach and archive the recorder log.
6. Kill the tmux pane/window.
7. Prune Splinter worktree when `auto_cleanup = true`.
8. Archive the agent.
9. Publish `TaskComplete`.

Missing or invalid receipts move the task and agent to `Failed` and preserve the worktree.

---

## 16. Shutdown

`shutdown()` is graceful and bounded:

1. Signal Watchdog, Scheduler, StateManager, and Telegram tasks to stop.
2. Stop accepting new commands.
3. Snapshot registry.
4. Leave active tmux agents running unless the user requested `aegis stop`.
5. Flush final system event.

`aegis stop` is stronger: it pauses or terminates project agents according to command flags, detaches recorders, archives logs for clean exits, and snapshots state.

---

## 17. Error Handling

Add controller-specific errors through `AegisError` variants only when the condition is useful to callers:

- invalid state transition;
- missing configured agent role;
- provider not found;
- no fallback provider available;
- worktree creation failure;
- launch command failure;
- receipt missing or invalid.

Subsystem errors should be converted without string matching where possible.

---

## 18. Test Strategy

### 18.1 Unit Tests

| Test | Asserts |
|---|---|
| `test_build_spawn_plan_bastion` | provider command, prompt, paths, and registry fields are correct |
| `test_build_spawn_plan_splinter` | unique worktree/log/profile paths and task metadata are set |
| `test_scheduler_respects_max_splinters` | semaphore prevents dispatch above configured limit |
| `test_watchdog_sink_decisions` | each `DetectedEvent` maps to the expected `WatchdogAction` |
| `test_invalid_state_transition_rejected` | registry is unchanged after invalid transition |
| `test_event_bus_publish_subscribe` | subscribers receive controller events |
| `test_receipt_processing_success` | task complete, agent archived, cleanup called |
| `test_receipt_processing_missing_receipt_fails` | worktree preserved and task marked failed |

### 18.2 Integration Tests

| Test | Asserts |
|---|---|
| `test_spawn_agent_with_mock_provider` | Dispatcher inserts registry entry and calls recorder attach |
| `test_spawn_agent_real_tmux_when_available` | tmux window is created and prompt is injected |
| `test_failover_switches_provider` | registry provider changes and recovery prompt is injected |
| `test_kill_agent_archives_log` | recorder detach/archive and registry archive are called |
| `test_command_api_spawn_pause_resume_kill` | command layer drives Dispatcher methods |

Real tmux tests must be skipped when `tmux` is unavailable, matching `aegis-tmux` integration-test behavior.

---

## 19. Implementation Tasks

| # | Task | Notes |
|---|---|---|
| 10.1 | Write `lld/controller.md` | This document |
| 10.2 | Add `ProjectStorage`, `EventBus`, and controller error helpers | Low-risk foundation |
| 10.3 | Add `AegisRuntime::build()` and subsystem construction | No background tasks yet |
| 10.4 | Add `AgentSpec`, `SpawnPlan`, and spawn-plan unit tests | Deterministic pre-launch validation |
| 10.5 | Implement Dispatcher Bastion spawn flow | tmux + provider + sandbox + recorder |
| 10.6 | Implement Git worktree helper and Splinter spawn flow | includes task assignment |
| 10.7 | Implement Scheduler queue dispatch and concurrency limit | `dispatch_once()` + background loop |
| 10.8 | Implement pause, resume, kill, and receipt processing | lifecycle state transitions |
| 10.9 | Implement Controller `WatchdogSink` and failover executor | uses provider cascade and recorder context |
| 10.10 | Implement `ControllerCommands` API | shared by CLI, Telegram, daemon IPC |
| 10.11 | Wire optional Watchdog and Telegram background tasks in `start()` | feature-gated |
| 10.12 | Add integration tests with mock subsystems and real tmux when available | covers lifecycle edges |
