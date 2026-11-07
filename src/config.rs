use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

// Represents the configuration for the application, including profiles.
#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) profiles: BTreeMap<String, Profile>,
}

// Configuration structure for the application.
impl Config {
    // Validates the configuration, ensuring that profiles using API authentication
    fn validate(&self) -> anyhow::Result<()> {
        for (name, profile) in &self.profiles {
            if profile.auth == AuthMode::Api
                && profile
                    .api_key_env
                    .as_deref()
                    .is_none_or(|name| name.trim().is_empty())
            {
                anyhow::bail!(
                    "Profile '{}' uses API auth mode but does not specify an API key environment variable.",
                    name
                );
            }
        }

        Ok(())
    }

    // Loads the configuration from a TOML file at the specified path.
    pub(crate) fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        Self::parse(&config_str)
    }

    // Parses the configuration from a TOML string.
    fn parse(input: &str) -> anyhow::Result<Self> {
        let config: Self = toml::from_str(input)?;
        config.validate()?;
        Ok(config)
    }

    // Retrieves a profile by name from the configuration.
    pub(crate) fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}

// Represents a profile in the configuration, including agent,
// authentication mode, and optional API key environment variable.
#[derive(Debug, Deserialize)]
pub(crate) struct Profile {
    pub(crate) agent: Agent,
    pub(crate) auth: AuthMode,
    pub(crate) api_key_env: Option<String>,
}

// Represents the different agents that can be used in the application.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Agent {
    ClaudeCode,
    Codex,
}

// Implements the Display trait for the Agent enum to provide a string representation.
impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
        };
        formatter.write_str(name)
    }
}

// Represents the different authentication modes that can be used in the application.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthMode {
    Subscription,
    Api,
}

// Implements the Display trait for the AuthMode enum to provide a string representation.
impl fmt::Display for AuthMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Subscription => "subscription",
            Self::Api => "api",
        };
        formatter.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::{Agent, AuthMode, Config};

    #[test]
    fn parses_named_profiles() -> anyhow::Result<()> {
        let config = Config::parse(
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
            let Err(error) = Config::parse(&format!("{profile}{field}")) else {
                panic!("accepted API profile with api_key_env input {field:?}");
            };
            assert!(error.to_string().contains("Profile 'work'"), "{error}");
        }
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(Config::parse("[profiles.work").is_err());
    }
}
