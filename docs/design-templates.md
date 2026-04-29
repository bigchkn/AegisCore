# Design Templates & `aegis design` Reference

AegisCore's **template system** lets you define, reuse, and spawn agents from structured blueprints. A template packages everything an agent needs to start: its system prompt, its opening instruction, its provider/model settings, and the variables it expects at spawn time.

---

## Concepts

### Template anatomy

Every template is a directory containing three files:

```
<template-name>/
  template.toml     — metadata, agent settings, variable declarations
  system_prompt.md  — injected as the system prompt at spawn time
  startup.md        — first user-turn message sent to the agent
```

Both Markdown files support `{{variable_name}}` placeholders that are substituted at render time.

### Built-in vs project-local templates

| Layer | Location | Priority |
|-------|----------|----------|
| Built-in | bundled inside the `aegis` binary | lower |
| Project-local | `.aegis/templates/<name>/` | higher (shadows built-ins) |

Project-local templates can override a built-in with the same name, letting you customise base behaviour per project.

### Variable resolution

When spawning a template, AegisCore resolves variables in this order (later wins):

1. **Standard** — `project_root` is always injected from the current project anchor.
2. **Taskflow context** — if `.aegis/designs/roadmap/index.toml` is present, `milestone_id`, `milestone_name`, and `lld_path` are read from the current milestone.
3. **Bastion agent ID** — `bastion_agent_id` is injected by the dispatcher when spawning splinters.
4. **CLI overrides** — `--var KEY=VALUE` flags take highest priority.

Variables declared as `required` in `template.toml` must be resolved; missing ones are an error. Variables declared as `optional` resolve to an empty string if absent.

---

## `template.toml` reference

```toml
[template]
name        = "my-agent"          # unique identifier
description = "What this does"
kind        = "bastion"           # bastion | splinter
version     = "1"
tags        = ["custom"]

[agent]
role             = "my-agent"     # tmux/registry role label
cli_provider     = "claude-code"  # claude-code | gemini-cli | codex | dirac | ollama
model            = "claude-sonnet-4-7"  # optional model override
auto_cleanup     = false          # true = destroy on completion
fallback_cascade = ["gemini-cli"] # ordered list of fallback providers

[agent.sandbox]
network = "outbound_only"         # none | outbound_only | any

[variables]
required = ["project_root"]
optional = ["bastion_agent_id"]
```

### `kind` values

| Kind | Description |
|------|-------------|
| `bastion` | Long-lived agent; holds project context; coordinates splinters |
| `splinter` | Ephemeral agent; spawned for a discrete task; cleaned up on completion |

### `network` policies

| Policy | What the agent can reach |
|--------|--------------------------|
| `none` | No network access |
| `outbound_only` | TCP connections out; no incoming |
| `any` | Unrestricted (use only for trusted agents) |

---

## Built-in templates

### `taskflow-bastion`

A continuous coordinator that drives the project roadmap to completion in a loop.

**Required variables:** `project_root`

**Behaviour:**
1. On startup, checks for any `in-progress` milestone and resumes it.
2. Calls `aegis taskflow next` to pick the next unblocked milestone.
3. Creates a git worktree (`aegis worktree create milestone/<ID>`).
4. Spawns one `taskflow-splinter` per pending task in parallel, sending each a `task` message with the LLD path and acceptance criteria.
5. Polls inbox every 10 seconds; retries blocked splinters up to 3 times before escalating to human clarification.
6. Merges the worktree into main (`aegis worktree merge milestone/<ID>`) and loops.
7. When the roadmap is exhausted, enters an idle loop: polls for a `roadmap_updated` notification (sent by `aegis taskflow notify`) or re-checks every 30 seconds.

### `taskflow-splinter`

A single-task implementer that works within the bastion's worktree.

**Required variables:** `project_root`, `task_description`, `task_id`, `lld_path`, `bastion_agent_id`

**Behaviour:**
1. Reads its inbox for context from the coordinator.
2. Implements the assigned task as described in the LLD.
3. Runs `cargo test -p <crate>`.
4. Commits changes with `git add` + `git commit`.
5. Notifies the coordinator: `aegis message send <bastion_agent_id> notification '{"status":"done",...}'`.
6. Stops. If blocked at any point, commits partial work and sends `{"status":"blocked",...}`.

---

## `aegis design` commands

### `aegis design list`

Lists all available templates, showing name, kind, layer (builtin/project), and description.

```
NAME                           KIND        LAYER        DESCRIPTION
---------------------------------------------------------------------
taskflow-bastion               bastion     builtin      Continuous coordinator: picks milestones…
taskflow-splinter              splinter    builtin      Single-task implementer…
my-custom-agent                bastion     project      My project-specific agent
```

Add `--json` for machine-readable output.

---

### `aegis design show <NAME>`

Shows full metadata for a template: kind, version, tags, provider, model, required/optional variables, and the first ten lines of the system prompt.

```bash
aegis design show taskflow-bastion
```

---

### `aegis design spawn <NAME>`

Renders the template with resolved variables and spawns a live agent via the daemon.

```bash
# Spawn the continuous bastion (no extra vars needed)
aegis design spawn taskflow-bastion

# Spawn a splinter manually (variables normally injected by the bastion)
aegis design spawn taskflow-splinter \
  --var task_id=14.1 \
  --var task_description="Implement the login flow" \
  --var lld_path=.aegis/designs/lld/auth.md \
  --var bastion_agent_id=<UUID>

# Override the model for this spawn only
aegis design spawn taskflow-bastion --model claude-sonnet-4-7
```

Output: `Agent spawned: <UUID>  role=bastion`

The daemon receives a fully-rendered `RenderedTemplate` payload: system prompt, startup message, provider, model, sandbox policy, and role — everything needed to start the agent immediately.

---

### `aegis design apply <NAME>`

Prints a `[agent.<role>]` TOML block ready to paste into `aegis.toml`. Useful for registering a template as a permanent project agent rather than a one-shot spawn.

```bash
aegis design apply taskflow-bastion
```

Output:
```toml
# Generated by: aegis design apply taskflow-bastion
[agent.bastion]
type = "bastion"
role = "bastion"
cli_provider = "claude-code"
auto_cleanup = false
[agent.bastion.sandbox]
network = "outbound_only"
```

Use `--role <NAME>` to override the role key in the output block.

---

### `aegis design new <NAME>`

Scaffolds a new project-local template at `.aegis/templates/<name>/`.

```bash
aegis design new my-reviewer --kind splinter
```

Creates three files pre-filled with stubs:
- `template.toml` — `required = ["project_root"]`, kind as specified
- `system_prompt.md` — placeholder header and TODO
- `startup.md` — TODO instruction stub

Edit these three files to define your agent. Run `aegis design show my-reviewer` to verify the result, then `aegis design spawn my-reviewer` to test it.

---

## Creating a custom template: walkthrough

```bash
# 1. Scaffold
aegis design new security-auditor --kind splinter

# 2. Edit template.toml — add required vars and sandbox settings
#    e.g. required = ["project_root", "target_crate"]

# 3. Edit system_prompt.md — describe role, tools, constraints

# 4. Edit startup.md — first instruction: what to check and how to report

# 5. Verify
aegis design show security-auditor

# 6. Spawn
aegis design spawn security-auditor --var target_crate=aegis-controller
```

Project-local templates live alongside your code in `.aegis/templates/` and are checked into git, so the whole team shares them.
