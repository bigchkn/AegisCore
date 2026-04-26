# LLD: Per-Agent Model Override

**Milestone:** M18  
**Status:** lld-done  
**Implements:** `crates/aegis-providers/`, `crates/aegis-core/`, `aegis.toml`

---

## 1. Problem

`ProviderConfig.model` and `ProviderConfig.extra_args` are stored but never applied: `GenericProvider::spawn_command` ignores both fields. Additionally, model is only configurable at the `[providers.<name>]` level — there is no way for two agents using the same provider binary to target different models.

---

## 2. Design

### 2.1 Fix `spawn_command` — apply `extra_args` and `model`

Add `model_flag: Option<String>` to `ProviderDefinition` so the manifest declares how each CLI accepts a model name. `GenericProvider::spawn_command` applies both fields.

**Argument order:**

```
<binary> [extra_args] [auto_approve_flags] [non_interactive_flags] [resume args] [--model <model>]
```

`extra_args` go first so they appear before framework flags. `model` goes last so it is easy to spot in logs.

**`builtin_providers.yaml` additions:**

```yaml
claude-code:
  model_flag: "--model"

gemini-cli:
  model_flag: "--model"

codex:
  model_flag: "--model"
```

**`ProviderDefinition` change:**

```rust
pub model_flag: Option<String>,
```

**`spawn_command` change:**

```rust
cmd.args(&self.user_config.extra_args);
cmd.args(&self.definition.auto_approve_flags);
cmd.args(&self.definition.non_interactive_flags);
// ... resume args unchanged ...
if let Some(model) = &self.user_config.model {
    if let Some(flag) = &self.definition.model_flag {
        cmd.arg(flag).arg(model);
    }
}
```

---

### 2.2 Per-agent model override in config

Add `model: Option<String>` to `RawAgentConfig` and `AgentEntry`. When an agent has a model set, it overrides the provider-level model at spawn time.

**`aegis.toml` user-facing API:**

```toml
[agent.bastion]
type = "bastion"
cli_provider = "claude-code"
model = "claude-opus-4-7"       # overrides [providers.claude-code] model

[splinter_defaults]
cli_provider = "claude-code"
model = "claude-sonnet-4-6"     # used for all splinters
```

**Data flow:**

`RawAgentConfig.model` → `AgentEntry.model` → `AgentSpec.model_override` → `build_spawn_plan` injects into a per-call `ProviderConfig` copy before calling `spawn_command`.

`build_spawn_plan` already holds the resolved `provider: &dyn Provider`. It constructs a `ProviderConfig` copy with `model` overridden:

```rust
let mut per_call_config = provider.config().clone();
if let Some(m) = &spec.model_override {
    per_call_config.model = Some(m.clone());
}
// spawn_command uses per_call_config instead of provider's stored config
```

Since `spawn_command` currently takes `&self` and reads `self.user_config`, the cleanest approach is to add an overload or pass the model separately. The preferred solution: add `model_override: Option<&str>` as a third parameter to `Provider::spawn_command`, keeping the trait change contained.

**Updated trait signature:**

```rust
fn spawn_command(&self, worktree: &Path, session: Option<&SessionRef>, model_override: Option<&str>) -> Command;
```

`GenericProvider` uses `model_override.or(self.user_config.model.as_deref())` to pick the final model.

---

### 2.3 `splinter_defaults` model field

`RawSplinterDefaults` and `SplinterDefaults` gain `model: Option<String>`. `build_splinter_spec` reads it and sets `spec.model_override`.

---

## 3. Files Changed

| File | Change |
|---|---|
| `crates/aegis-providers/src/builtin_providers.yaml` | Add `model_flag` to each provider entry |
| `crates/aegis-providers/src/manifest.rs` | Add `model_flag: Option<String>` to `ProviderDefinition` |
| `crates/aegis-providers/src/generic.rs` | Apply `extra_args` + `model` in `spawn_command`; accept `model_override` param |
| `crates/aegis-core/src/provider.rs` | Add `model_override` param to `Provider::spawn_command` trait |
| `crates/aegis-core/src/config.rs` | Add `model` to `RawAgentConfig`, `AgentEntry`, `RawSplinterDefaults`, `SplinterDefaults` |
| `crates/aegis-controller/src/lifecycle.rs` | Add `model_override` to `AgentSpec` |
| `crates/aegis-controller/src/dispatcher.rs` | Thread model through `build_splinter_spec`, `build_bastion_spec`, `build_spawn_plan` |

---

## 4. Test Strategy

| Test | Location | Asserts |
|---|---|---|
| `spawn_command_applies_extra_args` | `aegis-providers` | `extra_args` appear before auto_approve_flags |
| `spawn_command_applies_model` | `aegis-providers` | `--model <name>` appended when model + model_flag set |
| `spawn_command_model_override_takes_precedence` | `aegis-providers` | per-call override wins over provider-level model |
| `spawn_command_no_model_flag_skips_model` | `aegis-providers` | no `--model` arg when `model_flag` is None |
| `config_agent_model_parsed` | `aegis-core` | `model` field round-trips through TOML parse |
| `splinter_defaults_model_propagates` | `aegis-controller` | splinter spec carries model from `splinter_defaults` |
