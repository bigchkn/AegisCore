use std::process;

#[derive(Debug, thiserror::Error)]
pub enum AegisCliError {
    #[error("Not an AegisCore project (or any parent directory up to /).\nRun 'aegis init' to initialize.")]
    NotAnAegisProject,

    #[error("aegisd is not running. Start it with: aegis daemon start")]
    DaemonNotRunning,

    #[error("Daemon error: {0}")]
    DaemonError(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Invalid argument: {0}")]
    InvalidArg(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Core(#[from] aegis_core::AegisError),
}

impl AegisCliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotAnAegisProject | Self::DaemonNotRunning | Self::InvalidArg(_) => 1,
            Self::Config(_) => 2,
            Self::DaemonError(_) => 3,
            Self::Io(_) | Self::Core(_) => 1,
        }
    }

    pub fn print_and_exit(self) -> ! {
        eprintln!("error: {self}");
        process::exit(self.exit_code());
    }
}
