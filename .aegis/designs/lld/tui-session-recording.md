# LLD: Structured TUI Session Recording

**Milestone:** M41  
**Status:** draft  
**HLD ref:** §8  
**Implements:** `crates/aegis-recorder/`, `crates/aegis-tmux/`, `crates/aegis-watchdog/`, `crates/aegis-controller/`

---

## 1. Purpose

The current Aegis terminal capture story mixes two different concerns:

- **session recording**: preserving the real terminal byte stream and user input so a session can be replayed or handed off faithfully;
- **screen observation**: inspecting current visible terminal state for watchdog detection and web rendering.

This milestone introduces a structured recording pipeline that keeps the raw PTY/tmux stream as the source of truth and records terminal activity as timestamped events rather than repeated screen snapshots.

The design goal is more coherent logs for:

- provider failover handoff context;
- postmortem debugging;
- browser terminal replay;
- future summarization or semantic extraction from TUI sessions.

---

## 2. Design Summary

### 2.1 Source of Truth

The canonical session record is the raw pane byte stream emitted by tmux `pipe-pane`, plus explicit input and resize events emitted by Aegis.

### 2.2 Event Model

Recording moves from “append raw bytes to a log file and infer meaning later” to “append newline-delimited session events”.

Each event carries:

- relative timestamp;
- event type;
- payload.

Supported event kinds:

- `output`: bytes emitted by the terminal program;
- `input`: bytes injected by Aegis into the pane;
- `resize`: pane size change;
- `marker`: optional synthetic milestone/debug markers.

### 2.3 Derived Projections

Structured screen state is derived from the raw event stream, not recorded as the primary artifact.

For screen modeling, use the `vt100` crate as the first implementation step:

- lightweight;
- focused on parsing terminal byte streams;
- exposes rendered screen contents and diffs.

If richer terminal semantics become necessary later, `termwiz` remains an upgrade path.

### 2.4 Format Direction

The session event schema should be intentionally close to `asciicast v2`:

- streaming-friendly;
- append-only;
- replayable;
- widely understood terminal recording model.

We do not need full external compatibility on day one, but the internal shape should align with it to avoid format churn.

---

## 3. Non-Goals

- Replacing tmux with direct PTY ownership in this milestone.
- Building a full browser playback UI.
- Removing `capture-pane` from watchdog immediately if it remains the lowest-risk implementation for detection.
- Designing a generic analytics pipeline for all session artifacts.

---

## 4. Crate and Dependency Decisions

## 4.1 `portable-pty`

Do not adopt in this milestone.

Reason: Aegis is already tmux-centric. `portable-pty` is the right primitive if Aegis later decides to own subprocess PTYs directly instead of delegating lifecycle and multiplexing to tmux.

## 4.2 `vt100`

Adopt for derived screen projection.

Reason: It parses terminal byte streams into an in-memory screen model and can generate diffs against previous state. This is enough for coherent screen snapshots, replay checkpoints, and possible watchdog improvements without taking on a full terminal stack.

## 4.3 `termwiz`

Defer.

Reason: It offers richer terminal modeling and change tracking, but it is materially larger and more complex than needed for the first pass.

## 4.4 `shadow-terminal`

Do not adopt as a runtime dependency for this milestone.

Reason: It is useful as a reference design and for potential end-to-end testing, but it brings a heavier abstraction than needed for Aegis’s immediate recording needs.

---

## 5. Data Model

Add a structured event type in `aegis-core` for recorder-facing APIs.

```rust
pub enum SessionEventKind {
    Output,
    Input,
    Resize,
    Marker,
}

pub struct SessionEvent {
    pub ts_ms: u64,
    pub kind: SessionEventKind,
    pub data: Vec<u8>,
}
```

Serialization in the recorder crate should use newline-delimited JSON.

Representative wire format:

```json
{"version":1,"width":80,"height":24}
{"t":0,"k":"output","d":"...base64..."}
{"t":18,"k":"input","d":"...base64..."}
{"t":22,"k":"resize","cols":100,"rows":30}
```

Notes:

- `output` and `input` payloads are raw bytes encoded as base64.
- `resize` stores numeric dimensions and does not use `data`.
- A header line records initial dimensions and recorder version.

---

## 6. Recorder Architecture

## 6.1 Live Session Artifact

For each active agent, maintain:

- `session.cast`-style structured event log;
- optional compatibility plain-text tail view derived from `output` events only.

The structured file is authoritative. Plain text becomes a convenience view for human grep/tail workflows.

## 6.2 `FlightRecorder`

`FlightRecorder::attach()` continues to use tmux `pipe-pane`, but the sink changes:

- raw pane output is appended as `output` events, not naked bytes;
- `append_tmux_send` / `append_tmux_input` style helpers emit `input` events;
- pane relay or dispatcher emits `resize` events when size changes are observed.

## 6.3 Query APIs

Extend recorder querying so consumers can ask for:

- raw output lines for backward compatibility;
- recent structured events;
- derived plain-text tail;
- optional derived rendered screen snapshot.

Backward-compatible `query(LogQuery)` may remain initially, backed by a compatibility projection over structured events.

---

## 7. Screen Projection

Add a projection module in `aegis-recorder` or `aegis-controller` that:

1. replays `output` and `resize` events into a `vt100::Parser`;
2. ignores `input` for rendering purposes;
3. exposes:
   - current visible screen text;
   - ANSI-formatted screen snapshot;
   - diff since previous checkpoint.

This projection is for:

- coherent failover summaries;
- browser bootstrap snapshots;
- future reduction of snapshot polling in watchdog.

It is explicitly derived state and may be recomputed.

---

## 8. Watchdog Strategy

Short term:

- keep existing `capture_pane_plain` scanning for event detection if it remains simplest and stable;
- add an alternate path that can scan recent `output` events or a projected screen snapshot from the recorder.

Medium term:

- move watchdog matching away from live tmux screen scraping toward recorder-backed recent context plus projected current screen state.

This reduces coupling between monitoring fidelity and tmux snapshot semantics.

---

## 9. Web/UI Strategy

The browser pane should continue consuming raw terminal bytes for live rendering.

For log views and replay tooling:

- consume structured session events instead of plain text where possible;
- allow reconstruction of either:
  - a plain-text transcript;
  - a replay stream;
  - a rendered checkpoint.

This separates “terminal replay” from “grep-friendly logs” rather than forcing one format to serve both badly.

---

## 10. Migration Plan

### Phase 1

- introduce session event types and event writer;
- write structured event logs alongside current plain logs if necessary;
- add recorder tests around output/input/resize event emission.

### Phase 2

- add `vt100`-backed projection utilities;
- teach failover/context consumers to use projected text or recent structured events.

### Phase 3

- retire duplicated snapshot-style logging paths where recorder-backed projections are sufficient;
- decide whether plain logs remain first-class artifacts or become derived exports only.

---

## 11. Test Strategy

| Test | Asserts |
|---|---|
| `structured_recorder_writes_output_events` | `pipe-pane` output is serialized as ordered `output` events |
| `structured_recorder_writes_input_events` | Aegis-injected pane input is recorded separately from provider output |
| `structured_recorder_writes_resize_events` | pane size changes are persisted with correct dimensions |
| `vt100_projection_reconstructs_screen` | replaying structured events yields expected final visible screen |
| `projection_handles_overwrite_and_cursor_motion` | TUI cursor movement does not duplicate content in derived screen text |
| `failover_context_can_read_recent_events` | recorder query returns bounded recent structured context for watchdog/failover |

---

## 12. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Structured logs make ad hoc tailing harder | keep a compatibility plain-text projection during migration |
| Event volume grows for chatty TUIs | use append-only NDJSON and bounded projection windows |
| Reconstructed screen diverges from tmux behavior on edge cases | start with `vt100`, test against representative provider CLIs, retain tmux snapshot fallback where necessary |
| Multiple sources emit resize events inconsistently | centralize resize emission at pane relay / tmux observation boundary |

---

## 13. Acceptance Criteria

- New sessions are recorded as append-only structured event streams.
- Recorder preserves distinct `output`, `input`, and `resize` events.
- A derived screen projection can reconstruct coherent visible terminal state from structured events.
- Failover and debugging consumers can read recent session context without relying solely on repeated `capture-pane` snapshots.
- Existing live browser pane rendering continues to function during the migration.
