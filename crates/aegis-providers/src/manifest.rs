use aegis_core::SystemPromptMechanism;
use aegis_core::InteractionModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuiltinManifest {
    pub providers: HashMap<String, ProviderDefinition>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeMechanism {
    /// Resumed via CLI flags (e.g. --resume <id>)
    CliFlag,
    /// Resumed via post-spawn command injection (e.g. /resume <id>)
    Injection,
    /// Resumed by placing a provider subcommand before the session id.
    Subcommand,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderDefinition {
    pub binary: String,
    pub auto_approve_flags: Vec<String>,
    pub resume_mechanism: ResumeMechanism,
    pub resume_flag: Option<String>,
    pub resume_command: Option<String>,
    pub export_command: Option<String>,
    pub model_flag: Option<String>,
    pub interactive_flag: Option<String>,
    pub initial_prompt_arg: Option<String>,
    pub interaction_model: InteractionModel,
    pub system_prompt_mechanism: SystemPromptMechanism,
    pub error_patterns: ErrorPatterns,
    #[serde(default)]
    pub startup_delay_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorPatterns {
    pub rate_limit: Vec<String>,
    pub auth: Vec<String>,
}

impl BuiltinManifest {
    pub fn load() -> Result<Self, serde_yaml::Error> {
        let raw = include_str!("builtin_providers.yaml");
        serde_yaml::from_str(raw)
    }
}
