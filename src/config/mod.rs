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
    // API profiles without this field use the system credential store.
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

    pub(crate) fn validate(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.is_nil(), "Profile '{name}' has an empty ID");
        if let (AuthMode::Api, Some(variable)) = (&self.auth, &self.api_key_env) {
            anyhow::ensure!(
                !variable.trim().is_empty(),
                "Profile '{name}' has an empty API key environment variable name"
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_is_valid() -> anyhow::Result<()> {
        let config: Config = toml::from_str(include_str!("../../config.example.toml"))?;
        config.validate()?;
        assert!(!config.profiles.is_empty());
        Ok(())
    }

    #[test]
    fn accepts_config_without_profiles() -> anyhow::Result<()> {
        for input in ["", " \n", "# No profiles yet\n", "[profiles]\n"] {
            let config: Config = toml::from_str(input)?;
            config.validate()?;
            assert!(config.profiles.is_empty());
        }
        Ok(())
    }

    #[test]
    fn parses_named_profiles() -> anyhow::Result<()> {
        let config: Config = toml::from_str(
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

        config.validate()?;
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
    fn rejects_empty_api_key_env_names() {
        for key_env in ["", "   "] {
            let profile = Profile::new(Agent::Codex, AuthMode::Api, Some(key_env.into()));
            let error = profile.validate("work").unwrap_err();
            assert!(error.to_string().contains("Profile 'work'"), "{error}");
        }
    }

    #[test]
    fn accepts_api_profile_with_system_credentials() -> anyhow::Result<()> {
        Profile::new(Agent::Codex, AuthMode::Api, None).validate("work")
    }

    #[test]
    fn accepts_api_profile_with_a_key_env_name() -> anyhow::Result<()> {
        let profile = Profile::new(Agent::Codex, AuthMode::Api, Some("WORK_KEY".into()));
        profile.validate("work")
    }

    #[test]
    fn rejects_nil_profile_id() {
        let mut profile = Profile::new(Agent::Codex, AuthMode::Subscription, None);
        profile.id = uuid::Uuid::nil();
        let error = profile.validate("personal").unwrap_err();
        assert!(error.to_string().contains("empty ID"));
    }

    #[test]
    fn rejects_duplicate_profile_ids() {
        let first = Profile::new(Agent::Codex, AuthMode::Subscription, None);
        let mut second = Profile::new(Agent::ClaudeCode, AuthMode::Subscription, None);
        second.id = first.id;
        let config = Config {
            profiles: BTreeMap::from([("first".into(), first), ("second".into(), second)]),
        };
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("duplicate ID"));
    }

    #[test]
    fn rejects_blank_profile_names() {
        let config = Config {
            profiles: BTreeMap::new(),
        };
        for name in ["", "   ", "\t\n"] {
            let error = config.validate_profile_name(name).unwrap_err();
            assert!(error.to_string().contains("must not be empty"));
        }
    }

    #[test]
    fn rejects_existing_profile_name() -> anyhow::Result<()> {
        let profile = Profile::new(Agent::Codex, AuthMode::Subscription, None);
        let config = Config {
            profiles: BTreeMap::from([("work".into(), profile)]),
        };
        let error = config.validate_profile_name("work").unwrap_err();
        assert!(error.to_string().contains("already exists"));
        config.validate_profile_name("personal")
    }
}
