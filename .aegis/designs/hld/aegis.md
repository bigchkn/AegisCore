# AegisCore — High-Level Design

**Status:** Draft v0.3  
**Approach:** HLD → N LLD → N Roadmap Tasks  
**Platform:** macOS (Darwin / Apple Silicon primary)  
**Implementation Language:** Rust (preferred throughout)

---

## 1. Introduction

### 1.1 Purpose

AegisCore is a multi-agent orchestration engine that runs autonomous AI CLI agents inside kernel-enforced sandboxes on macOS. It coordinates a hierarchy of long-lived **Bastion** agents and ephemeral **Splinter** agents, routes work between them via a structured channel layer, and maintains unbroken context across CLI failures through a passive Flight Recorder.

### 1.2 Design Philosophy

| Principle | Expression |
|---|---|
| Zero-container isolation | macOS `sandbox-exec` (Seatbelt) instead of Docker/VMs |
| Rust-first | Controller, Watchdog, Sandbox Factory, Channel router all in Rust |
| Terminal-native | `tmux` is the execution plane; no web server, no broker |
| User-configured | All CLI providers, fallback cascades, concurrency limits, and sandbox policies are user-defined in TOML |
| Context indestructibility | Every agent's I/O is mirrored continuously; no state lives only in a process |
| Human-in-the-loop | Telegram bridge enables remote observation and manual override |
| Pluggable workspace | Cargo workspace of independent crates; subsystems compile in parallel and can be excluded via feature flags |

### 1.3 Core Properties

- **Bastion agents** maintain long-lived project context across sessions.
- **Splinter agents** are spawned on demand for discrete tasks, then evaporate.
- **Flight Recorder** mirrors all terminal I/O to an append-only log before any failure can occur.
- **Watchdog** monitors for rate limits, sandbox violations, and CLI errors; triggers failover automatically.
- **Sandbox Factory** generates per-agent `.sb` profiles at spawn time, locking each agent to its assigned Git worktree.
- **CLI Provider Layer** abstracts `claude-code`, `gemini-cli`, `codex`, `ollama`, and future providers behind a uniform interface with user-defined failover cascades.

---

## 2. System Architecture

### 2.1 Logical Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Remote Control Plane                                        │
│  Telegram Bot  ←→  AegisCore Controller                     │
└───────────────────────────┬─────────────────────────────────┘
                            │ commands / events
┌───────────────────────────▼─────────────────────────────────┐
│  Control Plane (Rust binary: aegisd)                         │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌─────────────┐  │
│  │ Dispatcher │ │ Registry │ │ Watchdog │ │ Scheduler   │  │
│  └────────────┘ └──────────┘ └──────────┘ └─────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │ tmux send-keys / capture-pane / pipe-pane
┌───────────────────────────▼─────────────────────────────────┐
│  Execution Plane (tmux sessions)                             │
│  ┌──────────────────────┐   ┌───────────────────────────┐   │
│  │ Bastion Window       │   │ Splinter Windows (n)       │   │
│  │ sandbox-exec + CLI   │   │ sandbox-exec + CLI         │   │
│  │ Git Worktree (main)  │   │ Git Worktrees (isolated)   │   │
│  └──────────────────────┘   └───────────────────────────┘   │
└───────────────────────────┬─────────────────────────────────┘
                            │ read/write
┌───────────────────────────▼─────────────────────────────────┐
│  Persistence Plane (.aegis/ directory)                       │
│  logs/  state/  channels/  profiles/  handoff/  prompts/    │
└─────────────────────────────────────────────────────────────┘
```

### 2.2 Component Responsibilities

| Component | Responsibility |
|---|---|
| **Dispatcher** | Spawn/terminate Bastion and Splinter agents; create git worktrees; launch tmux windows; invoke Sandbox Factory |
| **Registry** | Maintain authoritative state of all agents (`registry.json`); CRUD over agent records |
| **Watchdog** | Poll all active agent panes via `capture-pane`; detect error patterns; trigger Failover logic |
| **Scheduler** | Enforce `MAX_SPLINTERS` concurrency limit via semaphore; queue pending Splinter tasks |
| **Sandbox Factory** | Generate per-agent `.sb` profiles from template; inject worktree path at spawn time |
| **Channel Router** | Route messages between agents via Injection, Mailbox, and Observation sub-channels |
| **Flight Recorder** | Attach `tmux pipe-pane` log stream to every agent at spawn; manage log lifecycle |
| **CLI Provider Layer** | Abstract all supported CLIs; implement failover cascade logic |
| **Telegram Bridge** | Receive commands from authorized users; publish event notifications |
| **Storage** | Manage all `.aegis/` directory I/O, log rotation, and handoff receipts |

---

## 3. Agent Model

### 3.1 Bastion Agents (Long-Lived)

Bastions are the persistent intelligence layer. They hold the project plan, coordinate Splinters, and maintain session history across days or weeks.

**Lifecycle:**
```
  Configure → Spawn → Active ←→ Paused → Terminated
                          ↕
                       Cooling  (rate-limited, awaiting failover)
```

**Properties:**
- One tmux session per Bastion (named `aegis:<role>`)
- Assigned the project root as their worktree (or a dedicated long-lived worktree)
- CLI provider + fallback cascade configured per Bastion role
- Session resume flags (`--resume`, `--session`, etc.) passed at restart
- Context exported periodically to `.aegis/handoff/<role>/context.md` as a recovery checkpoint

**Roles (user-defined, examples):** `architect`, `reviewer`, `pm`

### 3.2 Splinter Agents (Ephemeral)

Splinters are short-lived specialists created by the Dispatcher (on instruction from a Bastion or the user) to complete a bounded task.

**Lifecycle:**
```
  Queued → Spawning → Active → Reporting → Terminated → [Cleanup]
                         ↕
                      Cooling  (failover mid-task)
```

**Properties:**
- Each Splinter gets a unique `agent_id` (UUID)
- Dedicated Git worktree created at spawn: `git worktree add .aegis/worktrees/<agent_id>`
- Terminated after writing a Receipt file to `.aegis/handoff/<task_id>/receipt.json`
- Worktree pruned on clean termination; preserved on crash for inspection
- Parent Bastion ID recorded in registry for lineage

**Concurrency:** Controlled by the Scheduler's configurable `max_splinters` value. Tasks beyond the limit are queued.

### 3.3 Agent Status States

| Status | Meaning |
|---|---|
| `queued` | Awaiting Scheduler slot |
| `starting` | tmux window created, CLI initializing |
| `active` | Operational, processing |
| `paused` | Manually suspended (Telegram `/pause` command) |
| `cooling` | Rate-limited; Watchdog managing failover |
| `reporting` | Writing receipt, pending cleanup |
| `terminated` | Cleaned up; registry entry archived |
| `failed` | Crashed without receipt; worktree preserved |

---

## 4. Channels

Channels are the inter-agent and agent-to-controller communication layer. AegisCore defines four channel types.

### 4.1 Injection Channel

**Mechanism:** `tmux send-keys -t <pane> "<message>" ENTER`  
**Direction:** Controller → Agent (one-way)  
**Use cases:** Deliver task instructions; inject recovery prompts; send handoff context  
**Guarantees:** Fire-and-forget; no acknowledgment. Used when the agent is known-active.

### 4.2 Mailbox Channel

**Mechanism:** Filesystem drop-box at `.aegis/channels/<agent_id>/inbox/`  
**Direction:** Any → Agent (polled by agent or injected as a file-read instruction)  
**Use cases:** Asynchronous task delivery; inter-Splinter coordination; Bastion-to-Splinter handoff  
**Message schema:**
```json
{
  "message_id": "uuid",
  "from_agent_id": "uuid | system",
  "to_agent_id": "uuid",
  "type": "task | handoff | notification | command",
  "priority": 0,
  "payload": { },
  "created_at": "iso8601"
}
```
The Controller delivers mailbox messages by either polling (agent reads its own inbox) or via an Injection Channel command instructing the agent to process its inbox.

### 4.3 Observation Channel

**Mechanism:** `tmux capture-pane -t <pane> -p -S -<N>`  
**Direction:** Agent → Controller (passive read)  
**Use cases:** Watchdog error detection; progress monitoring; context scraping for failover  
**Notes:** No agent participation required. The Controller reads any pane at any time. Configurable scan depth (default: last 50 lines).

### 4.4 Broadcast Channel

**Mechanism:** Dispatcher writes to all active agent mailboxes simultaneously  
**Direction:** Controller → All agents  
**Use cases:** Swarm-wide pause; emergency stop; global context update  
**Delivery:** Serialized fan-out via Mailbox Channel to each active agent

### 4.5 Telegram Channel

**Mechanism:** Telegram Bot API (outbound: `sendMessage`; inbound: webhook or long-poll)  
**Direction:** Bidirectional (user ↔ Controller)  
**Use cases:** Push notifications (task complete, rate limit, failover, sandbox violation); pull commands (`/status`, `/pause`, `/retry`, `/kill`, `/spawn`)  
**Security:** Allowlist of authorized Telegram Chat IDs in config; all other senders ignored

### 4.6 Channel Lifecycle & Management

Channels are divided into two classes:

**Implicit channels** — always active, no setup required, fundamental to the tmux execution model:
- Injection (send-keys)
- Observation (capture-pane)

**Explicit channels** — opt-in, runtime-managed via the `aegis channel` command:
- Mailbox (named instances)
- Broadcast
- Telegram

Explicit channels are added, configured, and removed independently of project init. This keeps `aegis init` minimal and makes channel configuration auditable as discrete operations.

```
aegis channel add telegram              # configure and start the Telegram bridge
aegis channel add mailbox <name>        # create a named mailbox channel
aegis channel list                      # show all active channels for this project
aegis channel status <name>             # health and message stats for a channel
aegis channel remove <name>             # stop and remove a channel
```

Channel state is persisted in `.aegis/state/channels.json` and managed by the global daemon. Adding a channel updates this file and notifies `aegisd` to activate the channel without requiring a project restart.

---

## 5. Security & Isolation

### 5.1 Sandbox Overview

AegisCore uses `sandbox-exec` (Apple's Seatbelt / SBPL) to apply mandatory access control to every agent process. This provides syscall-level filesystem jailing without virtualization overhead.

**Default policy (outbound-allowed):**
```scheme
(version 1)
(deny default)

; Core execution (required for CLI tools and shell)
(allow process-exec
  (subpath "/usr/bin")
  (subpath "/usr/local/bin")
  (subpath "/opt/homebrew/bin")
  (subpath "/bin"))

; System library reads (required or CLI crashes)
(allow file-read*
  (subpath "/usr/lib")
  (subpath "/usr/share")
  (subpath "/System/Library")
  (subpath "/private/var/folders"))

; THE JAIL: agent's assigned worktree only
(allow file-read* file-write*
  (subpath "@@WORKTREE_PATH@@"))

; Temp space
(allow file-read* file-write*
  (subpath "/tmp"))

; Network: outbound only (CLIs must reach their APIs)
(allow network-outbound)
(deny network-inbound)

; Explicit hard denials (belt-and-suspenders)
(deny file-read* (subpath "@@HOME@@/.ssh"))
(deny file-read* (subpath "@@HOME@@/.aws"))
(deny file-read* (subpath "@@HOME@@/.gnupg"))
```

### 5.2 Sandbox Factory

At spawn time, the Sandbox Factory:
1. Reads the agent's config for any `sandbox.extra_reads` overrides
2. Substitutes `@@WORKTREE_PATH@@` and `@@HOME@@` into the template
3. Writes the rendered profile to `.aegis/profiles/<agent_id>.sb`
4. Returns the profile path to the Dispatcher

**Invocation pattern:**
```
sandbox-exec -f .aegis/profiles/<agent_id>.sb <cli_command>
```

### 5.3 Worktree Isolation

Each Splinter receives a dedicated Git worktree. The Bastion uses the project root or its own named worktree.

```
git worktree add .aegis/worktrees/<agent_id> <branch>
```

The sandbox profile's `WORKTREE_PATH` points to this directory. The agent has full YOLO permissions within it and zero access outside it (beyond system paths).

### 5.4 Per-Agent Policy Overrides

Users can extend the default profile per agent role in `aegis.toml`:
```toml
[agent.architect.sandbox]
network = "outbound"          # outbound | none | any
extra_reads = ["/usr/local/share/zsh"]
extra_writes = []
```

---

## 6. CLI Provider Abstraction

### 6.1 Provider Trait (Rust)

All CLI providers implement a common interface:

```
Provider trait:
  - spawn(worktree: &Path, config: &ProviderConfig) -> AgentHandle
  - inject_task(handle: &AgentHandle, prompt: &str)
  - resume_session(handle: &AgentHandle, session_ref: &SessionRef)
  - export_context(handle: &AgentHandle) -> Option<String>
  - detect_cap_error(line: &str) -> bool
  - failover_handoff_prompt(context: &str, task: &str) -> String
```

### 6.2 Supported Providers (v1)

| Provider | CLI Binary | Session Resume | Context Export | Notes |
|---|---|---|---|---|
| `claude-code` | `claude` | `--resume` / `--session` | `/export` injected | Primary default |
| `gemini-cli` | `gemini` | `/resume <id>` | `/checkpoint save` | Supports `--compress` flag |
| `codex` | `codex` | Project-indexed skills | stdout capture | OpenAI Codex CLI |
| `ollama` | `ollama run` | Stateless | N/A | Local fallback; unlimited |

### 6.3 Failover Cascade Configuration

Each agent role defines an ordered fallback list. The Watchdog triggers the cascade on detected failure:

```toml
[agent.architect]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "ollama/gemma3"]

[splinter_defaults]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "ollama/gemma3"]
```

### 6.4 Failover Handoff Flow

```
1. Watchdog detects cap/error on agent A (via Observation Channel)
2. Watchdog signals Dispatcher: agent A → status: cooling
3. Dispatcher sends SIGINT to agent A's tmux pane
4. Flight Recorder provides last N lines of agent A's session log
5. CLI Provider Layer generates handoff prompt using provider B's template
6. Dispatcher spawns or activates provider B in same pane (or new pane)
7. Handoff context injected via Injection Channel
8. Agent A's registry entry updated: cli_provider → provider B
9. Telegram notification sent: "Architect: claude-code capped → gemini-cli"
```

---

## 7. Watchdog

The Watchdog is a background async task that continuously monitors all active agent panes.

### 7.1 Monitoring Loop

- Polls every `watchdog.poll_interval_ms` (configurable, default: 2000ms)
- Uses Observation Channel (`capture-pane`) to read the last `watchdog.scan_lines` lines (default: 50)
- Runs pattern matching against the captured text

### 7.2 Detection Patterns

Patterns are user-configurable strings/regexes in `aegis.toml`. Built-in defaults:

| Category | Pattern Examples |
|---|---|
| Rate limit | `"rate limit"`, `"429"`, `"credit balance exhausted"`, `"quota exceeded"` |
| Auth failure | `"401"`, `"authentication failed"`, `"invalid api key"` |
| CLI crash | Pane exits with non-zero code; window closes |
| Sandbox violation | `"Operation not permitted"` (from SBPL denial) |
| Task complete | User-defined: e.g., `"[AEGIS:DONE]"`, `"Receipt written"` |

### 7.3 Watchdog Actions

| Detected Event | Action |
|---|---|
| Rate limit / cap | Initiate Failover Cascade (§6.4) |
| Auth failure | Pause agent; notify Telegram; await manual resolution |
| CLI crash | Capture last 100 log lines; mark `failed`; preserve worktree; notify Telegram |
| Sandbox violation | Log violation; notify Telegram; agent continues unless repeated |
| Task complete | Trigger receipt processing; initiate Splinter cleanup |

---

## 8. Flight Recorder

### 8.1 Architecture

At spawn, the Dispatcher attaches a passive log stream to every agent pane:

```
tmux pipe-pane -t <pane> -o "cat >> .aegis/logs/sessions/<agent_id>.log"
```

This captures every byte of terminal output, including progress bars, tool outputs, and error messages, without any participation from the agent process.

The log path is stored in the agent's registry entry and is **never inside the agent's sandboxed worktree** — it lives in `.aegis/logs/` which the agent cannot write to.

### 8.2 Log Lifecycle

- Log file created at agent spawn
- Appended continuously until agent terminates
- On clean termination: archived to `.aegis/logs/archive/<agent_id>_<timestamp>.log`
- On crash: preserved in-place for inspection
- Log rotation policy: configurable max size and retention period in `aegis.toml`

### 8.3 Context Window for Failover

The Watchdog queries the Flight Recorder for the last `recorder.failover_context_lines` lines (configurable, default: 100) when generating the handoff prompt. This window is passed to the receiving CLI provider's `failover_handoff_prompt()` implementation.

---

## 9. Tracking & State

### 9.1 Registry

The Registry is the single source of truth for all active and historical agents. It is a JSON file at `.aegis/state/registry.json`, managed exclusively by the Controller.

**Agent record schema:**
```json
{
  "agent_id": "uuid",
  "name": "string",
  "type": "bastion | splinter",
  "status": "queued | starting | active | paused | cooling | reporting | terminated | failed",
  "role": "string",
  "parent_id": "uuid | null",
  "task_id": "uuid | null",
  "tmux_session": "string",
  "tmux_window": "integer",
  "tmux_pane": "string",
  "worktree_path": "string",
  "cli_provider": "string",
  "fallback_cascade": ["string"],
  "sandbox_profile": "string",
  "log_path": "string",
  "created_at": "iso8601",
  "updated_at": "iso8601",
  "terminated_at": "iso8601 | null"
}
```

### 9.2 Task Registry

Tasks are tracked separately in `.aegis/state/tasks.json`:
```json
{
  "task_id": "uuid",
  "description": "string",
  "status": "queued | active | complete | failed",
  "assigned_agent_id": "uuid | null",
  "created_by_agent_id": "uuid | system",
  "created_at": "iso8601",
  "completed_at": "iso8601 | null",
  "receipt_path": "string | null"
}
```

### 9.3 State Snapshots

Periodic snapshots of the full registry are written to `.aegis/state/snapshots/` for recovery:
- Frequency: configurable (`state.snapshot_interval_s`, default: 60s)
- Retained: last N snapshots (configurable, default: 10)
- Format: timestamped JSON (`registry_<iso8601>.json`)

---

## 10. Prompts

### 10.1 Prompt Types

| Type | Purpose | Storage |
|---|---|---|
| `system/<role>.md` | Role-level system prompt defining the agent's behavior and constraints | `.aegis/prompts/system/` |
| `task/<task_type>.md` | Task-specific instructions injected at Splinter spawn | `.aegis/prompts/task/` |
| `handoff/recovery.md` | Template for injecting context into a failover agent | `.aegis/prompts/handoff/` |
| `handoff/resume.md` | Template for resuming a Bastion after restart | `.aegis/prompts/handoff/` |

### 10.2 Template Variables

Prompt templates use `{{variable}}` substitution at injection time:

| Variable | Source |
|---|---|
| `{{context}}` | Last N lines from Flight Recorder |
| `{{task}}` | Task description from task registry |
| `{{task_id}}` | UUID of the current task |
| `{{previous_cli}}` | Name of the failed CLI provider |
| `{{worktree_path}}` | Absolute path to agent's worktree |
| `{{agent_id}}` | UUID of the receiving agent |
| `{{role}}` | Agent role string |

### 10.3 Prompt Resolution Order

On spawn, the Controller resolves prompts in this priority order:
1. Agent-specific override in `aegis.toml` (`agent.<name>.system_prompt`)
2. Role-level file in `.aegis/prompts/system/<role>.md`
3. Built-in default prompt for the role type (Bastion / Splinter)

---

## 11. Agent Configuration

### 11.1 Config Layering

AegisCore uses a two-layer configuration model identical in structure to `~/.gitconfig` vs `.git/config`:

| Layer | Location | Purpose |
|---|---|---|
| Global seed | `~/.aegis/config` | Machine-wide defaults; providers, sandbox policy, Telegram credentials |
| Project override | `<project-root>/aegis.toml` | Project-specific values; overrides or extends global |

When `aegis init` runs, it reads `~/.aegis/config` and uses it as the seed for the new `aegis.toml`. Values not present in `aegis.toml` fall back to `~/.aegis/config` at runtime. Both files share the same TOML schema — the merge is a simple key-level overlay (project wins on conflict).

This means provider credentials, Telegram tokens, and default sandbox policies only need to be configured once globally and are inherited by all projects automatically.

### 11.2 Config File: `aegis.toml`

Located at the project root. Values not present here fall back to `~/.aegis/config`.

**Top-level structure:**
```toml
[global]
max_splinters = 5
tmux_session_name = "aegis"
telegram_enabled = false

[watchdog]
poll_interval_ms = 2000
scan_lines = 50
failover_enabled = true

[recorder]
failover_context_lines = 100
log_rotation_max_mb = 50
log_retention_count = 20

[state]
snapshot_interval_s = 60
snapshot_retention_count = 10

[sandbox.defaults]
network = "outbound"
extra_reads = []
extra_writes = []

[providers.claude-code]
binary = "claude"
resume_flag = "--resume"

[providers.gemini-cli]
binary = "gemini"

[providers.codex]
binary = "codex"

[providers.ollama]
binary = "ollama"
model = "gemma3"

[telegram]
token_env = "AEGIS_TELEGRAM_TOKEN"
allowed_chat_ids = []

[agent.architect]
type = "bastion"
role = "architect"
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "ollama"]
system_prompt = ".aegis/prompts/system/architect.md"

[agent.architect.sandbox]
network = "outbound"
extra_reads = []

[splinter_defaults]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "ollama"]
auto_cleanup = true
```

---

## 12. Storage Layout

```
<project_root>/
├── aegis.toml                          # User config
│
├── .aegis/
│   ├── designs/
│   │   └── hld/
│   │       └── aegis.md               # This document
│   │   (lld/ and roadmap/ added per LLD cycle)
│   │
│   ├── logs/
│   │   ├── sessions/
│   │   │   └── <agent_id>.log         # Live Flight Recorder output
│   │   └── archive/
│   │       └── <agent_id>_<ts>.log    # Terminated agent logs
│   │
│   ├── state/
│   │   ├── registry.json              # Live agent registry
│   │   ├── tasks.json                 # Live task registry
│   │   ├── channels.json              # Active explicit channel configurations
│   │   └── snapshots/
│   │       └── registry_<ts>.json
│   │
│   ├── channels/
│   │   └── <agent_id>/
│   │       └── inbox/
│   │           └── <message_id>.json  # Pending mailbox messages
│   │
│   ├── profiles/
│   │   └── <agent_id>.sb              # Generated sandbox profiles
│   │
│   ├── worktrees/
│   │   └── <agent_id>/                # Git worktrees (Splinters)
│   │
│   ├── handoff/
│   │   └── <task_id>/
│   │       ├── receipt.json           # Splinter completion receipt
│   │       └── context.md             # Context checkpoint
│   │
│   └── prompts/
│       ├── system/
│       │   └── <role>.md
│       ├── task/
│       │   └── <task_type>.md
│       └── handoff/
│           ├── recovery.md
│           └── resume.md
```

---

## 13. Telegram Bridge

### 13.1 Architecture

The Telegram Bridge runs as an async task within `aegisd`. It uses the Telegram Bot API via long-polling or webhook (configurable). Only Chat IDs in `telegram.allowed_chat_ids` are processed.

### 13.2 Inbound Commands

| Command | Action |
|---|---|
| `/status` | Return current agent registry summary |
| `/agents` | List all active agents with status |
| `/pause <agent_id>` | Send pause signal to named agent |
| `/resume <agent_id>` | Send resume signal |
| `/kill <agent_id>` | Terminate agent and clean up |
| `/spawn <role> <task>` | Instruct Dispatcher to spawn a Splinter |
| `/logs <agent_id> <n>` | Return last N lines of agent's Flight Recorder |
| `/failover <agent_id>` | Manually trigger failover to next in cascade |

### 13.3 Outbound Events

| Event | Trigger |
|---|---|
| `Splinter spawned` | Dispatcher creates new Splinter |
| `Task complete` | Splinter writes receipt |
| `Rate limit detected` | Watchdog detects cap pattern |
| `Failover initiated` | Watchdog triggers cascade |
| `Sandbox violation` | SBPL denial detected |
| `Agent failed` | Crash without receipt |
| `Bastion cooling` | Bastion hits rate limit |

---

## 14. Workspace & Crate Structure

AegisCore is a Cargo workspace. Each major subsystem is an independent crate. Crates that have no dependency on each other compile fully in parallel — the workspace model does not incur the sequential penalty of a monolith. The core contract lives in `aegis-core`; every optional crate implements those traits.

### 14.1 Repository Layout

```
Cargo.toml                       ← workspace root (members list)
src/
└── main.rs                      ← aegisd binary; thin entry point
crates/
├── aegis-core/                  ← traits, types, registry schema, config schema, storage abstractions
├── aegis-tmux/                  ← tmux abstraction (send-keys, capture-pane, pipe-pane)
├── aegis-sandbox/               ← Sandbox Factory + sandbox-exec wrapper
├── aegis-channels/              ← Injection, Mailbox, Observation, Broadcast implementations
├── aegis-providers/             ← Provider trait impls; per-CLI feature flags
├── aegis-watchdog/              ← Monitor loop + failover cascade
├── aegis-recorder/              ← Flight Recorder
├── aegis-telegram/              ← Telegram bridge
├── aegis-taskflow/              ← HLD → LLD → Roadmap → Task engine
├── aegis-tui/                   ← ratatui-based terminal UI; connects via Unix socket
├── aegis-web/                   ← static web UI assets; embedded into aegisd binary
└── aegis-controller/            ← wires all subsystems together; exposes feature flags to binary
```

### 14.2 Dependency Graph

```
aegis (binary)
└── aegis-controller
      ├── aegis-core            (always)
      ├── aegis-tmux            (always — foundational shared primitive)
      ├── aegis-sandbox         (optional) ─────────── depends on: aegis-core
      ├── aegis-channels        (optional) ─────────── depends on: aegis-core, aegis-tmux
      ├── aegis-providers       (optional) ─────────── depends on: aegis-core
      ├── aegis-watchdog        (optional) ─────────── depends on: aegis-core, aegis-tmux, aegis-providers
      ├── aegis-recorder        (optional) ─────────── depends on: aegis-core, aegis-tmux
      ├── aegis-telegram        (optional) ─────────── depends on: aegis-core
      ├── aegis-taskflow        (optional) ─────────── depends on: aegis-core
      ├── aegis-tui             (optional) ─────────── depends on: aegis-core (socket client only)
      └── aegis-web             (optional) ─────────── depends on: aegis-core (static assets + HTTP handlers)
```

`aegis-sandbox`, `aegis-channels`, `aegis-telegram`, `aegis-taskflow`, and `aegis-recorder` share only `aegis-core` (and `aegis-tmux` where needed) and compile in parallel with each other.

### 14.3 Feature Flags on `aegis-controller`

```toml
[features]
default = ["channels", "sandbox", "watchdog", "recorder", "taskflow"]
channels  = ["dep:aegis-channels"]
sandbox   = ["dep:aegis-sandbox"]
watchdog  = ["dep:aegis-watchdog"]
recorder  = ["dep:aegis-recorder"]
telegram  = ["dep:aegis-telegram"]
taskflow  = ["dep:aegis-taskflow"]
```

Example configurations:

| Use Case | Flags |
|---|---|
| Core only (no channels, no Taskflow) | `--no-default-features --features sandbox,watchdog,recorder` |
| Full stack without Taskflow | `--no-default-features --features channels,sandbox,watchdog,recorder,telegram` |
| Minimal local (ollama only, no Telegram) | `--no-default-features --features channels,sandbox,watchdog,recorder` + ollama provider feature |

### 14.4 Provider Feature Flags (within `aegis-providers`)

Providers share the `Provider` trait and the failover cascade needs visibility across all of them, so they live in one crate with internal feature flags:

```toml
# crates/aegis-providers/Cargo.toml
[features]
default = ["claude", "gemini"]
claude  = []
gemini  = []
codex   = []
ollama  = []
```

### 14.5 `aegis-core` Trait Surface

`aegis-core` defines the contracts every subsystem implements. No subsystem-specific logic lives here — only types, traits, and error definitions.

```
crates/aegis-core/src/
├── agent.rs          ← Agent, BastionAgent, SplinterAgent types; AgentStatus; AgentHandle trait
├── task.rs           ← Task, TaskStatus types; TaskQueue trait
├── registry.rs       ← AgentRegistry trait; TaskRegistry trait
├── config.rs         ← GlobalConfig, AgentConfig, SandboxConfig schema (serde + TOML)
├── channel.rs        ← Channel trait; Message, MessageType, ChannelKind types
├── provider.rs       ← Provider trait; ProviderConfig; SessionRef; FailoverContext
├── sandbox.rs        ← SandboxProfile trait; SandboxPolicy enum
├── recorder.rs       ← Recorder trait; LogQuery type
├── watchdog.rs       ← WatchdogSink trait; DetectedEvent enum; WatchdogAction enum
├── storage.rs        ← StorageBackend trait; canonical path conventions
└── error.rs          ← AegisError; Result alias
```

### 14.6 Controller Builder Pattern

`aegis-controller` accepts whichever subsystem implementations are compiled in. Absent features produce no dead code — the builder simply lacks those methods:

```rust
// Illustrative — exact API defined in lld/controller.md
let runtime = AegisRuntime::builder()
    .registry(FileRegistry::new(&cfg))
    .tmux(TmuxClient::new(&cfg))
    .sandbox(SeatbeltSandbox::new(&cfg))            // feature = "sandbox"
    .channels(TmuxChannelLayer::new(&cfg))          // feature = "channels"
    .providers(ProviderRegistry::from_config(&cfg)) // feature = "watchdog"
    .watchdog(WatchdogMonitor::new(&cfg))           // feature = "watchdog"
    .recorder(FlightRecorder::new(&cfg))            // feature = "recorder"
    .telegram(TelegramBridge::new(&cfg))            // feature = "telegram"
    .taskflow(TaskflowEngine::new(&cfg))            // feature = "taskflow"
    .build()?;
```

---

## 15. LLD Candidates

Each item below warrants a dedicated Low-Level Design document before implementation. LLD maps 1:1 to workspace crates where possible.

| LLD | Crate | Scope |
|---|---|---|
| `lld/core.md` | `aegis-core` | Full trait surface; type definitions; error taxonomy; AegisError variants |
| `lld/controller.md` | `aegis-controller` | Builder pattern; Dispatcher spawn sequence; registry locking; tokio runtime design |
| `lld/sandbox.md` | `aegis-sandbox` | Full `.sb` template grammar; per-provider system path requirements; violation reporting |
| `lld/channels.md` | `aegis-channels` | Mailbox message schema; delivery ordering; Injection Channel escaping rules; broadcast fan-out; channel lifecycle commands |
| `lld/providers.md` | `aegis-providers` | Provider trait full interface; per-CLI session resume mechanics; error pattern catalogue |
| `lld/watchdog.md` | `aegis-watchdog` | Poll loop design; pattern matching engine; failover state machine; backoff strategy |
| `lld/recorder.md` | `aegis-recorder` | pipe-pane lifecycle; log file locking; context window query API; rotation policy |
| `lld/state.md` | `aegis-core` / `aegis-controller` | Registry file locking; snapshot format; crash recovery boot sequence |
| `lld/prompts.md` | `aegis-controller` | Template engine; variable resolution; prompt size limits per provider |
| `lld/telegram.md` | `aegis-telegram` | Bot auth; command parser; event queue design; outbound rate limiting |
| `lld/taskflow.md` | `aegis-taskflow` | HLD→LLD→Roadmap→Task pipeline; document schema; task state machine; integration with agent registry |
| `lld/config.md` | `aegis-core` | Full `aegis.toml` + `~/.aegis/config` schema; merge semantics; validation rules; default value catalogue |
| `lld/cli.md` | `src/` (binary) | Full command surface; `aegis init` scaffold sequence; session anchoring (walk-up); subcommand routing |
| `lld/daemon.md` | `aegis-controller` | Global daemon design; Unix socket protocol; HTTP + WebSocket server; project registry; startup/shutdown lifecycle |
| `lld/ui.md` | `aegis-tui` / `aegis-web` | TUI layout and component model; web API surface; WebSocket event schema; shared client protocol |

---

## 16. CLI Experience

### 16.1 Install

AegisCore is distributed as a single binary (`aegis`) plus a background daemon (`aegisd`). Both are installed together.

```sh
# Option A: install script (curl | sh)
curl -fsSL https://install.aegiscore.dev | sh

# Option B: Homebrew tap
brew tap aegiscore/tap
brew install aegis
```

The install script:
1. Downloads the appropriate pre-built binary for the host architecture (arm64 / x86_64)
2. Places `aegis` and `aegisd` in `/usr/local/bin` (or `~/.local/bin` if no root)
3. Creates `~/.aegis/` with a default global config at `~/.aegis/config`
4. Registers a launchd plist at `~/Library/LaunchAgents/dev.aegiscore.aegisd.plist` to auto-start the global daemon on login

```sh
aegis doctor    # verify: tmux, git, sandbox-exec, configured CLI tools
```

### 16.2 `aegis init` — Git-like Semantics

`aegis init` is non-interactive and silent, mirroring `git init`. It scaffolds the project and seeds config from `~/.aegis/config`.

```sh
cd my-project
aegis init
```

What it does:
1. Creates `.aegis/` in the current directory (errors if already initialized)
2. Reads `~/.aegis/config` and writes `aegis.toml` at the project root with those values as defaults
3. Creates `.aegis/designs/hld/` and `.aegis/designs/lld/` stubs
4. Creates `.aegis/prompts/` with built-in default prompt templates
5. Appends `.aegis/logs/`, `.aegis/state/`, `.aegis/channels/`, `.aegis/profiles/`, `.aegis/worktrees/`, `.aegis/handoff/` to `.gitignore`
6. Registers the project with the global `aegisd` (adds entry to `~/.aegis/state/projects.json`)

Output:
```
Initialized AegisCore project in /path/to/my-project/.aegis/
Edit aegis.toml to configure agents and providers.
Run 'aegis start' to launch Bastion agents.
```

### 16.3 Full Command Surface

```
# ── Machine-level ────────────────────────────────────────────────────
aegis daemon start          # start global aegisd (also done by launchd)
aegis daemon stop           # graceful shutdown of global daemon
aegis daemon status         # health, active projects, version
aegis doctor                # check all system dependencies
aegis projects              # list all registered projects and their status

# ── Project setup ────────────────────────────────────────────────────
aegis init                  # scaffold .aegis/, seed aegis.toml from ~/.aegis/config
aegis config validate       # lint aegis.toml against schema
aegis config show           # show effective merged config (global + project)

# ── Session lifecycle ─────────────────────────────────────────────────
aegis start                 # start Bastion agents for this project
aegis start --bastion <role>  # start a specific Bastion role
aegis stop                  # graceful shutdown of all project agents
aegis attach                # open the project's tmux session (tmux passthrough)
aegis attach <agent_id>     # attach to a specific agent's pane

# ── Channels ─────────────────────────────────────────────────────────
aegis channel add telegram              # configure and activate Telegram bridge
aegis channel add mailbox <name>        # create a named mailbox channel
aegis channel list                      # active channels for this project
aegis channel status <name>             # health and message stats
aegis channel remove <name>             # stop and remove a channel

# ── Agents ───────────────────────────────────────────────────────────
aegis agents                # list all agents with status and current provider
aegis spawn "<task>"        # manually spawn a Splinter with a task
aegis pause <agent_id>
aegis resume <agent_id>
aegis kill <agent_id>
aegis failover <agent_id>   # manually trigger cascade to next provider

# ── Observation ──────────────────────────────────────────────────────
aegis status                # overview: agents, tasks, channels, session health
aegis logs <agent_id>       # tail Flight Recorder (live)
aegis logs <agent_id> -n 50 # last N lines (non-live)

# ── Taskflow ─────────────────────────────────────────────────────────
aegis taskflow status                   # HLD→LLD→Roadmap→Task pipeline state
aegis taskflow assign <task_id> <agent_id>
```

### 16.4 Session Anchoring

Like `git`, the `aegis` CLI finds its project by walking up the directory tree looking for `.aegis/`. Commands can be run from any subdirectory of a project. If no `.aegis/` is found, the command exits with a clear error:

```
Not an AegisCore project (or any parent directory).
Run 'aegis init' to initialize.
```

Project identity is the absolute path of the directory containing `.aegis/`. This path is the key used in the global daemon's project registry.

---

## 17. Global Daemon & IPC

### 17.1 Daemon Model

A single `aegisd` process runs per machine and manages all registered projects. It is started automatically via launchd on login and communicates with the `aegis` CLI and UIs over two interfaces.

```
~/.aegis/
├── config                       ← global seed config
├── run/
│   └── aegisd.sock              ← Unix domain socket (CLI + TUI)
└── state/
    └── projects.json            ← machine-level project registry
```

### 17.2 IPC Interfaces

`aegisd` exposes two interfaces simultaneously:

**Unix Domain Socket** (`~/.aegis/run/aegisd.sock`)
- Used by the `aegis` CLI and the TUI
- Line-delimited JSON messages (request/response + event streams)
- Low-latency; no HTTP overhead; same-machine only
- All CLI subcommands serialize to a JSON request sent over this socket

**Local HTTP + WebSocket Server** (default port: `7437`, configurable)
- Used by the web UI
- REST endpoints for querying state (agents, tasks, channels, logs)
- WebSocket endpoint (`/ws/events`) for real-time event streaming
- Same underlying event model as the socket interface; HTTP is a transport wrapper

### 17.3 IPC Message Protocol

Both interfaces share the same message schema:

```json
// Request
{
  "request_id": "uuid",
  "project_id": "absolute-path | null",
  "command": "agents.list | agents.spawn | logs.tail | ...",
  "params": { }
}

// Response
{
  "request_id": "uuid",
  "ok": true,
  "data": { }
}

// Event (pushed without a request — socket stream or WebSocket)
{
  "event_id": "uuid",
  "project_id": "absolute-path",
  "type": "agent.status_changed | task.complete | failover.initiated | ...",
  "payload": { },
  "timestamp": "iso8601"
}
```

### 17.4 Project Registry (Machine-Level)

`~/.aegis/state/projects.json` is the daemon's index of all registered projects:

```json
{
  "projects": [
    {
      "project_id": "/Users/name/ws/my-project",
      "name": "my-project",
      "status": "active | stopped | error",
      "registered_at": "iso8601",
      "last_active": "iso8601"
    }
  ]
}
```

Registration happens automatically on `aegis init` and `aegis start`. Deregistration is explicit via `aegis daemon remove` or by deleting `.aegis/`.

---

## 18. UI Layer

### 18.1 TUI (`aegis-tui`)

A terminal UI built with `ratatui`. Launched via `aegis ui` (or `aegis tui`). Connects to `aegisd` over the Unix socket.

**Layout concept:**

```
┌─ AegisCore ──────────────────────────────── my-project ─┐
│ AGENTS              │ LOGS — architect                   │
│ ● architect  active │ [12:04:01] Reviewing auth.rs...    │
│ ● splinter-1 active │ [12:04:03] Suggestion: extract ... │
│ ○ splinter-2 queued │ [12:04:05] Writing diff...         │
│                     │                                    │
│ TASKS               │                                    │
│ [✓] auth-refactor   │                                    │
│ [ ] write-tests     │                                    │
│ [ ] review-pr       │                                    │
│─────────────────────│────────────────────────────────────│
│ CHANNELS            │ STATUS                             │
│ ● telegram   active │ Providers: claude-code (primary)   │
│ ● mailbox-1  active │ Splinters: 1/5 active              │
│                     │ Watchdog: polling (2s)             │
└─────────────────────┴────────────────────────────────────┘
[q]uit  [s]pawn  [p]ause  [f]ailover  [l]ogs  [a]ttach
```

### 18.2 Web UI (`aegis-web`)

A locally-served web interface, accessed at `http://localhost:7437`. Served by `aegisd`'s built-in HTTP server — no separate server process required.

Provides the same information as the TUI with richer interactions:
- Real-time log streaming via WebSocket
- Agent timeline / history view
- Taskflow pipeline visualization (HLD → LLD → Roadmap → Tasks)
- Per-project switching via sidebar

The web UI is a single-page app (static assets embedded in the `aegisd` binary via `include_dir!` or similar). It communicates exclusively with `http://localhost:7437` — no external network calls from the UI.

### 18.3 Shared Event Model

Both UIs subscribe to the same WebSocket/socket event stream from `aegisd`. Event types (defined in `aegis-core`):

| Event | Payload |
|---|---|
| `agent.status_changed` | `agent_id`, `old_status`, `new_status` |
| `agent.spawned` | full agent record |
| `agent.terminated` | `agent_id`, `reason` |
| `task.assigned` | `task_id`, `agent_id` |
| `task.complete` | `task_id`, `receipt_path` |
| `failover.initiated` | `agent_id`, `from_provider`, `to_provider` |
| `channel.added` | `channel_name`, `type` |
| `channel.removed` | `channel_name` |
| `watchdog.alert` | `agent_id`, `pattern_matched`, `action_taken` |
