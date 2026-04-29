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
                    interaction_model: definition.interaction_model.clone(),
                    interactive_flag: definition.interactive_flag.clone(),
                    initial_prompt_arg: definition.initial_prompt_arg.clone(),
                    startup_delay_ms: entry
                        .startup_delay_ms
                        .unwrap_or(definition.startup_delay_ms),
                }
            } else {
                aegis_core::provider::ProviderConfig {
                    name: name.clone(),
                    binary: definition.binary.clone(),
                    extra_args: Vec::new(),
                    resume_flag: definition.resume_flag.clone(),
                    model: None,
                    interaction_model: definition.interaction_model.clone(),
                    interactive_flag: definition.interactive_flag.clone(),
                    initial_prompt_arg: definition.initial_prompt_arg.clone(),
                    startup_delay_ms: definition.startup_delay_ms,
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
        assert!(manifest.providers.contains_key("dirac"));
    }

    #[test]
    fn test_claude_auto_approve_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let claude = registry.get("claude-code").unwrap();

        let cmd = claude.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--dangerously-skip-permissions"));
        assert!(!args.contains(&"--non-interactive"));
    }

    #[test]
    fn test_gemini_auto_approve_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let gemini = registry.get("gemini-cli").unwrap();

        let cmd = gemini.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--yolo"));
        assert!(!args.contains(&"--non-interactive"));
    }

    #[test]
    fn test_codex_auto_approve_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let codex = registry.get("codex").unwrap();

        let cmd = codex.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--full-auto"));
        assert!(args.contains(&"--no-alt-screen"));
    }

    #[test]
    fn test_dirac_auto_approve_flags() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let dirac = registry.get("dirac").unwrap();

        let cmd = dirac.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(args.contains(&"--auto-approve-all"));
    }

    #[test]
    fn test_codex_resume_subcommand() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let codex = registry.get("codex").unwrap();
        let session = aegis_core::provider::SessionRef {
            provider: "codex".into(),
            session_id: Some("00000000-0000-0000-0000-000000000001".into()),
            checkpoint: None,
        };

        assert_eq!(
            codex.resume_args(&session),
            vec![
                "resume".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string()
            ]
        );

        let cmd = codex.spawn_command(&PathBuf::from("/tmp"), Some(&session), None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert_eq!(args[0], "resume");
        assert_eq!(args[1], "00000000-0000-0000-0000-000000000001");
        assert!(args.contains(&"--full-auto"));
        assert!(args.contains(&"--no-alt-screen"));
    }

    #[test]
    fn test_dirac_resume_task_id_flag() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();
        let dirac = registry.get("dirac").unwrap();
        let session = aegis_core::provider::SessionRef {
            provider: "dirac".into(),
            session_id: Some("dirac-task-123".into()),
            checkpoint: None,
        };

        assert_eq!(
            dirac.resume_args(&session),
            vec!["--taskId".to_string(), "dirac-task-123".to_string()]
        );

        let cmd = dirac.spawn_command(&PathBuf::from("/tmp"), Some(&session), None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        let task_id_pos = args.iter().position(|a| *a == "--taskId").unwrap();
        assert_eq!(args[task_id_pos + 1], "dirac-task-123");
        assert!(args.contains(&"--auto-approve-all"));
    }

    #[test]
    fn test_error_pattern_matching_all() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();

        let claude = registry.get("claude-code").unwrap();
        assert!(claude.is_rate_limit_error("429 usage limit reached"));
        assert!(claude.is_rate_limit_error("APIError: 529 AuthenticationService"));

        let gemini = registry.get("gemini-cli").unwrap();
        assert!(gemini.is_rate_limit_error("quota exceeded"));
        assert!(gemini.is_rate_limit_error("Usage limit reached"));
        assert!(gemini.is_auth_error("permission denied"));

        let codex = registry.get("codex").unwrap();
        assert!(codex.is_rate_limit_error("429 too many requests"));
        assert!(codex.is_auth_error("invalid api key"));
        assert!(codex.is_auth_error("not logged in"));

        let dirac = registry.get("dirac").unwrap();
        assert!(dirac.is_rate_limit_error("429 too many requests"));
        assert!(dirac.is_auth_error("authentication failed"));
    }

    #[test]
    fn test_interaction_models_are_loaded_from_manifest() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();

        assert_eq!(
            registry.get("claude-code").unwrap().interaction_model(),
            aegis_core::InteractionModel::InjectedTui
        );
        assert_eq!(
            registry.get("gemini-cli").unwrap().interaction_model(),
            aegis_core::InteractionModel::InjectedTui
        );
        assert_eq!(
            registry.get("codex").unwrap().interaction_model(),
            aegis_core::InteractionModel::HeadlessIterative
        );
        assert_eq!(
            registry.get("dirac").unwrap().interaction_model(),
            aegis_core::InteractionModel::HeadlessIterative
        );
    }

    #[test]
    fn test_codex_and_dirac_do_not_use_system_prompt_transport() {
        let cfg = mock_config();
        let registry = ProviderRegistry::from_config(&cfg).unwrap();

        assert_eq!(
            registry.get("codex").unwrap().system_prompt_mechanism(),
            aegis_core::SystemPromptMechanism::None
        );
        assert_eq!(
            registry.get("dirac").unwrap().system_prompt_mechanism(),
            aegis_core::SystemPromptMechanism::None
        );
    }

    fn provider_with_model(model: Option<&str>) -> GenericProvider {
        let manifest = BuiltinManifest::load().unwrap();
        let definition = manifest.providers["claude-code"].clone();
        GenericProvider::new(
            definition,
            aegis_core::provider::ProviderConfig {
                name: "claude-code".into(),
                binary: "claude".into(),
                extra_args: vec![],
                resume_flag: None,
                model: model.map(str::to_owned),
                interaction_model: aegis_core::InteractionModel::InjectedTui,
                startup_delay_ms: 0,
                interactive_flag: None,
                initial_prompt_arg: None,
            },
        )
    }

    #[test]
    fn spawn_command_applies_extra_args() {
        let manifest = BuiltinManifest::load().unwrap();
        let definition = manifest.providers["claude-code"].clone();
        let provider = GenericProvider::new(
            definition,
            aegis_core::provider::ProviderConfig {
                name: "claude-code".into(),
                binary: "claude".into(),
                extra_args: vec!["--verbose".into(), "--debug".into()],
                resume_flag: None,
                model: None,
                interaction_model: aegis_core::InteractionModel::InjectedTui,
                startup_delay_ms: 0,
                interactive_flag: None,
                initial_prompt_arg: None,
            },
        );

        let cmd = provider.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        // extra_args must appear before auto-approve flags
        let verbose_pos = args.iter().position(|a| *a == "--verbose").unwrap();
        let yolo_pos = args
            .iter()
            .position(|a| *a == "--dangerously-skip-permissions")
            .unwrap();
        assert!(
            verbose_pos < yolo_pos,
            "extra_args must precede auto-approve flags"
        );
        assert!(args.contains(&"--debug"));
    }

    #[test]
    fn spawn_command_applies_model_from_config() {
        let provider = provider_with_model(Some("claude-opus-4-7"));
        let cmd = provider.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        let flag_pos = args.iter().position(|a| *a == "--model").unwrap();
        assert_eq!(args[flag_pos + 1], "claude-opus-4-7");
    }

    #[test]
    fn spawn_command_model_override_takes_precedence() {
        let provider = provider_with_model(Some("claude-sonnet-4-6"));
        let cmd = provider.spawn_command(&PathBuf::from("/tmp"), None, Some("claude-opus-4-7"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        let flag_pos = args.iter().position(|a| *a == "--model").unwrap();
        assert_eq!(
            args[flag_pos + 1],
            "claude-opus-4-7",
            "override must win over provider config"
        );
        // ensure --model appears only once
        assert_eq!(args.iter().filter(|a| **a == "--model").count(), 1);
    }

    #[test]
    fn spawn_command_no_model_emits_no_model_flag() {
        let provider = provider_with_model(None);
        let cmd = provider.spawn_command(&PathBuf::from("/tmp"), None, None);
        let args: Vec<_> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();

        assert!(
            !args.contains(&"--model"),
            "no --model flag when no model is set"
        );
    }
}
