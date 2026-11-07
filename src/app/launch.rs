use anyhow::Context;
use std::path::Path;
use std::process::Command;

use super::profiles;
use crate::config::{AuthMode, paths};

pub(crate) fn prepare(path: &Path, name: &str) -> anyhow::Result<Command> {
    let config = profiles::load(path)?;
    let profile = config
        .get_profile(name)
        .with_context(|| format!("Profile '{name}' not found"))?;

    let key = match profile.auth {
        AuthMode::Subscription => None,
        AuthMode::Api => {
            let key = match &profile.api_key_env {
                Some(variable) => std::env::var(variable).map_err(|_| {
                    anyhow::anyhow!("Set environment variable '{variable}' for profile '{name}'")
                })?,
                None => crate::credentials::load(profile.id)
                    .with_context(|| format!("Could not load API key for profile '{name}'"))?,
            };
            anyhow::ensure!(
                !key.trim().is_empty(),
                "API key for profile '{name}' is empty"
            );
            Some(key)
        }
    };

    let directory = paths::profile_dir(path, profile);
    Ok(crate::agents::command(profile, &directory, key.as_deref()))
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
    fn reports_missing_api_key_variable() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let variable = format!("ACLI_TEST_{}", uuid::Uuid::new_v4().simple());
        let profile = Profile::new(Agent::Codex, AuthMode::Api, Some(variable.clone()));
        storage::add_profile(&path, "work", profile)?;
        let error = prepare(&path, "work").unwrap_err();
        assert!(error.to_string().contains(&variable));
        assert!(error.to_string().contains("work"));
        Ok(())
    }

    #[test]
    fn prepares_profile_directory_without_changing_working_directory() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Subscription, None);
        let expected = paths::profile_dir(&path, &profile);
        storage::add_profile(&path, "personal", profile)?;
        let command = prepare(&path, "personal")?;
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
