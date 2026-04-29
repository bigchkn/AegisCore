# Getting Started with AegisCore

AegisCore is an orchestration engine for autonomous AI CLI agents running in macOS sandboxes. This guide will help you set up your first project and spawn your first agents.

## Prerequisites

- **macOS (Darwin)**: Mandatory for `sandbox-exec`.
- **tmux (≥ 3.0)**: Used for agent session management.
- **git**: Required for project anchoring and versioning.
- **AI CLI Providers**: You should have at least one supported provider installed (e.g., `claude-code`, `gemini-cli`, `codex`, or `dirac`).

## Installation

1. **Build from source**:
   ```bash
   cargo build --release
   ```
2. **Install**:
   ```bash
   ./install.sh
   ```
3. **Verify**:
   ```bash
   aegis doctor
   ```

## 1. Initialize a Project

Navigate to your project directory and run:

```bash
aegis init
```

This creates a `.aegis/` directory and an `aegis.toml` configuration file.

## 2. Start the Daemon

The AegisCore daemon (`aegisd`) manages all projects and agents.

```bash
aegis daemon start
```

## 3. Launch a Bastion Agent

A **Bastion** agent is a long-lived agent that can spawn sub-agents (**Splinters**).

```bash
aegis start
```

## 4. Spawn a Splinter Agent

You can spawn an agent to perform a specific task:

```bash
aegis spawn "Implement the login logic in src/auth.rs"
```

## 5. Observe and Interact

- **List Agents**: `aegis agents`
- **Watch Logs**: `aegis logs <agent_id> --follow`
- **Attach to Terminal**: `aegis attach <agent_id>`
- **TUI Dashboard**: `aegis ui`

## 6. Using the TUI

Run `aegis ui` to open the interactive dashboard.
- **`j`/`k`**: Navigate agent list.
- **`Enter`**: Attach to agent terminal (Pane mode).
- **`Esc`**: Return to Normal mode from Pane mode.
- **`i`**: Enter Input mode (send keys to agent).
- **`:`**: Command mode (e.g., `:spawn <task>`, `:kill`).
- **`?`**: Help overlay.

## Next Steps

- Edit `aegis.toml` to configure providers and watchdog intervals.
- Explore the [HLD](../.aegis/designs/hld/aegis.md) and [LLDs](../.aegis/designs/lld/) for architecture details.
- Learn about the template and design system: [Design Templates](design-templates.md).
