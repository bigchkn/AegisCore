# LLD: Config Schema & Merge (`aegis-core` config module)

**Milestone:** M0  
**Status:** done  
**HLD ref:** §11, §16.2, §17.1  
**Implements:** `crates/aegis-core/src/config.rs` + merge logic consumed by `aegis-controller`

---

## 1. Purpose

Defines the complete configuration schema, two-layer merge semantics, validation rules, and default value catalogue. Both `~/.aegis/config` (global seed) and `<project>/aegis.toml` (project override) use this schema. The merge is a key-level overlay computed at startup.

---

## 2. Two-Layer Merge Semantics

```
Effective config = global seed ← overlaid by → project config
```

**Rules:**

| Key type | Merge behaviour |
|---|---|
| Scalar (`string`, `int`, `bool`) | Project value wins; absent project key falls back to global |
| Array | Project array **replaces** global array entirely (no concat) |
| Inline table / object | Merged recursively; project wins on leaf conflicts |
| Missing in both | Built-in Rust default (see §5) |

**Implementation:** `EffectiveConfig::resolve(global: &RawConfig, project: &RawConfig) -> EffectiveConfig`

- Deserialize both files independently into `RawConfig` (all fields `Option<T>`)
- `resolve()` walks `RawConfig` and picks the first `Some` from project then global, falling back to the built-in default

---

## 3. Rust Types

### 3.1 `RawConfig` (used for deserialization of either file)

All fields are `Option<T>` so absent keys can be detected and the merge layer can distinguish "not set" from "set to default".

```rust
// crates/aegis-core/src/config.rs

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// Deserialization target for both ~/.aegis/config and aegis.toml.
/// All fields Option to support the two-layer merge.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawConfig {
    pub global:           Option<RawGlobalConfig>,
    pub watchdog:         Option<RawWatchdogConfig>,
    pub recorder:         Option<RawRecorderConfig>,
    pub state:            Option<RawStateConfig>,
    pub http:             Option<RawHttpConfig>,
    #[serde(default)]
    pub sandbox:          RawSandboxSection,
    #[serde(default)]
    pub providers:        HashMap<String, RawProviderConfig>,
    pub telegram:         Option<RawTelegramConfig>,
    #[serde(default)]
    pub agent:            HashMap<String, RawAgentConfig>,
    pub splinter_defaults: Option<RawSplinterDefaults>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawGlobalConfig {
    pub max_splinters:      Option<u8>,
    pub tmux_session_name:  Option<String>,
    pub telegram_enabled:   Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawWatchdogConfig {
    pub poll_interval_ms:   Option<u64>,
    pub scan_lines:         Option<usize>,
    pub failover_enabled:   Option<bool>,
    pub patterns:           Option<RawWatchdogPatterns>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawWatchdogPatterns {
    pub rate_limit:         Option<Vec<String>>,
    pub auth_failure:       Option<Vec<String>>,
    pub task_complete:      Option<Vec<String>>,
    pub sandbox_violation:  Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawRecorderConfig {
    pub failover_context_lines: Option<usize>,
    pub log_rotation_max_mb:    Option<u64>,
    pub log_retention_count:    Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawStateConfig {
    pub snapshot_interval_s:      Option<u64>,
    pub snapshot_retention_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawHttpConfig {
    pub port:    Option<u16>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawSandboxSection {
    pub defaults: Option<RawSandboxPolicy>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawSandboxPolicy {
    pub network:      Option<String>,  // "none" | "outbound" | "any"
    pub extra_reads:  Option<Vec<PathBuf>>,
    pub extra_writes: Option<Vec<PathBuf>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawProviderConfig {
    pub binary:      Option<String>,
    pub resume_flag: Option<String>,
    pub model:       Option<String>,
    pub extra_args:  Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawTelegramConfig {
    pub token_env:        Option<String>,
    pub allowed_chat_ids: Option<Vec<i64>>,
    pub webhook_url:      Option<String>, // None = long-poll mode
}

#[derive(Debug, Default, Deserialize)]
pub struct RawAgentConfig {
    #[serde(rename = "type")]
    pub kind:             Option<String>,  // "bastion" | "splinter"
    pub role:             Option<String>,
    pub cli_provider:     Option<String>,
    pub fallback_cascade: Option<Vec<String>>,
    pub system_prompt:    Option<PathBuf>,
    pub sandbox:          Option<RawSandboxPolicy>,
    pub auto_cleanup:     Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
pub struct RawSplinterDefaults {
    pub cli_provider:     Option<String>,
    pub fallback_cascade: Option<Vec<String>>,
    pub auto_cleanup:     Option<bool>,
}
```

### 3.2 `EffectiveConfig` (post-merge, fully-resolved)

All fields are concrete types with defaults applied. This is what the rest of the codebase uses at runtime.

```rust
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub global:            GlobalConfig,
    pub watchdog:          WatchdogConfig,
    pub recorder:          RecorderConfig,
    pub state:             StateConfig,
    pub http:              HttpConfig,
    pub sandbox_defaults:  SandboxPolicyConfig,
    pub providers:         HashMap<String, ProviderEntry>,
    pub telegram:          TelegramConfig,
    pub agents:            HashMap<String, AgentEntry>,
    pub splinter_defaults: SplinterDefaults,
}

#[derive(Debug, Clone)]
pub struct GlobalConfig {
    pub max_splinters:     u8,       // default: 5
    pub tmux_session_name: String,   // default: "aegis"
    pub telegram_enabled:  bool,     // default: false
}

#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    pub poll_interval_ms:  u64,      // default: 2000
    pub scan_lines:        usize,    // default: 50
    pub failover_enabled:  bool,     // default: true
    pub patterns:          WatchdogPatterns,
}

#[derive(Debug, Clone)]
pub struct WatchdogPatterns {
    pub rate_limit:        Vec<String>, // default: see §5
    pub auth_failure:      Vec<String>,
    pub task_complete:     Vec<String>,
    pub sandbox_violation: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecorderConfig {
    pub failover_context_lines: usize,  // default: 100
    pub log_rotation_max_mb:    u64,    // default: 50
    pub log_retention_count:    usize,  // default: 20
}

#[derive(Debug, Clone)]
pub struct StateConfig {
    pub snapshot_interval_s:      u64,   // default: 60
    pub snapshot_retention_count: usize, // default: 10
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub port:    u16,   // default: 7437
    pub enabled: bool,  // default: true
}

#[derive(Debug, Clone)]
pub enum NetworkPolicy {
    None,
    OutboundOnly, // default
    Any,
}

#[derive(Debug, Clone)]
pub struct SandboxPolicyConfig {
    pub network:      NetworkPolicy,
    pub extra_reads:  Vec<PathBuf>,
    pub extra_writes: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ProviderEntry {
    pub binary:      String,
    pub resume_flag: Option<String>,
    pub model:       Option<String>,
    pub extra_args:  Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TelegramConfig {
    pub token_env:        String,   // default: "AEGIS_TELEGRAM_TOKEN"
    pub allowed_chat_ids: Vec<i64>, // default: []
    pub webhook_url:      Option<String>,
}

#[derive(Debug, Clone)]
pub struct AgentEntry {
    pub kind:             AgentKind,
    pub role:             String,
    pub cli_provider:     String,
    pub fallback_cascade: Vec<String>,
    pub system_prompt:    Option<PathBuf>,
    pub sandbox:          SandboxPolicyConfig,
    pub auto_cleanup:     bool,
}

#[derive(Debug, Clone)]
pub struct SplinterDefaults {
    pub cli_provider:     String,
    pub fallback_cascade: Vec<String>,
    pub auto_cleanup:     bool, // default: true
}
```

---

## 4. Config Load API

```rust
impl EffectiveConfig {
    /// Load global config from ~/.aegis/config (missing file = all defaults).
    pub fn load_global() -> Result<RawConfig>;

    /// Load project config from <project_root>/aegis.toml (missing file = error).
    pub fn load_project(project_root: &Path) -> Result<RawConfig>;

    /// Merge global and project into an EffectiveConfig, applying built-in defaults.
    pub fn resolve(global: &RawConfig, project: &RawConfig) -> Result<EffectiveConfig>;

    /// Validate the resolved config. Returns a list of all validation errors.
    /// Empty Vec = valid.
    pub fn validate(&self) -> Vec<ConfigError>;
}

/// A single validation failure. Non-fatal: all errors collected before returning.
#[derive(Debug)]
pub struct ConfigError {
    pub field: String,
    pub reason: String,
}
```

`validate()` returns `Vec<ConfigError>` (all errors at once) rather than failing on the first, so the user can fix everything in one pass.

---

## 5. Default Value Catalogue

| Key path | Type | Default |
|---|---|---|
| `global.max_splinters` | `u8` | `5` |
| `global.tmux_session_name` | `String` | `"aegis"` |
| `global.telegram_enabled` | `bool` | `false` |
| `watchdog.poll_interval_ms` | `u64` | `2000` |
| `watchdog.scan_lines` | `usize` | `50` |
| `watchdog.failover_enabled` | `bool` | `true` |
| `watchdog.patterns.rate_limit` | `Vec<String>` | `["rate limit", "429", "quota exceeded", "credit balance exhausted", "too many requests"]` |
| `watchdog.patterns.auth_failure` | `Vec<String>` | `["401", "authentication failed", "invalid api key", "unauthorized"]` |
| `watchdog.patterns.task_complete` | `Vec<String>` | `["[AEGIS:DONE]"]` |
| `watchdog.patterns.sandbox_violation` | `Vec<String>` | `["Operation not permitted"]` |
| `recorder.failover_context_lines` | `usize` | `100` |
| `recorder.log_rotation_max_mb` | `u64` | `50` |
| `recorder.log_retention_count` | `usize` | `20` |
| `state.snapshot_interval_s` | `u64` | `60` |
| `state.snapshot_retention_count` | `usize` | `10` |
| `http.port` | `u16` | `7437` |
| `http.enabled` | `bool` | `true` |
| `sandbox.defaults.network` | `NetworkPolicy` | `OutboundOnly` |
| `sandbox.defaults.extra_reads` | `Vec<PathBuf>` | `[]` |
| `sandbox.defaults.extra_writes` | `Vec<PathBuf>` | `[]` |
| `telegram.token_env` | `String` | `"AEGIS_TELEGRAM_TOKEN"` |
| `telegram.allowed_chat_ids` | `Vec<i64>` | `[]` |
| `splinter_defaults.auto_cleanup` | `bool` | `true` |

---

## 6. Validation Rules

All rules are checked by `EffectiveConfig::validate()` after merging.

| Field | Rule |
|---|---|
| `global.max_splinters` | 1 ≤ value ≤ 32 |
| `global.tmux_session_name` | Non-empty; no whitespace; valid tmux name characters |
| `watchdog.poll_interval_ms` | ≥ 500 (faster risks tmux saturation) |
| `watchdog.scan_lines` | 1 ≤ value ≤ 5000 |
| `recorder.failover_context_lines` | 10 ≤ value ≤ 1000 |
| `recorder.log_rotation_max_mb` | ≥ 1 |
| `http.port` | 1024 ≤ value ≤ 65535 |
| `providers.<name>.binary` | Non-empty string; PATH check deferred to `aegis doctor` |
| `agent.<name>.kind` | Must be `"bastion"` or `"splinter"` |
| `agent.<name>.cli_provider` | Must reference a key in `providers` |
| `agent.<name>.fallback_cascade` | Each entry must reference a key in `providers`; no duplicates; must not contain `cli_provider` itself |
| `agent.<name>.system_prompt` | If set, path must exist at runtime (checked at `aegis start`, not `validate`) |
| `telegram.allowed_chat_ids` | Required to be non-empty if `global.telegram_enabled = true` |
| `telegram.token_env` | Non-empty string |
| `splinter_defaults.cli_provider` | Must reference a key in `providers` if set |

---

## 7. Example `aegis.toml`

This is the canonical shape of a project config file as produced by `aegis init`:

```toml
# Project: my-project
# Generated by: aegis init
# Seed from: ~/.aegis/config
# Edit this file to configure agents, providers, and sandbox policy.

[global]
max_splinters = 5
tmux_session_name = "aegis"
telegram_enabled = false

[watchdog]
poll_interval_ms = 2000
scan_lines = 50
failover_enabled = true

[recorder]
failover_context_lines = 100
log_rotation_max_mb = 50
log_retention_count = 20

[state]
snapshot_interval_s = 60
snapshot_retention_count = 10

[http]
port = 7437
enabled = true

[sandbox.defaults]
network = "outbound"
extra_reads = []
extra_writes = []

[providers.claude-code]
binary = "claude"
resume_flag = "--resume"

[providers.gemini-cli]
binary = "gemini"

[providers.codex]
binary = "codex"

[providers.dirac]
binary = "dirac"

[providers.ollama]
binary = "ollama"
model = "gemma3"

[telegram]
token_env = "AEGIS_TELEGRAM_TOKEN"
allowed_chat_ids = []

[agent.architect]
type = "bastion"
role = "architect"
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "codex", "dirac", "ollama"]
# system_prompt = ".aegis/prompts/system/architect.md"

[agent.architect.sandbox]
network = "outbound"

[splinter_defaults]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "codex", "dirac", "ollama"]
auto_cleanup = true
```

---

## 8. Example `~/.aegis/config`

Global seed. Typically contains provider binaries, Telegram credentials, and personal defaults. Projects inherit these and can override.

```toml
[providers.claude-code]
binary = "claude"
resume_flag = "--resume"

[providers.gemini-cli]
binary = "gemini"

[providers.codex]
binary = "codex"

[providers.dirac]
binary = "dirac"

[providers.ollama]
binary = "ollama"
model = "gemma3"

[telegram]
token_env = "AEGIS_TELEGRAM_TOKEN"
allowed_chat_ids = [123456789]

[sandbox.defaults]
network = "outbound"

[splinter_defaults]
cli_provider = "claude-code"
fallback_cascade = ["gemini-cli", "codex", "dirac", "ollama"]
auto_cleanup = true
```

---

## 9. `config.rs` Module Location in `aegis-core`

```
crates/aegis-core/src/config.rs
```

Additional Cargo dependency required (add to `aegis-core/Cargo.toml`):

```toml
toml = "0.8"
```

The config module is the only place in `aegis-core` that touches file I/O (`load_global`, `load_project`). All other modules are pure types/traits. This is acceptable because config loading is a foundational, side-effect-free operation with no platform-specific behaviour beyond reading a file path.
