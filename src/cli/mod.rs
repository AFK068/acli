use crate::config::Config;
use clap::{Parser, Subcommand};

mod profiles;

// Defines the structure of the command-line interface (CLI) using the `clap` crate.
#[derive(Parser)]
#[command(version, about = "A hub for terminal coding agents")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

// Implements the execution logic for the CLI commands.
impl Cli {
    // Executes the command specified in the CLI arguments.
    pub(crate) fn execute(&self, config: &Config) -> anyhow::Result<()> {
        match &self.command {
            Command::List => profiles::print(config)?,
        }

        Ok(())
    }
}

// Defines the available commands for the CLI.
#[derive(Subcommand)]
enum Command {
    /// List configured profiles.
    List,
}
