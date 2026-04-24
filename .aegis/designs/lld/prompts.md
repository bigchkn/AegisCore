# LLD — Prompts: `aegis-controller`

**Status:** Draft v0.1  
**Milestone:** M8  
**Crate:** `aegis-controller`

---

## 1. Overview

The Prompts system is responsible for resolving, rendering, and delivering task-specific and role-specific instructions to agents. It handles template variable substitution (e.g., injecting session context into a failover prompt) and manages the library of built-in and user-defined prompt templates.

## 2. Prompt Types & Resolution

| Type | Path (Project Root) | Purpose |
|---|---|---|
| **System** | `.aegis/prompts/system/<role>.md` | Behavior & constraints for a specific agent role. |
| **Task** | `.aegis/prompts/task/<task_type>.md` | Specific instructions for a discrete task (Splinters). |
| **Handoff** | `.aegis/prompts/handoff/recovery.md` | Template for resuming work after a provider failover. |
| **Resume** | `.aegis/prompts/handoff/resume.md` | Template for resuming a Bastion after a system restart. |

### 2.1 Resolution Logic

When an agent is spawned, the `PromptManager` resolves its system prompt:
1.  **Explicit Path:** Use the `system_prompt` path defined in `aegis.toml` for the agent's role.
2.  **Role File:** If 1 is missing, look for `.aegis/prompts/system/<role>.md`.
3.  **Built-in Default:** If 2 is missing, use the internal static default for the role type (Bastion/Splinter).

## 3. Template Engine

AegisCore uses a simple `{{variable}}` substitution engine. While full Handlebars is an option, a simpler regex-based or string-replacement engine is preferred to minimize dependencies unless complex logic (if/else, each) is required.

### 3.1 Supported Variables

| Variable | Description | Source |
|---|---|---|
| `{{context}}` | Scraped output or session history | `ObservationService` / `FlightRecorder` |
| `{{task}}` | Description of the current task | `TaskRegistry` |
| `{{task_id}}` | UUID of the task | `TaskRegistry` |
| `{{previous_cli}}` | The name of the CLI provider that failed | `AgentRegistry` |
| `{{worktree_path}}` | Absolute path to the agent's sandbox | `AgentRegistry` |
| `{{agent_id}}` | UUID of the receiving agent | `AgentRegistry` |
| `{{role}}` | The role string of the agent | `AgentRegistry` |

## 4. Components

### 4.1 `PromptManager`

The primary service for prompt operations.

```rust
pub struct PromptManager {
    project_root: PathBuf,
    // Storage for built-in templates (baked into binary)
}

impl PromptManager {
    /// Resolve and render a prompt for a specific agent/task context.
    pub fn resolve_prompt(
        &self, 
        template_type: PromptType, 
        context: &PromptContext
    ) -> Result<String>;
}
```

### 4.2 `PromptContext`

A container for all variables available for template substitution.

```rust
pub struct PromptContext {
    pub agent_id: Uuid,
    pub role: String,
    pub task_id: Option<Uuid>,
    pub task_description: Option<String>,
    pub context_snippet: Option<String>,
    pub worktree_path: PathBuf,
    pub previous_cli: Option<String>,
}
```

## 5. Implementation Strategy

1.  **Template Storage:**
    *   Default templates are embedded in the `aegis-controller` binary using `include_str!`.
    *   Users can override these by creating files in `.aegis/prompts/`.

2.  **Variable Resolution:**
    *   `PromptManager` queries the `AgentRegistry`, `TaskRegistry`, and `FlightRecorder` to populate the `PromptContext`.
    *   For `{{context}}`, it uses `recorder.failover_context_lines` from the config.

3.  **Handoff Prompt Generation:**
    *   When the Watchdog triggers a failover, it requests a "recovery" prompt.
    *   The `PromptManager` renders `handoff/recovery.md` with the last N lines of the failed agent's output.

## 6. Built-in Templates (Defaults)

### 6.1 `system/default.md`
```markdown
You are an AegisCore agent with the role of {{role}}.
Your workspace is located at {{worktree_path}}.
Operate autonomously and write a receipt to the handoff directory when complete.
```

### 6.2 `handoff/recovery.md`
```markdown
[SYSTEM] The previous CLI provider ({{previous_cli}}) was interrupted or hit a rate limit.
Below is the last part of the session history for context:

---
{{context}}
---

Please resume the task: {{task}}
```

## 7. Configuration Overrides

Users can specify custom prompt paths in `aegis.toml`:

```toml
[agent.architect]
system_prompt = "custom_prompts/my_architect.md"
```

If the path is relative, it is resolved against the project root.
