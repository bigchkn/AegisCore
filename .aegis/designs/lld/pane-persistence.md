# LLD: Terminal State Persistence

## Overview
Currently, the Aegis web-ui "Pane" view only shows terminal output produced *after* the user attaches. This LLD describes a fix to provide the full current state of the terminal pane immediately upon connection.

## Backend Changes

### 1. `aegis-tmux` (Bug Fix)
The `capture_pane` and `capture_pane_plain` methods in `TmuxClient` currently have inverted `-e` flag logic.
- `capture_pane` (should include escapes) currently omits `-e`.
- `capture_pane_plain` (should strip escapes) currently includes `-e`.

This will be corrected.

### 2. `PaneRelay` (Aegis Controller)
The `PaneRelay::relay` method in `crates/aegis-controller/src/daemon/logs.rs` will be updated:
- It will call `self.tmux.capture_pane(&target, 2000).await` to get the current buffer (with ANSI codes).
- It will send this buffer immediately to the `out_tx` sink.
- It will then enter the log-tailing loop as before.

## Frontend Changes
No changes are required in the frontend. `xterm.js` in the `Terminal` component already handles ANSI-encoded output and will render the initial snapshot correctly.

## Roadmap Integration
A new milestone **M40** will be added to track this improvement.
