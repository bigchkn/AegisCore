use std::collections::HashMap;
use aegis_core::config::{EffectiveConfig, AgentEntry};
use aegis_core::provider::Provider;
use aegis_core::error::{AegisError, Result};
use crate::manifest::BuiltinManifest;

#[cfg(feature = "claude")]
use crate::claude::ClaudeProvider;
#[cfg(feature = "gemini")]
use crate::gemini::GeminiProvider;
#[cfg(feature = "codex")]
use crate::codex::CodexProvider;
#[cfg(feature = "ollama")]
use crate::ollama::OllamaProvider;

pub struct ProviderRegistry {
    pub manifest: BuiltinManifest,
    pub providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    pub fn from_config(cfg: &EffectiveConfig) -> Result<Self> {
        let manifest = BuiltinManifest::load().map_err(|e| AegisError::Config {
            field: "builtin_manifest".into(),
            reason: e.to_string(),
        })?;

        let mut providers: HashMap<String, Box<dyn Provider>> = HashMap::new();

        for (name, definition) in &manifest.providers {
            // Map ProviderEntry from config to ProviderConfig for the provider
            let user_config = if let Some(entry) = cfg.providers.get(name) {
                aegis_core::provider::ProviderConfig {
                    name: name.clone(),
                    binary: entry.binary.clone(),
                    extra_args: entry.extra_args.clone(),
                    resume_flag: entry.resume_flag.clone().or_else(|| definition.resume_flag.clone()),
                    model: entry.model.clone(),
                }
            } else {
                aegis_core::provider::ProviderConfig {
                    name: name.clone(),
                    binary: definition.binary.clone(),
                    extra_args: Vec::new(),
                    resume_flag: definition.resume_flag.clone(),
                    model: None,
                }
            };

            let provider: Option<Box<dyn Provider>> = match name.as_str() {
                #[cfg(feature = "claude")]
                "claude-code" => Some(Box::new(ClaudeProvider::new(definition.clone(), user_config))),
                #[cfg(feature = "gemini")]
                "gemini-cli" => Some(Box::new(GeminiProvider::new(definition.clone(), user_config))),
                #[cfg(feature = "codex")]
                "codex" => Some(Box::new(CodexProvider::new(definition.clone(), user_config))),
                #[cfg(feature = "ollama")]
                "ollama" => Some(Box::new(OllamaProvider::new(definition.clone(), user_config))),
                _ => None,
            };

            if let Some(p) = provider {
                providers.insert(name.clone(), p);
            }
        }

        Ok(Self { manifest, providers })
    }

    pub fn get(&self, name: &str) -> Result<&dyn Provider> {
        self.providers.get(name).map(|p| p.as_ref()).ok_or_else(|| AegisError::ProviderNotFound {
            name: name.to_string(),
        })
    }

    pub fn cascade_for_agent(&self, agent: &AgentEntry) -> Result<Vec<&dyn Provider>> {
        let mut cascade = Vec::new();
        
        // Primary
        cascade.push(self.get(&agent.cli_provider)?);
        
        // Fallbacks
        for fallback in &agent.fallback_cascade {
            cascade.push(self.get(fallback)?);
        }
        
        Ok(cascade)
    }

    pub fn next_in_cascade<'a>(
        &'a self,
        cascade: &[&'a dyn Provider],
        current: &str,
    ) -> Option<&'a dyn Provider> {
        let idx = cascade.iter().position(|p| p.name() == current)?;
        cascade.get(idx + 1).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_core::config::{RawConfig, EffectiveConfig};
    use std::path::PathBuf;

    fn mock_config() -> EffectiveConfig {
        EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap()
    }

    #[test]
    fn test_manifest_loading() {
        let manifest = BuiltinManifest::load().unwrap();
        assert!(manifest.providers.contains_key("claude-code"));
        assert!(manifest.providers.contains_key("gemini-cli"));
    }

    #[test]
    fn test_registry_binary_override() {
        let mut raw_project = RawConfig::default();
        let mut claude_cfg = aegis_core::config::RawProviderConfig::default();
        claude_cfg.binary = Some("custom-claude".into());
        raw_project.providers.insert("claude-code".into(), claude_cfg);

        let cfg = EffectiveConfig::resolve(&RawConfig::default(), &raw_project).unwrap();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        
        let claude = registry.get("claude-code").unwrap();
        let cmd = claude.spawn_command(&PathBuf::from("/tmp"), None);
        assert_eq!(cmd.get_program(), "custom-claude");
    }

    #[test]
    fn test_claude_unattended_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let claude = registry.get("claude-code").unwrap();
        
        let cmd = claude.spawn_command(&PathBuf::from("/tmp"), None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        
        assert!(args.contains(&"--yolo"));
        assert!(args.contains(&"--non-interactive"));
    }

    #[test]
    fn test_error_pattern_matching() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let claude = registry.get("claude-code").unwrap();
        
        assert!(claude.is_rate_limit_error("error: 429 rate limit exceeded"));
        assert!(claude.is_auth_error("authentication failed: invalid key"));
        assert!(!claude.is_rate_limit_error("all systems nominal"));
    }
}
