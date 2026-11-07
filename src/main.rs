mod cli;
mod config;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let path = config::paths::default_path()?;

    cli.execute(&path)
}
