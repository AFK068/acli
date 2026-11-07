use anyhow::Context;
use std::path::Path;

use crate::config::{AuthMode, Config, Profile, storage};
use crate::credentials;

pub(crate) fn load(path: &Path) -> anyhow::Result<Config> {
    storage::load(path)
}

pub(crate) fn add(
    path: &Path,
    name: &str,
    profile: Profile,
    key: Option<&str>,
) -> anyhow::Result<()> {
    let config = load(path)?;
    config.validate_profile_name(name)?;
    profile.validate(name)?;
    let id = profile.id;
    anyhow::ensure!(
        !config.profiles.values().any(|existing| existing.id == id),
        "Profile '{name}' has a duplicate ID"
    );

    let needs_key = profile.auth == AuthMode::Api && profile.api_key_env.is_none();
    if needs_key {
        credentials::save(id, key.context("API key is required")?)?;
    }

    if let Err(error) = storage::add_profile(path, name, profile) {
        if needs_key {
            credentials::delete(id).with_context(|| {
                format!("{error}; could not remove the unsaved profile's API key")
            })?;
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Agent;

    fn api_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Api, None)
    }

    #[test]
    fn saves_key_separately_and_uses_it_for_launch() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = api_profile();
        let id = profile.id;
        add(&path, "work", profile, Some("test-secret"))?;
        let saved = std::fs::read_to_string(&path)?;
        assert!(!saved.contains("test-secret"));
        assert!(!saved.contains("api_key_env"));
        assert_eq!(credentials::load(id)?, "test-secret");

        let command = crate::app::launch::prepare(&path, "work")?;
        assert!(command.get_envs().any(|(name, value)| {
            name == "ACLI_API_KEY" && value == Some(std::ffi::OsStr::new("test-secret"))
        }));
        Ok(())
    }

    #[test]
    fn renaming_profile_keeps_its_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add(&path, "work", api_profile(), Some("test-secret"))?;
        let saved = std::fs::read_to_string(&path)?;
        std::fs::write(
            &path,
            saved.replace("[profiles.work]", "[profiles.renamed]"),
        )?;
        let command = crate::app::launch::prepare(&path, "renamed")?;
        assert!(command.get_envs().any(|(name, value)| {
            name == "ACLI_API_KEY" && value == Some(std::ffi::OsStr::new("test-secret"))
        }));
        Ok(())
    }

    #[test]
    fn removes_key_if_saving_profile_fails() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        std::fs::write(directory.path().join("profiles"), "not a directory")?;
        let profile = api_profile();
        let id = profile.id;
        assert!(add(&path, "work", profile, Some("test-secret")).is_err());
        assert!(!path.exists());
        assert!(
            credentials::load(id)
                .unwrap_err()
                .to_string()
                .contains("No API key saved")
        );
        Ok(())
    }

    #[test]
    fn duplicate_name_does_not_replace_existing_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let first = api_profile();
        let id = first.id;
        add(&path, "work", first, Some("original-key"))?;
        let saved = std::fs::read(&path)?;
        assert!(add(&path, "work", api_profile(), Some("replacement-key")).is_err());
        assert_eq!(credentials::load(id)?, "original-key");
        assert_eq!(std::fs::read(&path)?, saved);
        Ok(())
    }

    #[test]
    fn store_failure_does_not_save_profile_or_key_to_disk() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = api_profile();
        let entry = credentials::entry(profile.id)?;
        let mock = entry
            .inner
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .unwrap();
        mock.set_error(keyring::Error::NoStorageAccess(Box::new(
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        )));
        let error = add(&path, "work", profile, Some("test-secret")).unwrap_err();
        assert!(!path.exists());
        assert!(!format!("{error:?}").contains("test-secret"));
        Ok(())
    }

    #[test]
    fn requires_key_for_system_store_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for key in [None, Some(""), Some("   ")] {
            assert!(add(&path, "work", api_profile(), key).is_err());
            assert!(!path.exists());
        }
        Ok(())
    }

    #[test]
    fn keeps_environment_profiles_supported() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Api, Some("WORK_KEY".into()));
        add(&path, "work", profile, None)?;
        let config = storage::load(&path)?;
        assert_eq!(
            config.get_profile("work").unwrap().api_key_env.as_deref(),
            Some("WORK_KEY")
        );
        Ok(())
    }
}
