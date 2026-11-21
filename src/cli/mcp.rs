use crate::agents::mcp;
use crate::config::{McpServer, McpServers, storage};
use clap::Subcommand;
use dialoguer::Input;
use std::{io::IsTerminal, path::Path};

#[derive(Subcommand)]
pub(super) enum Command {
    /// Add interactively, or use: acli mcp add NAME -- PROGRAM [ARGS...]
    Add {
        #[arg(requires = "program")]
        name: Option<String>,
        #[arg(last = true, num_args = 1.., requires = "name")]
        program: Vec<String>,
    },
    /// List shared servers.
    List,
    /// Remove a server added with acli. Remove native servers in their original agent.
    Remove { name: String },
}

impl Command {
    pub(super) fn execute(&self, path: &Path) -> anyhow::Result<()> {
        match self {
            Self::Add { name, program } => {
                let (name, server) = match (name, McpServer::from_argv(program.clone())) {
                    (Some(name), Some(server)) => (name.clone(), server),
                    _ => prompt()?,
                };
                storage::update_mcp(path, &name, Some(&server))?;
                println!("Added MCP server '{name}'");
            }
            Self::List => {
                let servers = list(path)?;
                if servers.is_empty() {
                    println!("No shared MCP servers.");
                }
                for (name, server) in servers {
                    println!("{name}\t{}", server.command);
                }
            }
            Self::Remove { name } => {
                storage::update_mcp(path, name, None)?;
                println!("Removed MCP server '{name}'");
            }
        }
        Ok(())
    }
}

pub(super) fn list(path: &Path) -> anyhow::Result<McpServers> {
    mcp::shared(path, &storage::load(path)?, None, &mcp::user_dirs()?)
}

fn prompt() -> anyhow::Result<(String, McpServer)> {
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "Use: acli mcp add NAME -- PROGRAM [ARGS...]"
    );
    let name = Input::new().with_prompt("Server name").interact_text()?;
    let command = Input::new().with_prompt("Executable").interact_text()?;
    let mut args = Vec::new();
    loop {
        let arg: String = Input::new()
            .with_prompt("Next argument (Enter to finish)")
            .allow_empty(true)
            .interact_text()?;
        if arg.is_empty() {
            break;
        }
        args.push(arg);
    }
    Ok((name, McpServer { command, args }))
}
