mod cli;
mod config;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let config = config::storage::load(std::path::Path::new("config.toml"))?;

    cli.execute(&config)
}
