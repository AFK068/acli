use clap::{Parser, Subcommand};

mod add_profile;
mod list_profiles;

#[derive(Parser)]
#[command(version, about = "A hub for terminal coding agents")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn execute(&self, path: &std::path::Path) -> anyhow::Result<()> {
        match &self.command {
            Command::List => list_profiles::run(path),
            Command::Profile(ProfileCommand::Add) => add_profile::run(path),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// List configured profiles.
    List,
    /// Manage agent profiles.
    #[command(subcommand)]
    Profile(ProfileCommand),
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Add a profile interactively.
    Add,
}
