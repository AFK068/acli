use anyhow::{Context, ensure};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub(crate) mod claude;
mod links;
mod sync;
pub(crate) use sync::refresh;
pub(crate) mod source;
pub(crate) use links::connect;

pub(crate) fn directory() -> anyhow::Result<PathBuf> {
    let home = std::env::home_dir().context("Could not find home directory")?;
    Ok(home.join(".agents/skills"))
}

fn profile_skills(agent: crate::config::Agent) -> &'static str {
    use crate::config::Agent::*;
    match agent {
        OpenCode => "config/opencode/skills",
        ZCode => ".zcode/skills",
        ClaudeCode | Codex | Kimi | Pi => "skills",
    }
}

fn collect_skills(root: &Path, folders: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    if fs::read_link(root).is_err() {
        folders.extend(list(root)?.into_iter().map(|name| root.join(name)));
    }
    Ok(())
}

pub(crate) fn list(root: &Path) -> anyhow::Result<Vec<OsString>> {
    let mut names = Vec::new();
    if !root.try_exists()? {
        return Ok(names);
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with('.')
            && entry.path().join("SKILL.md").is_file()
        {
            names.push(entry.file_name());
        }
    }
    names.sort();
    Ok(names)
}

pub(crate) fn install(root: &Path, source: &Path) -> anyhow::Result<PathBuf> {
    let source = source.canonicalize()?;
    ensure!(source.join("SKILL.md").is_file(), "Missing SKILL.md");
    let name = source.file_name().context("Missing skill name")?;
    ensure!(
        !name.to_string_lossy().starts_with('.'),
        "Choose a non-hidden skill directory"
    );
    fs::create_dir_all(root)?;
    ensure!(
        !root.canonicalize()?.starts_with(&source),
        "Cannot install a skill inside itself"
    );
    let _lock = lock(root)?;
    let dest = root.join(name);
    ensure!(
        fs::symlink_metadata(&dest).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
        "Cannot install '{}': destination already exists or is inaccessible",
        dest.display()
    );

    // Publish only after the whole skill, including its supporting files, is copied.
    let staging = tempfile::tempdir_in(root)?;
    for entry in walkdir::WalkDir::new(&source)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let entry = entry?;
        let target = staging.path().join(entry.path().strip_prefix(&source)?);
        if entry.file_type().is_dir() && fs::read_link(entry.path()).is_err() {
            fs::create_dir(target)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            anyhow::bail!(
                "Unsupported link or special file: {}",
                entry.path().display()
            );
        }
    }
    fs::rename(staging.path(), &dest)?;
    Ok(dest)
}

fn valid_name(name: &str) -> bool {
    Path::new(name).file_name() == Some(name.as_ref()) && !name.starts_with('.')
}

pub(crate) fn remove(root: &Path, name: &str) -> anyhow::Result<()> {
    ensure!(valid_name(name), "Expected a skill name");
    ensure!(root.try_exists()?, "No shared skills installed");
    let _lock = lock(root)?;
    ensure!(
        !sync::managed(root, name)?,
        "Remove this skill in the profile where it was installed"
    );
    let path = root.join(name);
    ensure!(path.join("SKILL.md").is_file(), "Skill '{name}' not found");
    // remove_dir_all unlinks directory symlinks without following them.
    fs::remove_dir_all(path)?;
    Ok(())
}

fn lock(root: &Path) -> anyhow::Result<fs::File> {
    let file = fs::File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".acli.lock"))?;
    file.lock()?;
    Ok(file)
}
