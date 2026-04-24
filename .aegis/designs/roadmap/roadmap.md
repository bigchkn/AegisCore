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

| LLD | File | Milestone | Crate(s) | Status |
|---|---|---|---|---|
| Core traits & types | `lld/core.md` | M0 | `aegis-core` | `lld-done` |
| Config schema & merge | `lld/config.md` | M0 | `aegis-core` | `lld-done` |
| tmux abstraction | `lld/tmux.md` | M1 | `aegis-tmux` | `lld-done` |
| Sandbox profiles | `lld/sandbox.md` | M2 | `aegis-sandbox` | `lld-done` |
| State & registry | `lld/state.md` | M3 | `aegis-controller` | `lld-done` |
| CLI providers | `lld/providers.md` | M4 | `aegis-providers` | `lld-done` |
| Flight recorder | `lld/recorder.md` | M5 | `aegis-recorder` | `lld-done` |
| Channels | `lld/channels.md` | M6 | `aegis-channels` | `pending` |
| Watchdog & failover | `lld/watchdog.md` | M7 | `aegis-watchdog` | `pending` |
| Prompts | `lld/prompts.md` | M8 | `aegis-controller` | `pending` |
| Telegram bridge | `lld/telegram.md` | M9 | `aegis-telegram` | `pending` |
| Controller & dispatcher | `lld/controller.md` | M10 | `aegis-controller` | `pending` |
| Global daemon & IPC | `lld/daemon.md` | M11 | `aegis-controller` | `pending` |
| CLI binary | `lld/cli.md` | M12 | `src/` | `pending` |
| Taskflow engine | `lld/taskflow.md` | M13 | `aegis-taskflow` | `pending` |
| UI (TUI + web) | `lld/ui.md` | M14–M15 | `aegis-tui`, `aegis-web` | `pending` |

---

## Milestone 0 — Foundation: `aegis-core` + Config

**LLD:** `lld/core.md` + `lld/config.md`  
**Status:** `in-progress`  
**Depends on:** Nothing — must be first.  
**Why first:** Every other crate implements traits defined here. Config schema governs all other LLDs.

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 0.1 | Write `lld/core.md` | — | `done` | Trait surface, type definitions, AegisError taxonomy |
| 0.2 | Write `lld/config.md` | — | `done` | Full `aegis.toml` + `~/.aegis/config` schema, merge semantics |
| 0.3 | Scaffold Cargo workspace (`Cargo.toml`, all `crates/` stubs) | workspace | `done` | Root package + virtual workspace; stub lib.rs per crate |
| 0.4 | Implement `aegis-core`: agent types, status enum, AgentHandle trait | `aegis-core` | `done` | |
| 0.5 | Implement `aegis-core`: Task, TaskStatus, TaskQueue trait | `aegis-core` | `done` | |
| 0.6 | Implement `aegis-core`: AgentRegistry + TaskRegistry traits | `aegis-core` | `done` | |
| 0.7 | Implement `aegis-core`: Channel trait + Message types | `aegis-core` | `done` | |
| 0.8 | Implement `aegis-core`: Provider trait + ProviderConfig + SessionRef | `aegis-core` | `done` | |
| 0.9 | Implement `aegis-core`: SandboxProfile trait + SandboxPolicy enum | `aegis-core` | `done` | |
| 0.10 | Implement `aegis-core`: Recorder trait + WatchdogSink trait | `aegis-core` | `done` | |
| 0.11 | Implement `aegis-core`: StorageBackend trait + path conventions | `aegis-core` | `done` | |
| 0.12 | Implement `aegis-core`: AegisError + Result alias | `aegis-core` | `done` | |
| 0.13 | Implement config TOML parsing + two-layer merge (`~/.aegis/config` → `aegis.toml`) | `aegis-core` | `pending` | Covered by `lld/config.md`; separate task |
| 0.14 | Unit tests: trait object safety | `aegis-core` | `done` | Config merge tests deferred to 0.13 |

---

## Milestone 1 — tmux Abstraction: `aegis-tmux`

**LLD:** `lld/tmux.md`  
**Status:** `in-progress`  
**Depends on:** M0 (aegis-core types)

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 1.1 | Write `lld/tmux.md` | — | `done` | TmuxClient API, escaping strategy, pipe-pane lifecycle, test plan |
| 1.2 | Implement `TmuxClient`: session/window/pane lifecycle | `aegis-tmux` | `in-progress` | |
| 1.3 | Implement `TmuxClient`: `send-keys` + `-l` literal flag + escape | `aegis-tmux` | `in-progress` | |
| 1.4 | Implement `TmuxClient`: `capture-pane` (raw + plain) | `aegis-tmux` | `in-progress` | |
| 1.5 | Implement `TmuxClient`: `pipe-pane` attach/detach | `aegis-tmux` | `in-progress` | |
| 1.6 | Implement `TmuxClient`: pane liveness (`pane_is_alive`, `pane_exit_status`) | `aegis-tmux` | `in-progress` | |
| 1.7 | Integration tests against real tmux process | `aegis-tmux` | `pending` | CI must have tmux installed |

---

## Milestone 2 — Sandbox Factory: `aegis-sandbox`

**LLD:** `lld/sandbox.md`  
**Status:** `lld-done`  
**Depends on:** M0

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 2.1 | Write `lld/sandbox.md` | — | `done` | `.sb` template; variable substitution; per-provider paths; violation detection |
| 2.2 | Implement template + `@@VARIABLE@@` substitution + embed via `include_str!` | `aegis-sandbox` | `pending` | |
| 2.3 | Implement `SeatbeltSandbox::render()` | `aegis-sandbox` | `pending` | |
| 2.4 | Implement `SeatbeltSandbox::write()` (atomic write to `.aegis/profiles/<id>.sb`) | `aegis-sandbox` | `pending` | |
| 2.5 | Implement `exec_prefix()` returning `sandbox-exec -f <path>` | `aegis-sandbox` | `pending` | |
| 2.6 | Integration test: file access denied outside worktree on macOS | `aegis-sandbox` | `pending` | `#[cfg(target_os = "macos")]` |

---

## Milestone 3 — State & Registry: `aegis-controller` (partial)

**LLD:** `lld/state.md`  
**Status:** `lld-done`  
**Depends on:** M0

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 3.1 | Write `lld/state.md` | — | `done` | File locking strategy; on-disk format; snapshot writer; boot recovery |
| 3.2 | Implement `FileRegistry`: `AgentRegistry` + `TaskRegistry` + `ChannelRegistry` | `aegis-controller` | `pending` | fs2 advisory locking; atomic write |
| 3.3 | Implement `TaskQueue`: atomic `claim_next()` | `aegis-controller` | `pending` | |
| 3.4 | Implement `StateManager`: periodic snapshot writer + prune | `aegis-controller` | `pending` | tokio background task |
| 3.5 | Implement crash recovery boot sequence | `aegis-controller` | `pending` | Active agents → Failed on restart |
| 3.6 | Implement `FileRegistry::init()` for `aegis init` | `aegis-controller` | `pending` | |
| 3.7 | Tests: concurrent writes; snapshot round-trip; lock timeout; recovery | `aegis-controller` | `pending` | |

---

## Milestone 4 — CLI Providers: `aegis-providers`

**LLD:** `lld/providers.md`  
**Status:** `lld-done`  
**Depends on:** M0

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 4.1 | Write `lld/providers.md` | — | `done` | Per-CLI specs; ProviderRegistry; cascade logic; handoff prompt template |
| 4.2 | Implement `ClaudeProvider` | `aegis-providers` | `pending` | feature = `claude` |
| 4.3 | Implement `GeminiProvider` (post-spawn resume via send-keys) | `aegis-providers` | `pending` | feature = `gemini` |
| 4.4 | Implement `CodexProvider` | `aegis-providers` | `pending` | feature = `codex` |
| 4.5 | Implement `OllamaProvider` | `aegis-providers` | `pending` | feature = `ollama`; no rate limits |
| 4.6 | Implement `ProviderRegistry::from_config()` with feature-gated registration | `aegis-providers` | `pending` | |
| 4.7 | Implement `cascade_for_agent()` + `next_in_cascade()` | `aegis-providers` | `pending` | |
| 4.8 | Implement shared `render_handoff_prompt()` | `aegis-providers` | `pending` | |
| 4.9 | Tests: pattern detection; cascade ordering; handoff prompt content | `aegis-providers` | `pending` | |

---

## Milestone 5 — Flight Recorder: `aegis-recorder`

**LLD:** `lld/recorder.md`  
**Status:** `lld-done`  
**Depends on:** M0, M1 (aegis-tmux)

### Tasks

| # | Task | Crate | Status | Notes |
|---|---|---|---|---|
| 5.1 | Write `lld/recorder.md` | — | `done` | pipe-pane lifecycle; tail algorithm; rotation; failover context window |
| 5.2 | Implement `FlightRecorder::attach()` with internal pane map | `aegis-recorder` | `pending` | |
| 5.3 | Implement `FlightRecorder::detach()` + `archive()` | `aegis-recorder` | `pending` | |
| 5.4 | Implement `tail_lines()` backward-scan algorithm | `aegis-recorder` | `pending` | |
| 5.5 | Implement `prune_archive()`: count + size limits | `aegis-recorder` | `pending` | |
| 5.6 | Tests: capture round-trip; tail correctness; prune ordering | `aegis-recorder` | `pending` | |

---

## Milestone 6 — Channels: `aegis-channels`

**LLD:** `lld/channels.md`  
**Status:** `pending`  
**Depends on:** M0, M1

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 6.1 | Write `lld/channels.md` | — | Mailbox schema; delivery ordering; Injection escaping; broadcast fan-out; channel lifecycle |
| 6.2 | Implement `InjectionChannel`: `send-keys` with escaping + retry | `aegis-channels` | |
| 6.3 | Implement `MailboxChannel`: filesystem drop-box write; inbox polling | `aegis-channels` | |
| 6.4 | Implement `ObservationChannel`: `capture-pane` read with configurable depth | `aegis-channels` | |
| 6.5 | Implement `BroadcastChannel`: fan-out via Mailbox to all active agents | `aegis-channels` | |
| 6.6 | Implement channel lifecycle: `aegis channel add/remove` state machine | `aegis-channels` | Persists to `channels.json` |
| 6.7 | Unit tests: mailbox ordering; injection escaping edge cases | `aegis-channels` | |

---

## Milestone 7 — Watchdog: `aegis-watchdog`

**LLD:** `lld/watchdog.md`  
**Status:** `pending`  
**Depends on:** M0, M1, M4 (providers), M5 (recorder)

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 7.1 | Write `lld/watchdog.md` | — | Poll loop design; pattern matching engine; failover state machine; backoff strategy |
| 7.2 | Implement async poll loop: `capture-pane` sweep every `poll_interval_ms` | `aegis-watchdog` | tokio interval |
| 7.3 | Implement pattern matcher: configurable regex/string patterns per category | `aegis-watchdog` | Rate limit, auth failure, crash, sandbox violation, task complete |
| 7.4 | Implement failover state machine: detect → pause → capture → switch → inject | `aegis-watchdog` | |
| 7.5 | Implement backoff strategy: exponential backoff before cascade step | `aegis-watchdog` | |
| 7.6 | Implement pane exit detection (non-zero exit code / closed window) | `aegis-watchdog` | |
| 7.7 | Unit tests: pattern matching correctness; state machine transitions | `aegis-watchdog` | |

---

## Milestone 8 — Prompts: `aegis-controller` (partial)

**LLD:** `lld/prompts.md`  
**Status:** `pending`  
**Depends on:** M0

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 8.1 | Write `lld/prompts.md` | — | Template engine; variable resolution; prompt size limits per provider |
| 8.2 | Implement prompt template renderer: `{{variable}}` substitution | `aegis-controller` | |
| 8.3 | Implement prompt resolution: agent override → role file → built-in default | `aegis-controller` | |
| 8.4 | Ship built-in default prompt templates (system, handoff/recovery, handoff/resume) | `aegis-controller` | Embedded in binary |
| 8.5 | Implement `aegis init` prompt scaffold: copy defaults to `.aegis/prompts/` | `aegis-controller` | |
| 8.6 | Unit tests: resolution order; size limit truncation | `aegis-controller` | |

---

## Milestone 9 — Telegram Bridge: `aegis-telegram`

**LLD:** `lld/telegram.md`  
**Status:** `pending`  
**Depends on:** M0

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 9.1 | Write `lld/telegram.md` | — | Bot auth; command parser; event queue design; outbound rate limiting |
| 9.2 | Implement bot long-poll loop + webhook mode (configurable) | `aegis-telegram` | |
| 9.3 | Implement Chat ID allowlist enforcement | `aegis-telegram` | |
| 9.4 | Implement inbound command parser: `/status`, `/agents`, `/pause`, `/resume`, `/kill`, `/spawn`, `/logs`, `/failover` | `aegis-telegram` | |
| 9.5 | Implement outbound event publisher with rate limiting | `aegis-telegram` | |
| 9.6 | Implement `aegis channel add telegram` integration | `aegis-telegram` | Activated via channel lifecycle |
| 9.7 | Integration test: mock Telegram API; verify command dispatch | `aegis-telegram` | |

---

## Milestone 10 — Controller & Dispatcher: `aegis-controller`

**LLD:** `lld/controller.md`  
**Status:** `pending`  
**Depends on:** M1–M9 (all subsystems)

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 10.1 | Write `lld/controller.md` | — | Builder pattern; Dispatcher spawn sequence; registry locking; tokio runtime design |
| 10.2 | Implement `AegisRuntime` builder: accept optional subsystem impls | `aegis-controller` | |
| 10.3 | Implement Dispatcher: Bastion spawn sequence (worktree → sandbox profile → tmux window → pipe-pane) | `aegis-controller` | |
| 10.4 | Implement Dispatcher: Splinter spawn sequence + git worktree management | `aegis-controller` | |
| 10.5 | Implement Dispatcher: clean termination + receipt processing + worktree prune | `aegis-controller` | |
| 10.6 | Implement Scheduler: `MAX_SPLINTERS` semaphore + task queue | `aegis-controller` | |
| 10.7 | Implement agent status transitions + registry updates | `aegis-controller` | |
| 10.8 | Integration tests: full Bastion + Splinter lifecycle with mock CLI | `aegis-controller` | |

---

## Milestone 11 — Global Daemon & IPC: `aegisd`

**LLD:** `lld/daemon.md`  
**Status:** `pending`  
**Depends on:** M10

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 11.1 | Write `lld/daemon.md` | — | Unix socket protocol; HTTP + WebSocket server; project registry; startup/shutdown lifecycle |
| 11.2 | Implement Unix domain socket server: request/response + event stream | `aegis-controller` / `src/` | Line-delimited JSON |
| 11.3 | Implement HTTP server: REST endpoints for agents, tasks, channels, logs | `aegis-controller` | axum or similar |
| 11.4 | Implement WebSocket endpoint (`/ws/events`): subscribe to event stream | `aegis-controller` | |
| 11.5 | Implement machine-level project registry (`~/.aegis/state/projects.json`) | `aegis-controller` | |
| 11.6 | Implement launchd plist generation + registration (install-time) | `src/` | |
| 11.7 | Implement graceful shutdown: drain active agents; flush logs; close sockets | `aegis-controller` | |
| 11.8 | Integration tests: socket round-trip; HTTP endpoint responses | — | |

---

## Milestone 12 — CLI Binary: `aegis`

**LLD:** `lld/cli.md`  
**Status:** `pending`  
**Depends on:** M11

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 12.1 | Write `lld/cli.md` | — | Full command surface; `aegis init` scaffold; session anchoring walk-up; subcommand routing |
| 12.2 | Implement session anchoring: walk up directory tree for `.aegis/` | `src/` | |
| 12.3 | Implement `aegis init`: scaffold sequence; seed from `~/.aegis/config`; register with daemon | `src/` | |
| 12.4 | Implement `aegis doctor`: check tmux, git, sandbox-exec, configured CLIs | `src/` | |
| 12.5 | Implement daemon subcommands: `daemon start/stop/status`, `projects` | `src/` | |
| 12.6 | Implement session subcommands: `start`, `stop`, `attach` | `src/` | |
| 12.7 | Implement agent subcommands: `agents`, `spawn`, `pause`, `resume`, `kill`, `failover` | `src/` | |
| 12.8 | Implement channel subcommands: `channel add/list/status/remove` | `src/` | |
| 12.9 | Implement observation subcommands: `status`, `logs` | `src/` | |
| 12.10 | Implement config subcommands: `config validate`, `config show` | `src/` | |
| 12.11 | Implement `aegis taskflow status/assign` | `src/` | |
| 12.12 | Shell completion generation (zsh, bash, fish) | `src/` | via clap |
| 12.13 | End-to-end tests: init → start → spawn → logs → kill cycle | — | |

---

## Milestone 13 — Taskflow Engine: `aegis-taskflow`

**LLD:** `lld/taskflow.md`  
**Status:** `pending`  
**Depends on:** M0, M10 (agent registry integration)

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 13.1 | Write `lld/taskflow.md` | — | HLD→LLD→Roadmap→Task pipeline; document schema; task state machine; agent registry integration |
| 13.2 | Implement document schema: HLD, LLD, Roadmap, Task types | `aegis-taskflow` | Parsed from `.aegis/designs/` markdown |
| 13.3 | Implement task state machine: pending → lld-in-progress → lld-done → in-progress → done | `aegis-taskflow` | |
| 13.4 | Implement `aegis taskflow status`: render pipeline state from design directory | `aegis-taskflow` | |
| 13.5 | Implement `aegis taskflow assign`: link task to agent in registry | `aegis-taskflow` | |
| 13.6 | Implement roadmap parser: read `roadmap.md`; extract milestones and tasks | `aegis-taskflow` | |
| 13.7 | Unit tests: state machine transitions; roadmap parse correctness | `aegis-taskflow` | |

---

## Milestone 14 — TUI: `aegis-tui`

**LLD:** `lld/ui.md` (shared with M15)  
**Status:** `pending`  
**Depends on:** M11 (daemon + socket)

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 14.1 | Write `lld/ui.md` | — | TUI layout; component model; web API surface; WebSocket event schema; shared client protocol |
| 14.2 | Implement Unix socket client: connect, send requests, subscribe to event stream | `aegis-tui` | Shared with web client logic |
| 14.3 | Implement TUI layout: agents panel, logs panel, tasks panel, channels panel, status bar | `aegis-tui` | ratatui |
| 14.4 | Implement real-time log streaming: `logs.tail` subscription → log panel | `aegis-tui` | |
| 14.5 | Implement key bindings: spawn, pause, failover, attach, quit | `aegis-tui` | |
| 14.6 | Implement multi-project switching | `aegis-tui` | |

---

## Milestone 15 — Web UI: `aegis-web`

**LLD:** `lld/ui.md` (shared with M14)  
**Status:** `pending`  
**Depends on:** M11 (HTTP + WebSocket server)

### Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 15.1 | Implement REST client layer: agents, tasks, channels, logs endpoints | `aegis-web` | |
| 15.2 | Implement WebSocket event subscription + live updates | `aegis-web` | |
| 15.3 | Implement agent list view + status indicators | `aegis-web` | |
| 15.4 | Implement live log streaming view | `aegis-web` | |
| 15.5 | Implement Taskflow pipeline visualization (HLD → LLD → Roadmap → Tasks) | `aegis-web` | |
| 15.6 | Implement per-project sidebar switcher | `aegis-web` | |
| 15.7 | Embed static assets into `aegisd` binary (`include_dir!`) | `aegis-controller` | Zero separate server process |

---

## Milestone 16 — Install & Distribution

**LLD:** _(covered in `lld/cli.md`)_  
**Status:** `pending`  
**Depends on:** M12–M15 (all user-facing surfaces complete)

### Tasks

| # | Task | Notes |
|---|---|---|
| 16.1 | Write install shell script (`install.sh`): detect arch, download binary, install launchd plist | |
| 16.2 | Set up GitHub Actions: build matrix (arm64 + x86_64); release artifacts | |
| 16.3 | Set up Homebrew tap (`aegiscore/homebrew-tap`) | |
| 16.4 | Write `aegis doctor` checks for all runtime dependencies | |
| 16.5 | Write getting-started guide (linked from README) | |

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
