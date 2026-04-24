# LLD: CLI Binary (`aegis`)

**Milestone:** M12  
**Status:** draft  
**HLD ref:** §16.2, §16.3, §16.4, §17.1, §17.2, §17.3  
**Implements:** `src/main.rs` + `src/` submodules  
**Depends on:** M11 (daemon + IPC)

---

## 1. Purpose

The `aegis` CLI is the user-facing entry point to AegisCore. It:

- Discovers the current project via session anchoring (`.aegis/` walk-up).
- Performs local-only operations (`aegis init`, `aegis doctor`) without daemon involvement.
- Forwards all project-scoped and machine-level commands to `aegisd` over the Unix Domain Socket.
- Renders daemon responses as human-readable text or machine-readable JSON.

The CLI never manipulates registry files directly. All authoritative state mutations go through `aegisd`. The only exceptions are `aegis init` (scaffold before a project exists) and `aegis doctor` (local system checks).

---

## 2. Source Layout

```
src/
├── main.rs              ← clap root; construct CliContext; dispatch to command handlers
├── anchoring.rs         ← walk-up .aegis/ discovery; ProjectAnchor type
├── client.rs            ← UDS client: send request, recv response, subscribe event stream
├── output.rs            ← table/list/JSON rendering; Printer
├── error.rs             ← AegisCliError; exit-code mapping
└── commands/
    ├── mod.rs
    ├── init.rs          ← aegis init
    ├── doctor.rs        ← aegis doctor
    ├── daemon.rs        ← aegis daemon start/stop/status/install/uninstall + aegis projects
    ├── session.rs       ← aegis start / stop / attach
    ├── agents.rs        ← aegis agents / spawn / pause / resume / kill / failover
    ├── channels.rs      ← aegis channel add / list / status / remove
    ├── observe.rs       ← aegis status / logs
    ├── config.rs        ← aegis config validate / show
    ├── taskflow.rs      ← aegis taskflow status / assign
    └── completions.rs   ← aegis completions <shell>
```

All command handlers are `async fn` and receive a shared `CliContext`. Local-only commands (`init`, `doctor`, `attach`) do not touch the UDS client.

---

## 3. Session Anchoring

**File:** `src/anchoring.rs`

```rust
pub struct ProjectAnchor {
    pub project_root: PathBuf,  // directory containing .aegis/
    pub aegis_dir: PathBuf,     // project_root/.aegis/
}

impl ProjectAnchor {
    /// Walk up from `cwd` until a `.aegis/` directory is found.
    /// Returns `AegisCliError::NotAnAegisProject` if the filesystem root is reached.
    pub fn discover(cwd: &Path) -> Result<Self, AegisCliError>;

    /// Used by `aegis init` — returns `cwd` without requiring .aegis/ to exist.
    pub fn use_cwd(cwd: &Path) -> Self;
}
```

Walk-up algorithm:
1. Start at `std::env::current_dir()`.
2. At each directory, check for `.aegis/` as a child directory with `is_dir()`.
3. If found, return `ProjectAnchor { project_root: dir, aegis_dir: dir/.aegis/ }`.
4. Advance via `path.parent()`.
5. Stop when `parent()` returns `None` (filesystem root reached).
6. Return `AegisCliError::NotAnAegisProject`.

On failure, the error message is:
```
Not an AegisCore project (or any parent directory up to /).
Run 'aegis init' to initialize.
```

---

## 4. CLI Context

```rust
pub struct CliContext {
    pub anchor: Option<ProjectAnchor>,  // None for machine-level commands
    pub client: DaemonClient,
    pub printer: Printer,
}
```

Constructed in `main.rs` before dispatch:
- `anchor` is populated via `ProjectAnchor::discover()` for all project-scoped commands. Commands that require it call `ctx.anchor.as_ref().ok_or(AegisCliError::NotAnAegisProject)?`.
- Machine-level commands (`daemon *`, `projects`, `doctor`) set `anchor = None`.
- `client` and `printer` are always initialized from global flags.

---

## 5. UDS Client

**File:** `src/client.rs`

```rust
pub struct DaemonClient {
    uds_path: PathBuf,
}

impl DaemonClient {
    pub fn new(path: PathBuf) -> Self;

    /// Send one request; await one response line.
    pub async fn request(
        &self,
        project_path: Option<&Path>,
        command: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, AegisCliError>;

    /// Send subscribe; return a streaming line receiver.
    pub async fn subscribe(
        &self,
    ) -> Result<impl Stream<Item = Result<AegisEvent, AegisCliError>>, AegisCliError>;
}
```

Implementation:
1. `tokio::net::UnixStream::connect(&uds_path)`. On `ConnectionRefused` or `NotFound`, return `AegisCliError::DaemonNotRunning`.
2. Wrap with `Framed<UnixStream, LinesCodec>`.
3. Serialize `UdsRequest { id: Uuid::new_v4(), project_path, command, params }` and send.
4. Read one response line; deserialize `UdsResponse`.
5. If `status != "success"`, return `AegisCliError::DaemonError(error.message)`.
6. Return `payload`.

For `subscribe`: send `{ command: "subscribe", ... }` and return the remaining stream, mapping each line to `AegisEvent`.

---

## 6. Command Surface

### 6.1 UDS Command Mapping

Every CLI command that contacts the daemon sends a `UdsRequest`. The `project_path` field is set to `ProjectAnchor::project_root` for project-scoped commands, or `None` for machine-level commands.

| CLI Subcommand | UDS `command` | `params` keys | Project-scoped |
|---|---|---|---|
| `daemon status` | `daemon.status` | — | No |
| `projects` | `projects.list` | — | No |
| `start [--bastion <role>]` | `session.start` | `role?` | Yes |
| `stop [--force]` | `session.stop` | `force` | Yes |
| `agents` | `agents.list` | — | Yes |
| `spawn "<task>" [--role <role>] [--parent <id>]` | `agents.spawn` | `task, role?, parent_id?` | Yes |
| `pause <id>` | `agents.pause` | `agent_id` | Yes |
| `resume <id>` | `agents.resume` | `agent_id` | Yes |
| `kill <id>` | `agents.kill` | `agent_id` | Yes |
| `failover <id>` | `agents.failover` | `agent_id` | Yes |
| `channel add <type> [name]` | `channel.add` | `kind, name?, config?` | Yes |
| `channel list` | `channel.list` | — | Yes |
| `channel status <name>` | `channel.status` | `name` | Yes |
| `channel remove <name>` | `channel.remove` | `name` | Yes |
| `status` | `project.status` | — | Yes |
| `logs <id> [-n N] [--follow]` | `logs.tail` | `agent_id, lines?, follow` | Yes |
| `config show` | `config.show` | — | Yes |
| `taskflow status` | `taskflow.status` | — | Yes |
| `taskflow assign <task_id> <agent_id>` | `taskflow.assign` | `task_id, agent_id` | Yes |

Commands handled locally without a UDS call: `init`, `doctor`, `daemon start`, `daemon stop`, `daemon install`, `daemon uninstall`, `config validate`, `attach`, `completions`.

### 6.2 `aegis init`

```
aegis init [--force]
```

Scaffold sequence:

1. Check `.aegis/` does not exist in `cwd`. Exit with a clear error unless `--force`.
2. Create the `.aegis/` directory tree:
   ```
   .aegis/
   ├── state/
   ├── logs/sessions/
   ├── logs/archive/
   ├── channels/
   ├── profiles/
   ├── worktrees/
   ├── handoff/
   ├── designs/hld/
   ├── designs/lld/
   └── prompts/system/
       prompts/task/
       prompts/handoff/
   ```
3. Read `~/.aegis/config` (TOML) if it exists; use as seed. Otherwise use built-in defaults.
4. Write `aegis.toml` at `cwd` serialized from the seed config. Fields already set in `~/.aegis/config` are inherited; unset fields get their documented defaults.
5. Call `PromptManager::scaffold_defaults()` to write the three built-in prompt templates into `.aegis/prompts/`.
6. Append to `cwd/.gitignore` (create it if absent):
   ```gitignore
   # AegisCore runtime directories
   .aegis/logs/
   .aegis/state/
   .aegis/channels/
   .aegis/profiles/
   .aegis/worktrees/
   .aegis/handoff/
   ```
7. Attempt UDS `projects.register { root_path }`. If the daemon is not running, print a notice but do not fail:
   ```
   Note: aegisd is not running — project registered locally only.
   Run 'aegis daemon start' to start the daemon.
   ```
8. Print success:
   ```
   Initialized AegisCore project in /path/to/.aegis/
   Edit aegis.toml to configure agents and providers.
   Run 'aegis daemon start' then 'aegis start' to launch Bastion agents.
   ```

### 6.3 `aegis doctor`

```
aegis doctor
```

Local only. Checks (in order):

| # | Check | Pass criterion |
|---|---|---|
| 1 | `tmux` | `tmux -V` succeeds; version ≥ 3.0 |
| 2 | `git` | `git --version` succeeds |
| 3 | `sandbox-exec` | `/usr/bin/sandbox-exec` exists (macOS only; skip on other platforms) |
| 4 | `aegisd` | Connect to configured UDS path; `daemon.status` responds |
| 5 | Configured providers | For each `[providers.*]` in `aegis.toml`, configured binary is on `PATH` |

Output format:
```
AegisCore Doctor
────────────────
[✓] tmux 3.4
[✓] git 2.44.0
[✓] sandbox-exec
[✓] aegisd running (v0.1.0, uptime 4m)
[✓] claude provider: /opt/homebrew/bin/claude
[✗] gemini provider: not found — install gemini-cli
────────────────
1 issue found.
```

`[✗]` lines and the summary go to stderr. Exit 0 if all pass, exit 1 if any fail.

### 6.4 `aegis daemon start/stop/status/install/uninstall`

```
aegis daemon start       # launchctl load <plist>
aegis daemon stop        # launchctl unload <plist>
aegis daemon status      # UDS: daemon.status
aegis daemon install     # invoke 'aegisd install' (generates + writes plist)
aegis daemon uninstall   # invoke 'aegisd uninstall' (unloads + removes plist)
```

`daemon status` UDS response payload:
```json
{
  "version": "0.1.0",
  "uptime_s": 1234,
  "projects": 2,
  "socket_path": "/tmp/aegis.sock"
}
```

Rendered text:
```
aegisd v0.1.0 — running (uptime: 20m 34s)
Projects: 2 registered
Socket:   /tmp/aegis.sock
```

`aegis projects` sends `projects.list` and renders a table:
```
PATH                          STATUS   LAST ACTIVE
/Users/name/ws/my-project     active   2m ago
/Users/name/ws/other          stopped  1d ago
```

### 6.5 `aegis start` / `aegis stop` / `aegis attach`

```
aegis start [--bastion <role>]
aegis stop [--force]
aegis attach [<agent_id>]
```

`start` sends `session.start` and waits for the daemon response. On success, prints the spawned agent ID(s) and their tmux target.

`stop` sends `session.stop { force }`. Without `--force`, agents are paused and worktrees preserved. With `--force`, all agents are killed immediately.

`attach` is **local-only** — no UDS call. Runs:
```sh
tmux attach-session -t <session_name>           # no agent_id
tmux select-window -t <session>:<window>        # with agent_id (resolves from registry file)
```
The session name and pane target are read directly from `aegis.toml` (`global.tmux_session_name`) and the project anchor's `state/registry.json`, since attach needs to work even when the daemon is not responding.

### 6.6 `aegis agents` / `spawn` / `pause` / `resume` / `kill` / `failover`

```
aegis agents
aegis spawn "<task>" [--role <role>] [--parent <agent_id>]
aegis pause <agent_id>
aegis resume <agent_id>
aegis kill <agent_id>
aegis failover <agent_id>
```

`agents` table format:
```
ID (short)  TYPE      ROLE       STATUS   PROVIDER      TASK
a1b2c3d4    bastion   architect  active   claude-code   —
e5f6a7b8    splinter  —          active   claude-code   impl-auth
```

`spawn` prints the assigned `agent_id` on success:
```
Splinter spawned: e5f6a7b8-...
```

All other commands print a one-line confirmation or forward the daemon error.

Agent IDs accept either the full UUID or the first 8 characters (prefix match against the registry). The daemon must resolve the prefix — the CLI sends whatever the user typed as `agent_id`.

### 6.7 `aegis channel add/list/status/remove`

```
aegis channel add telegram [--token <tok>] [--chat-id <id>]
aegis channel add mailbox <name>
aegis channel list
aegis channel status <name>
aegis channel remove <name>
```

`channel add telegram`: if `--token` or `--chat-id` are absent, prompt interactively. After collecting, send `channel.add { kind: "telegram", config: { token, chat_ids } }`. The daemon writes the resolved config into `aegis.toml` and activates the bridge.

`channel list` table:
```
NAME       TYPE      STATUS
telegram   telegram  active
mailbox-1  mailbox   active
```

`channel remove` requires confirmation unless `--yes` is passed:
```
Remove channel 'telegram'? This will deactivate the Telegram bridge. [y/N]
```

### 6.8 `aegis status`

```
aegis status
```

Sends `project.status`. Expected daemon response:
```json
{
  "project_root": "/path/to/project",
  "session_name": "aegis",
  "agents": { "active": 2, "queued": 1, "total": 3 },
  "tasks":  { "active": 1, "complete": 5, "failed": 0 },
  "watchdog": { "polling": true, "interval_ms": 2000 },
  "providers": ["claude-code", "gemini-cli"]
}
```

Rendered text:
```
Project:  /path/to/project    Session: aegis
Agents:   2 active · 1 queued · 3 total
Tasks:    1 active · 5 complete · 0 failed
Watchdog: polling every 2s
Providers: claude-code (primary) → gemini-cli (fallback)
```

### 6.9 `aegis logs <agent_id>`

```
aegis logs <agent_id> [-n <lines>] [--follow]
```

Without `--follow`: send `logs.tail { agent_id, lines, follow: false }`, print response lines to stdout, exit.

With `--follow`: send `logs.tail { agent_id, follow: true }`, enter a streaming receive loop printing lines until SIGINT. The daemon holds the connection open and pushes new lines as the Flight Recorder appends them.

### 6.10 `aegis config validate` / `config show`

```
aegis config validate
aegis config show [--json]
```

`config validate` is **local-only**: reads `~/.aegis/config` and `./aegis.toml`, calls `EffectiveConfig::resolve()`, then `EffectiveConfig::validate()`. Prints each validation error; exits 0 on success, 2 on any error.

`config show` sends `config.show` to the daemon (which returns the merged effective config). Without `--json`, rendered as an annotated TOML-like human-readable dump indicating the source of each value (global vs. project vs. default):
```
[global]
max_splinters = 5        # project
tmux_session_name = "aegis"  # default

[watchdog]
poll_interval_ms = 2000  # default
...
```

### 6.11 `aegis taskflow status` / `taskflow assign`

```
aegis taskflow status
aegis taskflow assign <task_id> <agent_id>
```

`taskflow status` response rendered as a pipeline view:
```
AegisCore Taskflow Pipeline
───────────────────────────
[✓] M0  — Foundation: `aegis-core` + Config       (19/19)
[✓] M1  — tmux Abstraction                        (7/7)
[✓] M2  — Sandbox Factory                         (6/6)
[ ] M10 — Controller & Dispatcher                 (0/12)   lld-done
[ ] M12 — CLI Binary                              (0/12)   pending
```

`taskflow assign` sends `taskflow.assign { task_id, agent_id }` and prints a confirmation.

---

## 7. Output Rendering

**File:** `src/output.rs`

```rust
pub struct Printer {
    format: OutputFormat,
    color: bool,     // auto-detected from stdout isatty; overridden by --no-color
}

pub enum OutputFormat { Text, Json }

impl Printer {
    pub fn table(&self, headers: &[&str], rows: Vec<Vec<String>>);
    pub fn kv(&self, pairs: &[(&str, &str)]);
    pub fn status_line(&self, ok: bool, label: &str, detail: &str);
    pub fn line(&self, msg: &str);
    pub fn json(&self, value: &serde_json::Value);
    pub fn warn(&self, msg: &str);   // → stderr
    pub fn error(&self, msg: &str);  // → stderr
}
```

When `--json` is set: all command output is printed as the raw `payload` from the daemon response, with no additional decoration.

---

## 8. Error Handling

**File:** `src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum AegisCliError {
    #[error("Not an AegisCore project (or any parent directory up to /).")]
    NotAnAegisProject,

    #[error("aegisd is not running. Start it with: aegis daemon start")]
    DaemonNotRunning,

    #[error("Daemon error: {0}")]
    DaemonError(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Core(#[from] aegis_core::AegisError),
}
```

Exit code mapping:

| Code | Condition |
|---|---|
| `0` | Success |
| `1` | User error (`NotAnAegisProject`, `DaemonNotRunning`, `InvalidArg`) |
| `2` | Config error |
| `3` | Daemon returned an error response |
| `1` | Doctor check failure (not a programming error — exits 1 intentionally) |

`main` maps `AegisCliError` to an exit code, prints the error message to stderr with no Rust debug formatting, and calls `process::exit(code)`.

---

## 9. Global Flags

```
aegis [OPTIONS] <COMMAND>

Options:
  --socket <PATH>   Unix socket path [default: /tmp/aegis.sock] [env: AEGIS_SOCKET]
  --json            Emit raw JSON payload
  --no-color        Disable ANSI color output
  --quiet           Suppress informational messages; only data and errors
  -h, --help        Print help
  -V, --version     Print version
```

`--socket` overrides both `~/.aegis/config` `daemon.socket_path` and the compiled-in default. The `AEGIS_SOCKET` env var is equivalent (flag wins over env).

---

## 10. Shell Completions

```
aegis completions <SHELL>   # shell: zsh | bash | fish
```

Uses `clap_complete`. Prints the generated completion script to stdout. Users install it manually:
```sh
aegis completions zsh > ~/.zfunc/_aegis
aegis completions bash >> ~/.bashrc
aegis completions fish > ~/.config/fish/completions/aegis.fish
```

No autoinstall — following the same pattern as `rustup completions`.

---

## 11. Dependencies (`Cargo.toml` additions)

```toml
[dependencies]
aegis-core       = { workspace = true }
aegis-controller = { workspace = true }    # for UdsRequest / UdsResponse types only

clap             = { version = "4", features = ["derive", "env"] }
clap_complete    = "4"
tokio            = { workspace = true }
tokio-util       = { version = "0.7", features = ["codec"] }
futures-util     = "0.3"
serde_json       = { workspace = true }
uuid             = { workspace = true }
thiserror        = { workspace = true }
tracing          = { workspace = true }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
toml             = { workspace = true }
```

The CLI depends on `aegis-controller` only for the shared `UdsRequest`/`UdsResponse` types. No controller logic runs inside the `aegis` binary.

---

## 12. Test Strategy

### 12.1 Unit Tests

| Test | File | Asserts |
|---|---|---|
| `test_anchor_walks_up` | `anchoring.rs` | Discovers `.aegis/` two levels up from cwd |
| `test_anchor_fails_at_root` | `anchoring.rs` | Returns `NotAnAegisProject` when no `.aegis/` found anywhere |
| `test_doctor_all_pass` | `commands/doctor.rs` | All mock checks return ok; exit 0 |
| `test_doctor_missing_binary` | `commands/doctor.rs` | Reports missing provider; exit 1 |
| `test_output_table_text` | `output.rs` | Text table renders headers and rows correctly for 0, 1, N rows |
| `test_output_json_passthrough` | `output.rs` | `--json` emits raw `serde_json::Value` unchanged |
| `test_init_creates_directory_tree` | `commands/init.rs` | All `.aegis/` subdirs created in temp dir |
| `test_init_writes_gitignore_entries` | `commands/init.rs` | Runtime dirs added to `.gitignore` |
| `test_init_fails_if_already_initialized` | `commands/init.rs` | Second init without `--force` returns error |
| `test_init_force_overwrites` | `commands/init.rs` | `--force` succeeds on already-initialized dir |
| `test_config_validate_catches_invalid` | `commands/config.rs` | Bad TOML surfaces as `Config` error; exit 2 |
| `test_error_exit_codes` | `error.rs` | Each `AegisCliError` variant maps to the documented exit code |

### 12.2 Integration Tests

| Test | Asserts |
|---|---|
| `test_uds_roundtrip` | CLI client sends `agents.list`; mock UDS server responds; payload parsed correctly |
| `test_daemon_not_running_error` | Connect to nonexistent socket → `DaemonNotRunning`; correct exit code |
| `test_init_then_register` | `aegis init` in temp dir; verifies scaffold dirs, `aegis.toml`, gitignore, and `projects.register` UDS call |
| `test_subscribe_receives_events` | Client subscribes; mock server pushes `AegisEvent`; event stream decoded correctly |
| `test_init_start_spawn_logs_kill_cycle` | End-to-end over mock daemon socket: init → start → spawn → logs → kill |

Real tmux and sandbox tests are integration tests gated on `#[cfg(target_os = "macos")]` and `tmux` availability, matching the pattern established in `aegis-tmux`.

---

## 13. Implementation Task Map

| Roadmap Task | Source Files |
|---|---|
| 12.2 Session anchoring | `src/anchoring.rs` |
| 12.3 `aegis init` | `src/commands/init.rs` |
| 12.4 `aegis doctor` | `src/commands/doctor.rs` |
| 12.5 Daemon + projects subcommands | `src/commands/daemon.rs` |
| 12.6 Session subcommands | `src/commands/session.rs` |
| 12.7 Agent subcommands | `src/commands/agents.rs` |
| 12.8 Channel subcommands | `src/commands/channels.rs` |
| 12.9 Observation subcommands | `src/commands/observe.rs` |
| 12.10 Config subcommands | `src/commands/config.rs` |
| 12.11 Taskflow subcommands | `src/commands/taskflow.rs` |
| 12.12 Shell completions | `src/commands/completions.rs` |
| 12.13 End-to-end tests | `tests/cli_e2e.rs` |
