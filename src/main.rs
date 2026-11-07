mod agents;
mod app;
mod cli;
mod config;
mod credentials;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let path = config::paths::default_path()?;

    cli.execute(&path)
}
