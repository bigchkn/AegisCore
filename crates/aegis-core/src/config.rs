use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::{Path, PathBuf}};
use crate::agent::AgentKind;
use crate::error::{AegisError, Result};

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

// --- Effective Config (Resolved) ---

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct GlobalConfig {
    pub max_splinters:     u8,
    pub tmux_session_name: String,
    pub telegram_enabled:  bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WatchdogConfig {
    pub poll_interval_ms:  u64,
    pub scan_lines:        usize,
    pub failover_enabled:  bool,
    pub patterns:          WatchdogPatterns,
}

#[derive(Debug, Clone, Serialize)]
pub struct WatchdogPatterns {
    pub rate_limit:        Vec<String>,
    pub auth_failure:      Vec<String>,
    pub task_complete:     Vec<String>,
    pub sandbox_violation: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecorderConfig {
    pub failover_context_lines: usize,
    pub log_rotation_max_mb:    u64,
    pub log_retention_count:    usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StateConfig {
    pub snapshot_interval_s:      u64,
    pub snapshot_retention_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpConfig {
    pub port:    u16,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    None,
    OutboundOnly,
    Any,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxPolicyConfig {
    pub network:      NetworkPolicy,
    pub extra_reads:  Vec<PathBuf>,
    pub extra_writes: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderEntry {
    pub binary:      String,
    pub resume_flag: Option<String>,
    pub model:       Option<String>,
    pub extra_args:  Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TelegramConfig {
    pub token_env:        String,
    pub allowed_chat_ids: Vec<i64>,
    pub webhook_url:      Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentEntry {
    pub kind:             AgentKind,
    pub role:             String,
    pub cli_provider:     String,
    pub fallback_cascade: Vec<String>,
    pub system_prompt:    Option<PathBuf>,
    pub sandbox:          SandboxPolicyConfig,
    pub auto_cleanup:     bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SplinterDefaults {
    pub cli_provider:     String,
    pub fallback_cascade: Vec<String>,
    pub auto_cleanup:     bool,
}

/// A single validation failure.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigError {
    pub field: String,
    pub reason: String,
}

impl EffectiveConfig {
    /// Load global config from ~/.aegis/config (missing file = all defaults).
    pub fn load_global() -> Result<RawConfig> {
        let home = std::env::var("HOME").map_err(|_| AegisError::Config {
            field: "HOME".into(),
            reason: "HOME environment variable not set".into(),
        })?;
        let path = Path::new(&home).join(".aegis").join("config");
        if !path.exists() {
            return Ok(RawConfig::default());
        }
        let content = std::fs::read_to_string(&path).map_err(|e| AegisError::Io(e))?;
        toml::from_str(&content).map_err(|e| AegisError::Config {
            field: "global_config".into(),
            reason: e.to_string(),
        })
    }

    /// Load project config from <project_root>/aegis.toml (missing file = error).
    pub fn load_project(project_root: &Path) -> Result<RawConfig> {
        let path = project_root.join("aegis.toml");
        if !path.exists() {
            return Err(AegisError::Config {
                field: "project_config".into(),
                reason: format!("aegis.toml not found in {}", project_root.display()),
            });
        }
        let content = std::fs::read_to_string(&path).map_err(|e| AegisError::Io(e))?;
        toml::from_str(&content).map_err(|e| AegisError::Config {
            field: "project_config".into(),
            reason: e.to_string(),
        })
    }

    pub fn resolve(global: &RawConfig, project: &RawConfig) -> Result<Self> {
        // Helper to pick project then global then default
        macro_rules! merge {
            ($section:ident, $field:ident, $default:expr) => {
                project.$section.as_ref().and_then(|s| s.$field.clone())
                    .or_else(|| global.$section.as_ref().and_then(|s| s.$field.clone()))
                    .unwrap_or_else(|| $default)
            };
        }

        let global_section = GlobalConfig {
            max_splinters:     merge!(global, max_splinters, 5),
            tmux_session_name: merge!(global, tmux_session_name, "aegis".into()),
            telegram_enabled:  merge!(global, telegram_enabled, false),
        };

        let watchdog = WatchdogConfig {
            poll_interval_ms: merge!(watchdog, poll_interval_ms, 2000),
            scan_lines:       merge!(watchdog, scan_lines, 50),
            failover_enabled: merge!(watchdog, failover_enabled, true),
            patterns: WatchdogPatterns {
                rate_limit: project.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.rate_limit.clone())
                    .or_else(|| global.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.rate_limit.clone()))
                    .unwrap_or_else(|| vec![
                        "rate limit".into(), "429".into(), "quota exceeded".into(),
                        "credit balance exhausted".into(), "too many requests".into()
                    ]),
                auth_failure: project.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.auth_failure.clone())
                    .or_else(|| global.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.auth_failure.clone()))
                    .unwrap_or_else(|| vec![
                        "401".into(), "authentication failed".into(),
                        "invalid api key".into(), "unauthorized".into()
                    ]),
                task_complete: project.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.task_complete.clone())
                    .or_else(|| global.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.task_complete.clone()))
                    .unwrap_or_else(|| vec!["[AEGIS:DONE]".into()]),
                sandbox_violation: project.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.sandbox_violation.clone())
                    .or_else(|| global.watchdog.as_ref().and_then(|w| w.patterns.as_ref()).and_then(|p| p.sandbox_violation.clone()))
                    .unwrap_or_else(|| vec!["Operation not permitted".into()]),
            },
        };

        let recorder = RecorderConfig {
            failover_context_lines: merge!(recorder, failover_context_lines, 100),
            log_rotation_max_mb:    merge!(recorder, log_rotation_max_mb, 50),
            log_retention_count:    merge!(recorder, log_retention_count, 20),
        };

        let state = StateConfig {
            snapshot_interval_s:      merge!(state, snapshot_interval_s, 60),
            snapshot_retention_count: merge!(state, snapshot_retention_count, 10),
        };

        let http = HttpConfig {
            port:    merge!(http, port, 7437),
            enabled: merge!(http, enabled, true),
        };

        let sandbox_defaults = {
            let policy = project.sandbox.defaults.as_ref()
                .or_else(|| global.sandbox.defaults.as_ref());
            
            SandboxPolicyConfig {
                network: policy.and_then(|p| p.network.as_deref()).map(|n| match n {
                    "none" => NetworkPolicy::None,
                    "any" => NetworkPolicy::Any,
                    _ => NetworkPolicy::OutboundOnly,
                }).unwrap_or(NetworkPolicy::OutboundOnly),
                extra_reads: policy.and_then(|p| p.extra_reads.clone()).unwrap_or_default(),
                extra_writes: policy.and_then(|p| p.extra_writes.clone()).unwrap_or_default(),
            }
        };

        // Providers: merged by key
        let mut providers = HashMap::new();
        // First load from global
        for (name, raw) in &global.providers {
            providers.insert(name.clone(), ProviderEntry {
                binary:      raw.binary.clone().unwrap_or_else(|| name.clone()),
                resume_flag: raw.resume_flag.clone(),
                model:       raw.model.clone(),
                extra_args:  raw.extra_args.clone().unwrap_or_default(),
            });
        }
        // Then overlay with project
        for (name, raw) in &project.providers {
            let entry = providers.entry(name.clone()).or_insert_with(|| ProviderEntry {
                binary:      raw.binary.clone().unwrap_or_else(|| name.clone()),
                resume_flag: None,
                model:       None,
                extra_args:  Vec::new(),
            });
            if let Some(bin) = &raw.binary { entry.binary = bin.clone(); }
            if let Some(flag) = &raw.resume_flag { entry.resume_flag = Some(flag.clone()); }
            if let Some(model) = &raw.model { entry.model = Some(model.clone()); }
            if let Some(args) = &raw.extra_args { entry.extra_args = args.clone(); }
        }

        let telegram = TelegramConfig {
            token_env:        merge!(telegram, token_env, "AEGIS_TELEGRAM_TOKEN".into()),
            allowed_chat_ids: merge!(telegram, allowed_chat_ids, Vec::new()),
            webhook_url:      project.telegram.as_ref().and_then(|t| t.webhook_url.clone())
                                .or_else(|| global.telegram.as_ref().and_then(|t| t.webhook_url.clone())),
        };

        // Agents: merged by key
        let mut agents = HashMap::new();
        for (name, raw) in &global.agent {
            agents.insert(name.clone(), resolve_agent(raw, &sandbox_defaults)?);
        }
        for (name, raw) in &project.agent {
            agents.insert(name.clone(), resolve_agent(raw, &sandbox_defaults)?);
        }

        let splinter_defaults = SplinterDefaults {
            cli_provider:     project.splinter_defaults.as_ref().and_then(|s| s.cli_provider.clone())
                                .or_else(|| global.splinter_defaults.as_ref().and_then(|s| s.cli_provider.clone()))
                                .unwrap_or_else(|| "claude-code".into()),
            fallback_cascade: project.splinter_defaults.as_ref().and_then(|s| s.fallback_cascade.clone())
                                .or_else(|| global.splinter_defaults.as_ref().and_then(|s| s.fallback_cascade.clone()))
                                .unwrap_or_default(),
            auto_cleanup:     project.splinter_defaults.as_ref().and_then(|s| s.auto_cleanup)
                                .or_else(|| global.splinter_defaults.as_ref().and_then(|s| s.auto_cleanup))
                                .unwrap_or(true),
        };

        Ok(Self {
            global: global_section,
            watchdog,
            recorder,
            state,
            http,
            sandbox_defaults,
            providers,
            telegram,
            agents,
            splinter_defaults,
        })
    }

    pub fn validate(&self) -> Vec<ConfigError> {
        let mut errors = Vec::new();

        // global
        if self.global.max_splinters < 1 || self.global.max_splinters > 32 {
            errors.push(ConfigError { field: "global.max_splinters".into(), reason: "must be between 1 and 32".into() });
        }
        if self.global.tmux_session_name.is_empty() || self.global.tmux_session_name.contains(char::is_whitespace) {
            errors.push(ConfigError { field: "global.tmux_session_name".into(), reason: "must be non-empty and contains no whitespace".into() });
        }

        // watchdog
        if self.watchdog.poll_interval_ms < 500 {
            errors.push(ConfigError { field: "watchdog.poll_interval_ms".into(), reason: "must be >= 500".into() });
        }
        if self.watchdog.scan_lines < 1 || self.watchdog.scan_lines > 5000 {
            errors.push(ConfigError { field: "watchdog.scan_lines".into(), reason: "must be between 1 and 5000".into() });
        }

        // recorder
        if self.recorder.failover_context_lines < 10 || self.recorder.failover_context_lines > 1000 {
            errors.push(ConfigError { field: "recorder.failover_context_lines".into(), reason: "must be between 10 and 1000".into() });
        }
        if self.recorder.log_rotation_max_mb < 1 {
            errors.push(ConfigError { field: "recorder.log_rotation_max_mb".into(), reason: "must be >= 1".into() });
        }

        // http
        if self.http.port < 1024 {
            errors.push(ConfigError { field: "http.port".into(), reason: "must be >= 1024".into() });
        }

        // providers
        for (name, provider) in &self.providers {
            if provider.binary.is_empty() {
                errors.push(ConfigError { field: format!("providers.{}.binary", name), reason: "must be non-empty".into() });
            }
        }

        // agents
        for (name, agent) in &self.agents {
            if !self.providers.contains_key(&agent.cli_provider) {
                errors.push(ConfigError { field: format!("agent.{}.cli_provider", name), reason: format!("unknown provider '{}'", agent.cli_provider) });
            }
            for fallback in &agent.fallback_cascade {
                if !self.providers.contains_key(fallback) {
                    errors.push(ConfigError { field: format!("agent.{}.fallback_cascade", name), reason: format!("unknown provider '{}'", fallback) });
                }
                if fallback == &agent.cli_provider {
                    errors.push(ConfigError { field: format!("agent.{}.fallback_cascade", name), reason: "cannot contain the primary cli_provider".into() });
                }
            }
        }

        // telegram
        if self.global.telegram_enabled && self.telegram.allowed_chat_ids.is_empty() {
            errors.push(ConfigError { field: "telegram.allowed_chat_ids".into(), reason: "must be non-empty when telegram_enabled is true".into() });
        }
        if self.telegram.token_env.is_empty() {
            errors.push(ConfigError { field: "telegram.token_env".into(), reason: "must be non-empty".into() });
        }

        // splinter_defaults
        if !self.providers.contains_key(&self.splinter_defaults.cli_provider) {
            errors.push(ConfigError { field: "splinter_defaults.cli_provider".into(), reason: format!("unknown provider '{}'", self.splinter_defaults.cli_provider) });
        }
        for fallback in &self.splinter_defaults.fallback_cascade {
            if !self.providers.contains_key(fallback) {
                errors.push(ConfigError { field: "splinter_defaults.fallback_cascade".into(), reason: format!("unknown provider '{}'", fallback) });
            }
        }

        errors
    }
}

fn resolve_agent(raw: &RawAgentConfig, defaults: &SandboxPolicyConfig) -> Result<AgentEntry> {
    let kind = match raw.kind.as_deref() {
        Some("bastion") => AgentKind::Bastion,
        Some("splinter") => AgentKind::Splinter,
        _ => AgentKind::Splinter, // default
    };

    let sandbox = if let Some(raw_sb) = &raw.sandbox {
        SandboxPolicyConfig {
            network: raw_sb.network.as_deref().map(|n| match n {
                "none" => NetworkPolicy::None,
                "any" => NetworkPolicy::Any,
                _ => NetworkPolicy::OutboundOnly,
            }).unwrap_or(defaults.network),
            extra_reads: raw_sb.extra_reads.clone().unwrap_or_else(|| defaults.extra_reads.clone()),
            extra_writes: raw_sb.extra_writes.clone().unwrap_or_else(|| defaults.extra_writes.clone()),
        }
    } else {
        defaults.clone()
    };

    Ok(AgentEntry {
        kind,
        role: raw.role.clone().unwrap_or_else(|| "default".into()),
        cli_provider: raw.cli_provider.clone().unwrap_or_else(|| "claude-code".into()),
        fallback_cascade: raw.fallback_cascade.clone().unwrap_or_default(),
        system_prompt: raw.system_prompt.clone(),
        sandbox,
        auto_cleanup: raw.auto_cleanup.unwrap_or(true),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_defaults() {
        let global = RawConfig::default();
        let project = RawConfig::default();
        let resolved = EffectiveConfig::resolve(&global, &project).unwrap();

        assert_eq!(resolved.global.max_splinters, 5);
        assert_eq!(resolved.global.tmux_session_name, "aegis");
        assert_eq!(resolved.watchdog.poll_interval_ms, 2000);
        assert_eq!(resolved.sandbox_defaults.network, NetworkPolicy::OutboundOnly);
        assert!(resolved.watchdog.patterns.rate_limit.contains(&"429".to_string()));
    }

    #[test]
    fn test_resolve_merge_scalar() {
        let mut global = RawConfig::default();
        global.global = Some(RawGlobalConfig {
            max_splinters: Some(10),
            ..Default::default()
        });

        let mut project = RawConfig::default();
        project.global = Some(RawGlobalConfig {
            max_splinters: Some(15),
            ..Default::default()
        });

        let resolved = EffectiveConfig::resolve(&global, &project).unwrap();
        assert_eq!(resolved.global.max_splinters, 15); // Project wins
    }

    #[test]
    fn test_resolve_merge_fallback() {
        let mut global = RawConfig::default();
        global.global = Some(RawGlobalConfig {
            max_splinters: Some(10),
            ..Default::default()
        });

        let project = RawConfig::default(); // No project max_splinters

        let resolved = EffectiveConfig::resolve(&global, &project).unwrap();
        assert_eq!(resolved.global.max_splinters, 10); // Global fallback
    }

    #[test]
    fn test_resolve_merge_providers() {
        let mut global = RawConfig::default();
        global.providers.insert("claude".into(), RawProviderConfig {
            binary: Some("global-claude".into()),
            ..Default::default()
        });

        let mut project = RawConfig::default();
        project.providers.insert("claude".into(), RawProviderConfig {
            binary: Some("project-claude".into()),
            ..Default::default()
        });
        project.providers.insert("gemini".into(), RawProviderConfig {
            binary: Some("gemini".into()),
            ..Default::default()
        });

        let resolved = EffectiveConfig::resolve(&global, &project).unwrap();
        assert_eq!(resolved.providers.get("claude").unwrap().binary, "project-claude");
        assert_eq!(resolved.providers.get("gemini").unwrap().binary, "gemini");
    }

    #[test]
    fn test_validate_invalid_values() {
        let mut config = EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap();
        config.global.max_splinters = 0; // Invalid
        config.watchdog.poll_interval_ms = 100; // Invalid
        config.http.port = 80; // Invalid

        let errors = config.validate();
        assert!(errors.iter().any(|e| e.field == "global.max_splinters"));
        assert!(errors.iter().any(|e| e.field == "watchdog.poll_interval_ms"));
        assert!(errors.iter().any(|e| e.field == "http.port"));
    }

    #[test]
    fn test_validate_unknown_provider() {
        let mut config = EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap();
        config.agents.insert("test".into(), AgentEntry {
            kind: AgentKind::Bastion,
            role: "test".into(),
            cli_provider: "non-existent".into(),
            fallback_cascade: vec![],
            system_prompt: None,
            sandbox: config.sandbox_defaults.clone(),
            auto_cleanup: true,
        });

        let errors = config.validate();
        assert!(errors.iter().any(|e| e.field == "agent.test.cli_provider"));
    }
}
