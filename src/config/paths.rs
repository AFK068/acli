use anyhow::Context;

use super::{Config, Profile};

pub(crate) fn default_path() -> anyhow::Result<std::path::PathBuf> {
    let directories = directories::ProjectDirs::from("", "", "acli")
        .context("Could not determine the configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub(crate) fn profile_dir(path: &std::path::Path, profile: &Profile) -> std::path::PathBuf {
    parent_dir(path)
        .join("profiles")
        .join(profile.id.to_string())
        .join(profile.agent.to_string())
}

pub(super) fn create_profile_dirs(path: &std::path::Path, config: &Config) -> anyhow::Result<()> {
    for profile in config.profiles.values() {
        create_dir(&profile_dir(path, profile))?;
    }
    Ok(())
}

pub(super) fn parent_dir(path: &std::path::Path) -> &std::path::Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
}

pub(super) fn create_dir(path: &std::path::Path) -> anyhow::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .with_context(|| format!("Could not create directory '{}'", path.display()))
}
