use serde::Deserialize;
use std::collections::BTreeMap;

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
        let config: Config = toml::from_str(&config_str)?;
        config.validate()?;
        Ok(config)
    }

    // Retrieves a profile by name from the configuration.
    pub(crate) fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}

// Represents the configuration for the application, including profiles.
#[derive(Deserialize)]
pub(crate) struct Config {
    pub(crate) profiles: std::collections::BTreeMap<String, Profile>,
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

// Represents the different authentication modes that can be used in the application.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthMode {
    Subscription,
    Api,
}
