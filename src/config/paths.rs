use crate::files::parent_dir;
use anyhow::Context;
use std::path::{Path, PathBuf};

use super::Profile;

pub(crate) fn default_path() -> anyhow::Result<PathBuf> {
    let directories = directories::ProjectDirs::from("", "", "acli")
        .context("Could not determine the configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub(crate) fn profile_dir(path: &Path, profile: &Profile) -> PathBuf {
    parent_dir(path)
        .join("profiles")
        .join(profile.id.to_string())
        .join(profile.agent.to_string())
}
