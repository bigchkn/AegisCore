# LLD: `aegis-providers`

**Milestone:** M4  
**Status:** done  
**HLD ref:** §6  
**Implements:** `crates/aegis-providers/`

---

## 1. Purpose

`aegis-providers` implements the `Provider` trait from `aegis-core` for each supported CLI tool. Instead of hardcoding execution logic, it uses an **internal, application-owned manifest** (embedded via `include_str!`) that defines the CLI calling conventions, flags for auto-approval/non-interactive modes, and error patterns. 

This ensures that the "shape" of how to call external CLIs is owned by AegisCore and can be thoroughly validated via tests, while the user-facing config in `aegis.toml` only handles provider selection and basic binary paths.

---

## 2. Module Structure

```
crates/aegis-providers/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-exports ProviderRegistry + all providers behind feature flags
    ├── manifest.rs         ← ProviderManifest: internal YAML/TOML parser for builtin definitions
    ├── builtin_providers.yaml ← EMBEDDED: The authoritative definitions for all CLI tools
    ├── registry.rs         ← ProviderRegistry: load from manifest + user config
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

## 3. Provider Manifest Schema

The `builtin_providers.yaml` defines the execution template for each CLI.

```yaml
providers:
  claude-code:
    binary: "claude"
    auto_approve_flags: ["--yolo"]
    non_interactive_flags: ["--non-interactive"]
    resume_flag: "--resume"
    error_patterns:
      rate_limit: ["rate limit", "429", "usage limit reached"]
      auth: ["401", "authentication failed"]

  gemini-cli:
    binary: "gemini"
    auto_approve_flags: ["--yes"]
    non_interactive_flags: []
    resume_command: "/resume {session_id}" # Injected via send-keys
    error_patterns:
      rate_limit: ["quota exceeded", "429"]
      auth: ["401", "permission denied"]
```

---

## 4. `Provider` Trait Implementation Pattern

Each provider is a zero-sized struct that references its corresponding entry from the internal manifest.

```rust
pub struct ClaudeProvider {
    manifest: ProviderDefinition,
    user_config: ProviderConfig, // Contains binary override from aegis.toml
}

impl ClaudeProvider {
    pub fn new(manifest: ProviderDefinition, user_config: ProviderConfig) -> Self {
        Self { manifest, user_config }
    }
}
```

---

## 5. Per-Provider Specifications

### 5.1 `ClaudeProvider` — `claude-code`

The `ClaudeProvider` uses the manifest to build the command. Splinters always append `auto_approve_flags`.

```rust
impl Provider for ClaudeProvider {
    fn name(&self) -> &str { "claude-code" }

    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command {
        // Use user-provided binary if set, else manifest default
        let bin = self.user_config.binary.as_ref().unwrap_or(&self.manifest.binary);
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);

        // Splinters run unattended
        cmd.args(&self.manifest.auto_approve_flags);
        cmd.args(&self.manifest.non_interactive_flags);

        if let Some(s) = session {
            if let Some(flag) = &self.manifest.resume_flag {
                cmd.arg(flag).arg(&s.session_id);
            }
        }
        cmd
    }

    fn resume_args(&self, session: &SessionRef) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(flag) = &self.manifest.resume_flag {
            args.push(flag.clone());
            args.push(session.session_id.clone());
        }
        args
    }

    fn export_context_command(&self) -> Option<&str> {
        Some("/export")
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.rate_limit.iter().any(|p| l.contains(p))
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.auth.iter().any(|p| l.contains(p))
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

```rust
impl Provider for GeminiProvider {
    fn name(&self) -> &str { "gemini-cli" }

    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let bin = self.user_config.binary.as_ref().unwrap_or(&self.manifest.binary);
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);
        
        // Unattended flags from manifest
        cmd.args(&self.manifest.auto_approve_flags);
        cmd.args(&self.manifest.non_interactive_flags);

        cmd
    }

    fn resume_args(&self, _session: &SessionRef) -> Vec<String> {
        vec![] // Gemini resume via post-spawn injection
    }

    fn export_context_command(&self) -> Option<&str> {
        Some("/checkpoint save aegis-handoff")
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.rate_limit.iter().any(|p| l.contains(p))
    }

    fn is_auth_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.auth.iter().any(|p| l.contains(p))
    }
}
```

**Gemini resume flow** (handled by controller, not provider):
1. Spawn `gemini` in the pane
2. Wait for the prompt to appear (Watchdog observation)
3. Inject `/resume last` via `TmuxClient::send_text()`

### 5.3 `CodexProvider` — `codex`

```rust
impl Provider for CodexProvider {
    fn name(&self) -> &str { "codex" }

    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let bin = self.user_config.binary.as_ref().unwrap_or(&self.manifest.binary);
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);
        // Codex is typically stateless; context is passed in the initial prompt
        cmd
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        let l = line.to_lowercase();
        self.manifest.error_patterns.rate_limit.iter().any(|p| l.contains(p))
    }
}
```

### 5.4 `OllamaProvider` — local fallback

```rust
impl Provider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }

    fn spawn_command(&self, worktree: &Path, _session: Option<&SessionRef>) -> Command {
        let bin = self.user_config.binary.as_ref().unwrap_or(&self.manifest.binary);
        let mut cmd = Command::new(bin);
        cmd.current_dir(worktree);
        
        // Ollama specific: 'run <model>'
        let model = self.user_config.model.as_deref().unwrap_or("gemma3");
        cmd.args(["run", model]);
        cmd
    }

    fn is_rate_limit_error(&self, _line: &str) -> bool { false } // Local
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

Loads the internal manifest and user overrides, providing failover cascade resolution.

```rust
pub struct ProviderRegistry {
    manifest: BuiltinManifest,
    providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Load the internal manifest and merge with user binary overrides.
    pub fn from_config(cfg: &EffectiveConfig) -> Result<Self> {
        let manifest_raw = include_str!("builtin_providers.yaml");
        let manifest: BuiltinManifest = serde_yaml::from_str(manifest_raw)?;
        // ... build providers mapping
    }
}
```

### 7.1 Registration Logic

`from_config()` iterates the **internal manifest**, creating providers while applying any user binary overrides from `cfg.providers`.

```rust
for (name, definition) in &self.manifest.providers {
    // Get user-provided config (e.g. custom binary path)
    let user_config = cfg.providers.get(name).cloned().unwrap_or_default();

    let provider: Box<dyn Provider> = match name.as_str() {
        #[cfg(feature = "claude")]
        "claude-code" => Box::new(ClaudeProvider::new(definition.clone(), user_config)),
        #[cfg(feature = "gemini")]
        "gemini-cli"  => Box::new(GeminiProvider::new(definition.clone(), user_config)),
        // ...
    };
    providers.insert(name.clone(), provider);
}
```

---

## 8. Error Pattern Matching Note

`is_rate_limit_error()` and `is_auth_error()` are called by the Watchdog on each captured pane line. These methods now iterate patterns defined in the **internal manifest** (`self.manifest.error_patterns`). The Watchdog *also* checks `watchdog.patterns` from `aegis.toml` (user-defined) — both are OR'd together.

---

## 9. Test Strategy

| Test | Asserts |
|---|---|
| `test_manifest_parsing` | `builtin_providers.yaml` parses correctly into `BuiltinManifest` |
| `test_claude_spawn_unattended` | Command includes `--yolo` and `--non-interactive` |
| `test_claude_spawn_with_resume` | `--resume <id>` present when SessionRef provided |
| `test_gemini_spawn_unattended` | Command includes `--yes` |
| `test_rate_limit_detection_from_manifest` | Matches all strings defined in the manifest for a provider |
| `test_auth_error_detection_from_manifest` | Matches all strings defined in the manifest for a provider |
| `test_registry_binary_override` | User-provided `binary` in `aegis.toml` takes precedence over manifest |
| `test_registry_cascade_ordering` | `cascade_for_agent` returns `[primary, ...cascade]` in order |
| `test_handoff_prompt_contains_context` | Rendered prompt includes terminal context and task |
