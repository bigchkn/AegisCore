use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuiltinManifest {
    pub providers: HashMap<String, ProviderDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderDefinition {
    pub binary: String,
    pub auto_approve_flags: Vec<String>,
    pub non_interactive_flags: Vec<String>,
    pub resume_flag: Option<String>,
    pub resume_command: Option<String>,
    pub error_patterns: ErrorPatterns,
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
