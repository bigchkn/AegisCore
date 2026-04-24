use std::{io, path::PathBuf};

use aegis_core::AegisError;

#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("template variable not found: `{var}`")]
    TemplateVar { var: String },

    #[error("profile write failed at {path}: {source}")]
    WriteError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("non-UTF-8 path cannot be used in sandbox profile: {path:?}")]
    NonUtf8Path { path: PathBuf },

    #[error("sandbox-exec binary not found on PATH")]
    SandboxExecNotFound,
}

impl From<SandboxError> for AegisError {
    fn from(error: SandboxError) -> Self {
        match error {
            SandboxError::WriteError { path, source } => AegisError::StorageIo { path, source },
            other => AegisError::SandboxProfileRender {
                reason: other.to_string(),
            },
        }
    }
}
