# LLD: Agent Attach - Web UI

## 1. Purpose

Define the web UI path for attaching to a live agent. The web UI should present attachable agents, open the agent pane in interactive mode by default, and reuse the controller's existing pane relay path where feasible.

This LLD covers the frontend, websocket wiring, and route behavior only. Restart persistence is out of scope for M27 and belongs to M28.

## 2. Scope

Included:

- list attachable agents in the web UI
- open an interactive live pane for a selected agent
- reuse the current `/ws/pane/:agent_id` pathway if it fits the target model
- support detach/back navigation
- keep the selected agent as the source of truth for the pane target

Excluded:

- restart persistence of attach state
- new daemon storage formats
- a new remote execution broker
- changes to agent spawn or registry semantics

## 3. Desired Behavior

The default web flow should be:

1. Show a list of attachable agents.
2. Let the user click an agent row.
3. Open the live pane for that agent in interactive mode.
4. Forward input and resize events over websocket.
5. Detach back to the agent list or a neutral pane state.

## 4. Data Flow

The web UI already has:

- a Redux store with agent state
- a pane route
- a `Terminal` component using `/ws/pane/:agent_id`
- controller-side pane relay support

The attach flow should build on those pieces instead of adding a second terminal stack.

Proposed flow:

1. User selects an attachable agent from the list.
2. The route changes to the pane view for that agent.
3. The terminal opens the websocket stream for that `agent_id`.
4. Input and resize events are forwarded live.
5. Leaving the pane closes the websocket and returns to the list.

## 5. State Model

The web UI should track:

- the selected agent id,
- whether pane mode is active,
- whether the current pane is interactive,
- and the last known agent target for route restoration.

The existing Redux slices can likely absorb this with a small addition rather than a new domain model.

## 6. UI Model

### 6.1 Agents View

The agents view should present attachable agents as the primary action surface.

Selecting an agent row should open that agent's live pane.

### 6.2 Pane View

The pane view should remain the interactive attach surface.

It should display:

- agent name
- tmux target
- provider
- interactive status

### 6.3 Detach Behavior

Detaching should return the user to the attachable agent list and preserve the current selection when possible.

## 7. Transport Notes

The current websocket pane route is the preferred starting point.

If the controller can reuse the existing pane relay path without adding a new attach IPC boundary, that is the preferred option for M27.

## 8. Failure Handling

If the websocket disconnects because the agent exits or the daemon restarts, the UI should:

- show a recoverable disconnected state,
- preserve the selected agent if it still exists,
- and allow reattachment when the target becomes available again.

If the agent target is invalid, the pane view should fall back to the agent list rather than leaving the UI in a dead state.

## 9. Acceptance Criteria

- A user can click an agent in the web UI and open an interactive live pane.
- Input and resize flow through the existing websocket terminal.
- Detaching or closing the pane returns to the agent list.
- The implementation reuses the current controller pane relay path where feasible.

