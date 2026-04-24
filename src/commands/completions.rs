use clap::CommandFactory;
use clap_complete::{generate, Shell};
use crate::error::AegisCliError;

pub fn run<Cli: CommandFactory>(shell: Shell) -> Result<(), AegisCliError> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
