use clap::{Parser, Subcommand};

mod add_profile;
mod list_profiles;
mod mcp;
mod run_agent;
mod skills;

#[derive(Parser)]
#[command(version, about = "A hub for terminal coding agents")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn execute(&self, path: &std::path::Path) -> anyhow::Result<()> {
        match &self.command {
            Command::Mcp(cmd) => cmd.execute(path),
            Command::Skills(cmd) => cmd.execute(path),
            Command::List => list_profiles::run(path),
            Command::Profile(cmd) => cmd.execute(path),
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
    /// Manage local MCP servers shared by all agent profiles.
    #[command(subcommand)]
    Mcp(mcp::Command),
    /// Manage shared agent skills.
    #[command(subcommand)]
    Skills(skills::Command),
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
    /// Rename a profile, keeping its credentials and agent data.
    Rename { name: String, new_name: String },
}

impl ProfileCommand {
    fn execute(&self, path: &std::path::Path) -> anyhow::Result<()> {
        match self {
            Self::Add => add_profile::run(path)?,
            Self::Remove { name } => {
                crate::app::profiles::remove(path, name)?;
                println!("Profile '{name}' removed");
            }
            Self::Rename { name, new_name } => {
                crate::config::storage::rename_profile(path, name, new_name)?;
                println!("Profile '{name}' renamed to '{new_name}'");
            }
        }
        Ok(())
    }
}
