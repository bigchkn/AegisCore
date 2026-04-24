# LLD: `aegis-watchdog` (Watchdog & Failover)

**Milestone:** M7  
**Status:** done  
**HLD ref:** §2.2, §3, §4.3, §6, §8  
**Implements:** `crates/aegis-watchdog/`

---

## 1. Purpose

`aegis-watchdog` continuously monitors active agent tmux panes and turns terminal observations into structured `DetectedEvent`s. It is responsible for:

- polling active panes through `aegis-tmux`;
- matching rate-limit, authentication, crash, sandbox, and task-completion signals;
- notifying the Controller through `WatchdogSink`;
- coordinating provider failover when the Controller chooses `WatchdogAction::InitiateFailover`;
- collecting Flight Recorder context so a replacement provider can resume the interrupted task.

The Watchdog does not own agent spawning, task assignment, sandbox generation, or registry persistence. Those remain Controller responsibilities. The Watchdog drives the sequence and calls narrow interfaces owned by the Controller for actions that require process lifecycle changes.

---

## 2. Module Structure

```
crates/aegis-watchdog/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-exports Watchdog, PatternMatcher, FailoverCoordinator
    ├── monitor.rs      ← async poll loop and pane sweep logic
    ├── matcher.rs      ← configurable string/regex pattern matching
    ├── failover.rs     ← failover state machine and recovery prompt injection
    ├── backoff.rs      ← exponential backoff with jitter
    └── error.rs        ← watchdog-specific error conversions
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core      = { path = "../aegis-core" }
aegis-tmux      = { path = "../aegis-tmux" }
aegis-providers = { path = "../aegis-providers" }
aegis-recorder  = { path = "../aegis-recorder" }
tokio           = { version = "1", features = ["rt", "time", "sync"] }
regex           = "1"
rand            = "0.8"
tracing         = "0.1"
thiserror       = "2"
```

`aegis-core` remains the contract boundary. `aegis-watchdog` may depend on concrete tmux/provider/recorder crates because it is an orchestration crate, not a core contract crate.

---

## 4. Public API

```rust
pub struct Watchdog {
    tmux: Arc<TmuxClient>,
    agents: Arc<dyn AgentRegistry>,
    tasks: Arc<dyn TaskRegistry>,
    recorder: Arc<dyn Recorder>,
    providers: Arc<ProviderRegistry>,
    sink: Arc<dyn WatchdogSink>,
    matcher: PatternMatcher,
    failover: FailoverCoordinator,
    config: WatchdogConfig,
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
    ) -> Self;

    pub async fn run(&self, shutdown: CancellationToken) -> Result<()>;
    pub async fn sweep_once(&self) -> Result<Vec<DetectedEvent>>;
}
```

`run()` is the daemon path. `sweep_once()` is exposed for deterministic tests and manual Controller probes.

The initial implementation can use a `tokio::sync::watch::Receiver<bool>` or `CancellationToken` equivalent for shutdown. If `tokio-util` is not added, use `watch::Receiver<()>` to avoid another dependency.

---

## 5. Poll Loop

The monitor wakes every `config.watchdog.poll_interval_ms` and scans active agents:

```rust
pub async fn run(&self, shutdown: CancellationToken) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(self.config.poll_interval_ms));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let events = self.sweep_once().await?;
                for event in events {
                    self.handle_event(event).await?;
                }
            }
            _ = shutdown.cancelled() => break,
        }
    }
    Ok(())
}
```

### 5.1 Active Agent Selection

The Watchdog asks `AgentRegistry::list_active()` for agents to scan. The registry defines "active" as non-terminal, Controller-owned state. The Watchdog additionally skips agents in `Paused`, `Cooling`, `Reporting`, `Terminated`, and `Failed` states to avoid duplicate failovers and receipt handling.

### 5.2 Pane Capture

For each eligible agent:

1. Build `TmuxTarget` from `agent.tmux_target()`.
2. Call `tmux.pane_exit_status(&target)`.
3. If the pane is dead or missing, emit `DetectedEvent::CliCrash`.
4. Otherwise call `tmux.capture_pane_plain(&target, config.scan_lines)`.
5. Pass captured lines to `PatternMatcher`.

Pane capture uses plain text because matching should not depend on ANSI control sequences. The Flight Recorder remains the source of richer failover context.

### 5.3 Error Handling

| Failure | Result |
|---|---|
| tmux target missing | `DetectedEvent::CliCrash { exit_code: None }` |
| pane dead with status | `DetectedEvent::CliCrash { exit_code: Some(code) }` |
| transient capture failure | trace warning and continue scanning other agents |
| registry read failure | return error from `sweep_once()` |

The monitor must not stop the entire Watchdog because one pane capture failed.

---

## 6. Pattern Matcher

`PatternMatcher` combines user-configured Watchdog patterns with provider-owned patterns from `Provider`.

```rust
pub struct PatternMatcher {
    rate_limit: Vec<Pattern>,
    auth_failure: Vec<Pattern>,
    sandbox_violation: Vec<Pattern>,
    task_complete: Vec<Pattern>,
}

enum Pattern {
    Literal(String),
    Regex(regex::Regex),
}
```

### 6.1 Pattern Syntax

Config values are interpreted as:

- `re:<expr>`: compile as a case-insensitive regex.
- any other string: case-insensitive literal substring match.

Invalid regexes are configuration errors at Watchdog construction time, not runtime warnings.

### 6.2 Match Priority

One scan can contain multiple signals. The Watchdog emits only the highest-priority event per agent per sweep:

| Priority | Event | Reason |
|---|---|---|
| 1 | `AuthFailure` | failover usually cannot solve bad credentials; pause for human action |
| 2 | `SandboxViolation` | indicates containment denial; preserve state and notify |
| 3 | `RateLimit` | normal failover candidate |
| 4 | `CliCrash` | provider process failed or pane closed |
| 5 | `TaskComplete` | receipt flow, not failure |

Provider methods `is_rate_limit_error()`, `is_auth_error()`, and `is_task_complete()` are evaluated alongside configured Watchdog patterns. Configured patterns are still authoritative so users can recognize provider-specific output before the provider manifest is updated.

### 6.3 Provider Pattern Source of Truth

Common provider errors for `claude-code`, `gemini-cli`, `codex`, and future providers are owned by `aegis-providers/src/builtin_providers.yaml`:

```yaml
providers:
  claude-code:
    error_patterns:
      rate_limit: ["rate limit", "429", "usage limit reached"]
      auth: ["401", "authentication failed"]
```

`aegis-watchdog` must not duplicate those strings in its implementation. It should ask the active provider:

```rust
provider.is_rate_limit_error(line)
provider.is_auth_error(line)
provider.is_task_complete(line)
```

This keeps provider expansion straightforward:

1. Add or update provider patterns in `builtin_providers.yaml`.
2. Add targeted provider manifest tests in `aegis-providers`.
3. Watchdog provider-compatibility tests automatically exercise the loaded `ProviderRegistry`.

Project-specific or fast-moving patterns remain configurable through:

```toml
[watchdog.patterns]
rate_limit = ["re:temporarily unavailable", "too many requests"]
auth_failure = ["invalid api key"]
sandbox_violation = ["Operation not permitted"]
task_complete = ["[AEGIS:DONE]"]
```

Those config patterns are additive to provider-owned patterns for matching. They are not written back into the provider manifest.

### 6.4 Duplicate Suppression

The monitor maintains an in-memory recent-event cache:

```rust
HashMap<(Uuid, EventKind, String), Instant>
```

Events with the same agent, kind, and matched pattern are suppressed for `max(2 * poll_interval_ms, 5s)`. This prevents repeated handling while the same error remains visible in the pane capture window.

---

## 7. Event Handling

```rust
async fn handle_event(&self, event: DetectedEvent) -> Result<()> {
    let action = self.sink.on_event(event.clone());
    match action {
        WatchdogAction::InitiateFailover if self.config.failover_enabled => {
            self.failover.initiate(event).await
        }
        WatchdogAction::PauseAndNotify => {
            self.pause_agent(event.agent_id())?;
            Ok(())
        }
        WatchdogAction::CaptureAndMarkFailed => {
            self.capture_and_mark_failed(event).await
        }
        WatchdogAction::TriggerReceiptProcessing => {
            self.trigger_receipt_processing(event).await
        }
        WatchdogAction::LogAndContinue | WatchdogAction::InitiateFailover => Ok(()),
    }
}
```

The Controller remains the policy authority through `WatchdogSink`; the Watchdog only executes the action selected by the sink.

---

## 8. Failover State Machine

Failover follows the roadmap sequence: detect -> pause -> capture -> switch -> inject.

```text
Detected
   |
   v
PauseCurrent
   |
   v
CaptureContext
   |
   v
SelectProvider
   |
   v
Backoff
   |
   v
Relaunch
   |
   v
InjectRecovery
   |
   v
ResumeMonitoring
```

### 8.1 Local Types

```rust
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

pub struct FailoverAttempt {
    pub agent_id: Uuid,
    pub from_provider: String,
    pub to_provider: String,
    pub attempt: u32,
    pub started_at: DateTime<Utc>,
}
```

### 8.2 Controller Execution Boundary

The Watchdog must not directly mutate process lifecycle outside the interfaces it owns. Relaunch is delegated to a Controller-provided executor:

```rust
#[async_trait]
pub trait FailoverExecutor: Send + Sync {
    async fn pause_current(&self, agent: &Agent) -> Result<()>;
    async fn relaunch_with_provider(
        &self,
        agent: &Agent,
        provider_name: &str,
    ) -> Result<Agent>;
    async fn inject_recovery(&self, agent: &Agent, prompt: &str) -> Result<()>;
    async fn mark_failed(&self, agent_id: Uuid, reason: &str) -> Result<()>;
    async fn mark_cooling(&self, agent_id: Uuid) -> Result<()>;
    async fn mark_active(&self, agent_id: Uuid, provider_name: &str) -> Result<()>;
}
```

Implementation note: if the workspace avoids `async-trait`, make these methods sync and let the Controller bridge to async internally. The LLD prefers async because relaunch and injection naturally call tmux and process APIs.

### 8.3 Context Capture

The failover coordinator queries the Flight Recorder, not the current pane capture:

```rust
let lines = recorder.query(&LogQuery {
    agent_id: agent.agent_id,
    last_n_lines: Some(recorder_config.failover_context_lines),
    since: None,
    follow: false,
})?;

let ctx = FailoverContext {
    agent_id: agent.agent_id,
    task_id: agent.task_id,
    previous_provider: agent.cli_provider.clone(),
    terminal_context: lines.join("\n"),
    task_description,
    worktree_path: agent.worktree_path.clone(),
    role: agent.role.clone(),
};
```

Task description is loaded from `TaskRegistry` when `agent.task_id` is present. Missing task records are tolerated; the handoff prompt receives `None`.

### 8.4 Provider Selection

Provider selection uses the agent's current provider and `fallback_cascade`:

1. Build ordered candidates: `[agent.cli_provider] + agent.fallback_cascade`.
2. Select the provider after `agent.cli_provider`.
3. If no provider remains, transition to `Exhausted` and call `mark_failed()`.
4. Generate the recovery prompt with the target provider's `failover_handoff_prompt(&ctx)`.

The target provider owns the prompt format because each CLI may require different resume instructions.

### 8.5 Relaunch and Injection

After backoff, `FailoverExecutor::relaunch_with_provider()` returns the updated `Agent` with a new live tmux pane and `cli_provider`. The Watchdog then injects the recovery prompt through `FailoverExecutor::inject_recovery()`.

The Flight Recorder must already be attached by the Controller during relaunch. The Watchdog validates this by checking `recorder.log_path(agent_id).exists()` after relaunch and emits a warning if not.

---

## 9. Backoff

Backoff is per-agent and per-provider transition.

```rust
pub struct BackoffPolicy {
    pub initial_delay: Duration, // default: 5s
    pub max_delay: Duration,     // default: 5m
    pub multiplier: f64,         // default: 2.0
    pub jitter_ratio: f64,       // default: 0.2
}
```

Delay formula:

```text
delay = min(max_delay, initial_delay * multiplier^attempt)
jitter = random range [-jitter_ratio, +jitter_ratio]
```

The first failover attempt uses attempt `0`. Successful relaunch clears the agent's backoff state. Exhausted cascades keep the agent in `Failed` until a human retries.

Backoff policy is internal to `aegis-watchdog` for M7. It can become configurable in a future config LLD update without changing the monitor/failover APIs.

---

## 10. Task Completion

Task completion is detected from configured `watchdog.patterns.task_complete` and provider-owned `is_task_complete()`. On `DetectedEvent::TaskComplete`, the Controller's sink should return `WatchdogAction::TriggerReceiptProcessing`.

The Watchdog does not parse receipt files. It only triggers the Controller path that moves the agent to `Reporting`, validates the receipt, and eventually marks the task complete.

---

## 11. Sandbox Violations

Sandbox violations are text-level signals from CLI or shell output, typically:

- `Operation not permitted`
- `Permission denied`
- `sandbox-exec`
- `deny file-read`
- `deny file-write`

The default configured pattern remains `["Operation not permitted"]` from `aegis-core`. Projects can add stricter regexes through `watchdog.patterns.sandbox_violation`.

Recommended action: `WatchdogAction::PauseAndNotify` for first occurrence, or `CaptureAndMarkFailed` if the same agent repeatedly violates policy.

---

## 12. State and Idempotency

The Watchdog stores only runtime coordination state:

- recent event suppression cache;
- per-agent failover lock;
- per-agent backoff attempt counter.

Authoritative state remains in `AgentRegistry` and `TaskRegistry`. Before starting failover, the coordinator acquires an in-memory per-agent lock and re-reads the agent. If the agent is no longer eligible, failover is skipped.

This prevents duplicate failovers when multiple sweeps observe the same pane output.

---

## 13. Test Strategy

| Test | Asserts |
|---|---|
| `test_literal_pattern_case_insensitive` | Literal patterns match independent of case |
| `test_regex_pattern_prefix` | `re:<expr>` compiles and matches correctly |
| `test_invalid_regex_is_config_error` | Bad regex fails construction |
| `test_match_priority_auth_over_rate_limit` | Higher-priority event wins when multiple patterns match |
| `test_duplicate_suppression` | Same event is emitted once within suppression window |
| `test_sweep_detects_task_complete` | Captured pane text yields `TaskComplete` |
| `test_sweep_detects_dead_pane` | Dead pane yields `CliCrash` with exit status |
| `test_failover_selects_next_provider` | Current provider advances to next cascade entry |
| `test_failover_exhausted_marks_failed` | No fallback provider calls `mark_failed()` |
| `test_failover_uses_recorder_context` | Recovery prompt includes Flight Recorder tail, not only pane capture |
| `test_backoff_increases_and_caps` | Delay grows exponentially and never exceeds max |
| `test_provider_manifest_rate_limit_patterns_are_detected` | Every provider manifest rate-limit sample is detected by Watchdog matching |
| `test_provider_manifest_auth_patterns_are_detected` | Every provider manifest auth sample is detected by Watchdog matching |
| `test_watchdog_config_patterns_extend_provider_patterns` | Configured patterns match without replacing provider-owned patterns |

Integration tests against real tmux should create an isolated session, write recognizable output, run `sweep_once()`, and clean up the session at the end. Failover tests should use a fake `FailoverExecutor` rather than launching real provider CLIs.

Provider compatibility tests should be table-driven from `ProviderRegistry::from_config()` rather than copying pattern strings into Watchdog tests. For each loaded provider, construct representative lines by embedding each configured pattern in terminal-like text and assert the Watchdog emits the expected `DetectedEvent`. This protects currently supported providers and makes adding a provider require only manifest coverage plus the generated compatibility test.

---

## 14. Implementation Order

1. Add `matcher.rs` and unit tests for literal/regex matching and priority.
2. Add `backoff.rs` and deterministic tests with injected RNG or jitter disabled.
3. Add `monitor.rs` with `sweep_once()` using fake registries and a tmux adapter seam.
4. Add `failover.rs` with fake `FailoverExecutor`, fake recorder, and fake provider registry tests.
5. Wire `Watchdog::run()` and event handling.
6. Add one real tmux integration test for pane capture and dead-pane detection.

---

## 15. Non-Goals

- No Telegram notification transport; the sink/controller owns external notification.
- No provider manifest editing; provider-owned patterns remain in `aegis-providers`.
- No task receipt parsing; M7 only triggers receipt processing.
- No persistent Watchdog database; registry files are the source of truth.
- No automatic sandbox policy relaxation after violations.
