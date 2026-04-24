# LLD: UI — TUI + Web (`aegis-tui` / `aegis-web`)

**Milestones:** M14 (TUI), M15 (Web UI)  
**Status:** draft  
**HLD ref:** §18  
**Implements:** `crates/aegis-tui/`, `crates/aegis-web/`  
**Depends on:** M11 (daemon: Unix socket, HTTP, WebSocket)

---

## 1. Purpose

This LLD covers both UI surfaces for AegisCore:

- **`aegis-tui`**: A terminal UI (ratatui) that connects to `aegisd` over the Unix domain socket. Full operational control: observe all agents, send input to any running session, spawn tasks, pause/resume/failover agents, and switch projects.
- **`aegis-web`**: A React SPA served by `aegisd`'s built-in HTTP server at `http://localhost:7437`. All the same control surface plus richer views: embedded live terminal (xterm.js), Taskflow pipeline visualization, per-project sidebar. Assets embedded in the binary — no separate server process.

Both UIs are fully interactive control planes. Users can observe any agent's live session output and communicate with it directly (send keystrokes). All mutations route through `ControllerCommands` via IPC.

---

## 2. Protocol Extensions Required

The daemon IPC surfaces (UDS + HTTP + WebSocket) need several additions before either UI can be implemented. All changes land in `aegis-controller`, not in the UI crates.

### 2.1 Extended `AegisEvent` Variants

Add to `aegis_core::AegisEvent` (in `crates/aegis-core/src/lib.rs`):

```rust
AgentTerminated {
    agent_id: Uuid,
    reason: String,
},
FailoverInitiated {
    agent_id: Uuid,
    from_provider: String,
    to_provider: String,
},
TaskAssigned {
    task_id: Uuid,
    agent_id: Uuid,
},
ChannelAdded {
    channel_name: String,
    channel_type: ChannelKind,
},
ChannelRemoved {
    channel_name: String,
},
```

### 2.2 New UDS Commands

Add to `dispatch_command()` in `crates/aegis-controller/src/daemon/uds.rs`:

| Command | Params | Response |
|---|---|---|
| `agents.list_all` | — | `Vec<Agent>` (active + recently terminated) |
| `tasks.list` | — | `Vec<Task>` |
| `channels.list` | — | `Vec<ChannelRecord>` |
| `logs.tail` | `agent_id`, `last_n: usize` | streaming — see §2.4 |
| `pane.attach` | `agent_id` | bidirectional stream — see §2.5 |

### 2.3 New HTTP Endpoints

Add to `crates/aegis-controller/src/daemon/http.rs`:

| Method | Path | Description |
|---|---|---|
| `GET` | `/projects/:id/tasks` | List all tasks for a project |
| `GET` | `/projects/:id/channels` | List channels for a project |
| `GET` | `/projects/:id/taskflow/status` | `ProjectIndex` JSON |
| `GET` | `/projects/:id/taskflow/show/:milestone_id` | `Milestone` JSON |
| `GET` | `/ws/logs/:agent_id` | WebSocket: live log line stream |
| `GET` | `/ws/pane/:agent_id` | WebSocket: bidirectional terminal relay |

### 2.4 Log Streaming Protocol

Streams cleaned log lines from the flight recorder file. Used by the text-based log view in both UIs. Distinct from the pane relay (§2.5), which streams raw VT bytes.

**UDS:** When `command = "logs.tail"`, the server switches the connection to log-stream mode:

```
// Client sends:
{"id": "uuid", "command": "logs.tail", "params": {"agent_id": "uuid", "last_n": 50}}

// Server emits initial N lines then new lines as they're appended:
{"type": "log_line", "agent_id": "uuid", "line": "...", "seq": 0}
{"type": "log_line", "agent_id": "uuid", "line": "...", "seq": 1}
// streams until client disconnects
```

**WebSocket:** `/ws/logs/:agent_id?last_n=50` uses the same message schema.

**Server-side `LogTailer`** (new `crates/aegis-controller/src/daemon/logs.rs`):

```rust
pub struct LogTailer {
    storage: Arc<ProjectStorage>,
    registry: Arc<FileRegistry>,
}

impl LogTailer {
    // Emits last_n ANSI-stripped lines from the recorder file, then streams
    // new content as it's appended. Polls at 100ms; no notify dependency.
    pub async fn tail(
        &self,
        agent_id: Uuid,
        last_n: usize,
        tx: impl Sink<String, Error = ()> + Unpin,
    ) -> Result<()>;
}
```

### 2.5 Pane Interaction Protocol

The pane relay gives clients a bidirectional channel to a live agent tmux pane: they see raw terminal output (with ANSI escape sequences intact) and can send keystrokes. This is the "attach and communicate" feature.

**Architecture:** The flight recorder is already capturing all pane output via `pipe-pane`. The relay reads from the same log file but sends raw bytes (not ANSI-stripped) to the client. Input from the client is forwarded via `tmux send-keys`.

```
Flight Recorder log file (raw bytes, written by pipe-pane)
        │
        ├── LogTailer: strip ANSI → clean text lines (log view, §2.4)
        └── PaneRelay: raw bytes → xterm.js / TUI terminal panel
                            ↑
                    Client keystrokes → tmux send-keys
```

**WebSocket messages** (`/ws/pane/:agent_id`):

```
// Server → Client (terminal output):
{"type": "output", "data": "<base64-encoded raw terminal bytes>"}

// Server → Client (current terminal dimensions from tmux):
{"type": "resize", "cols": 220, "rows": 50}

// Client → Server (keystroke input from xterm.js):
{"type": "input", "data": "<base64-encoded VT sequence>"}

// Client → Server (resize from browser):
{"type": "resize", "cols": 200, "rows": 40}
```

The server responds to `resize` by calling `tmux resize-pane -t <pane> -x <cols> -y <rows>`.

**UDS pane attach:** When `command = "pane.attach"`, the UDS connection switches to the same bidirectional relay mode. Frames are the same JSON schema but newline-delimited over the socket.

**Server-side `PaneRelay`** (added to `crates/aegis-controller/src/daemon/logs.rs`):

```rust
pub struct PaneRelay {
    storage: Arc<ProjectStorage>,
    registry: Arc<FileRegistry>,
    tmux: Arc<TmuxClient>,
}

impl PaneRelay {
    // Starts bidirectional relay: streams raw log bytes to `out_tx`, reads
    // keystroke messages from `in_rx` and calls tmux send-keys.
    pub async fn relay(
        &self,
        agent_id: Uuid,
        out_tx: impl Sink<Vec<u8>, Error = ()> + Unpin,
        in_rx: impl Stream<Item = Vec<u8>> + Unpin,
    ) -> Result<()>;
}
```

### 2.6 Type Sharing via `ts-rs`

The primary cross-stack consistency risk is type drift: a TypeScript `Agent` interface hand-maintained separately from the Rust `Agent` struct will eventually diverge. `ts-rs` eliminates this by auto-generating TypeScript interfaces directly from `#[derive(TS)]` annotations on `aegis-core` types.

**Annotated types** (add `#[derive(ts_rs::TS)]` + `#[ts(export)]` to each in `aegis-core`):

| Rust type | Generated TypeScript |
|---|---|
| `Agent` | `Agent.ts` |
| `AgentStatus` | `AgentStatus.ts` |
| `AgentKind` | `AgentKind.ts` |
| `Task` | `Task.ts` |
| `TaskStatus` | `TaskStatus.ts` |
| `ChannelRecord` | `ChannelRecord.ts` |
| `ChannelKind` | `ChannelKind.ts` |
| `AegisEvent` | `AegisEvent.ts` |
| `ProjectRecord` | `ProjectRecord.ts` (in `aegis-controller`) |
| `ProjectStatus` | `ProjectStatus.ts` (in `aegis-controller`) |

**Generation:** a `#[test]` function in `aegis-core` (and `aegis-controller`) calls `Type::export_all_to(path)`. The output directory is `crates/aegis-web/frontend/src/types/`. This test is invoked by `aegis-web/build.rs` before the Vite step:

```rust
// aegis-web/build.rs
println!("cargo:rerun-if-changed=../aegis-core/src");
println!("cargo:rerun-if-changed=../aegis-controller/src/commands.rs");

// Step 1: generate TypeScript bindings from Rust types
let gen = std::process::Command::new("cargo")
    .args(["test", "-p", "aegis-core", "--", "export_ts_bindings", "--nocapture"])
    .status()
    .expect("ts-rs export failed");
assert!(gen.success());

// Step 2: build the SPA
let build = std::process::Command::new("npm")
    .args(["run", "build"])
    .current_dir("frontend")
    .status()
    .expect("vite build failed — run `npm install` in crates/aegis-web/frontend/");
assert!(build.success());
```

The test itself lives in `crates/aegis-core/src/lib.rs`:

```rust
#[cfg(test)]
mod ts_export {
    use ts_rs::TS;
    use crate::*;

    #[test]
    fn export_ts_bindings() {
        let out = concat!(env!("CARGO_MANIFEST_DIR"),
                          "/../../crates/aegis-web/frontend/src/types");
        Agent::export_all_to(out).unwrap();
        AegisEvent::export_all_to(out).unwrap();
        Task::export_all_to(out).unwrap();
        ChannelRecord::export_all_to(out).unwrap();
    }
}
```

`ProjectRecord` and `ProjectStatus` export from an equivalent test in `aegis-controller`.

**Frontend consumption:** all Redux slices and API client functions import from `../types/` instead of declaring types inline. This makes type drift a compile-time error on the next `cargo build`.

**`ts-rs` Cargo dependency** (add to `aegis-core` and `aegis-controller`):

```toml
[dependencies]
ts-rs = { version = "10", optional = true }

[features]
ts-export = ["ts-rs"]
```

The feature is enabled only when `aegis-web` triggers the build, not in production builds.

---

## 3. TUI (`aegis-tui`)

### 3.1 Module Structure

```
crates/aegis-tui/src/
├── lib.rs           ← pub entry point; re-exports run()
├── app.rs           ← AppState, main event loop
├── client.rs        ← Unix socket client (request, subscribe, tail, pane relay)
├── pane.rs          ← in-TUI pane mode: capture-pane render + send-keys input
├── ui/
│   ├── mod.rs       ← root layout composition
│   ├── agents.rs    ← agent list panel
│   ├── logs.rs      ← log text panel (ANSI-stripped flight recorder)
│   ├── pane.rs      ← live pane panel (raw capture-pane with VT rendering)
│   ├── tasks.rs     ← tasks panel
│   ├── channels.rs  ← channels panel
│   └── status.rs    ← status bar
└── events.rs        ← InputEvent enum; terminal + server event merging
```

`aegis-tui` is a library crate. The `aegis` binary calls `aegis_tui::run()` for `aegis ui`.

### 3.2 App State

```rust
pub struct AppState {
    // Project context
    pub projects: Vec<ProjectRecord>,
    pub active_project_idx: usize,

    // Live data (updated from server events; full refresh on reconnect/project switch)
    pub agents: Vec<Agent>,
    pub tasks: Vec<Task>,
    pub channels: Vec<ChannelRecord>,
    pub status: ProjectStatus,

    // Log panel (ANSI-stripped flight recorder lines)
    pub log_agent_id: Option<Uuid>,
    pub log_lines: VecDeque<String>,   // ring buffer; LOG_BUFFER_LINES = 2000
    pub log_follow: bool,

    // Pane panel (raw terminal; active when focus = Panel::Pane)
    pub pane_agent_id: Option<Uuid>,
    pub pane_lines: VecDeque<Vec<u8>>, // raw bytes; PANE_BUFFER_LINES = 500
    pub pane_relay_active: bool,

    // Focus and selection
    pub focus: Panel,
    pub selected_agent_idx: usize,
    pub selected_task_idx: usize,
    pub input_mode: InputMode,

    // Overlay
    pub overlay: Option<Overlay>,
}

pub enum Panel { Agents, Logs, Pane, Tasks, Channels }
pub enum InputMode { Normal, Pane, Command }
pub enum Overlay { SpawnPrompt(String), Confirm(ConfirmAction), HelpScreen, Error(String) }
pub enum ConfirmAction { Kill(Uuid) }
```

### 3.3 Layout

Two layout modes depending on whether the pane panel is active:

**Normal mode** (four-quadrant):
```
┌─ AegisCore ────────────────────── my-project [1/3] ───┐
│ AGENTS (33%)        │ LOGS — architect (67%)           │
│ ● architect  active │ [12:04:01] Reviewing auth.rs...  │
│ ● splinter-1 active │ [12:04:03] Suggestion: extract . │
│ ○ splinter-2 queued │                                  │
│                     │                                  │
│ TASKS               │                                  │
│ [✓] auth-refactor   │                                  │
│─────────────────────│──────────────────────────────────│
│ CHANNELS (33%)      │ STATUS (67%)                     │
│ ● telegram   active │ Providers: claude-code            │
│ ● mailbox-1  active │ Splinters: 1/5  Watchdog: 2s     │
└─────────────────────┴──────────────────────────────────┘
 [q]uit [s]pawn [p]ause [r]esume [f]ail [i]nteractive [?]
```

**Pane mode** (full-screen terminal panel):
```
┌─ AegisCore ─────────── PANE: architect ─ [Esc to exit] ┐
│                                                          │
│  (live capture-pane output rendered here)                │
│  > Reviewing auth.rs for security issues...              │
│  > Identified: session token not invalidated on logout   │
│  > Writing fix...                                        │
│  █                                                       │
│                                                          │
└──────────────────────────────────────────────────────────┘
 [Esc] return to dashboard    keystrokes forwarded to agent
```

In pane mode the full terminal area is used. The status bar shows the active agent name and the Esc-to-exit reminder. All non-Esc keypresses are forwarded to the agent via `send-keys`.

### 3.4 Key Bindings

| Key | Action | Mode |
|---|---|---|
| `q` | Quit | Normal |
| `?` | Toggle help overlay | Normal |
| `s` | Open spawn prompt | Normal |
| `p` | Pause selected agent | Normal |
| `r` | Resume selected agent | Normal |
| `f` | Failover selected agent | Normal |
| `K` | Kill selected agent (confirm overlay) | Normal |
| `a` | Native tmux attach — exits TUI, execs `tmux switch-client`, returns on detach | Normal |
| `i` | Enter in-TUI pane mode for selected agent | Normal |
| `l` | Pin log panel to selected agent | Normal |
| `L` | Toggle log follow (autoscroll) | Normal |
| `j` / `↓` | Next item in focused list | Normal |
| `k` / `↑` | Previous item in focused list | Normal |
| `Tab` | Cycle panel focus | Normal |
| `Shift+Tab` | Reverse cycle panel focus | Normal |
| `n` | Next project | Normal |
| `N` | Previous project | Normal |
| `Enter` | Confirm overlay action | Normal |
| `Esc` | Cancel overlay / clear error | Normal |
| `Esc` | Exit pane mode, return to Normal | Pane |
| (any other key) | Forward keystroke to agent via send-keys | Pane |
| (typing) | Append to spawn prompt | Command |
| `Backspace` | Delete char in prompt | Command |
| `Enter` | Submit spawn prompt | Command |
| `Esc` | Cancel prompt | Command |

**`a` vs `i`:** `a` gives full native tmux access (proper scrollback, copy mode, etc.) at the cost of temporarily leaving the TUI. `i` keeps the TUI running and shows a live pane mirror, suitable for light supervision. For serious interaction, `a` is preferred.

**In-TUI pane mode implementation (`pane.rs`):** Polls `capture-pane -p -e` (with ANSI escape codes) via `TmuxClient` every 200ms and stores raw bytes in `pane_lines`. The pane panel renders using `tui-term` (a VT100 state machine for ratatui) so control sequences are interpreted rather than shown as raw bytes. Keystrokes in pane mode are forwarded via `TmuxClient::send_keys()`.

### 3.5 Event Loop

Merges three async sources via `tokio::select!`:

1. **Terminal events**: `crossterm::event::poll()` in a `spawn_blocking` task → `InputEvent::Key(KeyEvent)`.
2. **Server events**: UDS `subscribe` stream → `InputEvent::Server(AegisEvent)`.
3. **Pane refresh**: `tokio::time::interval(200ms)` when `input_mode = Pane` → `InputEvent::PaneRefresh`.

```
loop {
    select! {
        Some(ev) = input_rx.recv()  => handle_input(ev, &mut state, &client).await,
        Some(ev) = server_rx.recv() => handle_server_event(ev, &mut state),
        _ = pane_tick.tick(), if state.input_mode == InputMode::Pane
                                    => refresh_pane(&mut state, &client).await,
    }
    terminal.draw(|f| ui::render(f, &state))?;
}
```

`pane_tick` is a `tokio::time::Interval`; it is only polled when in pane mode.

### 3.6 Unix Socket Client

```rust
pub struct AegisClient {
    socket_path: PathBuf,
}

impl AegisClient {
    pub async fn connect(socket_path: &Path) -> Result<Self>;

    // Opens a new connection per call; suitable for one-shot requests.
    pub async fn send(&self, req: UdsRequest) -> Result<UdsResponse>;

    // Opens a dedicated connection that stays open for the event stream.
    pub async fn subscribe(&self) -> Result<impl Stream<Item = AegisEvent>>;

    // Opens a dedicated connection in log-tail mode.
    pub async fn tail_logs(
        &self,
        agent_id: Uuid,
        last_n: usize,
    ) -> Result<impl Stream<Item = String>>;

    // Opens a dedicated connection in pane-relay mode.
    // Returns (output_stream, input_sink) for the bidirectional channel.
    pub async fn attach_pane(
        &self,
        agent_id: Uuid,
    ) -> Result<(impl Stream<Item = Vec<u8>>, impl Sink<Vec<u8>>)>;
}
```

Each long-running stream uses its own socket connection. Connection errors trigger a 3-second retry with an `Error` overlay displayed. After 3 consecutive failures, the TUI exits with a clear message.

### 3.7 Multi-Project Switching

On `n`/`N`, the TUI:
1. Advances `active_project_idx` cyclically.
2. Exits pane mode if active; closes pane relay connection.
3. Closes any open log tail stream.
4. Sends `agents.list`, `tasks.list`, `channels.list`, `status` for the new project.
5. Updates `AppState`; resets selections and log buffer.

Project list is fetched from `projects.list` on startup and updated on `projects.register` events.

### 3.8 Cargo.toml Additions

```toml
[dependencies]
aegis-core = { workspace = true }
tokio      = { workspace = true, features = ["rt", "rt-multi-thread", "net", "io-util", "sync", "time"] }
ratatui    = "0.27"
crossterm  = "0.27"
tui-term   = "0.2"          # VT100 state machine widget for pane rendering
serde      = { workspace = true }
serde_json = { workspace = true }
uuid       = { workspace = true }
tracing    = { workspace = true }
base64     = "0.22"
```

---

## 4. Web UI (`aegis-web`)

### 4.1 Architecture

```
aegisd (process)
└── HTTP server (axum, :7437)
    ├── /projects, /projects/:id/*     — REST API (daemon LLD §4 + §2.3 additions)
    ├── /ws/events                     — global event stream
    ├── /ws/logs/:agent_id             — log line stream (§2.4)
    ├── /ws/pane/:agent_id             — bidirectional terminal relay (§2.5)
    └── / (fallback)                   — static SPA assets embedded via rust-embed
```

The SPA communicates exclusively with `http://localhost:7437` — no external network calls.

### 4.2 Frontend Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| Language | TypeScript (strict) | Type safety on API payloads; good tooling |
| Types | `ts-rs` (generated) | TypeScript interfaces auto-generated from Rust structs; type drift is impossible (see §2.6) |
| Framework | React 18 | Component model fits the multi-view dashboard well; large ecosystem |
| State | Redux Toolkit | Structured global state; RTK simplifies slice boilerplate; devtools support |
| Terminal | xterm.js + FitAddon | De-facto standard browser terminal; handles ANSI/VT100 correctly |
| Bundler | esbuild (via Vite) | Fast HMR in dev; single-file production bundle; no ejection needed |
| Styling | CSS Modules + CSS variables | Scoped styles; dark theme via variables; no preprocessor |

### 4.3 Frontend Module Structure

```
crates/aegis-web/
├── Cargo.toml
├── build.rs                 ← invokes `vite build` when src changes
├── src/
│   ├── lib.rs               ← rust-embed asset struct + asset_for_path()
│   └── routes.rs            ← Axum static asset handler
└── frontend/
    ├── package.json         ← react, redux-toolkit, xterm, vite, typescript
    ├── tsconfig.json
    ├── vite.config.ts
    ├── index.html           ← shell HTML
    ├── src/
    │   ├── types/           ← AUTO-GENERATED by ts-rs; do not edit manually
    │   │   ├── Agent.ts
    │   │   ├── AgentStatus.ts
    │   │   ├── AgentKind.ts
    │   │   ├── AegisEvent.ts
    │   │   ├── Task.ts
    │   │   ├── TaskStatus.ts
    │   │   ├── ChannelRecord.ts
    │   │   ├── ChannelKind.ts
    │   │   ├── ProjectRecord.ts
    │   │   └── ProjectStatus.ts
    │   ├── main.tsx         ← React root + Redux Provider + WebSocket init
    │   ├── store/
    │   │   ├── index.ts     ← configureStore(); root reducer
    │   │   ├── projectsSlice.ts
    │   │   ├── agentsSlice.ts
    │   │   ├── tasksSlice.ts
    │   │   ├── channelsSlice.ts
    │   │   ├── uiSlice.ts   ← active project, active view, selected agent
    │   │   └── wsMiddleware.ts ← Redux middleware: WS events → dispatch
    │   ├── api/
    │   │   ├── rest.ts      ← typed fetch wrappers for all REST endpoints
    │   │   └── thunks.ts    ← RTK createAsyncThunk actions (initial data load)
    │   ├── components/
    │   │   ├── App.tsx      ← layout shell: sidebar + main content area
    │   │   ├── Sidebar.tsx  ← project switcher + nav links
    │   │   ├── StatusBadge.tsx
    │   │   └── Terminal.tsx ← xterm.js component (pane relay, §4.6)
    │   └── views/
    │       ├── AgentsView.tsx
    │       ├── PaneView.tsx     ← embedded interactive terminal
    │       ├── LogView.tsx      ← ANSI-stripped log text stream
    │       ├── TasksView.tsx
    │       ├── ChannelsView.tsx
    │       └── TaskflowView.tsx
    └── dist/                ← gitignored; Vite output embedded at compile time
```

### 4.4 Redux Store

All domain types (`Agent`, `Task`, `ChannelRecord`, `AegisEvent`, etc.) are imported from `../types/` — the `ts-rs`-generated interfaces. Slices never redeclare these shapes.

**Slices:**

```typescript
// agentsSlice.ts
import type { Agent } from '../types/Agent';

interface AgentsState {
    items: Agent[];
    loading: boolean;
}
// reducers: setAgents, upsertAgent, removeAgent

// tasksSlice.ts
interface TasksState { items: Task[]; loading: boolean; }

// channelsSlice.ts
interface ChannelsState { items: ChannelRecord[]; loading: boolean; }

// projectsSlice.ts
interface ProjectsState { items: ProjectRecord[]; loading: boolean; }

// uiSlice.ts
interface UIState {
    activeProjectId: string | null;
    activeView: 'agents' | 'pane' | 'logs' | 'tasks' | 'channels' | 'taskflow';
    selectedAgentId: string | null;   // for pane/log views
    error: string | null;
}
```

**WebSocket middleware (`wsMiddleware.ts`):**

RTK middleware that opens `/ws/events` on store init. Maps incoming `AegisEvent` types to slice actions:

| Event type | Dispatched action |
|---|---|
| `agent_spawned` | `agentsSlice.upsertAgent` |
| `agent_status_changed` | `agentsSlice.upsertAgent` |
| `agent_terminated` | `agentsSlice.removeAgent` |
| `task_complete` | `tasksSlice.upsertTask` |
| `task_assigned` | `tasksSlice.upsertTask` |
| `failover_initiated` | `agentsSlice.upsertAgent` |
| `channel_added` | `channelsSlice.upsertChannel` |
| `channel_removed` | `channelsSlice.removeChannel` |
| `watchdog_alert` | `uiSlice.setError` (ephemeral banner) |

### 4.5 Views

**`AgentsView.tsx`**
- Table of agents: role, kind, status badge, CLI provider, uptime.
- Status badges: `active=green`, `paused=yellow`, `cooling=orange`, `failed=red`, `queued=grey`.
- Row action buttons: Pause, Resume, Failover, Kill.
- Row click sets `uiSlice.selectedAgentId` and navigates to `PaneView`.

**`PaneView.tsx`** — interactive terminal

The primary interactive surface. Renders `<Terminal>` (xterm.js) and establishes the pane relay WebSocket.

```tsx
function PaneView({ agentId }: { agentId: string }) {
    const termRef = useRef<HTMLDivElement>(null);
    const xtermRef = useRef<XTerm | null>(null);
    const wsRef = useRef<WebSocket | null>(null);

    useEffect(() => {
        const term = new XTerm({ cursorBlink: true, theme: darkTheme });
        const fitAddon = new FitAddon();
        term.loadAddon(fitAddon);
        term.open(termRef.current!);
        fitAddon.fit();
        xtermRef.current = term;

        const ws = new WebSocket(`ws://localhost:7437/ws/pane/${agentId}`);
        wsRef.current = ws;

        ws.onmessage = (ev) => {
            const msg = JSON.parse(ev.data);
            if (msg.type === 'output') {
                term.write(base64ToUint8Array(msg.data));
            }
        };

        term.onData((data) => {
            ws.send(JSON.stringify({
                type: 'input',
                data: uint8ArrayToBase64(new TextEncoder().encode(data)),
            }));
        });

        const onResize = () => {
            fitAddon.fit();
            ws.send(JSON.stringify({
                type: 'resize',
                cols: term.cols,
                rows: term.rows,
            }));
        };
        window.addEventListener('resize', onResize);

        return () => {
            ws.close();
            term.dispose();
            window.removeEventListener('resize', onResize);
        };
    }, [agentId]);

    return <div ref={termRef} style={{ width: '100%', height: '100%' }} />;
}
```

**`LogView.tsx`** — flight recorder log stream

- Opens `/ws/logs/:agent_id?last_n=200` on mount.
- Renders lines in a virtualized list (no full re-render on new lines).
- Follow toggle (autoscroll to bottom).
- Client-side filter input (regex or plain text).
- Closes WebSocket on unmount.

**`TasksView.tsx`**
- Task list: description, status, assigned agent link (navigates to PaneView), times.
- Filter tabs: All / Active / Complete / Failed.

**`ChannelsView.tsx`**
- Channel list with name, type, status dot.

**`TaskflowView.tsx`**

Renders the HLD → LLD → Roadmap → Tasks pipeline as a collapsible tree. Milestones expand to show tasks with status icons.

```
▶ M0: Foundation                    ✓ done
▼ M13: Taskflow Engine              ● in-progress
    ✓ 13.1 Write lld/taskflow.md
    ● 13.2 Implement TOML parser
    ○ 13.3 Link registry
▶ M14: TUI                          ○ pending
```

Data from `GET /projects/:id/taskflow/status` (initial load) + expand-on-demand `GET /projects/:id/taskflow/show/:milestone_id`.

**`Sidebar.tsx`**
- Left sidebar: project list from Redux `projectsSlice`.
- Active project highlighted; click dispatches `uiSlice.setActiveProject` and triggers a full data reload (agents, tasks, channels).
- Nav links: Agents / Tasks / Channels / Taskflow.

### 4.6 Real-Time Updates

The WebSocket middleware keeps `/ws/events` open for the lifetime of the app. On project switch, it sends `subscribe` with the new `project_id` filter. Re-connection uses exponential backoff (1s, 2s, 4s, max 30s); an error banner is shown during disconnection.

### 4.7 Asset Embedding

`crates/aegis-web/src/lib.rs`:

```rust
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "frontend/dist/"]
struct WebAssets;

pub fn asset_for_path(path: &str) -> Option<std::borrow::Cow<'static, [u8]>> {
    let key = path.trim_start_matches('/');
    // Exact match first; fall back to index.html for SPA client-side routing.
    WebAssets::get(key).or_else(|| WebAssets::get("index.html"))
}

pub fn mime_for_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("js")   => "application/javascript",
        Some("css")  => "text/css",
        Some("html") => "text/html",
        Some("ico")  => "image/x-icon",
        Some("woff2")=> "font/woff2",
        _            => "application/octet-stream",
    }
}
```

`crates/aegis-web/src/routes.rs` exports a fallback `axum::Router`:

```rust
pub fn static_routes() -> Router {
    Router::new().fallback(serve_static)
}

async fn serve_static(uri: Uri) -> impl IntoResponse {
    match aegis_web::asset_for_path(uri.path()) {
        Some(data) => (
            [(header::CONTENT_TYPE, aegis_web::mime_for_path(uri.path()))],
            data.into_owned(),
        ).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
```

`aegis-controller`'s `HttpServer::new()` merges these static routes last (after all API routes):

```rust
let router = Router::new()
    .route("/projects", ...)
    // ... all API and WebSocket routes ...
    .merge(aegis_web::routes::static_routes());  // SPA catch-all last
```

### 4.8 Build Pipeline

`crates/aegis-web/build.rs` runs two steps in sequence (full implementation in §2.6):

1. **Type generation** — `cargo test -p aegis-core -- export_ts_bindings` writes generated TypeScript interfaces into `frontend/src/types/`. Triggered whenever `aegis-core/src` or `aegis-controller/src/commands.rs` change.
2. **Vite build** — `npm run build` in `frontend/` bundles React + Redux + xterm.js into `frontend/dist/`. Triggered whenever `frontend/src` or `frontend/index.html` change.

Cargo's `rerun-if-changed` directives ensure each step is skipped when its inputs are unchanged.

First-time build: `cd crates/aegis-web/frontend && npm install`. The `dist/` directory is gitignored; a pre-built `dist/` can be committed for CI environments that lack Node.

### 4.9 Cargo.toml Additions

`crates/aegis-web/Cargo.toml`:

```toml
[dependencies]
aegis-core       = { workspace = true, features = ["ts-export"] }
aegis-controller = { workspace = true, features = ["ts-export"] }
rust-embed       = { version = "8", features = ["debug-embed"] }
axum             = { workspace = true }
tokio            = { workspace = true }
tracing          = { workspace = true }
```

`crates/aegis-core/Cargo.toml` (addition):

```toml
[dependencies]
ts-rs = { version = "10", optional = true }

[features]
ts-export = ["dep:ts-rs"]
```

Same `ts-export` feature addition applies to `aegis-controller`.

---

## 5. Shared: `aegis ui` Entry Point

```
aegis ui        # open TUI (default)
aegis ui --web  # print http://localhost:7437 and exit (user opens browser)
```

`--web` checks that `aegisd` is running (socket ping) before printing the URL, and exits with an error if the daemon is not up.

---

## 6. Test Strategy

### 6.1 TUI Unit Tests

| Test | Asserts |
|---|---|
| `test_handle_agent_status_changed` | `AgentStatusChanged` event updates `state.agents` in place |
| `test_handle_agent_terminated` | removed from agents list; pane mode exits if it was the pane agent |
| `test_project_switch_resets_state` | clears log buffer, clears pane state, resets selection |
| `test_key_i_enters_pane_mode` | `i` sets `input_mode = Pane` and `pane_agent_id` |
| `test_pane_mode_esc_exits` | `Esc` in Pane mode returns `input_mode = Normal` |
| `test_key_spawn_prompt` | `s` opens spawn overlay and enters Command mode |
| `test_key_quit` | `q` returns `Action::Quit` |
| `test_log_buffer_ring_evicts_oldest` | pushing past `LOG_BUFFER_LINES` removes oldest |
| `test_pane_buffer_ring_evicts_oldest` | pushing past `PANE_BUFFER_LINES` removes oldest |

### 6.2 TUI Integration Tests

| Test | Asserts |
|---|---|
| `test_client_send_receive` | `AegisClient::send` round-trips against a mock UDS server |
| `test_client_subscribe_delivers_events` | subscription stream receives events from mock server |
| `test_log_tail_stream` | delivers initial lines then simulated appended content |
| `test_pane_relay_output` | `attach_pane` output stream receives raw bytes from mock server |
| `test_pane_relay_input` | keystrokes written to input sink appear in mock server |

### 6.3 Web Unit Tests (TypeScript, `npm test`)

Uses Vitest (included with Vite dev dependencies).

| Test | Asserts |
|---|---|
| `agentsSlice_upsert` | `upsertAgent` creates or updates by `agent_id` |
| `agentsSlice_remove` | `removeAgent` removes by `agent_id` |
| `wsMiddleware_dispatches_status_changed` | `agent_status_changed` event → `upsertAgent` dispatch |
| `wsMiddleware_dispatches_failover` | `failover_initiated` event → `upsertAgent` dispatch |
| `rest_listAgents_parses` | `api.listAgents()` deserializes mock JSON into `Agent[]` |
| `uiSlice_setActiveProject_resets_selection` | switching project clears `selectedAgentId` |

### 6.4 Web Integration Tests (Rust)

| Test | Asserts |
|---|---|
| `test_asset_embedded_bundle_js` | `asset_for_path("/assets/index.js")` returns non-empty bytes |
| `test_spa_catch_all_returns_html` | `asset_for_path("/agents/some-uuid")` returns `index.html` |
| `test_axum_get_root_200` | HTTP GET `/` returns 200 with `text/html` content-type |
| `test_pane_relay_ws_round_trip` | mock WS client connects to `/ws/pane/:id`, sends input, receives output |

---

## 7. Implementation Tasks

| # | Task | Crate | Notes |
|---|---|---|---|
| 14.1 | Write `lld/ui.md` | — | This document |
| 14.2 | Add `AegisEvent` variants: `AgentTerminated`, `FailoverInitiated`, `TaskAssigned`, `ChannelAdded`, `ChannelRemoved` | `aegis-core` | Prerequisite for both UIs |
| 14.2a | Add `#[derive(TS)]` + `ts-export` feature to `aegis-core` and `aegis-controller`; add `export_ts_bindings` test | `aegis-core`, `aegis-controller` | Generates `frontend/src/types/`; run before any frontend work |
| 14.3 | Implement `LogTailer` + `PaneRelay` in `daemon/logs.rs` | `aegis-controller` | Shared by TUI and web; prerequisite for all live views |
| 14.4 | Add `logs.tail` and `pane.attach` UDS commands | `aegis-controller` | |
| 14.5 | Add `/ws/logs/:agent_id` and `/ws/pane/:agent_id` WebSocket endpoints | `aegis-controller` | |
| 14.6 | Add `channels.list`, `tasks.list` UDS commands; add HTTP task/channel/taskflow endpoints | `aegis-controller` | |
| 14.7 | Implement `AegisClient` (connect, send, subscribe, tail_logs, attach_pane) | `aegis-tui` | |
| 14.8 | Implement `AppState`, event handlers, and pane mode state machine | `aegis-tui` | No rendering yet |
| 14.9 | Implement TUI layout and all panel renderers (agents, logs, pane, tasks, channels, status) | `aegis-tui` | ratatui + tui-term for pane |
| 14.10 | Implement key bindings, overlays (spawn, confirm-kill, help) | `aegis-tui` | |
| 14.11 | Implement multi-project switching | `aegis-tui` | |
| 14.12 | Wire `aegis ui` subcommand in `src/` | `src/` | |
| 14.13 | TUI unit and integration tests | `aegis-tui` | |
| 15.1 | Scaffold `frontend/` with React + Redux Toolkit + xterm.js + Vite + TypeScript | `aegis-web` | `npm create vite` base; task 14.2a must run first to populate `src/types/` |
| 15.2 | Implement Redux store: all slices + WebSocket middleware | `aegis-web` | |
| 15.3 | Implement REST API client (`api/rest.ts`) and RTK async thunks | `aegis-web` | |
| 15.4 | Implement `AgentsView` + `StatusBadge` + Sidebar skeleton | `aegis-web` | |
| 15.5 | Implement `PaneView` with xterm.js + `/ws/pane` WebSocket relay | `aegis-web` | Interactive terminal |
| 15.6 | Implement `LogView` with `/ws/logs` WebSocket stream | `aegis-web` | |
| 15.7 | Implement `TasksView` and `ChannelsView` | `aegis-web` | |
| 15.8 | Implement `TaskflowView` (collapsible tree) | `aegis-web` | |
| 15.9 | Implement `Sidebar` project switcher with Redux dispatch | `aegis-web` | |
| 15.10 | Implement `build.rs` Vite build invocation | `aegis-web` | |
| 15.11 | Implement rust-embed asset embedding + `asset_for_path` + `static_routes()` | `aegis-web` | |
| 15.12 | Merge `static_routes()` into `HttpServer::new()` in `aegis-controller` | `aegis-controller` | |
| 15.13 | TypeScript unit tests (Vitest) | `aegis-web` | |
| 15.14 | Rust asset embedding + pane relay WebSocket integration tests | `aegis-web` | |
