# LLD: `aegis-sandbox`

**Milestone:** M2  
**Status:** done  
**HLD ref:** §5  
**Implements:** `crates/aegis-sandbox/`

---

## 1. Purpose

`aegis-sandbox` generates per-agent macOS Seatbelt profiles (`.sb` files) and wraps process execution with `sandbox-exec`. It implements the `SandboxProfile` trait from `aegis-core`.

Every agent process launched by `aegis-controller` is prefixed with the output of `exec_prefix()` — the profile path is injected at spawn time with the agent's specific worktree path.

---

## 2. Module Structure

```
crates/aegis-sandbox/
├── Cargo.toml
└── src/
    ├── lib.rs          ← re-exports SeatbeltSandbox, ProfileVars
    ├── profile.rs      ← SeatbeltSandbox implementing SandboxProfile trait
    ├── template.rs     ← .sb template string + variable substitution
    └── error.rs        ← SandboxError
```

---

## 3. Dependencies

```toml
[dependencies]
aegis-core = { path = "../aegis-core" }
tracing = "0.1"
```

No async dependency — profile generation is synchronous file I/O.

---

## 4. Seatbelt Profile Template

### 4.1 Base Template

The canonical `.sb` template embedded in the binary. Variables use `@@NAME@@` syntax.

```scheme
; AegisCore agent sandbox profile
; Generated at spawn time — do not edit manually.
(version 1)
(deny default)

; ── Core execution ────────────────────────────────────────────────────
; Required for shell, CLI tools, and Node/Python runtimes
(allow process-exec
  (subpath "/usr/bin")
  (subpath "/usr/local/bin")
  (subpath "/opt/homebrew/bin")
  (subpath "/opt/homebrew/Cellar")
  (subpath "/bin")
  (subpath "/usr/sbin"))

(allow process-fork)
(allow signal)

; ── System reads (required or CLIs crash) ────────────────────────────
(allow file-read*
  (subpath "/usr/lib")
  (subpath "/usr/share")
  (subpath "/usr/local/lib")
  (subpath "/opt/homebrew/lib")
  (subpath "/opt/homebrew/share")
  (subpath "/System/Library")
  (subpath "/Library/Apple")
  (subpath "/private/var/folders")
  (subpath "/private/tmp")
  (subpath "/tmp")
  (literal "/dev/null")
  (literal "/dev/random")
  (literal "/dev/urandom"))

; ── Dynamic linker and runtime ────────────────────────────────────────
(allow file-read*
  (subpath "/usr/local/Cellar")
  (subpath "@@NODE_MODULES_PATH@@"))  ; e.g. /opt/homebrew/lib/node_modules

; ── Temp space (build tools, npm, pip caches) ─────────────────────────
(allow file-read* file-write*
  (subpath "/tmp")
  (subpath "/private/tmp")
  (subpath "/var/folders"))

; ── THE JAIL: agent worktree only ─────────────────────────────────────
(allow file-read* file-write*
  (subpath "@@WORKTREE_PATH@@"))

; ── Extra reads (user-configured per agent role) ──────────────────────
@@EXTRA_READS@@

; ── Extra writes (user-configured per agent role) ─────────────────────
@@EXTRA_WRITES@@

; ── Network policy ────────────────────────────────────────────────────
@@NETWORK_POLICY@@

; ── Hard denials (belt-and-suspenders, override any subpath above) ────
(deny file-read*
  (subpath "@@HOME@@/.ssh")
  (subpath "@@HOME@@/.aws")
  (subpath "@@HOME@@/.gnupg")
  (subpath "@@HOME@@/.config/1Password")
  (subpath "@@HOME@@/Library/Keychains"))

(deny file-read*
  (subpath "@@AEGIS_LOGS_DIR@@")) ; agents cannot read their own flight recorder logs
```

### 4.2 Network Policy Variants

| `SandboxNetworkPolicy` | Rendered SBPL |
|---|---|
| `None` | `(deny network*)` |
| `OutboundOnly` (default) | `(allow network-outbound)` `(deny network-inbound)` |
| `Any` | `(allow network*)` |

### 4.3 Variable Substitution

| Variable | Source |
|---|---|
| `@@WORKTREE_PATH@@` | Agent's `worktree_path` |
| `@@HOME@@` | `std::env::var("HOME")` |
| `@@AEGIS_LOGS_DIR@@` | `StorageBackend::logs_dir()` |
| `@@NODE_MODULES_PATH@@` | Fixed: `/opt/homebrew/lib/node_modules` |
| `@@EXTRA_READS@@` | Rendered list of `(allow file-read* (subpath "..."))` |
| `@@EXTRA_WRITES@@` | Rendered list of `(allow file-write* (subpath "..."))` |
| `@@NETWORK_POLICY@@` | One of the network policy snippets above |

### 4.4 Extra Read/Write Rendering

```
# SandboxPolicyConfig.extra_reads = ["/usr/local/share/zsh"]
# renders as:
(allow file-read*
  (subpath "/usr/local/share/zsh"))
```

One `(allow ...)` block per configured path. Empty list = no block emitted.

---

## 5. `profile.rs` — `SeatbeltSandbox`

```rust
use aegis_core::{SandboxPolicy, SandboxProfile, Result};
use std::path::{Path, PathBuf};

pub struct SeatbeltSandbox {
    template: &'static str, // embedded template string
}

impl SeatbeltSandbox {
    pub fn new() -> Self {
        Self { template: include_str!("../templates/agent_jail.sb") }
    }
}

impl SandboxProfile for SeatbeltSandbox {
    fn render(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy) -> Result<String>;
    fn write(&self, worktree: &Path, home: &Path, policy: &SandboxPolicy, dest: &Path) -> Result<()>;
    fn exec_prefix(&self, profile_path: &Path) -> Vec<String>;
}
```

### 5.1 `render()` Implementation

1. Start with the embedded template string
2. Substitute all `@@VARIABLE@@` tokens in one pass
3. Render `@@EXTRA_READS@@` from `policy.extra_reads` (empty = remove line)
4. Render `@@EXTRA_WRITES@@` from `policy.extra_writes`
5. Render `@@NETWORK_POLICY@@` from `policy.network` variant
6. Render `@@HOME@@` hard denial paths
7. Return the rendered string

### 5.2 `write()` Implementation

1. Call `render()` to get the profile string
2. Create parent directory if absent (`std::fs::create_dir_all`)
3. Write atomically: write to `<dest>.tmp`, then `fs::rename` to `dest`
4. Set file permissions to `0o600`

### 5.3 `exec_prefix()` Implementation

```rust
fn exec_prefix(&self, profile_path: &Path) -> Vec<String> {
    vec![
        "sandbox-exec".to_string(),
        "-f".to_string(),
        profile_path.to_string_lossy().into_owned(),
    ]
}
```

The controller prepends these to the CLI command when spawning an agent process.

---

## 6. `ProfileVars` — Helper Struct

Used internally to collect substitution values before rendering:

```rust
pub struct ProfileVars {
    pub worktree_path: PathBuf,
    pub home: PathBuf,
    pub aegis_logs_dir: PathBuf,
    pub policy: SandboxPolicy,
}
```

---

## 7. `error.rs`

```rust
use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("template variable not found: `{var}`")]
    TemplateVar { var: String },

    #[error("profile write failed at {path}: {source}")]
    WriteError { path: PathBuf, #[source] source: io::Error },

    #[error("non-UTF-8 path cannot be used in sandbox profile: {path:?}")]
    NonUtf8Path { path: PathBuf },

    #[error("sandbox-exec binary not found on PATH")]
    SandboxExecNotFound,
}

impl From<SandboxError> for aegis_core::AegisError {
    fn from(e: SandboxError) -> Self {
        match e {
            SandboxError::WriteError { path, source } =>
                AegisError::StorageIo { path, source },
            other => AegisError::SandboxProfileRender { reason: other.to_string() },
        }
    }
}
```

---

## 8. Template File Location

```
crates/aegis-sandbox/
└── templates/
    └── agent_jail.sb    ← embedded via include_str!()
```

The template is compiled into the binary. No runtime file read required.

---

## 9. Per-Provider System Path Requirements

Some CLI tools need additional read access beyond the defaults. These are applied by the controller when calling `render()` for a specific provider — the controller merges the agent's `sandbox.extra_reads` config with these hardcoded provider requirements.

| Provider | Additional paths required |
|---|---|
| `claude-code` | `~/.claude/` (session storage) |
| `gemini-cli` | `~/.gemini/` (session + checkpoint storage) |
| `codex` | `~/.codex/` |
| `ollama` | `~/.ollama/` (model weights) |

These are defaults applied by the controller's `ProviderRegistry` — they can be suppressed or extended via `agent.<name>.sandbox.extra_reads` in config.

---

## 10. Violation Detection

Sandbox violations are surface-level detected by the Watchdog, which scans captured pane output for:

```
Operation not permitted
```

The sandbox module itself does not detect violations — it only generates profiles. The Watchdog's `patterns.sandbox_violation` config key governs detection (default: `["Operation not permitted"]`).

---

## 11. Integration Test Strategy

Tests run on macOS only (gated via `#[cfg(target_os = "macos")]`).

| Test | Asserts |
|---|---|
| `test_render_contains_worktree` | Rendered profile contains the worktree subpath rule |
| `test_render_outbound_only` | OutboundOnly policy renders correct SBPL network lines |
| `test_render_no_network` | None policy renders `(deny network*)` |
| `test_render_extra_reads` | Extra read paths appear in rendered profile |
| `test_hard_deny_ssh` | `~/.ssh` deny rule always present regardless of policy |
| `test_write_atomic` | Write creates file at expected path; `.tmp` file not left behind |
| `test_exec_prefix` | Returns `["sandbox-exec", "-f", "<path>"]` |
| `test_file_access_denied_outside_worktree` | Run a subprocess under `sandbox-exec` with a test profile; verify read of `/etc/passwd` fails |
