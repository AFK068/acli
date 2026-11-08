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
    use crate::config::Agent;

    fn api_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Api)
    }

    #[test]
    fn saves_key_separately_and_uses_it_for_launch() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for (agent, variable) in [
            (Agent::Codex, "ACLI_API_KEY"),
            (Agent::ClaudeCode, "ANTHROPIC_API_KEY"),
            (Agent::Kimi, "KIMI_MODEL_API_KEY"),
        ] {
            let name = agent.to_string();
            let profile = Profile::new(agent, AuthMode::Api);
            let id = profile.id;
            add(&path, &name, profile, Some("test-secret"))?;
            let saved = std::fs::read_to_string(&path)?;
            assert!(!saved.contains("test-secret"));
            assert_eq!(credentials::load(id)?, "test-secret");

            let command = crate::app::launch::prepare(&path, &name)?;
            assert!(command.get_envs().any(|(name, value)| {
                name == variable && value == Some(std::ffi::OsStr::new("test-secret"))
            }));
        }
        Ok(())
    }

    #[test]
    fn renaming_profile_keeps_its_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add(&path, "work", api_profile(), Some("test-secret"))?;
        rename(&path, "work", "renamed")?;
        let command = crate::app::launch::prepare(&path, "renamed")?;
        assert!(command.get_envs().any(|(name, value)| {
            name == "ACLI_API_KEY" && value == Some(std::ffi::OsStr::new("test-secret"))
        }));
        Ok(())
    }

    #[test]
    fn opencode_uses_saved_provider_and_key() -> anyhow::Result<()> {
        use crate::config::Provider;
        use std::collections::BTreeMap;
        use std::ffi::OsStr;

        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for (provider, name, variable) in [
            (Provider::OpenAi, "openai", "OPENAI_API_KEY"),
            (Provider::Anthropic, "anthropic", "ANTHROPIC_API_KEY"),
            (
                Provider::KimiCodePlanCn,
                "kimi-code-plan-cn",
                "KIMI_API_KEY",
            ),
            (
                Provider::KimiCodePlanGlobal,
                "kimi-code-plan-global",
                "KIMI_API_KEY",
            ),
        ] {
            let mut profile = Profile::new(Agent::OpenCode, AuthMode::Api);
            profile.provider = Some(provider);
            let id = profile.id;
            add(&path, name, profile, Some("test-opencode-secret"))?;
            let saved = std::fs::read_to_string(&path)?;
            assert!(saved.contains(&format!("provider = \"{name}\"")));
            assert!(!saved.contains("test-opencode-secret"));
            assert_eq!(load(&path)?.profile(name)?.provider, Some(provider));

            rename(&path, name, "renamed")?;
            let command = crate::app::launch::prepare(&path, "renamed")?;
            let env: BTreeMap<_, _> = command.get_envs().collect();
            assert_eq!(
                env[OsStr::new(variable)],
                Some(OsStr::new("test-opencode-secret"))
            );
            for other in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "KIMI_API_KEY"] {
                if other != variable {
                    assert_eq!(env[OsStr::new(other)], None);
                }
            }
            let policy = env[OsStr::new("OPENCODE_CONFIG_CONTENT")]
                .unwrap()
                .to_str()
                .unwrap();
            assert_eq!(
                policy
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>(),
                format!(
                    r#"{{"policies":[{{"action":"provider.use","resource":"*","effect":"deny"}},{{"action":"provider.use","resource":"{name}","effect":"allow"}}]}}"#
                )
            );
            assert!(!format!("{:?}", command.get_args()).contains("test-opencode-secret"));

            remove(&path, "renamed")?;
            assert!(credentials::load(id).is_err());
        }
        Ok(())
    }

    #[test]
    fn missing_opencode_provider_does_not_save_key_or_config() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::OpenCode, AuthMode::Api);
        let id = profile.id;
        let error = add(&path, "work", profile, Some("test-secret")).unwrap_err();
        assert!(error.to_string().contains("select an OpenCode provider"));
        assert!(!path.exists());
        assert!(credentials::load(id).is_err());
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
    fn removes_profile_and_its_saved_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = api_profile();
        let id = profile.id;
        add(&path, "work", profile, Some("test-secret"))?;

        remove(&path, "work")?;

        assert!(load(&path)?.profiles.is_empty());
        assert!(
            credentials::load(id)
                .unwrap_err()
                .to_string()
                .contains("No API key saved")
        );
        Ok(())
    }

    #[test]
    fn removes_profiles_without_saved_keys() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for profile in [Profile::new(Agent::Codex, AuthMode::Native), api_profile()] {
            storage::add_profile(&path, "work", profile)?;
            remove(&path, "work")?;
            assert!(load(&path)?.profiles.is_empty());
        }
        Ok(())
    }

    #[test]
    fn reports_key_cleanup_failure_after_removing_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = api_profile();
        let id = profile.id;
        add(&path, "work", profile, Some("test-secret"))?;
        let entry = credentials::entry(id)?;
        entry
            .inner
            .as_any()
            .downcast_ref::<keyring_core::mock::Cred>()
            .unwrap()
            .set_error(keyring::Error::NoStorageAccess(Box::new(
                std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            )));

        let error = remove(&path, "work").unwrap_err();

        assert!(error.to_string().contains("was removed, but"));
        assert!(!format!("{error:?}").contains("test-secret"));
        assert!(load(&path)?.profiles.is_empty());
        assert_eq!(credentials::load(id)?, "test-secret");
        credentials::delete(id)?;
        Ok(())
    }
}
