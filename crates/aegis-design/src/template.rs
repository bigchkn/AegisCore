use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateKind {
    Bastion,
    Splinter,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateMetadata {
    pub name: String,
    pub description: String,
    pub kind: TemplateKind,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemplateSandboxConfig {
    pub network: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateAgentConfig {
    pub role: String,
    pub cli_provider: String,
    pub model: Option<String>,
    #[serde(default)]
    pub auto_cleanup: bool,
    #[serde(default)]
    pub fallback_cascade: Vec<String>,
    #[serde(default)]
    pub sandbox: TemplateSandboxConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TemplateVariables {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawTemplateToml {
    template: TemplateMetadata,
    agent: TemplateAgentConfig,
    #[serde(default)]
    variables: TemplateVariables,
}

#[derive(Debug, Clone)]
pub struct Template {
    pub metadata: TemplateMetadata,
    pub agent: TemplateAgentConfig,
    pub variables: TemplateVariables,
    pub system_prompt: String,
    pub startup: Option<String>,
}

impl Template {
    pub fn from_parts(
        name: &str,
        toml_content: &str,
        system_prompt: String,
        startup: Option<String>,
    ) -> crate::error::Result<Self> {
        let raw: RawTemplateToml =
            toml::from_str(toml_content).map_err(|e| crate::error::DesignError::ParseTemplate {
                name: name.to_owned(),
                reason: e.to_string(),
            })?;
        Ok(Self {
            metadata: raw.template,
            agent: raw.agent,
            variables: raw.variables,
            system_prompt,
            startup,
        })
    }
}
