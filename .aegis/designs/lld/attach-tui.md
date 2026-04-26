# LLD: Agent Attach - TUI

## 1. Purpose

Define the TUI path for attaching to a live agent. The TUI should present attachable agents, open an interactive live pane for the selected agent, and keep the existing selection-driven workflow consistent with the current app model.

This LLD covers the TUI-side client, state, overlay, and pane rendering behavior only. Restart persistence is out of scope for M27 and belongs to M28.

## 2. Scope

Included:

- list attachable agents in the TUI
- open an interactive attach surface for a selected agent
- reuse the existing UDS client and pane relay path where feasible
- provide clean detach/back navigation
- keep the current selected-agent model intact

Excluded:

- restart persistence of attach state
- new daemon storage formats
- a new tmux broker or websocket transport
- changes to agent spawn or registry semantics

## 3. Desired Behavior

The default TUI flow should be:

1. Show a list of attachable agents.
2. Let the user select an agent row.
3. Open the live pane for that agent in interactive mode.
4. Allow input forwarding while attached.
5. Allow the user to detach and return to the list without losing selection context.

## 4. Data Flow

The TUI already has:

- `AegisClient::send_command(...)`
- `AegisClient::attach_pane(...)`
- `AppState::selected_agent_id`
- `PaneMode::Input`
- an overlay system

The attach flow should reuse those pieces rather than introducing a parallel transport.

Proposed flow:

1. User selects an agent from the list.
2. TUI requests pane resolution or attach readiness from the daemon.
3. TUI opens the interactive pane socket for the selected agent.
4. The terminal component forwards input and resize events.
5. Detach closes the socket and returns to the selection view.

## 5. State Model

The TUI should track:

- the selected agent id,
- whether the user is currently attached,
- whether the current attach is interactive,
- and the last known pane target for display purposes.

The current `AppState` can be extended incrementally. A dedicated attach state struct is only needed if the existing fields become too fragmented.

## 6. UI Model

### 6.1 Agents View

The agents view should present attachable agents, not just a static registry table.

Row selection should be the primary navigation path. The selected row should drive attach, and the UI should make the selected agent obvious without requiring a separate “open” flow.

### 6.2 Pane View

The pane view should be the interactive attach surface.

It should display:

- agent name
- agent id
- tmux target when available
- interactive status

### 6.3 Detach Behavior

Detach should return the user to the agent list or pane selection view without clearing the agent list state.

## 7. Command Handling

The TUI should reuse the current command handling model:

- selection changes still use the existing key navigation
- attach should be triggered from the current row selection path
- input mode should feed the live pane while attached
- escape/detach should leave the pane without terminating the agent

The implementation should prefer the existing `AegisClient::attach_pane(...)` path if it is sufficient.

## 8. Transport Notes

The TUI already speaks UDS to the daemon. The attach implementation should use that channel and should not add a separate transport unless the current pane relay proves insufficient.

Reuse of existing pane relay endpoints is preferred.

## 9. Failure Handling

If the agent disappears, the TUI should:

- exit attach mode cleanly,
- show a recoverable error,
- and keep the rest of the app usable.

If the daemon returns a stale or invalid pane target, the TUI should fall back to the agent list rather than crashing the session.

## 10. Acceptance Criteria

- A user can choose an agent in the TUI and open an interactive live pane.
- The attach surface can forward input.
- Detaching returns to the selection view.
- The implementation reuses the existing UDS client and pane relay where feasible.

