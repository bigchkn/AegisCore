# LLD: `aegis-providers`

**Milestone:** M4  
**Status:** done  
**HLD ref:** §6  
**Implements:** `crates/aegis-providers/`

---

## 1. Purpose

`aegis-providers` implements the `Provider` trait from `aegis-core` for each supported CLI tool. It also provides `ProviderRegistry`, which loads provider instances from config and builds failover cascades at startup.

Each provider knows how to: spawn its CLI in a worktree, resume a previous session, detect its own error conditions, and generate a handoff prompt for the receiving CLI during failover.

---

## 2. Module Structure

```
crates/aegis-providers/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-exports ProviderRegistry + all providers behind feature flags
    ├── registry.rs         ← ProviderRegistry: load from config, resolve cascades
    ├── claude.rs           ← ClaudeProvider   (feature = "claude")
    ├── gemini.rs           ← GeminiProvider   (feature = "gemini")
    ├── codex.rs            ← CodexProvider    (feature = "codex")
    ├── ollama.rs           ← OllamaProvider   (feature = "ollama")
    └── handoff.rs          ← shared handoff prompt template rendering
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core = { path = "../aegis-core" }
tracing = "0.1"

[features]
default = ["claude", "gemini"]
claude = []
gemini = []
codex  = []
ollama = []
```

No network dependencies — providers only construct `std::process::Command`. Network calls are made by the spawned CLI process itself.

---

## 4. `Provider` Trait Implementation Pattern

Each provider is a zero-sized struct with a `ProviderConfig` captured at construction time.

```rust
pub struct ClaudeProvider {
    config: ProviderConfig,
}

impl ClaudeProvider {
    pub fn from_config(config: ProviderConfig) -> Self {
        Self { config }
    }
}
```

---

## 5. Per-Provider Specifications

### 5.1 `ClaudeProvider` — `claude-code`

| Property | Value |
|---|---|
| Default binary | `claude` |
| Session resume flag | `--resume <session_id>` |
| Context export command | `/export` (injected as send-keys text) |
| Rate limit patterns | `"rate limit"`, `"429"`, `"credit balance exhausted"`, `"usage limit reached"` |
| Auth error patterns | `"401"`, `"authentication failed"`, `"invalid api key"` |
| Task complete patterns | User-defined via `watchdog.patterns.task_complete` |

```rust
impl Provider for ClaudeProvider {
    fn name(&self) -> &str { "claude-code" }

    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command {
        let mut cmd = Command::new(&self.config.binary);
        cmd.current_dir(worktree);
        if let Some(s) = session {
            if let Some(flag) = &self.config.resume_flag {
                cmd.arg(flag).arg(&s.session_id);
            }
        }
        cmd
    }

    fn resume_args(&self, session: &SessionRef) -> Vec<String> {
        vec![
            self.config.resume_flag.clone().unwrap_or_else(|| "--resume".into()),
            session.session_id.clone(),
        ]
    }

    fn export_context_command(&self) -> Option<&str> {
        Some("/export")
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        l.contains("rate limit") || l.contains("429")
            || l.contains("credit balance exhausted")
            || l.contains("usage limit reached")
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        l.contains("401") || l.contains("authentication failed")
            || l.contains("invalid api key")
    }

    fn is_task_complete(&self, line: &str) -> bool {
        false // governed by watchdog.patterns.task_complete config; handled in watchdog
    }

    fn failover_handoff_prompt(&self, ctx: &FailoverContext) -> String {
        render_handoff_prompt(ctx) // shared template in handoff.rs
    }
}
```

### 5.2 `GeminiProvider` — `gemini-cli`

| Property | Value |
|---|---|
| Default binary | `gemini` |
| Session resume | `/resume <session_id>` injected via send-keys (not a CLI flag) |
| Context export command | `/checkpoint save aegis-handoff` |
| Rate limit patterns | `"quota exceeded"`, `"429"`, `"resource exhausted"`, `"too many requests"` |
| Auth error patterns | `"401"`, `"api key"`, `"permission denied"` |

```rust
impl Provider for GeminiProvider {
    fn resume_args(&self, _session: &SessionRef) -> Vec<String> {
        // Gemini resume is done via injected command, not CLI args.
        // The controller calls export_context_command() + send_text() instead.
        vec![]
    }

    fn export_context_command(&self) -> Option<&str> {
        Some("/checkpoint save aegis-handoff")
    }

    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let mut cmd = Command::new(&self.config.binary);
        cmd.current_dir(worktree);
        // Session resume for Gemini is injected post-spawn via send-keys, not as CLI arg.
        cmd
    }
    // ... is_rate_limit_error, is_auth_error, failover_handoff_prompt as above
}
```

**Gemini resume flow** (handled by controller, not provider):
1. Spawn `gemini` in the pane
2. Wait for the prompt to appear (Watchdog observation)
3. Inject `/resume last` via `TmuxClient::send_text()`

### 5.3 `CodexProvider` — `codex`

| Property | Value |
|---|---|
| Default binary | `codex` |
| Session resume | Project-indexed (stateless per run; context passed via initial prompt) |
| Context export command | `None` |
| Rate limit patterns | `"rate limit"`, `"429"`, `"insufficient_quota"`, `"exceeded your current quota"` |
| Auth error patterns | `"401"`, `"incorrect api key"`, `"api key"` |

### 5.4 `OllamaProvider` — local fallback

| Property | Value |
|---|---|
| Default binary | `ollama` |
| Default model | `gemma3` (from config `providers.ollama.model`) |
| Session resume | Stateless — context injected in initial prompt only |
| Context export command | `None` |
| Rate limit patterns | `[]` — local; no rate limits |
| Auth error patterns | `[]` — no auth |

```rust
impl Provider for OllamaProvider {
    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let mut cmd = Command::new(&self.config.binary);
        cmd.args(["run", self.config.model.as_deref().unwrap_or("gemma3")]);
        cmd.current_dir(worktree);
        cmd
    }

    fn is_rate_limit_error(&self, _line: &str) -> bool { false }
    fn is_auth_error(&self, _line: &str) -> bool { false }
    fn export_context_command(&self) -> Option<&str> { None }
}
```

---

## 6. `handoff.rs` — Shared Handoff Prompt Template

All providers call `render_handoff_prompt()`. The template is designed to be provider-agnostic.

```rust
pub fn render_handoff_prompt(ctx: &FailoverContext) -> String {
    format!(
        "You are resuming work from a previous agent ({previous}) that stopped unexpectedly.\n\
         \n\
         Your working directory: {worktree}\n\
         Your role: {role}\n\
         {task_section}\
         \n\
         Below is the terminal output from the previous agent (last {lines} lines). \
         Review it to understand what was completed and what remains:\n\
         \n\
         ---\n\
         {context}\n\
         ---\n\
         \n\
         Resume the task from where the previous agent left off. \
         Do not restart from scratch. Write [AEGIS:DONE] when complete.",
        previous = ctx.previous_provider,
        worktree = ctx.worktree_path.display(),
        role = ctx.role,
        task_section = ctx.task_description.as_deref()
            .map(|t| format!("Task: {t}\n"))
            .unwrap_or_default(),
        lines = ctx.terminal_context.lines().count(),
        context = ctx.terminal_context,
    )
}
```

---

## 7. `ProviderRegistry`

Loads all configured providers from `EffectiveConfig` and provides failover cascade resolution.

```rust
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Build from resolved config. Only registers providers that are
    /// compiled in (feature-gated) and present in config.
    pub fn from_config(cfg: &EffectiveConfig) -> Result<Self>;

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Result<&dyn Provider>;

    /// Build the ordered failover sequence for an agent:
    /// [primary_provider, ...fallback_cascade providers]
    /// Validates all names exist in the registry.
    pub fn cascade_for_agent(&self, agent: &AgentEntry) -> Result<Vec<&dyn Provider>>;

    /// Step to the next provider in the cascade.
    /// Returns None if already at the last provider.
    pub fn next_in_cascade<'a>(
        &'a self,
        cascade: &[&'a dyn Provider],
        current: &str,
    ) -> Option<&'a dyn Provider>;
}
```

### 7.1 Registration Logic

`from_config()` iterates `cfg.providers` and constructs the appropriate provider type based on the key name and compiled features:

```rust
for (name, entry) in &cfg.providers {
    let config = ProviderConfig {
        name: name.clone(),
        binary: entry.binary.clone(),
        resume_flag: entry.resume_flag.clone(),
        model: entry.model.clone(),
        extra_args: entry.extra_args.clone(),
    };
    let provider: Box<dyn Provider> = match name.as_str() {
        #[cfg(feature = "claude")]
        "claude-code" => Box::new(ClaudeProvider::from_config(config)),
        #[cfg(feature = "gemini")]
        "gemini-cli"  => Box::new(GeminiProvider::from_config(config)),
        #[cfg(feature = "codex")]
        "codex"       => Box::new(CodexProvider::from_config(config)),
        #[cfg(feature = "ollama")]
        "ollama"      => Box::new(OllamaProvider::from_config(config)),
        other => {
            tracing::warn!("provider `{other}` is configured but not compiled in; skipping");
            continue;
        }
    };
    providers.insert(name.clone(), provider);
}
```

---

## 8. Error Pattern Matching Note

`is_rate_limit_error()` and `is_auth_error()` are called by the Watchdog on each captured pane line. The patterns in §5 are hardcoded defaults. The Watchdog *also* checks `watchdog.patterns.rate_limit` from config (user-defined) — both are OR'd together. Provider methods handle provider-specific known strings; config handles user-added patterns.

---

## 9. Test Strategy

| Test | Asserts |
|---|---|
| `test_claude_spawn_command_no_resume` | Command has correct binary and working dir |
| `test_claude_spawn_command_with_resume` | `--resume <id>` present when SessionRef provided |
| `test_claude_rate_limit_patterns` | All known rate-limit strings match |
| `test_claude_auth_patterns` | All known auth-error strings match |
| `test_ollama_no_rate_limit` | `is_rate_limit_error` always false |
| `test_gemini_export_command` | Returns `/checkpoint save aegis-handoff` |
| `test_handoff_prompt_contains_context` | Rendered prompt includes terminal context and task |
| `test_registry_cascade_ordering` | `cascade_for_agent` returns `[primary, ...cascade]` in order |
| `test_registry_next_in_cascade` | Steps through cascade correctly; returns None at end |
| `test_registry_unknown_provider_skipped` | Unrecognized provider name logged and skipped |
