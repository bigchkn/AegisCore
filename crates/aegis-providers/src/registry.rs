use crate::generic::GenericProvider;
use crate::manifest::BuiltinManifest;
use aegis_core::config::{AgentEntry, EffectiveConfig};
use aegis_core::error::{AegisError, Result};
use aegis_core::provider::Provider;
use std::collections::HashMap;

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
            let user_config = if let Some(entry) = cfg.providers.get(name) {
                aegis_core::provider::ProviderConfig {
                    name: name.clone(),
                    binary: entry.binary.clone(),
                    extra_args: entry.extra_args.clone(),
                    resume_flag: entry
                        .resume_flag
                        .clone()
                        .or_else(|| definition.resume_flag.clone()),
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

            // All providers now use GenericProvider driven by the manifest
            providers.insert(
                name.clone(),
                Box::new(GenericProvider::new(definition.clone(), user_config)),
            );
        }

        Ok(Self {
            manifest,
            providers,
        })
    }

    pub fn get(&self, name: &str) -> Result<&dyn Provider> {
        self.providers
            .get(name)
            .map(|p| p.as_ref())
            .ok_or_else(|| AegisError::ProviderNotFound {
                name: name.to_string(),
            })
    }

    pub fn cascade_for_agent(&self, agent: &AgentEntry) -> Result<Vec<&dyn Provider>> {
        let mut cascade = Vec::new();
        cascade.push(self.get(&agent.cli_provider)?);
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
    use aegis_core::config::{EffectiveConfig, RawConfig};
    use std::path::PathBuf;

    fn mock_config() -> EffectiveConfig {
        EffectiveConfig::resolve(&RawConfig::default(), &RawConfig::default()).unwrap()
    }

    #[test]
    fn test_manifest_loading() {
        let manifest = BuiltinManifest::load().unwrap();
        assert!(manifest.providers.contains_key("claude-code"));
        assert!(manifest.providers.contains_key("gemini-cli"));
        assert!(manifest.providers.contains_key("codex"));
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
    fn test_gemini_unattended_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let gemini = registry.get("gemini-cli").unwrap();

        let cmd = gemini.spawn_command(&PathBuf::from("/tmp"), None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--yes"));
    }

    #[test]
    fn test_codex_unattended_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let codex = registry.get("codex").unwrap();

        let cmd = codex.spawn_command(&PathBuf::from("/tmp"), None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--full-auto"));
        assert!(args.contains(&"--no-alt-screen"));
    }

    #[test]
    fn test_codex_resume_subcommand() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let codex = registry.get("codex").unwrap();
        let session = aegis_core::provider::SessionRef {
            provider: "codex".into(),
            session_id: "00000000-0000-0000-0000-000000000001".into(),
            checkpoint: None,
        };

        assert_eq!(
            codex.resume_args(&session),
            vec![
                "resume".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string()
            ]
        );

        let cmd = codex.spawn_command(&PathBuf::from("/tmp"), Some(&session));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert_eq!(args[0], "resume");
        assert_eq!(args[1], "00000000-0000-0000-0000-000000000001");
        assert!(args.contains(&"--full-auto"));
        assert!(args.contains(&"--no-alt-screen"));
    }

    #[test]
    fn test_error_pattern_matching_all() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();

        let claude = registry.get("claude-code").unwrap();
        assert!(claude.is_rate_limit_error("429 usage limit reached"));

        let gemini = registry.get("gemini-cli").unwrap();
        assert!(gemini.is_rate_limit_error("quota exceeded"));
        assert!(gemini.is_auth_error("permission denied"));

        let codex = registry.get("codex").unwrap();
        assert!(codex.is_rate_limit_error("429 too many requests"));
        assert!(codex.is_auth_error("invalid api key"));
        assert!(codex.is_auth_error("not logged in"));
    }
}
