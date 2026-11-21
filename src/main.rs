mod agents;
mod app;
mod cli;
mod config;
mod credentials;
mod skills;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let path = config::paths::default_path()?;

    cli.execute(&path)
}

#[cfg(test)]
mod test_support;
