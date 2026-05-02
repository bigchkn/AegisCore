# AegisCore

**Hardened Orchestration. Shielded Intelligence. Absolute Control.**

AegisCore is a multi-agent orchestration engine for macOS that runs autonomous AI CLI agents inside kernel-enforced sandboxes — no Docker, no VMs, no web servers. Just tmux, git worktrees, and Apple's native Seatbelt security layer.

It coordinates a hierarchy of long-lived **Bastion** agents and ephemeral **Splinter** agents, routes work between them through a structured channel layer, and maintains unbroken context across CLI failures through a passive Flight Recorder.

---

## Why AegisCore?

**Zero-container isolation.** Every agent runs under `sandbox-exec` (macOS Seatbelt / SBPL), locked to its own Git worktree at the syscall level. Using an "allow-default" read strategy and "deny-default" write strategy, agents have the freedom to read system libraries and project context while being strictly forbidden from modifying anything outside their assigned worktree.

**Context indestructibility.** The Flight Recorder mirrors every agent's terminal I/O to an append-only log the moment it spawns. If a CLI hits a rate limit or crashes, the Watchdog captures the last known context and injects it into a failover agent without you lifting a finger.

**CLI-agnostic.** AegisCore treats `claude-code`, `gemini-cli`, `opencode`, `codex`, `dirac`, and local `ollama` models as interchangeable providers behind a uniform interface. Failover cascades are user-defined in TOML — primary, fallback, and local fallback, in whatever order you prefer.

**Native Daemonization.** AegisCore includes a background daemon (`aegisd`) that manages agent lifecycles, registries, and terminal sessions. It integrates natively with macOS `launchd` for persistent, unattended operation.

---

## Core Concepts

| Concept             | Description                                                                        |
| ------------------- | ---------------------------------------------------------------------------------- |
| **Bastion**         | Long-lived agent holding project context; coordinates Splinters                    |
| **Splinter**        | Ephemeral agent spawned for a discrete task; evaporates on completion              |
| **Flight Recorder** | Passive I/O mirror attached to every agent via `tmux pipe-pane`                    |
| **Watchdog**        | Background monitor that detects failures and triggers failover cascades            |
| **Sandbox Factory** | Generates per-agent `.sb` profiles at spawn time with environment preservation     |
| **Channel Layer**   | Injection (send-keys), Mailbox (filesystem), Observation (capture-pane), Broadcast |

---

## Architecture at a Glance

```
Aegis Dashboard (TUI/Web)  ←→  AegisCore Daemon (aegisd)
                                      │
        ┌─────────────┬───────────────┴──────────────┐
   Dispatcher    Watchdog          Scheduler      Registry
        │              │                 │              │
   tmux sessions  capture-pane    MAX_SPLINTERS   SQLite/File
        │          (Observation)   semaphore      Persistence
   ┌────┴────┐
   Bastion   Splinter × n
   (main     (isolated
   worktree)  worktrees)
        │
   sandbox-exec (Seatbelt)
```

---

## Supported CLI Providers

- `claude-code` (Anthropic)
- `gemini-cli` (Google)
- `opencode` (High-performance open weights)
- `codex` (OpenAI)
- `dirac`
- `ollama` (local models — unlimited, no API cap)

All providers are configured in `aegis.toml`. Failover cascades are user-defined per agent role.

---

## Platform

- **macOS** (Apple Silicon primary; Intel supported)
- Requires: `tmux`, `git`, `sandbox-exec` (built into macOS)
- Written in **Rust**

---

## Documentation

- **[Getting Started](docs/getting-started.md)**: Installation, `aegisd install`, and your first agent.
- **[TUI Guide](docs/tui.md)**: Full reference for the interactive terminal dashboard — layout, modes, key bindings, and common workflows.
- **[Taskflow System](docs/taskflow.md)**: How AegisCore manages project roadmaps and agent alignment.
- **[Design Templates](docs/design-templates.md)**: Template system and `aegis design` commands — spawn, customise, and author agent templates.
- **[Architecture (HLD)](.aegis/designs/hld/aegis.md)**: Deep dive into the system design.

---

## Status

**Alpha / Active Development.** The core orchestration and sandboxing layers are stabilized. Telegram and Web UI integrations are currently in progress.

Design documents: [`.aegis/designs/hld/aegis.md`](.aegis/designs/hld/aegis.md) · [docs/](docs/)

---

## License

MIT — see [LICENSE](LICENSE)
