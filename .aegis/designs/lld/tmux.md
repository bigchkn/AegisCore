# LLD: `aegis-tmux`

**Milestone:** M1  
**Status:** done  
**HLD ref:** §2.1, §4.1, §4.3, §8, §7.1  
**Implements:** `crates/aegis-tmux/` — all tmux I/O used by channels, recorder, and watchdog.

---

## 1. Purpose

`aegis-tmux` is the single point of contact with the tmux process. All other crates that need to interact with tmux (`aegis-channels`, `aegis-recorder`, `aegis-watchdog`, `aegis-controller`) depend on this crate and never invoke `tmux` directly.

**Responsibilities:**
- Session, window, and pane lifecycle (create, destroy, query)
- `send-keys` — inject text into a pane's stdin
- `capture-pane` — read terminal output from a pane
- `pipe-pane` — attach/detach a log stream from a pane
- Pane liveness detection (exit code / closed window)

**Non-responsibilities:** Message semantics, failover logic, sandbox policy, log rotation.

---

## 2. Module Structure

```
crates/aegis-tmux/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-exports TmuxClient, TmuxTarget, PaneStatus, TmuxError
    ├── client.rs       ← TmuxClient struct + all command methods
    ├── target.rs       ← TmuxTarget parsing and formatting
    ├── escape.rs       ← send-keys input escaping
    └── error.rs        ← TmuxError (wraps AegisError::TmuxCommand variants)
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core = { path = "../aegis-core" }
tokio = { version = "1", features = ["process", "io-util", "rt"] }
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Uses `tokio::process::Command` for async subprocess execution.

---

## 4. `target.rs` — `TmuxTarget`

A validated tmux target string in the form `session:window.pane`.

```rust
/// A validated tmux target: "session:window.pane"
/// e.g. "aegis:0.%3"
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxTarget(String);

impl TmuxTarget {
    /// Parse a raw target string. Returns Err if empty.
    pub fn parse(s: &str) -> Result<Self, TmuxError>;

    /// Construct from components.
    pub fn new(session: &str, window: u32, pane: &str) -> Self;

    pub fn as_str(&self) -> &str;

    /// Returns the session name component.
    pub fn session(&self) -> &str;
}

impl std::fmt::Display for TmuxTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

---

## 5. `escape.rs` — Input Escaping

`send-keys` interprets certain byte sequences as tmux key bindings (e.g. `Enter`, `Escape`, `C-c`). Arbitrary text must be escaped before injection.

### 5.1 Escaping Strategy

Use `send-keys` with the `-l` (literal) flag for all user content:

```
tmux send-keys -t <target> -l "<escaped_text>"
```

The `-l` flag sends keys literally, bypassing tmux key table lookup. This means no escaping of special tmux sequences is needed for the content itself — only shell quoting of the argument string is required.

### 5.2 `ENTER` Injection

After the content, a separate `send-keys` call sends the Enter key:

```
tmux send-keys -t <target> "" Enter
```

Splitting content from Enter into two calls ensures Enter is always interpreted as a key event, not as literal text.

### 5.3 API

```rust
/// Returns the content argument suitable for: tmux send-keys -t <target> -l <content>
/// Applies shell-level escaping for the single-argument string.
pub fn escape_for_send_keys(input: &str) -> String;
```

Implementation: wrap in single quotes; replace internal single quotes with `'\''`.

---

## 6. `client.rs` — `TmuxClient`

All methods are `async` and return `Result<_, TmuxError>`. Internally they `spawn` a tmux subprocess and await its output.

```rust
pub struct TmuxClient {
    tmux_bin: String, // path to tmux binary, default "tmux"
}

impl TmuxClient {
    pub fn new() -> Self;
    pub fn with_binary(bin: &str) -> Self;
```

### 6.1 Session & Window Management

```rust
    /// Create a new detached tmux session. Returns the session name.
    /// Errors if the session name already exists.
    pub async fn new_session(&self, name: &str) -> Result<String, TmuxError>;

    /// Create a new window in an existing session. Returns the window index.
    pub async fn new_window(
        &self,
        session: &str,
        name: Option<&str>,
    ) -> Result<u32, TmuxError>;

    /// Create a new pane by splitting an existing window. Returns the pane ID ("%N").
    pub async fn split_window(&self, target: &TmuxTarget) -> Result<String, TmuxError>;

    /// Kill a specific pane (and its process).
    pub async fn kill_pane(&self, target: &TmuxTarget) -> Result<(), TmuxError>;

    /// Kill an entire window and all its panes.
    pub async fn kill_window(&self, target: &TmuxTarget) -> Result<(), TmuxError>;

    /// Kill an entire session.
    pub async fn kill_session(&self, session: &str) -> Result<(), TmuxError>;

    /// Returns true if the named session exists.
    pub async fn session_exists(&self, session: &str) -> Result<bool, TmuxError>;

    /// Returns the pane IDs of all panes in a window.
    pub async fn list_panes(&self, target: &TmuxTarget) -> Result<Vec<String>, TmuxError>;
```

### 6.2 `send-keys` — Injection

```rust
    /// Inject text into the pane as if typed. Uses -l (literal) flag.
    /// Sends a trailing Enter after the content.
    pub async fn send_text(&self, target: &TmuxTarget, text: &str) -> Result<(), TmuxError>;

    /// Send a named key event (e.g. "C-c", "Enter", "Escape").
    /// Does NOT use -l flag — intended for control sequences only.
    pub async fn send_key(&self, target: &TmuxTarget, key: &str) -> Result<(), TmuxError>;

    /// Send Ctrl-C to interrupt the running process in the pane.
    pub async fn interrupt(&self, target: &TmuxTarget) -> Result<(), TmuxError>;
```

### 6.3 `capture-pane` — Observation

```rust
    /// Capture the last `lines` lines of terminal output from the pane.
    /// Returns raw text including ANSI sequences (stripped by caller if needed).
    pub async fn capture_pane(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError>;

    /// Capture the last `lines` lines with ANSI escape codes stripped.
    pub async fn capture_pane_plain(
        &self,
        target: &TmuxTarget,
        lines: usize,
    ) -> Result<String, TmuxError>;
```

`capture_pane_plain` appends `-e` to strip escape sequences: `tmux capture-pane -t <target> -p -e -S -<lines>`.

### 6.4 `pipe-pane` — Flight Recorder Attach/Detach

```rust
    /// Attach a log stream: all pane output is appended to `log_path`.
    /// Idempotent — calling again on an already-piped pane is a no-op.
    pub async fn pipe_attach(&self, target: &TmuxTarget, log_path: &Path) -> Result<(), TmuxError>;

    /// Detach the log stream from the pane. Idempotent.
    pub async fn pipe_detach(&self, target: &TmuxTarget) -> Result<(), TmuxError>;
```

`pipe_attach` runs: `tmux pipe-pane -t <target> -o 'cat >> <log_path>'`  
`pipe_detach` runs: `tmux pipe-pane -t <target>` (no shell command = stop piping)

### 6.5 Pane Liveness

```rust
    /// Returns the exit status of the process running in the pane.
    /// None = still running. Some(code) = exited.
    pub async fn pane_exit_status(
        &self,
        target: &TmuxTarget,
    ) -> Result<Option<i32>, TmuxError>;

    /// Returns true if the pane exists and its process is still running.
    pub async fn pane_is_alive(&self, target: &TmuxTarget) -> Result<bool, TmuxError>;
```

Uses `tmux display-message -t <target> -p "#{pane_dead} #{pane_dead_status}"`.  
`pane_dead = 0` means alive; `pane_dead = 1` means exited.

```rust
} // end impl TmuxClient
```

---

## 7. `error.rs`

```rust
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    #[error("tmux binary not found or not executable: {reason}")]
    BinaryNotFound { reason: String },

    #[error("tmux command failed (exit {code}): {stderr}")]
    CommandFailed { code: i32, stderr: String },

    #[error("tmux session not found: {target}")]
    SessionNotFound { target: String },

    #[error("tmux window not found: {target}")]
    WindowNotFound { target: String },

    #[error("tmux pane not found: {target}")]
    PaneNotFound { target: String },

    #[error("invalid tmux target string: `{raw}`")]
    InvalidTarget { raw: String },

    #[error("tmux pipe-pane failed: {source}")]
    PipeFailed { #[source] source: io::Error },

    #[error("I/O error: {source}")]
    Io { #[source] source: io::Error },
}

impl From<TmuxError> for aegis_core::AegisError {
    fn from(e: TmuxError) -> Self {
        match e {
            TmuxError::CommandFailed { code: _, stderr } =>
                AegisError::TmuxCommand { command: String::new(), stderr },
            TmuxError::SessionNotFound { target } =>
                AegisError::TmuxSessionNotFound { target },
            TmuxError::PaneNotFound { target } =>
                AegisError::TmuxPaneNotFound { target },
            other => AegisError::Unexpected(Box::new(other)),
        }
    }
}
```

---

## 8. Subprocess Execution Pattern

All tmux calls follow the same internal pattern:

```rust
async fn run_tmux(&self, args: &[&str]) -> Result<String, TmuxError> {
    let output = tokio::process::Command::new(&self.tmux_bin)
        .args(args)
        .output()
        .await
        .map_err(|e| TmuxError::Io { source: e })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(TmuxError::CommandFailed {
            code: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
```

`tracing::debug!` is emitted before every subprocess call with the full argument list.

---

## 9. Integration Test Strategy

Tests require tmux to be installed (checked via `which tmux` at test start; test is skipped if absent — CI must install tmux).

Each test:
1. Creates a uniquely-named tmux session (prefixed `aegis-test-<uuid>`)
2. Exercises the relevant `TmuxClient` method
3. Asserts the expected output or state
4. Kills the session in a `defer`-style cleanup (using `scopeguard` or explicit `tokio::spawn`)

Key test cases:

| Test | Asserts |
|---|---|
| `test_send_text_and_capture` | Text sent via `send_text` appears in `capture_pane_plain` |
| `test_send_text_special_chars` | Content with single quotes, newlines, backslashes round-trips correctly |
| `test_pipe_attach_writes_log` | After `pipe_attach`, pane output appears in the log file |
| `test_pipe_detach_stops_log` | After `pipe_detach`, new pane output is not appended |
| `test_pane_liveness_running` | `pane_is_alive` returns true for a running shell |
| `test_pane_liveness_exited` | `pane_is_alive` returns false after the pane process exits |
| `test_interrupt_sends_ctrl_c` | `interrupt` causes a running `sleep 999` to terminate |
