use std::io;

#[derive(Debug, thiserror::Error)]
pub enum TmuxError {
    #[error("tmux binary not found or not executable: {reason}")]
    BinaryNotFound { reason: String },

    #[error("tmux command failed (exit {code}): {stderr}")]
    CommandFailed { code: i32, stderr: String },

    #[error("tmux session not found: {target}")]
    SessionNotFound { target: String },

    #[error("tmux window not found: {target}")]
    WindowNotFound { target: String },

    #[error("tmux pane not found: {target}")]
    PaneNotFound { target: String },

    #[error("invalid tmux target string: `{raw}`")]
    InvalidTarget { raw: String },

    #[error("tmux pipe-pane failed: {source}")]
    PipeFailed {
        #[source]
        source: io::Error,
    },

    #[error("I/O error: {source}")]
    Io {
        #[source]
        source: io::Error,
    },
}

impl From<TmuxError> for aegis_core::AegisError {
    fn from(e: TmuxError) -> Self {
        use aegis_core::AegisError;
        match e {
            TmuxError::CommandFailed { code: _, stderr } => {
                AegisError::TmuxCommand { command: String::new(), stderr }
            }
            TmuxError::SessionNotFound { target } => AegisError::TmuxSessionNotFound { target },
            TmuxError::PaneNotFound { target } => AegisError::TmuxPaneNotFound { target },
            other => AegisError::Unexpected(Box::new(other)),
        }
    }
}
