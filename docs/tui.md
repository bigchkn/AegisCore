# AegisCore TUI Guide

The TUI (`aegis ui`) is a full-screen terminal dashboard for monitoring and controlling a running AegisCore project. It connects to the local `aegisd` daemon over a Unix socket and updates in real time as agents spawn, finish tasks, and emit events.

---

## Prerequisites

`aegisd` must be running before you open the TUI. The TUI will connect automatically — if the daemon is not reachable the header border turns red and the agent/task/channel panes will be empty.

```
aegisd start           # start the daemon (separate terminal or background)
aegis ui               # open the TUI
```

---

## Layout

```
┌─────────────────────────────────────────────────────────────────────────┐
│  AegisCore | Project: /path/to/project           [header — status color] │
├──────────────────┬────────────────────────────────┬──────────────────────┤
│  Agents          │                                │  Tasks               │
│  ─────────────── │           Logs                 │  ─────────────────── │
│  [Active] name   │  (or Terminal in Input mode)   │  [Pending] task desc │
│  [Cooling] name  │                                │  [Complete] task desc│
│  ...             │                                │  ...                 │
├──────────────────┤                                ├──────────────────────┤
│  Channels        │                                │                      │
│  ─────────────── │                                │                      │
│  (Mailbox) name  │                                │                      │
│  ...             │                                │                      │
└──────────────────┴────────────────────────────────┴──────────────────────┘
│  MODE: NORMAL | [q]uit | [p]rojects | [s]pawn | [x]kill | [?]help       │
└─────────────────────────────────────────────────────────────────────────┘
```

| Column | Width | Contents |
|--------|-------|----------|
| Left (top) | 25% | Active agents, colour-coded by selection |
| Left (bottom) | 25% | Channel list with kind labels |
| Center | 50% | Agent logs (Normal mode) or live terminal (Input mode) |
| Right | 25% | Task list with status labels |
| Header | full | Project path; border colour = connection state |
| Footer | full | Current mode and available key hints |

### Header border colours

| Colour | Meaning |
|--------|---------|
| Green | Connected to daemon |
| Yellow | Connecting / handshaking |
| Red | Disconnected |
| Magenta | Connection error |

---

## Modes

The TUI has three modes. The current mode is always shown in the footer.

### Normal mode (default)

The starting mode. Navigate agents, open overlays, and issue commands.

| Key | Action |
|-----|--------|
| `j` / `↓` | Select next agent |
| `k` / `↑` | Select previous agent |
| `s` | Open the Spawn Agent overlay |
| `x` | Open the Kill confirmation overlay (requires an agent to be selected) |
| `p` | Open the Project Switcher overlay |
| `i` | Switch to Input (interactive terminal) mode |
| `:` | Switch to Command mode |
| `?` / `h` | Open the Help overlay |
| `q` | Quit the TUI |

### Input mode — interactive terminal

Attaches the center pane to the selected agent's tmux pane. Keystrokes are forwarded directly to the agent process. Use this to send ad-hoc instructions or observe live output.

| Key | Action |
|-----|--------|
| `Esc` | Return to Normal mode |
| _anything else_ | Forwarded to the agent's terminal |

### Command mode

Reserved for future command-bar input. Currently pressing `Enter` or `Esc` returns to Normal mode.

---

## Overlays

Overlays appear centred over the layout and block all other input until dismissed.

### Spawn Agent (`s`)

Prompts for a task description. The description becomes the task handed to a new Splinter agent.

| Key | Action |
|-----|--------|
| _type_ | Append character to description |
| `Backspace` | Delete last character |
| `Enter` | Spawn agent and dismiss |
| `Esc` | Cancel without spawning |

The description can be as long as you need — it wraps inside the input panel. Press `Enter` only when you are ready; the daemon receives the full text as a single task string.

### Project Switcher (`p`)

Lists all projects registered with the running daemon.

| Key | Action |
|-----|--------|
| `j` / `↓` | Move selection down |
| `k` / `↑` | Move selection up |
| `Enter` | Switch to selected project |
| `Esc` | Cancel |

Switching project reloads the agent, task, and channel panes for the new project root without restarting the TUI.

### Kill Confirmation (`x`)

Asks for confirmation before terminating the selected agent. The agent name is shown to avoid mistakes.

| Key | Action |
|-----|--------|
| `y` / `Y` / `Enter` | Confirm — kill the agent |
| _any other key_ | Cancel |

### Help (`?` or `h`)

Displays the key-binding reference. Press `Esc` to dismiss.

---

## Common Workflows

### Monitor a running project

1. Start the TUI: `aegis ui`
2. The left column shows all active agents; the right column shows all tasks.
3. Use `j`/`k` to select an agent — its logs stream into the center pane automatically.
4. Events (status changes, task completions, failovers) update the display in real time without any interaction.

### Spawn a new Splinter agent

1. Press `s` to open the Spawn overlay.
2. Type the task description (e.g. `Refactor the payment module to use the new API`).
3. Press `Enter`. The daemon enqueues the task and spawns a Splinter.
4. The new agent appears in the Agents pane within one refresh cycle (~100 ms).

### Kill a misbehaving agent

1. Use `j`/`k` to select the agent in the Agents pane.
2. Press `x` to open the Kill overlay.
3. Confirm with `y` or `Enter`.
4. The agent's status changes to `Terminated` and its tmux session is cleaned up by the daemon.

### Watch live terminal output

1. Select the target agent with `j`/`k`.
2. Press `i` to enter Input mode.
3. The center pane switches from the log view to a live terminal mirror.
4. Press `Esc` to return to Normal mode and the log view.

### Switch between projects

1. Press `p` to open the Project Switcher.
2. Navigate to the target project with `j`/`k`.
3. Press `Enter`. The TUI reconnects to the same daemon but scopes all queries to the new project root.

---

## Troubleshooting

**Header border is red / panes are empty**
The TUI cannot reach the daemon. Run `aegisd start` in another terminal, or check `aegis doctor` for configuration issues.

**Agents list is empty after spawn**
The daemon accepted the command but the agent may still be initialising. Wait one or two seconds; the TUI polls every 100 ms.

**Logs pane shows "No logs found for selected agent"**
The agent has not emitted any output yet, or no agent is selected. Use `j` or `k` to select one.

**Keystrokes in Input mode aren't reaching the agent**
Input mode requires a selected agent with an attached tmux pane. Return to Normal mode (`Esc`), select an agent, and re-enter Input mode.
