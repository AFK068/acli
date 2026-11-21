use crate::skills::{self, source::Source};
use anyhow::{Context, ensure};
use clap::Subcommand;
use std::{
    io::IsTerminal,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub(super) enum Command {
    /// List installed shared skills.
    List,
    /// Install a skill from a local directory, owner/repo or GitHub URL.
    Add {
        source: String,
        /// Choose a skill by name or relative path.
        #[arg(long)]
        skill: Option<String>,
    },
    /// Remove an installed skill.
    Remove { name: String },
}

impl Command {
    pub(super) fn execute(&self, config_path: &Path) -> anyhow::Result<()> {
        let root = skills::directory()?;
        match self {
            Self::List => {
                let config = crate::app::profiles::load(config_path)?;
                skills::refresh(&root, &skills::claude::directory()?, config_path, &config)?;
                let names = skills::list(&root)?;
                if names.is_empty() {
                    println!("No shared skills installed.");
                }
                for name in names {
                    println!("{}", name.to_string_lossy());
                }
            }
            Self::Add { source, skill } => {
                let source = Source::open(source)?;
                let Some(path) = select_skill(&source, skill.as_deref())? else {
                    return Ok(());
                };
                let installed = skills::install(&root, &path)?;
                println!("Installed {}", installed.display());
            }
            Self::Remove { name } => {
                skills::remove(&root, name)?;
                println!("Removed {name}");
            }
        }
        Ok(())
    }
}

fn select_skill(source: &Source, name: Option<&str>) -> anyhow::Result<Option<PathBuf>> {
    let mut paths = source.candidates()?;
    if let Some(name) = name {
        paths.retain(|path| {
            path.file_name().is_some_and(|file| file == name)
                || (name.contains(['/', '\\'])
                    && path.strip_prefix(&source.root) == Ok(Path::new(name)))
        });
        ensure!(
            paths.len() <= 1,
            "Ambiguous skill name. Use its relative path"
        );
        return Ok(Some(paths.pop().context("Skill not found in source")?));
    }
    if paths.len() == 1 {
        return Ok(paths.pop());
    }
    let mut labels = Vec::new();
    for path in &paths {
        labels.push(path.strip_prefix(&source.root)?.display().to_string());
    }
    ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "Choose with --skill: {}",
        labels.join(", ")
    );
    let selected = dialoguer::Select::new()
        .with_prompt("Skill")
        .items(&labels)
        .default(0)
        .interact_opt()?;
    Ok(selected.map(|i| paths.swap_remove(i)))
}
