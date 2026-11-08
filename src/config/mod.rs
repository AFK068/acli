use anyhow::Context;
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
            !self.profiles.contains_key(name),
            "Profile '{name}' already exists"
        );
        Ok(())
    }

    pub(crate) fn profile(&self, name: &str) -> anyhow::Result<&Profile> {
        self.profiles
            .get(name)
            .with_context(|| format!("Profile '{name}' not found"))
    }

    pub(crate) fn validate_new_profile(&self, name: &str, profile: &Profile) -> anyhow::Result<()> {
        self.validate_profile_name(name)?;
        profile.validate(name)?;
        anyhow::ensure!(
            !self
                .profiles
                .values()
                .any(|existing| existing.id == profile.id),
            "Profile '{name}' has a duplicate ID"
        );
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) id: uuid::Uuid,
    pub(crate) agent: Agent,
    pub(crate) auth: AuthMode,
}

impl Profile {
    pub(crate) fn new(agent: Agent, auth: AuthMode) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            agent,
            auth,
        }
    }

    pub(crate) fn validate(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.is_nil(), "Profile '{name}' has an empty ID");
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
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

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AuthMode {
    Native,
    Api,
}

impl fmt::Display for AuthMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Native => "native",
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
id = "8d8ed9a1-9c7e-4ad3-a69a-6e9dc9a05745"
agent = "claude-code"
auth = "native"

[profiles.work]
id = "5be806a5-2288-4bc4-918c-dc463f751132"
agent = "codex"
auth = "api"
"#,
        )?;

        config.validate()?;
        let personal = config.profile("personal")?;
        assert!(matches!(personal.agent, Agent::ClaudeCode));
        assert_eq!(personal.auth, AuthMode::Native);

        let work = config.profile("work")?;
        assert!(matches!(work.agent, Agent::Codex));
        assert_eq!(work.auth, AuthMode::Api);
        assert!(config.profile("missing").is_err());
        Ok(())
    }

    #[test]
    fn accepts_api_profile_with_system_credentials() -> anyhow::Result<()> {
        Profile::new(Agent::Codex, AuthMode::Api).validate("work")
    }

    #[test]
    fn rejects_unknown_profile_fields() {
        let input = format!(
            "id = '{}'\nagent = 'codex'\nauth = 'api'\nunknown = 'value'",
            uuid::Uuid::new_v4()
        );
        let error = toml::from_str::<Profile>(&input).unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_nil_profile_id() {
        let mut profile = Profile::new(Agent::Codex, AuthMode::Native);
        profile.id = uuid::Uuid::nil();
        let error = profile.validate("personal").unwrap_err();
        assert!(error.to_string().contains("empty ID"));
    }

    #[test]
    fn rejects_duplicate_profile_ids() {
        let first = Profile::new(Agent::Codex, AuthMode::Native);
        let mut second = Profile::new(Agent::ClaudeCode, AuthMode::Native);
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
        let profile = Profile::new(Agent::Codex, AuthMode::Native);
        let config = Config {
            profiles: BTreeMap::from([("work".into(), profile)]),
        };
        let error = config.validate_profile_name("work").unwrap_err();
        assert!(error.to_string().contains("already exists"));
        config.validate_profile_name("personal")
    }
}
