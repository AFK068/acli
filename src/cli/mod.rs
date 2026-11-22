use clap::{Parser, Subcommand};

mod add_profile;
mod list_profiles;
mod mcp;
mod run_agent;
mod skills;
mod tui;

#[derive(Parser)]
#[command(version, about = "A hub for terminal coding agents")]
pub(crate) struct Cli {
    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn execute(self, path: &std::path::Path) -> anyhow::Result<()> {
        match self.command {
            Command::Mcp(cmd) => cmd.execute(path),
            Command::Skills(cmd) => cmd.execute(path),
            Command::List => list_profiles::run(path),
            Command::Profile(cmd) => cmd.execute(path),
            Command::Run { name } => run_agent::run(path, name.as_deref()),
            Command::Default { name, clear } => default_profile(path, name.as_deref(), clear),
            Command::Tui => tui::run(path),
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// List configured profiles.
    List,
    /// Run the named profile or the default profile.
    Run { name: Option<String> },
    /// Show, set or clear the default profile.
    Default {
        name: Option<String>,
        #[arg(long, conflicts_with = "name")]
        clear: bool,
    },
    /// Browse profiles, skills and MCP servers in an interactive terminal.
    Tui,
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

fn default_profile(path: &std::path::Path, name: Option<&str>, clear: bool) -> anyhow::Result<()> {
    use crate::config::storage;
    if let Some(name) = name {
        storage::set_default(path, Some(name))?;
        println!("Default profile set to '{name}'");
    } else if clear {
        storage::set_default(path, None)?;
        println!("Default profile cleared");
    } else {
        let config = storage::load(path)?;
        match config.default_profile {
            Some(name) => println!("{name}"),
            None => println!("No default profile set"),
        }
    }
    Ok(())
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
