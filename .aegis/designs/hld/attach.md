# AegisCore — High-Level Design: Agent Attach & Live Inspection

**Status:** Draft v0.2  
**Milestone:** M27  
**Platform:** macOS (Darwin / Apple Silicon primary)  
**Implementation Language:** Rust

## 1. Purpose

AegisCore can already spawn agents, list them, and expose their panes in separate slices across the CLI, TUI, and web UI. What is missing is a shared attach model that makes a live agent a first-class interactive target instead of a one-off tmux lookup.

This feature defines how an operator attaches to a running agent, how the controller resolves that agent, and how the CLI, TUI, and web UI present the same underlying session.

## 2. Scope

M27 covers live attach and inspection only.

Persistence of attach state across daemon restarts is intentionally split into M28, `Agent Attach Persistence`, so this milestone stays focused on the live interaction path.

## 3. Problem Statement

The current surface area is fragmented:

- `aegis attach <agent_id>` is a local tmux passthrough that jumps into the project session or a specific agent pane.
- The TUI has a pane view, but it is selection-driven and does not express an attach lifecycle.
- The web UI has a pane view over WebSocket, but it behaves like a viewer rather than a true attach target.

That means there is no shared concept of:

- which agent is being attached to,
- whether the session is interactive or passive,
- how multiple clients should behave when attached to the same agent,
- or how the controller should mediate the live pane target.

## 4. Goal

Define a controller-mediated attach flow that lets a user:

- attach to a specific agent by `agent_id`,
- interact with the agent live from the TUI and web UI,
- keep the existing direct tmux fallback for the CLI,
- and detach without disturbing the agent.

The attach model should work across the existing tmux execution plane rather than replacing it.

## 5. Non-Goals

- Replacing tmux as the execution plane.
- Rewriting agent spawn or registry semantics.
- Designing persistence for restart recovery.
- Designing provider-specific terminal capture logic.
- Adding a new remote execution broker.

## 6. Current State

Existing pieces already point toward the feature:

- `aegis attach [<agent_id>]` can jump to the project session or a pane.
- The controller tracks tmux session, window, and pane metadata for each agent.
- The web UI already has a pane route backed by controller WebSocket relay.
- The TUI already has pane and agent views, and can select an agent row.

What is missing is the shared attach contract and the controller-owned lifecycle around it.

## 7. Proposed Model

The feature should treat attach as a controller-resolved session, not as a UI-local tmux shortcut.

### 7.1 Attachment Target

An attachment target is always identified by `agent_id` for CLI, TUI, and web UI.

The controller resolves that id to the underlying tmux session, window, and pane.

### 7.2 Attachment Modes

At minimum, the system should distinguish:

- `interactive` - live input/output forwarding to the agent pane,
- `inspect` - snapshot-oriented viewing without an active attach.

Interactive is the default behavior for TUI and web UI attach surfaces.

The CLI may continue to use direct tmux passthrough when the user asks for it, but the controller should still be able to resolve the same agent target.

### 7.3 Concurrent Viewers

Multiple concurrent viewers are allowed.

The HLD does not require a single-owner model, and it does not require coordination between viewers beyond valid target resolution and clean detach.

### 7.4 Attach State

For M27, attach state may remain in-memory as long as the controller can resolve a live target from the registry.

Restart persistence is not part of this milestone and is deferred to M28.

## 8. UI Surfaces

### 8.1 CLI

The CLI should continue to support direct `aegis attach [<agent_id>]` passthrough behavior.

That path can remain tmux-centric for local operator workflows.

### 8.2 TUI

The TUI should present a list of attachable agents.

Selecting an agent should attach to that agent’s live pane, and the attach surface should support interaction rather than only passive viewing.

### 8.3 Web UI

The web UI should present the same list of attachable agents.

Selecting an agent should open the live pane surface for that agent and support interactive attachment where the transport allows it.

## 9. Controller Responsibilities

The controller should own:

- agent-to-pane resolution,
- attach target validation,
- attach invalidation on termination,
- and mediation of the live agent target for TUI and web clients.

The controller should remain the source of truth for the live agent identity and pane target. Clients should not invent their own mapping logic.

## 10. Lifecycle

1. User selects an attachable agent.
2. Client requests resolution for that `agent_id`.
3. Controller resolves the pane and exposes the live target.
4. Client streams pane data and forwards input if the attach mode is interactive.
5. Client detaches or the agent terminates.
6. Controller invalidates the live target for the current session.

## 11. Open Decisions

The remaining design questions have been resolved for this milestone:

- TUI and web UI default to `interactive`.
- Attach events do not need to be emitted as a separate event class.
- Reuse of existing controller pane relay paths is preferred where feasible.
