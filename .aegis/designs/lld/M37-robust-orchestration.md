# LLD: Robust Agent Orchestration & Interaction (M37) - REFINED

**Milestone:** M37  
**Status:** draft  
**Implements:** Interaction Models, Environmental Injection, and Tmux Pane Hardening.

---

## 1. Problem Statement

The current orchestration logic in `dispatcher.rs` is **procedural and brittle**. It treats all AI CLIs as generic "black boxes" and attempts to drive them using hardcoded `sleep` delays and `tmux send-keys` events.

**Long-term issues with current approach:**
*   **Race Conditions:** If a system is under load, the 8s `startup_delay_ms` might be too short, causing the trigger ("Begin.") to be sent before the TUI is listening.
*   **Quoting Hell:** Passing complex system prompts and triggers via shell arguments leads to escaping issues across different OSs/shells.
*   **TUI Conflicts:** Tmux often intercepts the very escape sequences (like `modifyOtherKeys`) that modern TUIs like Claude Code or Gemini CLI use to provide a rich experience.

---

## 2. Proposed Solution: Managed Interactive Sessions

The tmux layer should own the session lifecycle. The child CLI should not discover tmux; the controller creates the session, records the pane identity, and decides when input is safe to send.

### 2.1 Interaction Models (The "Why")

1.  **Interactive Gemini Session:**
    *   **Mechanism:** Gemini stays in its TUI and receives the first goal only after the controller confirms the pane is stable.
    *   **Benefit:** Gemini remains interactive for follow-up prompts, clarifications, and resume flow without switching to a headless mode.
    *   **Input Rule:** Prefer human-like typed input with explicit clear-line handling and an Enter key at the end. Avoid raw paste for the first prompt when the pane is still settling.
2.  **Injected TUI Session (e.g., Claude Code):**
    *   **Mechanism:** Boots into a waiting-for-input state and receives a normalized trigger after `startup_delay_ms`.
    *   **Benefit:** Keeps the existing `Begin.` / `Continue.` flow, but makes it explicit that the controller, not the provider, decides when to submit.

### 2.2 Prompt Delivery (The "Where")

Prompt transport is already split in the codebase and should stay that way:

*   `crates/aegis-core/src/provider.rs` defines `SystemPromptMechanism`, `InteractionModel`, `SessionRef`, and `ProviderConfig`.
*   `crates/aegis-providers/src/manifest.rs` and `builtin_providers.yaml` define per-provider interactive flags, initial prompt arguments, resume behavior, and prompt injection mechanism.
*   `crates/aegis-controller/src/dispatcher.rs` uses those provider capabilities to build the launch command.

For M37, the refinement is not "always use env vars" or "always use flags". It is:

*   Gemini should use the provider-defined interactive path and remain in the TUI.
*   Claude should continue using the injected prompt-file path.
*   The controller should normalize the prompt string before submission and choose the right launch shape from provider metadata.

---

## 3. Location of Changes

### 3.1 `crates/aegis-core` (Data Structures)
*   **`provider.rs`**: Keep `SystemPromptMechanism`, `InteractionModel`, `SessionRef`, and `ProviderConfig` as the provider contract surface.
*   **Reason:** This is where the controller learns whether a provider is interactive, how to inject prompts, and how to resume.

### 3.2 `crates/aegis-providers` (Manifest & YAML)
*   **`manifest.rs`**: Keep the provider manifest fields for `system_prompt_mechanism`, `interactive_flag`, `initial_prompt_arg`, and `resume_mechanism`.
*   **`builtin_providers.yaml`**: Gemini should stay interactive, with `interactive_flag` / `initial_prompt_arg` set for its TUI flow; Claude keeps the injected prompt-file path.
*   **Reason:** The provider manifest is the source of truth for launch behavior, so the controller does not need provider-specific branching sprinkled throughout the orchestration code.

### 3.3 `crates/aegis-tmux` (Pane Hardening)
*   **`client.rs`**: Add the missing tmux helpers needed for safe interactive Gemini input:
    *   pane stability polling using `capture_pane_plain` plus cursor state;
    *   serialized send helper for one writer at a time;
    *   `harden_pane(target)` for tmux options that improve TUI passthrough.
*   **Reason:** The current client already has `new_session`, `kill_session`, `list_panes`, `send_raw_input`, `send_key`, `capture_pane_plain`, and `pane_has_agent`. What it does not yet have is a stability-aware send path.

### 3.4 `crates/aegis-controller` (The Orchestrator)
*   **`dispatcher.rs`**: Refine `launch_or_insert_plan`, `spawn_bastion`, `relaunch_bastion_in_place`, and failover relaunch so they all use the same managed-session tmux flow.
    *   Gemini path: create or reuse the controller-owned session, wait for tmux stability, harden the pane, then type the initial prompt interactively.
    *   Claude path: keep the injected trigger flow, but only submit after the startup delay and with normalized prompt text.
    *   Session replacement path: if the pane is stale or in a terminal modal, kill and recreate the tmux session, update `tmux_window` / `tmux_pane`, and reattach the recorder before launching.

### 3.5 `crates/aegis-controller/src/runtime/mod.rs` (Lifecycle)
*   **`start_background_tasks`**: keep the watchdog and scheduler running under controller ownership.
*   **`shutdown`**: stop background tasks and terminate agent sessions cleanly.
*   **Reason:** tmux session management belongs to the runtime boundary, not the agent binary.

---

## 4. Expected Long-term Outcomes

1.  **Deterministic Startup:** Gemini stays interactive, but the first prompt is only sent once the pane is stable enough to accept it.
2.  **Controller-Owned Sessions:** Session and pane identity stay in the registry, so restarts and failovers can adopt the same tmux resources instead of rediscovering them.
3.  **Extensibility:** New providers only need manifest updates for prompt injection, resume style, and interactive behavior; the tmux safety model stays in one place.
