# LLD: `aegis-providers`

**Milestone:** M4  
**Status:** done  
**HLD ref:** §6  
**Implements:** `crates/aegis-providers/`

---

## 1. Purpose

`aegis-providers` implements the `Provider` trait from `aegis-core` using an **internal, application-owned manifest** (embedded via `include_str!`). Instead of separate implementations per CLI, it uses a unified `GenericProvider` that is data-driven by the manifest.

This ensures that the "shape" of how to call external CLIs (flags, error patterns, resume mechanisms) is owned by AegisCore and can be thoroughly validated via tests.

---

## 2. Module Structure

```
crates/aegis-providers/
├── Cargo.toml
└── src/
    ├── lib.rs              ← re-exports ProviderRegistry
    ├── manifest.rs         ← ProviderManifest: internal YAML parser
    ├── builtin_providers.yaml ← EMBEDDED: The authoritative definitions for all CLI tools
    ├── registry.rs         ← ProviderRegistry: load from manifest + user config
    ├── generic.rs          ← GenericProvider: The single Provider trait implementation
    └── handoff.rs          ← shared handoff prompt template rendering
```

---

## 3. Provider Manifest Schema

The `builtin_providers.yaml` defines the execution template for each CLI.

```yaml
providers:
  claude-code:
    binary: "claude"
    auto_approve_flags: ["--yolo"]
    non_interactive_flags: ["--non-interactive"]
    resume_mechanism: "cli_flag"
    resume_flag: "--resume"
    export_command: "/export"
    error_patterns:
      rate_limit: ["rate limit", "429", "usage limit reached"]
      auth: ["401", "authentication failed"]

  gemini-cli:
    binary: "gemini"
    auto_approve_flags: ["--yes"]
    non_interactive_flags: []
    resume_mechanism: "injection"
    resume_command: "/resume {session_id}"
    export_command: "/checkpoint save aegis-handoff"
    error_patterns:
      rate_limit: ["quota exceeded", "429"]
      auth: ["401", "permission denied"]
```

---

## 4. `GenericProvider` Implementation

The `GenericProvider` struct implements the `Provider` trait by reading directly from its assigned `ProviderDefinition` from the manifest.

```rust
pub struct GenericProvider {
    pub definition: ProviderDefinition,
    pub user_config: ProviderConfig,
}

impl Provider for GenericProvider {
    fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>) -> Command {
        let mut cmd = Command::new(&self.user_config.binary);
        // ... appends auto_approve_flags and resume_flag if ResumeMechanism::CliFlag
        cmd
    }
    
    fn resume_args(&self, session: &SessionRef) -> Vec<String> {
        // ... returns flags if ResumeMechanism::CliFlag
    }

    fn export_context_command(&self) -> Option<&str> {
        self.definition.export_command.as_deref()
    }

    fn is_rate_limit_error(&self, line: &str) -> bool {
        self.definition.error_patterns.rate_limit.iter().any(|p| line.to_lowercase().contains(p))
    }
}
```

---

## 5. `ProviderRegistry`

Loads the internal manifest and user overrides, providing failover cascade resolution. It instantiates `GenericProvider` for every entry in the manifest.

---

## 6. Test Strategy

| Test | Asserts |
|---|---|
| `test_manifest_loading` | `builtin_providers.yaml` parses correctly |
| `test_claude_unattended_flags` | Command includes `--yolo` and `--non-interactive` |
| `test_gemini_unattended_flags` | Command includes `--yes` |
| `test_registry_binary_override` | User-provided `binary` takes precedence |
| `test_error_pattern_matching_all` | Matches strings defined in manifest for all providers |
