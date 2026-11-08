use anyhow::Context;
use std::path::Path;
use std::process::Command;

use super::profiles;
use crate::config::{AuthMode, paths};

pub(crate) fn prepare(path: &Path, name: &str) -> anyhow::Result<Command> {
    let config = profiles::load(path)?;
    let profile = config.profile(name)?;

    let key = match profile.auth {
        AuthMode::Native => None,
        AuthMode::Api => {
            let key = crate::credentials::load(profile.id)
                .with_context(|| format!("Could not load API key for profile '{name}'"))?;
            anyhow::ensure!(
                !key.trim().is_empty(),
                "API key for profile '{name}' is empty"
            );
            Some(key)
        }
    };

    let directory = paths::profile_dir(path, profile);
    paths::create_dir(&directory)?;
    Ok(crate::agents::command(
        profile.agent,
        &directory,
        key.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, Profile, storage};

    #[test]
    fn reports_missing_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let error = prepare(&directory.path().join("config.toml"), "missing").unwrap_err();
        assert_eq!(error.to_string(), "Profile 'missing' not found");
        Ok(())
    }

    #[test]
    fn reports_missing_saved_api_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Api);
        storage::add_profile(&path, "work", profile)?;
        let error = prepare(&path, "work").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not load API key for profile 'work'"
        );
        assert!(format!("{error:#}").contains("No API key saved"));
        Ok(())
    }

    #[test]
    fn prepares_profile_directory_without_changing_working_directory() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Native);
        let expected = paths::profile_dir(&path, &profile);
        storage::add_profile(&path, "personal", profile)?;
        std::fs::remove_dir(&expected)?;
        let command = prepare(&path, "personal")?;
        assert!(expected.is_dir());
        assert_eq!(command.get_program(), "codex");
        assert!(command.get_current_dir().is_none());
        assert!(
            command.get_envs().any(|(name, value)| {
                name == "CODEX_HOME" && value == Some(expected.as_os_str())
            })
        );
        Ok(())
    }
}
