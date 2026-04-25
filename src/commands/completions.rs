use crate::error::AegisCliError;
use clap::CommandFactory;
use clap_complete::{generate, Shell};

pub fn run<Cli: CommandFactory>(shell: Shell) -> Result<(), AegisCliError> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
