pub mod context;
pub mod engine;
pub mod error;
pub mod registry;
pub mod template;

#[cfg(test)]
mod tests;

pub use context::BootstrapContext;
pub use engine::{DesignEngine, RenderedTemplate};
pub use error::{DesignError, Result};
pub use registry::{ResolvedTemplate, TemplateLayer, TemplateRegistry};
pub use template::{Template, TemplateAgentConfig, TemplateKind, TemplateMetadata, TemplateVariables};
