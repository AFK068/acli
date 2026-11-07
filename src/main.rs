mod cli;
mod config;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let path = config::storage::default_path()?;
    let config = config::storage::load(&path)?;

    cli.execute(&config)
}
