# LLD: Robust Agent Orchestration & Interaction (M37)

**Milestone:** M37  
**Status:** draft  
**HLD ref:** §4.3, §7, §10  
**Implements:** Unified provider launch, Robust TUI triggers, and Session cleanup.

---

## 1. Purpose

The current orchestration logic relies on brittle `tmux send-keys` timing which often results in agents "hanging" at initial prompts (especially Gemini) or failing to register the first command (Claude). This document defines a unified strategy for robust interaction across `claude-code`, `gemini-cli`, and others.

---

## 2. Unified Provider Interface

We will formalize the `Provider` trait to distinguish between different interaction models.

### 2.1 Trigger Mechanisms

1.  **Direct (CLI-based):** The provider accepts an initial goal/prompt as a CLI argument and enters interactive mode (e.g., `gemini -i "Begin."`). This is preferred as it is atomic.
2.  **Injected (TUI-based):** The provider boots into a TUI and requires a simulated keyboard event to start (e.g., Claude Code). This requires `startup_delay_ms` and explicit `Enter` events.

### 2.3 Updated Schema (`ProviderDefinition`)

```yaml
system_prompt:
  mechanism: "flag" | "env"
  key: "--append-system-prompt-file" | "GEMINI_SYSTEM_MD"

interaction:
  type: "direct" | "injected"
  interactive_flag: "-i"
  initial_prompt_arg: "-i"
  startup_delay_ms: 8000
```

---

## 3. Robust Dispatcher Flow

The `Dispatcher` will follow a strict state machine for launching agents:

1.  **Preparation:** Generate the System Prompt temp file and Sandbox profile.
2.  **Environment:** Inject `AEGIS_AGENT_ID`, `AEGIS_PROJECT_ROOT`, and System Prompt environment variables.
3.  **Launch Command Construction:**
    *   If `interaction.type == "direct"`: Include the `initial_prompt_arg` and trigger text (e.g., "Begin.") in the `exec` command.
    *   If `interaction.type == "injected"`: Launch the binary only.
4.  **TMUX Execution:**
    *   Set `allow-passthrough on` and `extended-keys on` for the pane.
    *   Run the generated launch script.
5.  **Post-Launch Trigger (Only for `injected` types):**
    *   Wait for `startup_delay_ms`.
    *   Use `send_raw_input` for the prompt text.
    *   Follow immediately with an explicit `send_key("Enter")`.

---

## 4. Termination & Cleanup (M36/M37 Convergence)

To prevent "Zombie Splinters" (agents that finish work but stay open):

1.  **Agent Identity:** Every splinter is launched with `AEGIS_AGENT_ID` in its env.
2.  **Explicit Exit:** The `aegis agent exit self` command is the mandatory final step in all splinter templates.
3.  **Tmux Hook:** We will explore adding a `remain-on-exit off` option to tmux windows for splinters so the pane closes automatically when the process terminates, with the Controller detecting the `pane_dead` state as a backup to the explicit exit command.

---

## 5. Localized Testing Strategy

To avoid regressions:
1.  **Mock TUI Test:** A rust-based mock TUI that requires a specific sequence of raw bytes to "succeed." The Dispatcher must be able to drive this mock TUI successfully.
2.  **Provider Matrix Test:** A unit test in `aegis-controller` that asserts the `launch_shell_command` generated for every entry in `builtin_providers.yaml` matches the expected string (checking flags, env vars, and triggers).
