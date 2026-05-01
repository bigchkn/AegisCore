# LLD: Provider Nudging & Custom Behavior (M42)

**Milestone:** M42  
**Status:** draft  
**Implements:** Automated interactive recovery and loop management for specialized AI CLIs.

---

## 1. Problem Statement

Many AI CLIs (providers) do not follow a simple "one-shot" or "cleanly resumable" execution model. They often require manual intervention to continue after an interruption or to reset state between tasks in a way that the standard `--resume` flags do not handle.

**Examples:**
*   **Claude Code (`claude-code`):** Can get interrupted during long-running tasks and requires the user to type `Continue` to proceed.
*   **Dirac (`dirac`):** After completing a task, it expects the user to enter `1` (to continue/ack), wait for a prompt, and then re-provide the initial task context to continue the session effectively.

The current `dispatcher.rs` and `Provider` trait do not have a mechanism to define or execute these "nudges".

---

## 2. Proposed Solution: Nudge Mechanisms

We will introduce a `Nudge` system into the provider contract. A Nudge is a sequence of interactive actions (text injection and delays) triggered by specific lifecycle events or patterns.

### 2.1 Core Data Structures (`aegis-core`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NudgeTrigger {
    /// Triggered when the provider output has stalled for N seconds.
    Stalled { timeout_ms: u64 },
    /// Triggered immediately after a task is detected as complete (via Watchdog).
    TaskComplete,
    /// Triggered when a specific output pattern is matched in the streaming output.
    Pattern(String),
    /// Triggered when the TUI screen content matches a scraping rule.
    ScreenScrape {
        /// Regex pattern to look for on the screen.
        pattern: String,
        /// Optional: Only search within a specific rectangular region [x1, y1, x2, y2].
        /// If None, searches the entire visible screen.
        region: Option<[u16; 4]>,
        /// How often to scrape the screen (ms).
        interval_ms: u64,
    },
    /// Triggered when the screen content and cursor have been stable for N ms.
    /// Useful for detecting when a TUI is waiting for input without a clear text prompt.
    Stability { stable_ms: u64, timeout_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NudgeAction {
    /// Send literal text to the pane.
    SendText(String),
    /// Wait for a specific duration.
    Wait { duration_ms: u64 },
    /// Re-inject the initial system/task prompt.
    SendInitialPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeDefinition {
    pub trigger: NudgeTrigger,
    pub actions: Vec<NudgeAction>,
    /// Whether to repeat this nudge every time the trigger matches.
    pub repeat: bool,
}
```

### 2.2 Manifest Extensions (`aegis-providers`)

The `ProviderDefinition` in `builtin_providers.yaml` will be extended with a `nudges` list.

```yaml
  claude-code:
    # ... existing config ...
    nudges:
      - trigger:
          type: stalled
          timeout_ms: 30000
        actions:
          - type: send_text
            text: "Continue"
        repeat: true

  dirac:
    dirac:
      # ... existing config ...
      nudges:
        - trigger:
            type: screen_scrape
            pattern: "Task completed successfully"
            interval_ms: 2000
          actions:
            - type: send_text
              text: "1"
            - type: wait
              duration_ms: 1000
            - type: send_initial_prompt
          repeat: true

    ---

    ## 3. Implementation Details

    ### 3.1 `aegis-core` Refinement
    *   Add `NudgeTrigger`, `NudgeAction`, and `NudgeDefinition` to `provider.rs`.
    *   Update `Provider` trait to return `Vec<NudgeDefinition>`.

    ### 3.2 `aegis-controller` (Dispatcher & Watchdog)
    *   **Watchdog Integration:** When the Watchdog detects `is_task_complete`, it notifies the Dispatcher.
    *   **Dispatcher Loop:** The Dispatcher will maintain a "Nudge Manager" for each active session.
      *   For `Stalled` triggers: A timer resets every time new output is received from the recorder.
      *   For `ScreenScrape` triggers: Uses `TmuxClient::capture_pane_plain` periodically. If a `region` is specified, the string is sliced/processed to focus the regex match on those coordinates.
      *   For `Stability` triggers: Uses `TmuxClient::wait_for_stability` to detect rendering completion.
      *   For `TaskComplete` triggers: When notified by the Watchdog, the sequence is injected.
      *   `SendInitialPrompt` action will pull the original prompt from the `Agent` or `FailoverContext` associated with the session.


### 3.3 `aegis-tmux`
*   Ensure `send_raw_input` (or a new `send_interactive_sequence`) can handle the multi-step nudge actions reliably, ensuring the pane is ready for each step.

---

## 4. Verification Plan

1.  **Unit Tests:** Verify `NudgeDefinition` serialization/deserialization.
2.  **Mock Provider Tests:** Create a test provider that stalls or finishes tasks, and verify the Dispatcher injects the expected "nudge" strings into a mock tmux pane.
3.  **Integration Tests:**
    *   Run `claude-code` in a controlled environment, simulate a stall, and verify "Continue" is sent.
    *   Run `dirac`, mark a task as complete, and verify the `1` -> wait -> prompt sequence is executed.
