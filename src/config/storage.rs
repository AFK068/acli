use anyhow::Context;
use std::io::Write;

use super::{Config, Profile};

pub(crate) fn default_path() -> anyhow::Result<std::path::PathBuf> {
    let directories = directories::ProjectDirs::from("", "", "acli")
        .context("Could not determine the configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub(crate) fn load(path: &std::path::Path) -> anyhow::Result<Config> {
    let config_str = read(path)?;
    parse(&config_str).with_context(|| format!("Invalid configuration '{}'", path.display()))
}

pub(crate) fn add_profile(
    path: &std::path::Path,
    name: &str,
    profile: Profile,
) -> anyhow::Result<()> {
    // Reload after the prompts so edits made while answering are preserved.
    let input = read(path)?;
    let config =
        parse(&input).with_context(|| format!("Invalid configuration '{}'", path.display()))?;
    config.validate_profile_name(name)?;
    profile.validate(name)?;

    let mut document: toml_edit::DocumentMut = input.parse()?;
    document
        .entry("profiles")
        .or_insert(toml_edit::table())
        .as_table_like_mut()
        .context("Expected profiles to be a TOML table")?
        .insert(name, toml_edit::Item::Table(profile_table(&profile)));

    write(path, &document.to_string())
}

fn write(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("Could not create directory '{}'", parent.display()))?;
    // Replace the original only after the entire new file has been written.
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Could not save configuration '{}'", path.display()))?;
    Ok(())
}

fn read(path: &std::path::Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(input) => Ok(input),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => {
            Err(error).with_context(|| format!("Could not read configuration '{}'", path.display()))
        }
    }
}

fn parse(input: &str) -> anyhow::Result<Config> {
    let config: Config = toml::from_str(input)?;
    config.validate()?;
    Ok(config)
}

fn profile_table(profile: &Profile) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["agent"] = toml_edit::value(profile.agent.to_string());
    table["auth"] = toml_edit::value(profile.auth.to_string());
    if let Some(key) = &profile.api_key_env {
        table["api_key_env"] = toml_edit::value(key);
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, AuthMode};

    fn subscription_profile() -> Profile {
        Profile {
            agent: Agent::Codex,
            auth: AuthMode::Subscription,
            api_key_env: None,
        }
    }

    #[test]
    fn creates_config_and_preserves_existing_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("acli/config.toml");
        assert!(load(&path)?.profiles.is_empty());
        assert!(!path.exists());

        add_profile(&path, "personal", subscription_profile())?;
        add_profile(
            &path,
            "work.test",
            Profile {
                agent: Agent::ClaudeCode,
                auth: AuthMode::Api,
                api_key_env: Some("WORK_KEY".into()),
            },
        )?;

        let config = load(&path)?;
        assert_eq!(config.profiles.len(), 2);
        assert!(config.get_profile("personal").is_some());
        let work = config.get_profile("work.test").expect("new profile");
        assert!(matches!(work.agent, Agent::ClaudeCode));
        assert_eq!(work.api_key_env.as_deref(), Some("WORK_KEY"));
        Ok(())
    }

    #[test]
    fn preserves_comments_and_unknown_settings() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for input in [
            "# My settings\ncustom = true\n[profiles.personal]\nagent = 'codex' # keep this\nauth = 'subscription'\n",
            "# My settings\ncustom = true\nprofiles = { personal = { agent = 'codex', auth = 'subscription' } } # keep this\n",
        ] {
            std::fs::write(&path, input)?;
            add_profile(&path, "second", subscription_profile())?;
            let saved = std::fs::read_to_string(&path)?;
            assert!(saved.contains("# My settings"));
            assert!(saved.contains("# keep this"));
            assert!(saved.contains("custom = true"));
            assert_eq!(load(&path)?.profiles.len(), 2);
        }
        Ok(())
    }

    #[test]
    fn refuses_duplicate_names_without_changing_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "work", subscription_profile())?;
        let before = std::fs::read(&path)?;
        let error = add_profile(&path, "work", subscription_profile()).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(std::fs::read(&path)?, before);
        Ok(())
    }

    #[test]
    fn refuses_invalid_config_and_invalid_new_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let malformed = "[profiles.work";
        std::fs::write(&path, malformed)?;
        assert!(add_profile(&path, "new", subscription_profile()).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, malformed);

        let missing = directory.path().join("new/config.toml");
        assert!(add_profile(&missing, "   ", subscription_profile()).is_err());
        assert!(
            add_profile(
                &missing,
                "new",
                Profile {
                    agent: Agent::Codex,
                    auth: AuthMode::Api,
                    api_key_env: None,
                }
            )
            .is_err()
        );
        assert!(!missing.exists());
        assert!(load(directory.path()).is_err());
        Ok(())
    }

    #[test]
    fn example_config_is_valid() -> anyhow::Result<()> {
        let config = parse(include_str!("../../config.example.toml"))?;
        assert!(!config.profiles.is_empty());
        Ok(())
    }

    #[test]
    fn accepts_config_without_profiles() -> anyhow::Result<()> {
        for input in ["", " \n", "# No profiles yet\n", "[profiles]\n"] {
            let config = parse(input)?;
            assert!(config.profiles.is_empty());
        }
        Ok(())
    }

    #[test]
    fn parses_named_profiles() -> anyhow::Result<()> {
        let config = parse(
            r#"
[profiles.personal]
agent = "claude-code"
auth = "subscription"

[profiles.work]
agent = "codex"
auth = "api"
api_key_env = "CODEX_WORK_KEY"
"#,
        )?;

        let personal = config.get_profile("personal").expect("personal profile");
        assert!(matches!(personal.agent, Agent::ClaudeCode));
        assert_eq!(personal.auth, AuthMode::Subscription);
        assert!(personal.api_key_env.is_none());

        let work = config.get_profile("work").expect("work profile");
        assert!(matches!(work.agent, Agent::Codex));
        assert_eq!(work.auth, AuthMode::Api);
        assert_eq!(work.api_key_env.as_deref(), Some("CODEX_WORK_KEY"));
        assert!(config.get_profile("missing").is_none());
        Ok(())
    }

    #[test]
    fn rejects_api_profiles_without_a_key_env_name() {
        let profile = "[profiles.work]\nagent = 'codex'\nauth = 'api'\n";

        for field in ["", "api_key_env = ''", "api_key_env = '   '"] {
            let Err(error) = parse(&format!("{profile}{field}")) else {
                panic!("accepted API profile with api_key_env input {field:?}");
            };
            assert!(error.to_string().contains("Profile 'work'"), "{error}");
        }
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse("[profiles.work").is_err());
    }
}
