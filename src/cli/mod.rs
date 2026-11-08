use clap::{Parser, Subcommand};

mod add_profile;
mod list_profiles;
mod run_agent;

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
            Command::Profile(ProfileCommand::Remove { name }) => {
                crate::app::profiles::remove(path, name)?;
                println!("Profile '{name}' removed");
                Ok(())
            }
            Command::Run { name } => run_agent::run(path, name),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// List configured profiles.
    List,
    /// Run an agent using a saved profile.
    Run { name: String },
    /// Manage agent profiles.
    #[command(subcommand)]
    Profile(ProfileCommand),
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// Add a profile interactively.
    Add,
    /// Remove a profile and its saved API key; keep agent data.
    Remove { name: String },
}
