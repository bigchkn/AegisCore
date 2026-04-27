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

## 2. Proposed Solution: Structured Interaction Models

Instead of "guessing" how to start an agent, we categorize providers into two distinct **Interaction Models**.

### 2.1 Interaction Models (The "Why")

1.  **Direct Model (e.g., Gemini CLI):**
    *   **Mechanism:** Uses the `-i <prompt>` flag to start the agent with a goal already set.
    *   **Benefit:** **Zero race conditions.** The prompt is part of the process launch. No `send-keys` or `sleep` is required.
2.  **Injected Model (e.g., Claude Code):**
    *   **Mechanism:** Boots into a "Waiting for Input" state. Requires a simulated Enter key.
    *   **Benefit:** Standardizes the "Wait then Enter" sequence into a reusable workflow with explicit `startup_delay_ms` and `C-m` (Carriage Return) handling.

### 2.2 Environmental Injection (The "Where")

We will shift from CLI arguments to **Environment Variables** for core metadata.

*   **Location:** `crates/aegis-controller/src/dispatcher.rs`
*   **Change:** Instead of appending `--append-system-prompt-file` to the CLI, we will always set `AEGIS_SYSTEM_PROMPT_PATH`. Providers will be configured in `builtin_providers.yaml` to either use a flag or look for this env var.
*   **Reason:** Environment variables are safer for passing paths and long strings than CLI arguments, which can be truncated or mis-parsed by shell wrappers.

---

## 3. Location of Changes

### 3.1 `crates/aegis-core` (Data Structures)
*   **`provider.rs`**: Add `InteractionModel` enum (`Direct` | `Injected`). Add `startup_delay_ms` to `ProviderConfig`.
*   **Reason:** Formalizes the contract between the Controller and the AI Providers.

### 3.2 `crates/aegis-providers` (Manifest & YAML)
*   **`manifest.rs`**: Add fields for `interaction_model`, `interactive_flag`, and `initial_prompt_arg`.
*   **`builtin_providers.yaml`**: Define these for `claude-code` (Injected), `gemini-cli` (Direct), and `codex` (Injected).
*   **Reason:** Moves orchestration logic into configuration, allowing new AI tools to be added without code changes.

### 3.3 `crates/aegis-tmux` (Pane Hardening)
*   **`client.rs`**: Add `harden_pane(target)` method. 
*   **Implementation:** Executes `tmux set-window-option allow-passthrough on` and `tmux set-option extended-keys on`.
*   **Reason:** This is the **primary change for long-term stability**. It ensures that the TUI has direct, un-interrupted communication with the terminal, preventing "stuck" prompts or broken rendering.

### 3.4 `crates/aegis-controller` (The Orchestrator)
*   **`dispatcher.rs`**: Rewrite `launch_or_insert_plan` to use a match statement on `provider.interaction_model()`.
    *   **Case Direct:** Prepend `AEGIS_AGENT_ID=...` and append `-i "Begin."` to the launch command.
    *   **Case Injected:** Launch binary -> `harden_pane` -> `sleep` -> `send_raw_input("Begin.")` -> `send_key("Enter")`.

---

## 4. Expected Long-term Outcomes

1.  **Deterministic Startup:** Agents using the `Direct` model will succeed 100% of the time regardless of system load.
2.  **Platform Parity:** Standardizing on `C-m` and `extended-keys` ensures the orchestration works identically on Linux and macOS.
3.  **Extensibility:** Adding a local LLM via `ollama` or `llama.cpp` will only require a YAML update to specify it uses the `Direct` model with a specific CLI flag.
