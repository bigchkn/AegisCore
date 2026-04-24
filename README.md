# AegisCore

| Note : Software is still pre-release and not suitable to install yet.

**Hardened Orchestration. Shielded Intelligence. Absolute Control.**

AegisCore is a multi-agent orchestration engine for macOS that runs autonomous AI CLI agents inside kernel-enforced sandboxes — no Docker, no VMs, no web servers. Just tmux, git worktrees, and Apple's native Seatbelt security layer.

It coordinates a hierarchy of long-lived **Bastion** agents and ephemeral **Splinter** agents, routes work between them through a structured channel layer, and maintains unbroken context across CLI failures through a passive Flight Recorder.

---

## Why AegisCore?

**Zero-container isolation.** Every agent runs under `sandbox-exec` (macOS Seatbelt / SBPL), locked to its own Git worktree at the syscall level. Agents have full YOLO permissions inside their directory and zero access outside it — no Docker overhead, no virtualization penalty.

**Context indestructibility.** The Flight Recorder mirrors every agent's terminal I/O to an append-only log the moment it spawns. If a CLI hits a rate limit or crashes, the Watchdog captures the last known context and injects it into a failover agent without you lifting a finger.

**CLI-agnostic.** AegisCore treats `claude-code`, `gemini-cli`, `codex`, and local `ollama` models as interchangeable providers behind a uniform interface. Failover cascades are user-defined in TOML — primary, fallback, and local fallback, in whatever order you prefer.

**Remote command and control.** A Telegram bridge gives you real-time push notifications (task complete, rate limit, failover, sandbox violation) and pull commands (`/status`, `/kill`, `/spawn`, `/logs`) from your phone.

---

## Core Concepts

| Concept             | Description                                                                        |
| ------------------- | ---------------------------------------------------------------------------------- |
| **Bastion**         | Long-lived agent holding project context; coordinates Splinters                    |
| **Splinter**        | Ephemeral agent spawned for a discrete task; evaporates on completion              |
| **Flight Recorder** | Passive I/O mirror attached to every agent via `tmux pipe-pane`                    |
| **Watchdog**        | Background monitor that detects failures and triggers failover cascades            |
| **Sandbox Factory** | Generates per-agent `.sb` profiles at spawn time; injects worktree path            |
| **Channel Layer**   | Injection (send-keys), Mailbox (filesystem), Observation (capture-pane), Broadcast |

---

## Architecture at a Glance

```
Telegram Bot  ←→  AegisCore Controller (Rust)
                      │
        ┌─────────────┼─────────────────┐
   Dispatcher    Watchdog          Scheduler
        │              │                 │
   tmux sessions  capture-pane    MAX_SPLINTERS
        │          (Observation)   semaphore
   ┌────┴────┐
   Bastion   Splinter × n
   (main     (isolated
   worktree)  worktrees)
        │
   sandbox-exec (Seatbelt)
```

---

## Supported CLI Providers (v1)

- `claude-code` (Anthropic)
- `gemini-cli` (Google)
- `codex` (OpenAI)
- `ollama` (local models — unlimited, no API cap)

All providers are configured in `aegis.toml`. Failover cascades are user-defined per agent role.

---

## Platform

- **macOS** (Apple Silicon primary; Intel supported)
- Requires: `tmux`, `git`, `sandbox-exec` (built into macOS)
- Written in **Rust**

---

## Documentation

- **[Getting Started](GETTING_STARTED.md)**: Installation and your first agent.
- **[Taskflow System](TASKFLOW.md)**: How AegisCore manages project roadmaps and agent alignment.
- **[Architecture (HLD)](.aegis/designs/hld/aegis.md)**: Deep dive into the system design.

---

## Status

Early development. Following a structured HLD → LLD → Roadmap task approach.

Design documents: [`.aegis/designs/hld/aegis.md`](.aegis/designs/hld/aegis.md)

---

## License

MIT — see [LICENSE](LICENSE)
