# HLD: Agent Design & Template System

**Status:** Draft v0.2 — Pending Review  
**Milestones:** M19 (design) · M20 (template engine) · M21 (bootstrap integration) · M22 (taskflow suite)  
**Platform:** macOS (Darwin / Apple Silicon primary)  
**Implementation Language:** Rust

---

## 1. Purpose & Motivation

AegisCore today can spawn agents, but there is no way to encode *how an agent should behave* in a reusable, portable form. A user standing up a new project must hand-write `aegis.toml` config entries and manually compose system prompts. The two pieces are also disconnected: the config governs mechanics (provider, sandbox), while prompts govern behaviour — but they are authored separately and have no shared schema.

The **Agent Design** feature introduces a **Template System** that packages mechanics and behaviour together into a single, shareable unit. A template encodes everything needed to spawn a purposeful agent: provider defaults, model, sandbox policy, role, and a pre-written system prompt with context-sensitive variable substitution.

The first and primary target is **Taskflow Templates**: built-in templates that bootstrap a Bastion and Splinter pair capable of driving the AegisCore taskflow lifecycle autonomously — reading milestones, delegating implementation tasks to Splinters, monitoring completion, and advancing the roadmap — without user hand-holding after initial spawn.

---

## 2. Core Concepts

| Concept | Definition |
|---|---|
| **Template** | A directory bundle containing metadata (`template.toml`) and prose prompts (`.md` files). It is a reusable blueprint for a class of agent. |
| **Design** | The act of selecting a template, binding it to a specific project context, and resolving all variables into a concrete `AgentSpec`. |
| **Bootstrap** | Executing a Design — either writing config to `aegis.toml` (deferred spawn) or directly spawning a live agent (immediate spawn). |
| **Template Registry** | The discovery and loading service for all templates: built-in, global user, and project-local. |
| **DesignEngine** | The rendering service that takes a template + context and produces a fully resolved prompt and agent configuration. |

---

## 3. Template Model

### 3.1 Directory Structure

A template is a directory with a fixed layout:

```
<template-name>/
├── template.toml       ← required: metadata + agent config defaults
├── system_prompt.md    ← required: the agent's system prompt (injected at spawn)
└── startup.md          ← optional: first message sent after the agent is live
```

**`template.toml` schema:**

```toml
[template]
name = "taskflow-bastion"
description = "Coordinates an AegisCore milestone: reads the taskflow, delegates tasks to Splinters, monitors completion."
kind = "bastion"         # "bastion" | "splinter"
version = "1"
tags = ["taskflow", "coordinator"]

[agent]
role = "taskflow-coordinator"
cli_provider = "claude-code"
model = "claude-opus-4-7"
auto_cleanup = false
fallback_cascade = ["gemini-cli"]

[agent.sandbox]
network = "outbound_only"

[variables]
# Declares the variables this template expects at render time.
# Values are provided by the bootstrap context; missing required vars
# cause an error at design time, not spawn time.
required = ["project_root", "milestone_id"]
optional = ["lld_path", "task_id", "task_description", "bastion_agent_id"]
```

For `taskflow-splinter`, `bastion_agent_id` is **required** — the splinter must know where to send completion notifications.

```toml
# taskflow-splinter/template.toml
[variables]
required = ["project_root", "task_description", "bastion_agent_id"]
optional = ["lld_path", "task_id", "milestone_id"]
```

### 3.2 Variable System

Templates use `{{variable}}` substitution in all prose files (`system_prompt.md`, `startup.md`).

**Standard variables (always available):**

| Variable | Source |
|---|---|
| `{{project_root}}` | `ProjectAnchor` |
| `{{agent_id}}` | Generated at spawn |
| `{{worktree_path}}` | Dispatcher |
| `{{role}}` | Template `agent.role` |
| `{{provider}}` | Template `agent.cli_provider` |
| `{{bastion_agent_id}}` | Set by `DesignEngine` when spawning a splinter from a bastion context; empty string for top-level spawns |

**Taskflow variables (available when milestone context is bound):**

| Variable | Source |
|---|---|
| `{{milestone_id}}` | Bootstrap call or current milestone from `index.toml` |
| `{{milestone_name}}` | Taskflow index |
| `{{lld_path}}` | Milestone `lld` field |
| `{{task_id}}` | Specific task being delegated (splinter context) |
| `{{task_description}}` | Task text |

**Resolution:** The `DesignEngine` populates a `TemplateContext` struct from all available sources, renders templates using a simple string-replace pass, then validates that no `{{...}}` placeholders remain. Unresolved required variables are a hard error; unresolved optional variables are replaced with an empty string.

### 3.3 Template Kinds

| Kind | Agent Type | Worktree | Lifetime |
|---|---|---|---|
| `bastion` | `AgentKind::Bastion` | Project root | Long-lived |
| `splinter` | `AgentKind::Splinter` | Dedicated git worktree | Task-scoped |

### 3.4 Built-in vs User Templates

| Layer | Location | Priority |
|---|---|---|
| Project-local | `.aegis/templates/<name>/` | Highest |
| Global user | `~/.aegis/templates/<name>/` | Middle |
| Built-in | Embedded in binary (`include_dir!`) | Lowest |

Built-in templates ship as part of the `aegis-design` crate. They are always available and cannot be deleted, but any layer above can shadow them by providing a template with the same name.

---

## 4. Template Storage & Discovery

The `TemplateRegistry` discovers templates at load time by scanning the three layers in reverse priority order, building a map of `name → TemplateSource`. Later entries (higher priority layers) overwrite earlier ones.

```
TemplateRegistry::load(project_root: &Path) → Self
  1. Scan built-in templates (embedded dir)
  2. Scan ~/.aegis/templates/ (if exists)
  3. Scan .aegis/templates/ (if exists)
  → HashMap<String, ResolvedTemplate>
```

`ResolvedTemplate` carries the raw file contents plus the source layer tag (`BuiltIn | Global | Project`), used for display in `aegis design list`.

---

## 5. Bootstrap Flow

```
User: aegis design spawn taskflow-bastion [--milestone 13]
              │
              ▼
   TemplateRegistry::load(project_root)
              │
              ▼
   template = registry.get("taskflow-bastion")?
              │
              ▼
   ctx = BootstrapContext::build(template, cli_args, taskflow_state)
   // resolves milestone_id, milestone_name, lld_path from index.toml
              │
              ▼
   rendered = DesignEngine::render(template, ctx)?
   // substitutes all {{vars}}, validates no placeholders remain
              │
              ▼
   spec = AgentSpec::from_rendered(&rendered)
   // kind, role, provider, model, sandbox from template.toml
   // system_prompt = rendered.system_prompt
   // initial_message = rendered.startup (if present)
              │
              ▼
   Dispatcher::spawn_from_spec(spec) → Agent
              │
              ▼
   // After tmux window is live, inject rendered.startup via send-keys
   // (replaces the generic taskflow_snippet currently hardcoded in build_spawn_plan)
```

**Deferred spawn (`aegis design apply`):** Instead of calling `Dispatcher::spawn_from_spec`, the rendered `AgentSpec` is serialised back to a `[agent.<role>]` TOML block and merged into the project's `aegis.toml`. The user can then `aegis start` at their discretion.

---

## 6. CLI Surface

All commands live under `aegis design`:

```
aegis design list
    Lists all available templates from all layers.
    Columns: NAME, KIND, LAYER (built-in/global/project), DESCRIPTION

aegis design show <name>
    Prints the resolved template: metadata, variables, system_prompt preview.

aegis design spawn <name> [--milestone <id>] [--model <model>] [--var KEY=VALUE ...]
    Immediately spawns an agent from the template.
    --milestone  Binds taskflow milestone context (auto-reads from index.toml if omitted).
    --model      Overrides template's default model.
    --var        Arbitrary variable injection for custom templates.

aegis design apply <name> [--role <role>] [--var KEY=VALUE ...]
    Writes the template as a named [agent.<role>] block in aegis.toml.
    Does not spawn — use `aegis start --bastion <role>` afterward.

aegis design new <name> [--kind bastion|splinter]
    Scaffolds a blank template directory at .aegis/templates/<name>/.
    Creates template.toml and system_prompt.md with commented placeholders.
```

---

## 7. Taskflow Template Suite

This is the primary built-in template set. It implements an autonomous taskflow coordinator pattern using only the existing `aegis` CLI tools available to agents inside their sandbox.

### 7.1 `taskflow-bastion` — Milestone Coordinator

**Purpose:** Drives one milestone to completion. Reads the milestone, reads its LLD, delegates each task to a Splinter, monitors execution, and advances task statuses.

**Startup sequence (encoded in `startup.md`):**
```
1. Run `aegis taskflow status` to orient yourself.
2. Run `aegis taskflow show {{milestone_id}}` to load the milestone.
3. Read the LLD at {{lld_path}} for full technical context.
4. For each pending task:
   a. Run `aegis spawn "<task description from LLD>"` to create a splinter.
      Note the returned task UUID.
   b. Run `aegis taskflow assign {{milestone_id}}.<task_id> <uuid>` to link
      the roadmap task to the runtime task.
   c. Send rich context to the splinter:
      `aegis message send <splinter_agent_id> task '{"lld_path":"{{lld_path}}","task_id":"<task_id>","acceptance_criteria":"..."}'`
5. Check inbox for splinter completions:
   `aegis message inbox`
   When a splinter reports done, run `aegis taskflow sync` to update the roadmap.
6. Repeat step 5 until all tasks are complete, then report milestone done.
```

**System prompt focus:** Project coordinator, delegator, non-implementer. The bastion never writes code directly — it plans, delegates, and verifies.

### 7.2 `taskflow-splinter` — Task Implementer

**Purpose:** Implements one specific taskflow task. Reads the relevant LLD section, performs the implementation, ensures tests pass, and signals completion.

**Startup sequence (encoded in `startup.md`):**
```
Your task: {{task_description}}
LLD context: {{lld_path}} (read this for design decisions and constraints)

1. Check your inbox for additional context from the coordinator:
   `aegis message inbox`
2. Read the LLD section relevant to your task.
3. Implement the task as described.
4. Run tests: `cargo test -p <crate>` (check LLD for which crate).
5. When complete, notify your coordinator:
   `aegis message send {{bastion_agent_id}} notification '{"status":"done","task_description":"{{task_description}}","summary":"<one line of what changed>"}'`
```

**System prompt focus:** Implementer with strong read-only context access. Writes only within its assigned worktree.

### 7.3 Coordination Protocol

The bastion and splinters coordinate through the M23 agent-to-agent messaging layer. The controller mediates all message delivery — no direct tmux peer-to-peer links are used.

```
Bastion                                    Splinter(s)
  │                                              │
  │  aegis spawn "implement task X"              │
  ├─────────────────────────────────────────────►│ spawned; bastion_agent_id injected
  │                                              │
  │  aegis taskflow assign M{{milestone_id}}.X <uuid>  │
  │  (links roadmap task ↔ runtime task)         │
  │                                              │
  │  aegis message send <splinter_id> task {...} │
  ├─────────────────────────────────────────────►│ receives rich context in inbox
  │                                              │
  │                                              │  implements task
  │                                              │  runs tests
  │                                              │
  │◄── aegis message send {{bastion_agent_id}} ──┤ sends notification{status:done}
  │        notification {...}                    │
  │                                              │
  │  aegis message inbox  (reads completion)     │
  │  aegis taskflow sync                         │
  │  → roadmap task marked done                  │
```

Message delivery is durable: the notification JSON is written atomically to the bastion's mailbox before any tmux nudge is attempted. If the bastion is temporarily inactive, the message queues and is read on the next `aegis message inbox` call.

This pattern composes naturally for multi-bastion scenarios: an architect bastion can send a `handoff` message to a reviewer bastion with implementation context, and the reviewer replies with a `notification` containing findings — all through the same `message.send` / `message.inbox` path.

### 7.4 Variable Binding at Spawn

When the user runs `aegis design spawn taskflow-bastion`, the `BootstrapContext` auto-reads the current milestone from `.aegis/designs/roadmap/index.toml` (`current_milestone` field). The LLD path is resolved from the milestone's TOML file (`lld` field). Both are injected into the rendered startup prompt automatically — the user only needs to invoke the command.

When the bastion subsequently spawns a splinter (via `aegis spawn`), the `DesignEngine` is called again with `spawn_from_template("taskflow-splinter", ...)`. At that point `{{bastion_agent_id}}` is populated with the live bastion's `agent_id` from the registry. This is the only variable that cannot be known at user-invocation time and must be resolved dynamically at splinter spawn time.

---

## 8. Integration with Existing Systems

### 8.1 PromptManager

The `DesignEngine` supersedes the hardcoded `taskflow_snippet` injected in `Dispatcher::build_spawn_plan` (currently appended to every prompt unconditionally). After this feature:

- If the agent was spawned from a template, its startup instructions come entirely from `startup.md`.
- The `taskflow_snippet` injection is removed from `build_spawn_plan`.
- Agents not using a template fall back to the existing `PromptManager` built-in defaults.

### 8.2 Dispatcher

`Dispatcher` gains a new entry point:

```rust
pub async fn spawn_from_template(
    &self,
    template: &RenderedTemplate,
) -> Result<Agent>
```

Internally this calls `build_spawn_plan` with a constructed `AgentSpec` then, after tmux launch, sends `template.startup` as the first message via `send_keys`.

The existing `spawn_bastion(name: &str)` path continues to work for config-driven agents (non-template).

### 8.3 Config (`aegis.toml`)

`aegis design apply` emits valid TOML that merges cleanly with `aegis.toml`. The schema is compatible with existing `[agent.<name>]` config — no new fields required in `EffectiveConfig`. The only new `aegis.toml` field is an optional `template` pointer for documentation/reference:

```toml
[agent.taskflow-coordinator]
# Generated by: aegis design apply taskflow-bastion
# Template: built-in/taskflow-bastion v1
type = "bastion"
role = "taskflow-coordinator"
cli_provider = "claude-code"
model = "claude-opus-4-7"
```

### 8.4 Sandbox

Template `[agent.sandbox]` overrides use the existing `SandboxPolicyConfig` schema. No new sandbox fields are needed. Templates declaring `network = "outbound_only"` produce the same profile as a manually configured agent with that policy.

---

## 9. New Crate: `aegis-design`

The template engine is isolated in a new crate to keep the boundary clear:

```
crates/aegis-design/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-exports TemplateRegistry, DesignEngine, RenderedTemplate
    ├── template.rs         ← Template, TemplateMetadata, TemplateKind schema types
    ├── registry.rs         ← TemplateRegistry: load from all layers
    ├── engine.rs           ← DesignEngine: render, validate, produce RenderedTemplate
    ├── context.rs          ← BootstrapContext: variable resolution from project state
    └── builtin/            ← embedded built-in templates (include_dir!)
        ├── taskflow-bastion/
        │   ├── template.toml
        │   ├── system_prompt.md
        │   └── startup.md
        └── taskflow-splinter/
            ├── template.toml
            ├── system_prompt.md
            └── startup.md
```

`aegis-design` depends on `aegis-core` (for `AgentSpec`, `AgentKind`, `SandboxPolicyConfig`). It does **not** depend on `aegis-controller` — the controller depends on `aegis-design`, not the reverse.

---

## 10. LLD Candidates

| LLD | File | Milestone | Covers |
|---|---|---|---|
| Template Engine | `lld/agent-design-engine.md` | M20 | `aegis-design` crate: types, registry, rendering engine, variable resolution, `include_dir!` embedding |
| Bootstrap Integration | `lld/agent-design-bootstrap.md` | M21 | `aegis design` CLI subcommands, `Dispatcher::spawn_from_template`, `apply` TOML emission, removal of hardcoded `taskflow_snippet` |
| Taskflow Template Suite | `lld/agent-design-taskflow.md` | M22 | Built-in template content, startup protocol, variable binding from `index.toml`, messaging-based coordination protocol, tests. **Depends on M23.** |

---

## 11. Design Decisions & Trade-offs

| Decision | Choice | Rationale |
|---|---|---|
| Template format | Directory (`.toml` + `.md`) | Long prompts are unreadable in TOML multi-line strings. Markdown is the natural format for prose that agents themselves will read and write. |
| Variable syntax | `{{var}}` string replace | Consistent with existing `PromptManager`. Keeps the engine zero-dependency (no templating library needed). |
| New crate | `aegis-design` | Prevents a circular dep (controller→design, not design→controller). Keeps template logic independently testable. |
| Built-in embedding | `include_dir!` macro | Consistent with how `builtin_providers.yaml` is embedded. Single binary, no install-time file placement. |
| Bootstrap reads `index.toml` | Auto | The common case is "drive the current milestone". Manual `--milestone` override available for edge cases. |
| Hardcoded snippet removal | Replace entirely | The `taskflow_snippet` injected today is blunt and appended to every agent regardless of intent. Template-based startup is more precise and user-controllable. |
| Completion signalling | M23 messaging (`aegis message send`) | Durable, structured, and independent of watchdog poll timing. `[AEGIS:DONE]` string matching is fragile — it evaporates from the pane once the agent moves on. Messaging delivers a persistent JSON notification to the bastion's mailbox regardless of whether the bastion is active at that moment. |
| Rich task delegation | Two-step: spawn + message | `aegis spawn` creates the agent; a follow-up `aegis message send task {...}` delivers LLD context, acceptance criteria, and any prior context that won't fit in a shell argument. |
| `{{bastion_agent_id}}` resolution | Dynamic at splinter spawn | This variable cannot be known at user invocation time. The `DesignEngine` resolves it from the registry at the moment the bastion calls `aegis design spawn taskflow-splinter`, not when the user first boots the bastion. |
