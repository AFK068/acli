use anyhow::Context;

use super::Config;

pub(crate) fn default_path() -> anyhow::Result<std::path::PathBuf> {
    let directories = directories::ProjectDirs::from("", "", "acli")
        .context("Could not determine the configuration directory")?;
    Ok(directories.config_dir().join("config.toml"))
}

pub(crate) fn load(path: &std::path::Path) -> anyhow::Result<Config> {
    let config_str = read(path)?;
    parse(&config_str).with_context(|| format!("Invalid configuration '{}'", path.display()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, AuthMode};

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
