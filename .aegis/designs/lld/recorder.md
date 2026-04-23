# LLD: `aegis-recorder` (Flight Recorder)

**Milestone:** M5  
**Status:** done  
**HLD ref:** §8  
**Implements:** `crates/aegis-recorder/`

---

## 1. Purpose

`aegis-recorder` implements the `Recorder` trait from `aegis-core`. It attaches a passive I/O mirror to every agent's tmux pane at spawn time using `tmux pipe-pane`, writes all terminal output to an append-only log file, and provides a query API used by the Watchdog (for failover context) and the CLI (`aegis logs`).

The recorder never modifies or deletes a live log. Archival and rotation are the only write operations on completed logs.

---

## 2. Module Structure

```
crates/aegis-recorder/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-exports FlightRecorder
    ├── recorder.rs     ← FlightRecorder implementing Recorder trait
    ├── query.rs        ← LogQuery execution (file tail, line counting)
    └── rotation.rs     ← log archival and size-based rotation
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core = { path = "../aegis-core" }
aegis-tmux  = { path = "../aegis-tmux" }
tokio = { version = "1", features = ["fs", "io-util", "time"] }
tracing = "0.1"
chrono = { version = "0.4", features = ["serde"] }
```

---

## 4. `FlightRecorder`

```rust
pub struct FlightRecorder {
    tmux: Arc<TmuxClient>,
    storage: Arc<dyn StorageBackend>,
    config: RecorderConfig,
}

impl FlightRecorder {
    pub fn new(
        tmux: Arc<TmuxClient>,
        storage: Arc<dyn StorageBackend>,
        config: RecorderConfig,
    ) -> Self;
}
```

---

## 5. `Recorder` Trait Implementation

### 5.1 `attach()`

Called by the Dispatcher immediately after the agent's tmux pane is created.

```rust
fn attach(&self, agent: &Agent) -> Result<()> {
    let log_path = self.storage.agent_log_path(agent.agent_id);
    // Ensure parent directory exists
    std::fs::create_dir_all(log_path.parent().unwrap())?;
    // Attach pipe-pane: all pane output appended to log file
    let target = TmuxTarget::parse(&agent.tmux_target())?;
    self.tmux.pipe_attach(&target, &log_path).await?;
    tracing::info!(agent_id = %agent.agent_id, log = %log_path.display(), "flight recorder attached");
    Ok(())
}
```

The log file is created by `cat >>` inside the shell spawned by `pipe-pane`. The file is opened in append mode; if the agent restarts (e.g. after a failover), new output continues to append to the same file.

### 5.2 `detach()`

Called by the Dispatcher before killing the pane.

```rust
fn detach(&self, agent_id: Uuid) -> Result<()> {
    // Reconstruct target from registry (caller must pass or this crate holds a map)
    // pipe_detach stops the pipe; the log file is left intact
    self.tmux.pipe_detach(&target).await?;
    Ok(())
}
```

Design note: `detach()` needs the tmux target. `FlightRecorder` maintains an internal `HashMap<Uuid, TmuxTarget>` populated at `attach()` time and cleared at `detach()` time.

```rust
// Internal field added to FlightRecorder:
active_panes: Arc<tokio::sync::RwLock<HashMap<Uuid, TmuxTarget>>>,
```

### 5.3 `archive()`

Moves the live log to the archive directory with a timestamp suffix.

```rust
fn archive(&self, agent_id: Uuid) -> Result<PathBuf> {
    let src = self.storage.agent_log_path(agent_id);
    let ts = Utc::now().format("%Y%m%dT%H%M%SZ");
    let dest = self.storage.archive_dir()
        .join(format!("{}_{}.log", agent_id, ts));
    std::fs::create_dir_all(&self.storage.archive_dir())?;
    std::fs::rename(&src, &dest)?;
    tracing::info!(agent_id = %agent_id, archive = %dest.display(), "log archived");
    self.rotation.prune_archive(&self.storage, &self.config)?;
    Ok(dest)
}
```

### 5.4 `query()`

Returns lines from the agent's log file.

```rust
fn query(&self, query: &LogQuery) -> Result<Vec<String>> {
    let log_path = self.storage.agent_log_path(query.agent_id);
    if !log_path.exists() {
        // Check archive
        return Err(AegisError::LogFileNotFound { agent_id: query.agent_id, path: log_path });
    }
    match query.last_n_lines {
        Some(n) => tail_lines(&log_path, n),
        None    => read_all_lines(&log_path),
    }
}
```

`follow: true` is a hint to the caller (CLI `aegis logs`) that streaming is intended — the recorder returns the current snapshot; the CLI layer polls or uses `inotify`/`kqueue` to watch for new lines.

### 5.5 `log_path()`

```rust
fn log_path(&self, agent_id: Uuid) -> PathBuf {
    self.storage.agent_log_path(agent_id)
}
```

---

## 6. `query.rs` — Line Reading

### 6.1 `tail_lines(path, n)`

Reads the last `n` lines efficiently without loading the whole file:

```
Algorithm:
1. Open file, seek to end.
2. Walk backwards in 4KB chunks.
3. Count newlines until n+1 found (or start of file).
4. Seek to the position after the (n+1)th newline from end.
5. Read forward to end, split on newlines.
```

Returns `Vec<String>` with newlines stripped. Maximum `n` is clamped to `recorder.failover_context_lines` (enforced by the Watchdog caller, not here).

### 6.2 `read_all_lines(path)`

```rust
fn read_all_lines(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AegisError::StorageIo { path: path.to_owned(), source: e })?;
    Ok(content.lines().map(str::to_owned).collect())
}
```

---

## 7. `rotation.rs` — Archival & Size Rotation

### 7.1 Size-Based Rotation

Live log files are never rotated mid-session. Size checks happen at two points:

1. **At `archive()` time** — after archiving, prune old archive files.
2. **Periodic task (optional)** — a background task wakes every hour and checks archive directory total size.

### 7.2 `prune_archive()`

```rust
pub fn prune_archive(
    storage: &dyn StorageBackend,
    config: &RecorderConfig,
) -> Result<()> {
    let archive_dir = storage.archive_dir();
    let mut entries: Vec<(PathBuf, SystemTime)> = fs::read_dir(&archive_dir)?
        .filter_map(|e| {
            let e = e.ok()?;
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mtime))
        })
        .collect();

    // Sort oldest first
    entries.sort_by_key(|(_, mtime)| *mtime);

    // Remove oldest until count <= retention_count
    while entries.len() > config.log_retention_count {
        let (path, _) = entries.remove(0);
        fs::remove_file(&path)?;
        tracing::debug!(removed = %path.display(), "pruned old log archive");
    }
    Ok(())
}
```

Max size check (`log_rotation_max_mb`): if total archive directory size exceeds the limit after pruning by count, continue removing oldest until under the limit.

---

## 8. Failover Context Window

The Watchdog calls `query()` with:

```rust
LogQuery {
    agent_id,
    last_n_lines: Some(config.recorder.failover_context_lines),
    since: None,
    follow: false,
}
```

The result is passed as `FailoverContext::terminal_context` to `Provider::failover_handoff_prompt()`.

---

## 9. Test Strategy

| Test | Asserts |
|---|---|
| `test_attach_creates_log_file` | After `attach()`, log file path exists |
| `test_pipe_pane_captures_output` | Text sent to pane via `send_text` appears in log file |
| `test_detach_stops_capture` | After `detach()`, new pane output not appended to log |
| `test_archive_moves_log` | `archive()` moves file to archive dir with timestamp suffix |
| `test_query_last_n_lines` | `tail_lines(path, 10)` returns exactly the last 10 lines |
| `test_query_entire_log` | `read_all_lines` returns all lines |
| `test_query_missing_log` | `LogFileNotFound` error for unknown agent_id |
| `test_prune_keeps_retention_count` | Archive dir never exceeds `retention_count` files |
| `test_prune_removes_oldest_first` | Oldest archive files removed before newer ones |
