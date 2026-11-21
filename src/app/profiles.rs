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
    load(path)?.validate_new_profile(name, &profile)?;
    let key = match profile.auth {
        AuthMode::Api => Some(key.context("API key is required")?),
        AuthMode::Native => None,
    };
    let id = profile.id;
    if let Some(key) = key {
        credentials::save(id, key)?;
    }
    if let Err(error) = storage::add_profile(path, name, profile) {
        if key.is_some() {
            credentials::delete(id).with_context(|| {
                format!("{error}; could not remove the unsaved profile's API key")
            })?;
        }
        return Err(error);
    }
    Ok(())
}

pub(crate) fn remove(path: &Path, name: &str) -> anyhow::Result<()> {
    let profile = storage::remove_profile(path, name)?;
    if profile.auth == AuthMode::Api {
        credentials::delete(profile.id).with_context(|| {
            format!("Profile '{name}' was removed, but its saved API key could not be deleted")
        })?;
    }
    Ok(())
}

pub(crate) fn rename(path: &Path, name: &str, new_name: &str) -> anyhow::Result<()> {
    storage::rename_profile(path, name, new_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, Provider};
    use crate::test_support::{TestApp, env};

    fn api_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Api)
    }

    fn deny_keyring_access(id: uuid::Uuid) -> anyhow::Result<()> {
        let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        credentials::set_mock_error(id, keyring::Error::NoStorageAccess(Box::new(error)))
    }

    fn assert_key_missing(id: uuid::Uuid) {
        let error = credentials::load(id).unwrap_err();
        assert!(error.to_string().contains("No API key saved"));
    }

    #[test]
    fn saves_key_separately_and_uses_it_for_launch() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        for (agent, variable) in [
            (Agent::Codex, "ACLI_API_KEY"),
            (Agent::ClaudeCode, "ANTHROPIC_API_KEY"),
            (Agent::Kimi, "KIMI_MODEL_API_KEY"),
        ] {
            let name = agent.to_string();
            let profile = Profile::new(agent, AuthMode::Api);
            let id = profile.id;
            add(path, &name, profile, Some("test-secret"))?;
            let saved = std::fs::read_to_string(path)?;
            assert!(!saved.contains("test-secret"));
            assert_eq!(credentials::load(id)?, "test-secret");

            assert_eq!(
                env(&app.prepare(&name)?)[variable],
                Some("test-secret".as_ref())
            );
        }
        Ok(())
    }

    #[test]
    fn renaming_profile_keeps_its_key() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        add(&app.config, "work", api_profile(), Some("test-secret"))?;
        rename(&app.config, "work", "renamed")?;
        assert_eq!(
            env(&app.prepare("renamed")?)["ACLI_API_KEY"],
            Some("test-secret".as_ref())
        );
        Ok(())
    }

    #[test]
    fn opencode_launch_uses_the_saved_provider() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let mut profile = Profile::new(Agent::OpenCode, AuthMode::Api);
        profile.provider = Some(Provider::Anthropic);
        add(&app.config, "work", profile, Some("test-secret"))?;
        assert_eq!(
            load(&app.config)?.profile("work")?.provider,
            Some(Provider::Anthropic)
        );
        assert_eq!(
            env(&app.prepare("work")?)["ANTHROPIC_API_KEY"],
            Some("test-secret".as_ref())
        );
        Ok(())
    }

    #[test]
    fn missing_opencode_provider_does_not_save_key_or_config() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        let profile = Profile::new(Agent::OpenCode, AuthMode::Api);
        let id = profile.id;
        let error = add(path, "work", profile, Some("test-secret")).unwrap_err();
        assert!(error.to_string().contains("select an OpenCode provider"));
        assert!(!path.exists());
        assert!(credentials::load(id).is_err());
        Ok(())
    }

    #[test]
    fn removes_key_if_saving_profile_fails() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        std::fs::write(app.root.path().join("profiles"), "not a directory")?;
        let profile = api_profile();
        let id = profile.id;
        assert!(add(path, "work", profile, Some("test-secret")).is_err());
        assert!(!path.exists());
        assert_key_missing(id);
        Ok(())
    }

    #[test]
    fn duplicate_name_does_not_replace_existing_key() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        let first = api_profile();
        let id = first.id;
        add(path, "work", first, Some("original-key"))?;
        let saved = std::fs::read(path)?;
        assert!(add(path, "work", api_profile(), Some("replacement-key")).is_err());
        assert_eq!(credentials::load(id)?, "original-key");
        assert_eq!(std::fs::read(path)?, saved);
        Ok(())
    }

    #[test]
    fn store_failure_does_not_save_profile_or_key_to_disk() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        let profile = api_profile();
        deny_keyring_access(profile.id)?;
        let error = add(path, "work", profile, Some("test-secret")).unwrap_err();
        assert!(!path.exists());
        assert!(!format!("{error:?}").contains("test-secret"));
        Ok(())
    }

    #[test]
    fn requires_key_for_system_store_profiles() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        for key in [None, Some(""), Some("   ")] {
            assert!(add(path, "work", api_profile(), key).is_err());
            assert!(!path.exists());
        }
        Ok(())
    }

    #[test]
    fn removes_profile_and_its_saved_key() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        let profile = api_profile();
        let id = profile.id;
        add(path, "work", profile, Some("test-secret"))?;

        remove(path, "work")?;

        assert!(load(path)?.profiles.is_empty());
        assert_key_missing(id);
        Ok(())
    }

    #[test]
    fn removes_profiles_without_saved_keys() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        for profile in [Profile::new(Agent::Codex, AuthMode::Native), api_profile()] {
            storage::add_profile(path, "work", profile)?;
            remove(path, "work")?;
            assert!(load(path)?.profiles.is_empty());
        }
        Ok(())
    }

    #[test]
    fn reports_key_cleanup_failure_after_removing_profile() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        let profile = api_profile();
        let id = profile.id;
        add(path, "work", profile, Some("test-secret"))?;
        deny_keyring_access(id)?;

        let error = remove(path, "work").unwrap_err();

        assert!(error.to_string().contains("was removed, but"));
        assert!(!format!("{error:?}").contains("test-secret"));
        assert!(load(path)?.profiles.is_empty());
        assert_eq!(credentials::load(id)?, "test-secret");
        credentials::delete(id)?;
        Ok(())
    }
}
