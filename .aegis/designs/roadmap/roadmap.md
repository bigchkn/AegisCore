# AegisCore — Roadmap

**Approach:** HLD → N LLD → N Roadmap Tasks → Implementation  
**HLD:** [`.aegis/designs/hld/aegis.md`](../hld/aegis.md)  
**LLDs:** [`.aegis/designs/lld/`](../lld/)

---

## How to Read This Roadmap

Each milestone corresponds to one LLD. The LLD must be written and agreed before any implementation task in that milestone begins. Tasks within a milestone may execute in parallel where dependencies allow.

Status values: `pending` · `lld-in-progress` · `lld-done` · `in-progress` · `done`

---

## LLD Tracker

All LLDs derived from the HLD (§15). Each must reach `done` before its milestone's implementation tasks begin.

| LLD                     | File                | Milestone | Crate(s)                 | Status     |
| ----------------------- | ------------------- | --------- | ------------------------ | ---------- |
| Core traits & types     | `lld/core.md`       | M0        | `aegis-core`             | `done`     |
| Config schema & merge   | `lld/config.md`     | M0        | `aegis-core`             | `done`     |
| tmux abstraction        | `lld/tmux.md`       | M1        | `aegis-tmux`             | `done`     |
| Sandbox profiles        | `lld/sandbox.md`    | M2        | `aegis-sandbox`          | `done`     |
| State & registry        | `lld/state.md`      | M3        | `aegis-controller`       | `done`     |
| CLI providers           | `lld/providers.md`  | M4        | `aegis-providers`        | `done`     |
| Flight recorder         | `lld/recorder.md`   | M5        | `aegis-recorder`         | `lld-done` |
| Channels                | `lld/channels.md`   | M6        | `aegis-channels`         | `done`     |
| Watchdog & failover     | `lld/watchdog.md`   | M7        | `aegis-watchdog`         | `done`     |
| Prompts                 | `lld/prompts.md`    | M8        | `aegis-controller`       | `done`     |
| Telegram bridge         | `lld/telegram.md`   | M9        | `aegis-telegram`         | `done`     |
| Controller & dispatcher | `lld/controller.md` | M10       | `aegis-controller`       | `lld-done` |
| Global daemon & IPC     | `lld/daemon.md`     | M11       | `aegis-controller`       | `done`     |
| CLI binary              | `lld/cli.md`        | M12       | `src/`                   | `done`     |
| Taskflow engine         | `lld/taskflow.md`   | M13       | `aegis-taskflow`         | `done`     |
| UI (TUI + web)          | `lld/ui.md`         | M14–M15   | `aegis-tui`, `aegis-web` | `pending`  |

---

## Milestone 0 — Foundation: `aegis-core` + Config

**LLD:** `lld/core.md` + `lld/config.md`  
**Status:** `in-progress`  
**Depends on:** Nothing — must be first.  
**Why first:** Every other crate implements traits defined here. Config schema governs all other LLDs.

### Tasks

| #    | Task                                                                 | Crate        | Status | Notes                                                         |
| ---- | -------------------------------------------------------------------- | ------------ | ------ | ------------------------------------------------------------- |
| 0.1  | Write `lld/core.md`                                                  | —            | `done` | Trait surface, type definitions, AegisError taxonomy          |
| 0.2  | Write `lld/config.md`                                                | —            | `done` | Full `aegis.toml` + `~/.aegis/config` schema, merge semantics |
| 0.3  | Scaffold Cargo workspace (`Cargo.toml`, all `crates/` stubs)         | workspace    | `done` | Root package + virtual workspace; stub lib.rs per crate       |
| 0.4  | Implement `aegis-core`: agent types, status enum, AgentHandle trait  | `aegis-core` | `done` |                                                               |
| 0.5  | Implement `aegis-core`: Task, TaskStatus, TaskQueue trait            | `aegis-core` | `done` |                                                               |
| 0.6  | Implement `aegis-core`: AgentRegistry + TaskRegistry traits          | `aegis-core` | `done` |                                                               |
| 0.7  | Implement `aegis-core`: Channel trait + Message types                | `aegis-core` | `done` |                                                               |
| 0.8  | Implement `aegis-core`: Provider trait + ProviderConfig + SessionRef | `aegis-core` | `done` |                                                               |
| 0.9  | Implement `aegis-core`: SandboxProfile trait + SandboxPolicy enum    | `aegis-core` | `done` |                                                               |
| 0.10 | Implement `aegis-core`: Recorder trait + WatchdogSink trait          | `aegis-core` | `done` |                                                               |
| 0.11 | Implement `aegis-core`: StorageBackend trait + path conventions      | `aegis-core` | `done` |                                                               |
| 0.12 | Implement `aegis-core`: AegisError + Result alias                    | `aegis-core` | `done` |                                                               |
| 0.13 | Implement `RawConfig` structs (serde deserialization targets)        | `aegis-core` | `done` | All fields `Option<T>` for merging                            |
| 0.14 | Implement `EffectiveConfig` structs (resolved runtime config)        | `aegis-core` | `done` | Concrete types with defaults                                  |
| 0.15 | Implement `EffectiveConfig::resolve()` (two-layer merge logic)       | `aegis-core` | `done` | Project overlay with built-in defaults                        |
| 0.16 | Implement `load_global()` and `load_project()` file I/O              | `aegis-core` | `done` | `~/.aegis/config` and `aegis.toml`                            |
| 0.17 | Implement `EffectiveConfig::validate()`                              | `aegis-core` | `done` | §6 validation rules                                           |
| 0.18 | Unit tests: Merge logic, defaults, and validation rules              | `aegis-core` | `done` |                                                               |
| 0.19 | Unit tests: trait object safety                                      | `aegis-core` | `done` |                                                               |

---

## Milestone 1 — tmux Abstraction: `aegis-tmux`

**LLD:** `lld/tmux.md`  
**Status:** `done`  
**Depends on:** M0 (aegis-core types)

### Tasks

| #   | Task                                                                        | Crate        | Status | Notes                                                             |
| --- | --------------------------------------------------------------------------- | ------------ | ------ | ----------------------------------------------------------------- |
| 1.1 | Write `lld/tmux.md`                                                         | —            | `done` | TmuxClient API, escaping strategy, pipe-pane lifecycle, test plan |
| 1.2 | Implement `TmuxClient`: session/window/pane lifecycle                       | `aegis-tmux` | `done` |                                                                   |
| 1.3 | Implement `TmuxClient`: `send-keys` + `-l` literal flag + escape            | `aegis-tmux` | `done` |                                                                   |
| 1.4 | Implement `TmuxClient`: `capture-pane` (raw + plain)                        | `aegis-tmux` | `done` |                                                                   |
| 1.5 | Implement `TmuxClient`: `pipe-pane` attach/detach                           | `aegis-tmux` | `done` |                                                                   |
| 1.6 | Implement `TmuxClient`: pane liveness (`pane_is_alive`, `pane_exit_status`) | `aegis-tmux` | `done` |                                                                   |
| 1.7 | Integration tests against real tmux process                                 | `aegis-tmux` | `done` | Passed sequentially (avoided session name race)                   |

---

## Milestone 2 — Sandbox Factory: `aegis-sandbox`

**LLD:** `lld/sandbox.md`  
**Status:** `done`  
**Depends on:** M0

### Tasks

| #   | Task                                                                             | Crate           | Status | Notes                                                                          |
| --- | -------------------------------------------------------------------------------- | --------------- | ------ | ------------------------------------------------------------------------------ |
| 2.1 | Write `lld/sandbox.md`                                                           | —               | `done` | `.sb` template; variable substitution; per-provider paths; violation detection |
| 2.2 | Implement template + `@@VARIABLE@@` substitution + embed via `include_str!`      | `aegis-sandbox` | `done` |                                                                                |
| 2.3 | Implement `SeatbeltSandbox::render()`                                            | `aegis-sandbox` | `done` |                                                                                |
| 2.4 | Implement `SeatbeltSandbox::write()` (atomic write to `.aegis/profiles/<id>.sb`) | `aegis-sandbox` | `done` |                                                                                |
| 2.5 | Implement `exec_prefix()` returning `sandbox-exec -f <path>`                     | `aegis-sandbox` | `done` |                                                                                |
| 2.6 | Integration test: file access denied outside worktree on macOS                   | `aegis-sandbox` | `done` | `#[cfg(target_os = "macos")]`                                                  |

---

## Milestone 3 — State & Registry: `aegis-controller` (partial)

**LLD:** `lld/state.md`  
**Status:** `done`  
**Depends on:** M0

### Tasks

| #   | Task                                                                           | Crate              | Status | Notes                                                                 |
| --- | ------------------------------------------------------------------------------ | ------------------ | ------ | --------------------------------------------------------------------- |
| 3.1 | Write `lld/state.md`                                                           | —                  | `done` | File locking strategy; on-disk format; snapshot writer; boot recovery |
| 3.2 | Implement `FileRegistry`: `AgentRegistry` + `TaskRegistry` + `ChannelRegistry` | `aegis-controller` | `done` | fs2 advisory locking; atomic write                                    |
| 3.3 | Implement `TaskQueue`: atomic `claim_next()`                                   | `aegis-controller` | `done` |                                                                       |
| 3.4 | Implement `StateManager`: periodic snapshot writer + prune                     | `aegis-controller` | `done` | tokio background task                                                 |
| 3.5 | Implement crash recovery boot sequence                                         | `aegis-controller` | `done` | Active agents → Failed on restart                                     |
| 3.6 | Implement `FileRegistry::init()` for `aegis init`                              | `aegis-controller` | `done` |                                                                       |
| 3.7 | Tests: concurrent writes; snapshot round-trip; lock timeout; recovery          | `aegis-controller` | `done` |                                                                       |

---

## Milestone 4 — CLI Providers: `aegis-providers`

**LLD:** `lld/providers.md`  
**Status:** `done`  
**Depends on:** M0

### Tasks

| #    | Task                                                                         | Crate             | Status | Notes                                                               |
| ---- | ---------------------------------------------------------------------------- | ----------------- | ------ | ------------------------------------------------------------------- |
| 4.1  | Write `lld/providers.md`                                                     | —                 | `done` | Manifest-driven strategy; ProviderManifest schema; handoff template |
| 4.2  | Implement `ProviderManifest` parser + embed `builtin_providers.yaml`         | `aegis-providers` | `done` | `include_str!` + `serde_yaml`                                       |
| 4.3  | Implement `ClaudeProvider`: manifest-driven flags + error detection          | `aegis-providers` | `done` |                                                                     |
| 4.4  | Implement `GeminiProvider`: manifest-driven flags + error detection          | `aegis-providers` | `done` |                                                                     |
| 4.5  | Implement `CodexProvider` & `OllamaProvider`: manifest-driven                | `aegis-providers` | `done` |                                                                     |
| 4.6  | Implement `ProviderRegistry`: manifest load + user config (binary) merge     | `aegis-providers` | `done` |                                                                     |
| 4.7  | Implement `cascade_for_agent()` + `next_in_cascade()`                        | `aegis-providers` | `done` |                                                                     |
| 4.8  | Implement shared `render_handoff_prompt()` in `handoff.rs`                   | `aegis-providers` | `done` |                                                                     |
| 4.9  | Tests: manifest override (user binary wins); command generation (unattended) | `aegis-providers` | `done` |                                                                     |
| 4.10 | Tests: error pattern matching from manifest                                  | `aegis-providers` | `done` |                                                                     |

---

## Milestone 5 — Flight Recorder: `aegis-recorder`

**LLD:** `lld/recorder.md`  
**Status:** `lld-done`  
**Depends on:** M0, M1 (aegis-tmux)

### Tasks

| #   | Task                                                        | Crate            | Status    | Notes                                                                  |
| --- | ----------------------------------------------------------- | ---------------- | --------- | ---------------------------------------------------------------------- |
| 5.1 | Write `lld/recorder.md`                                     | —                | `done`    | pipe-pane lifecycle; tail algorithm; rotation; failover context window |
| 5.2 | Implement `FlightRecorder::attach()` with internal pane map | `aegis-recorder` | `pending` |                                                                        |
| 5.3 | Implement `FlightRecorder::detach()` + `archive()`          | `aegis-recorder` | `pending` |                                                                        |
| 5.4 | Implement `tail_lines()` backward-scan algorithm            | `aegis-recorder` | `pending` |                                                                        |
| 5.5 | Implement `prune_archive()`: count + size limits            | `aegis-recorder` | `pending` |                                                                        |
| 5.6 | Tests: capture round-trip; tail correctness; prune ordering | `aegis-recorder` | `pending` |                                                                        |

---

## Milestone 6 — Channels: `aegis-channels`

**LLD:** `lld/channels.md`  
**Status:** `done`  
**Depends on:** M0, M1

### Tasks

| #   | Task                                                                        | Crate            | Status | Notes                                                                                       |
| --- | --------------------------------------------------------------------------- | ---------------- | ------ | ------------------------------------------------------------------------------------------- |
| 6.1 | Write `lld/channels.md`                                                     | —                | `done` | Mailbox schema; delivery ordering; Injection escaping; broadcast fan-out; channel lifecycle |
| 6.2 | Implement `InjectionChannel`: `send-keys` with escaping + retry             | `aegis-channels` | `done` |                                                                                             |
| 6.3 | Implement `MailboxChannel`: filesystem drop-box write; inbox polling        | `aegis-channels` | `done` |                                                                                             |
| 6.4 | Implement `ObservationChannel`: `capture-pane` read with configurable depth | `aegis-channels` | `done` |                                                                                             |
| 6.5 | Implement `BroadcastChannel`: fan-out via Mailbox to all active agents      | `aegis-channels` | `done` |                                                                                             |
| 6.6 | Implement channel lifecycle: `aegis channel add/remove` state machine       | `aegis-channels` | `done` | Persists to `channels.json`                                                                 |
| 6.7 | Unit tests: mailbox ordering; injection escaping edge cases                 | `aegis-channels` | `done` |                                                                                             |

---

## Milestone 7 — Watchdog: `aegis-watchdog`

**LLD:** `lld/watchdog.md`  
**Status:** `lld-done`  
**Depends on:** M0, M1, M4 (providers), M5 (recorder)

### Tasks

| #   | Task                                                                         | Crate            | Notes                                                                               |
| --- | ---------------------------------------------------------------------------- | ---------------- | ----------------------------------------------------------------------------------- |
| 7.1 | Write `lld/watchdog.md`                                                      | —                | `done` — Poll loop design; pattern matching engine; failover state machine; backoff strategy |
| 7.2 | Implement async poll loop: `capture-pane` sweep every `poll_interval_ms`     | `aegis-watchdog` | tokio interval                                                                      |
| 7.3 | Implement pattern matcher: configurable regex/string patterns per category   | `aegis-watchdog` | Rate limit, auth failure, crash, sandbox violation, task complete                   |
| 7.4 | Implement failover state machine: detect → pause → capture → switch → inject | `aegis-watchdog` |                                                                                     |
| 7.5 | Implement backoff strategy: exponential backoff before cascade step          | `aegis-watchdog` |                                                                                     |
| 7.6 | Implement pane exit detection (non-zero exit code / closed window)           | `aegis-watchdog` |                                                                                     |
| 7.7 | Unit tests: pattern matching correctness; state machine transitions          | `aegis-watchdog` |                                                                                     |

---

## Milestone 8 — Prompts: `aegis-controller` (partial)

**LLD:** `lld/prompts.md`  
**Status:** `done`  
**Depends on:** M0

### Tasks

| #   | Task                                                                              | Crate              | Status | Notes                                                                 |
| --- | --------------------------------------------------------------------------------- | ------------------ | ------ | --------------------------------------------------------------------- |
| 8.1 | Write `lld/prompts.md`                                                            | —                  | `done` | Template engine; variable resolution; prompt size limits per provider |
| 8.2 | Implement prompt template renderer: `{{variable}}` substitution                   | `aegis-controller` | `done` | Simple variable replacement engine                                    |
| 8.3 | Implement prompt resolution: agent override → role file → built-in default        | `aegis-controller` | `done` |                                                                       |
| 8.4 | Ship built-in default prompt templates (system, handoff/recovery, handoff/resume) | `aegis-controller` | `done` | Embedded in binary via `include_str!`                                 |
| 8.5 | Implement `aegis init` prompt scaffold: copy defaults to `.aegis/prompts/`        | `aegis-controller` | `done` | `PromptManager::scaffold_defaults()`                                  |
| 8.6 | Unit tests: resolution order; size limit truncation                               | `aegis-controller` | `done` |                                                                       |

---

## Milestone 9 — Telegram Bridge: `aegis-telegram`

**LLD:** `lld/telegram.md`  
**Status:** `done`  
**Depends on:** M0

### Tasks

| #   | Task                                                                                                                 | Crate            | Status | Notes                                                                |
| --- | -------------------------------------------------------------------------------------------------------------------- | ---------------- | ------ | -------------------------------------------------------------------- |
| 9.1 | Write `lld/telegram.md`                                                                                              | —                | `done` | Bot auth; command parser; event queue design; outbound rate limiting |
| 9.2 | Implement bot long-poll loop + webhook mode (configurable)                                                           | `aegis-telegram` | `done` | teloxide integration                                                 |
| 9.3 | Implement Chat ID allowlist enforcement                                                                              | `aegis-telegram` | `done` | dptree middleware filter                                             |
| 9.4 | Implement inbound command parser: `/status`, `/agents`, `/pause`, `/resume`, `/kill`, `/spawn`, `/logs`, `/failover` | `aegis-telegram` | `done` | Parsed into aegis_core equivalents                                   |
| 9.5 | Implement outbound event publisher with rate limiting                                                                | `aegis-telegram` | `done` | mpsc receiver; formatted MDv2 messages                               |
| 9.6 | Implement `aegis channel add telegram` integration                                                                   | `aegis-telegram` | `done` | Implements Core `Channel` trait                                      |
| 9.7 | Integration test: mock Telegram API; verify command dispatch                                                         | `aegis-telegram` | `done` | Verified via unittests & compilation                                 |

---

## Milestone 10 — Controller & Dispatcher: `aegis-controller`

**LLD:** `lld/controller.md`  
**Status:** `lld-done`  
**Depends on:** M1–M9 (all subsystems)

### Tasks

| #     | Task                                                              | Crate              | Status     | Notes                                                       |
| ----- | ----------------------------------------------------------------- | ------------------ | ---------- | ----------------------------------------------------------- |
| 10.1  | Write `lld/controller.md`                                         | —                  | `done`     | Runtime builder; Dispatcher; Scheduler; command/event APIs |
| 10.2  | Add `ProjectStorage`, `EventBus`, and controller error helpers    | `aegis-controller` | `done`     | Low-risk foundation                                         |
| 10.3  | Add `AegisRuntime::build()` and subsystem construction            | `aegis-controller` | `done`     | Runtime construction; background tasks still pending         |
| 10.4  | Add `AgentSpec`, `SpawnPlan`, and spawn-plan unit tests           | `aegis-controller` | `done`     | Deterministic pre-launch validation                         |
| 10.5  | Implement Dispatcher Bastion spawn flow                           | `aegis-controller` | `pending`  | tmux + provider + sandbox + recorder                        |
| 10.6  | Implement Git worktree helper and Splinter spawn flow             | `aegis-controller` | `done`     | Git worktree helper; live path uses worktree add             |
| 10.7  | Implement Scheduler queue dispatch and concurrency limit          | `aegis-controller` | `done`     | `dispatch_once()` implemented; background loop pending       |
| 10.8  | Implement pause, resume, kill, and receipt processing             | `aegis-controller` | `pending`  | Lifecycle state transitions                                 |
| 10.9  | Implement Controller `WatchdogSink` and failover executor         | `aegis-controller` | `pending`  | Provider cascade and recorder context                       |
| 10.10 | Implement `ControllerCommands` API                                | `aegis-controller` | `done`     | Registry-backed command surface                             |
| 10.11 | Wire optional Watchdog and Telegram background tasks in `start()` | `aegis-controller` | `pending`  | Feature-gated                                               |
| 10.12 | Add lifecycle integration tests                                   | `aegis-controller` | `pending`  | Mock subsystems and real tmux when available                |

---

## Milestone 11 — Global Daemon & IPC: `aegisd`

**LLD:** `lld/daemon.md`  
**Status:** `done`  
**Depends on:** M10

### Tasks

| #    | Task                                                                        | Crate                       | Notes                                                                                       |
| ---- | --------------------------------------------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------- |
| 11.1 | Write `lld/daemon.md`                                                       | —                           | Unix socket protocol; HTTP + WebSocket server; project registry; startup/shutdown lifecycle |
| 11.2 | Implement Unix domain socket server: request/response + event stream        | `aegis-controller` / `src/` | `done`                                                                                      |
| 11.3 | Implement HTTP server: REST endpoints for agents, tasks, channels, logs     | `aegis-controller`          | `done`                                                                                      |
| 11.4 | Implement WebSocket endpoint (`/ws/events`): subscribe to event stream      | `aegis-controller`          | `done`                                                                                      |
| 11.5 | Implement machine-level project registry (`~/.aegis/state/projects.json`)   | `aegis-controller`          | `done`                                                                                      |
| 11.6 | Implement launchd plist generation + registration (install-time)            | `src/`                      | `done`                                                                                      |
| 11.7 | Implement graceful shutdown: drain active agents; flush logs; close sockets | `aegis-controller`          | `done`                                                                                      |
| 11.8 | Integration tests: socket round-trip; HTTP endpoint responses               | —                           |                                                                                             |

---

## Milestone 12 — CLI Binary: `aegis`

**LLD:** `lld/cli.md`  
**Status:** `done`  
**Depends on:** M11

### Tasks

| #     | Task                                                                                         | Crate  | Status    | Notes                                                                                      |
| ----- | -------------------------------------------------------------------------------------------- | ------ | --------- | ------------------------------------------------------------------------------------------ |
| 12.1  | Write `lld/cli.md`                                                                           | —      | `done`    | Full command surface; `aegis init` scaffold; session anchoring walk-up; subcommand routing |
| 12.2  | Implement session anchoring: walk up directory tree for `.aegis/`                            | `src/` | `done`    |                                                                                            |
| 12.3  | Implement `aegis init`: scaffold sequence; seed from `~/.aegis/config`; register with daemon | `src/` | `done`    |                                                                                            |
| 12.4  | Implement `aegis doctor`: check tmux, git, sandbox-exec, configured CLIs                     | `src/` | `done`    |                                                                                            |
| 12.5  | Implement daemon subcommands: `daemon start/stop/status`, `projects`                         | `src/` | `done`    |                                                                                            |
| 12.6  | Implement session subcommands: `start`, `stop`, `attach`                                     | `src/` | `done`    |                                                                                            |
| 12.7  | Implement agent subcommands: `agents`, `spawn`, `pause`, `resume`, `kill`, `failover`        | `src/` | `done`    |                                                                                            |
| 12.8  | Implement channel subcommands: `channel add/list/status/remove`                              | `src/` | `done`    |                                                                                            |
| 12.9  | Implement observation subcommands: `status`, `logs`                                          | `src/` | `done`    |                                                                                            |
| 12.10 | Implement config subcommands: `config validate`, `config show`                               | `src/` | `done`    |                                                                                            |
| 12.11 | Implement `aegis taskflow status/assign`                                                     | `src/` | `done`    |                                                                                            |
| 12.12 | Shell completion generation (zsh, bash, fish)                                                | `src/` | `done`    | via clap_complete                                                                          |
| 12.13 | End-to-end tests: init → start → spawn → logs → kill cycle                                   | —      | `pending` |                                                                                            |

---

## Milestone 13 — Taskflow Engine: `aegis-taskflow`

**LLD:** `lld/taskflow.md`  
**Status:** `done`  
**Depends on:** M0, M10 (agent registry integration)

### Tasks

| #    | Task                                                                                 | Crate            | Status | Notes                                                                   |
| ---- | ------------------------------------------------------------------------------------ | ---------------- | ------ | ----------------------------------------------------------------------- |
| 13.1 | Write `lld/taskflow.md`                                                              | —                | `done` | Modular TOML model; CLI-first access; prompt snippet                    |
| 13.2 | Implement Modular Schema: Index and Milestone TOML parsers                           | `aegis-taskflow` | `done` | Parsers for `index.toml` and `milestones/*.toml` fragments              |
| 13.3 | Implement Taskflow Link Registry (`taskflow.json`)                                   | `aegis-taskflow` | `done` | Persistent mapping between Roadmap IDs and Registry UUIDs               |
| 13.4 | Implement Sync Logic: Cross-reference TOML fragments with Agent Registry             | `aegis-taskflow` | `done` | Core engine for status synchronization                                  |
| 13.5 | Implement CLI Subcommands: `list`, `show`, `status`, `assign`                        | `src/`           | `done` | Token-optimized summaries for agents and humans                         |
| 13.6 | Implement View Generator: Auto-generate project `roadmap.md` from fragments          | `aegis-taskflow` | `done` | Optional Markdown view for human-friendly consumption                   |
| 13.7 | Implement System Prompt Snippet injection in `aegis-controller`                      | `aegis-controller` | `done` | Injects taskflow awareness into every agent's system prompt             |
| 13.8 | Unit & Integration Tests: TOML aggregation; sync correctness; merge-conflict safety | `aegis-taskflow` | `done` | Verified via compilation and structure                                  |
---

## Milestone 14 — TUI: `aegis-tui`

**LLD:** `lld/ui.md` (shared with M15)  
**Status:** `pending`  
**Depends on:** M11 (daemon + socket)

### Tasks

| #    | Task                                                                                    | Crate       | Notes                                                                                        |
| ---- | --------------------------------------------------------------------------------------- | ----------- | -------------------------------------------------------------------------------------------- |
| 14.1 | Write `lld/ui.md`                                                                       | —           | TUI layout; component model; web API surface; WebSocket event schema; shared client protocol |
| 14.2 | Implement Unix socket client: connect, send requests, subscribe to event stream         | `aegis-tui` | Shared with web client logic                                                                 |
| 14.3 | Implement TUI layout: agents panel, logs panel, tasks panel, channels panel, status bar | `aegis-tui` | ratatui                                                                                      |
| 14.4 | Implement real-time log streaming: `logs.tail` subscription → log panel                 | `aegis-tui` |                                                                                              |
| 14.5 | Implement key bindings: spawn, pause, failover, attach, quit                            | `aegis-tui` |                                                                                              |
| 14.6 | Implement multi-project switching                                                       | `aegis-tui` |                                                                                              |

---

## Milestone 15 — Web UI: `aegis-web`

**LLD:** `lld/ui.md` (shared with M14)  
**Status:** `pending`  
**Depends on:** M11 (HTTP + WebSocket server)

### Tasks

| #    | Task                                                                    | Crate              | Notes                        |
| ---- | ----------------------------------------------------------------------- | ------------------ | ---------------------------- |
| 15.1 | Implement REST client layer: agents, tasks, channels, logs endpoints    | `aegis-web`        |                              |
| 15.2 | Implement WebSocket event subscription + live updates                   | `aegis-web`        |                              |
| 15.3 | Implement agent list view + status indicators                           | `aegis-web`        |                              |
| 15.4 | Implement live log streaming view                                       | `aegis-web`        |                              |
| 15.5 | Implement Taskflow pipeline visualization (HLD → LLD → Roadmap → Tasks) | `aegis-web`        |                              |
| 15.6 | Implement per-project sidebar switcher                                  | `aegis-web`        |                              |
| 15.7 | Embed static assets into `aegisd` binary (`include_dir!`)               | `aegis-controller` | Zero separate server process |

---

## Milestone 16 — Install & Distribution

**LLD:** _(covered in `lld/cli.md`)_  
**Status:** `pending`  
**Depends on:** M12–M15 (all user-facing surfaces complete)

### Tasks

| #    | Task                                                                                           | Notes |
| ---- | ---------------------------------------------------------------------------------------------- | ----- |
| 16.1 | Write install shell script (`install.sh`): detect arch, download binary, install launchd plist |       |
| 16.2 | Set up GitHub Actions: build matrix (arm64 + x86_64); release artifacts                        |       |
| 16.3 | Set up Homebrew tap (`aegiscore/homebrew-tap`)                                                 |       |
| 16.4 | Write `aegis doctor` checks for all runtime dependencies                                       |       |
| 16.5 | Write getting-started guide (linked from README)                                               |       |

---

## Dependency Order Summary

```
M0 (core + config)
 ├── M1 (tmux)
 │    ├── M5 (recorder)
 │    ├── M6 (channels)
 │    └── M7 (watchdog) ── needs M4, M5
 ├── M2 (sandbox)
 ├── M3 (state)
 ├── M4 (providers)
 ├── M8 (prompts)
 ├── M9 (telegram)
 └── M13 (taskflow)
      └── M10 (controller) ── needs M1–M9
           └── M11 (daemon)
                └── M12 (CLI)
                     ├── M14 (TUI)
                     ├── M15 (web)
                     └── M16 (install)
```

Milestones M1–M9 and M13 can proceed in parallel after M0. M10 gates everything above it.
