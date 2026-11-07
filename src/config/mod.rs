use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;

pub(crate) mod storage;

#[derive(Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) profiles: BTreeMap<String, Profile>,
}

impl Config {
    fn validate(&self) -> anyhow::Result<()> {
        for (name, profile) in &self.profiles {
            profile.validate(name)?;
        }

        Ok(())
    }

    pub(crate) fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Profile {
    pub(crate) agent: Agent,
    pub(crate) auth: AuthMode,
    pub(crate) api_key_env: Option<String>,
}

impl Profile {
    fn validate(&self, name: &str) -> anyhow::Result<()> {
        let key_env = self.api_key_env.as_deref().unwrap_or("").trim();
        anyhow::ensure!(
            self.auth != AuthMode::Api || !key_env.is_empty(),
            "Profile '{name}' uses API auth mode but does not specify an API key environment variable."
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Agent {
    ClaudeCode,
    Codex,
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthMode {
    Subscription,
    Api,
}

impl fmt::Display for AuthMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Subscription => "subscription",
            Self::Api => "api",
        };
        formatter.write_str(name)
    }
}
