use thiserror::Error;

#[derive(Debug, Error)]
pub enum DesignError {
    #[error("template not found: {name}")]
    TemplateNotFound { name: String },

    #[error("failed to parse template '{name}': {reason}")]
    ParseTemplate { name: String, reason: String },

    #[error("required variable not resolved: {{{{{name}}}}}")]
    UnresolvedRequired { name: String },

    #[error("unresolved placeholders remain after rendering: {placeholders:?}")]
    UnresolvedPlaceholders { placeholders: Vec<String> },

    #[error("taskflow index not found at {path}")]
    IndexNotFound { path: String },

    #[error("io error reading {path}: {reason}")]
    Io { path: String, reason: String },
}

pub type Result<T> = std::result::Result<T, DesignError>;
