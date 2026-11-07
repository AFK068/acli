use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

pub(crate) mod paths;
pub(crate) mod storage;

#[derive(Deserialize)]
pub(crate) struct Config {
    #[serde(default)]
    pub(crate) profiles: BTreeMap<String, Profile>,
}

impl Config {
    fn validate(&self) -> anyhow::Result<()> {
        let mut ids = HashSet::new();
        for (name, profile) in &self.profiles {
            profile.validate(name)?;
            anyhow::ensure!(
                ids.insert(profile.id),
                "Profile '{name}' has a duplicate ID"
            );
        }

        Ok(())
    }

    pub(crate) fn validate_profile_name(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!name.trim().is_empty(), "Profile name must not be empty");
        anyhow::ensure!(
            self.get_profile(name).is_none(),
            "Profile '{name}' already exists"
        );
        Ok(())
    }

    pub(crate) fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Profile {
    // Storage saves generated IDs when migrating existing profiles.
    #[serde(default = "uuid::Uuid::new_v4")]
    pub(crate) id: uuid::Uuid,
    pub(crate) agent: Agent,
    pub(crate) auth: AuthMode,
    pub(crate) api_key_env: Option<String>,
}

impl Profile {
    pub(crate) fn new(agent: Agent, auth: AuthMode, api_key_env: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            agent,
            auth,
            api_key_env,
        }
    }

    fn validate(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.is_nil(), "Profile '{name}' has an empty ID");
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
