use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;

mod mcp;
pub(crate) use mcp::{McpServer, McpServers};
pub(crate) mod paths;
pub(crate) mod storage;

#[derive(Default, Deserialize)]
#[serde(default)]
pub(crate) struct Config {
    pub(crate) profiles: BTreeMap<String, Profile>,
    pub(crate) mcp: McpServers,
}

impl Config {
    fn validate(&self) -> anyhow::Result<()> {
        for (name, server) in &self.mcp {
            server.validate(name)?;
        }
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    pub(crate) id: uuid::Uuid,
    pub(crate) agent: Agent,
    pub(crate) auth: AuthMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider: Option<Provider>,
}

impl Profile {
    pub(crate) fn new(agent: Agent, auth: AuthMode) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            agent,
            auth,
            provider: None,
        }
    }

    pub(crate) fn requires_provider(&self) -> bool {
        matches!(self.agent, Agent::OpenCode) && self.auth == AuthMode::Api
    }

    pub(crate) fn validate(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(!self.id.is_nil(), "Profile '{name}' has an empty ID");
        anyhow::ensure!(
            self.auth != AuthMode::Api || self.agent.supports_api_key(),
            "Profile '{name}': API key authentication for {} is not supported yet; sign in inside the agent instead",
            self.agent
        );
        if self.requires_provider() {
            anyhow::ensure!(
                self.provider.is_some(),
                "Profile '{name}': select an OpenCode provider"
            );
        } else {
            anyhow::ensure!(
                self.provider.is_none(),
                "Profile '{name}': provider is only supported for OpenCode API profiles"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Agent {
    ClaudeCode,
    Codex,
    Kimi,
    #[serde(rename = "opencode")]
    OpenCode,
    Pi,
    ZCode,
}

impl Agent {
    pub(crate) fn supports_api_key(self) -> bool {
        matches!(
            self,
            Self::ClaudeCode | Self::Codex | Self::Kimi | Self::OpenCode
        )
    }
}

impl fmt::Display for Agent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Kimi => "kimi",
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::ZCode => "z-code",
        };
        formatter.write_str(name)
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Provider {
    #[serde(rename = "openai")]
    OpenAi,
    Anthropic,
    KimiCodePlanCn,
    KimiCodePlanGlobal,
}

impl fmt::Display for Provider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::KimiCodePlanCn => "kimi-code-plan-cn",
            Self::KimiCodePlanGlobal => "kimi-code-plan-global",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_contains_valid_named_profiles() -> anyhow::Result<()> {
        let config: Config = toml_edit::de::from_str(include_str!("../../config.example.toml"))?;
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
    fn accepts_config_without_profiles() -> anyhow::Result<()> {
        for input in ["", " \n", "# No profiles yet\n", "[profiles]\n"] {
            let config: Config = toml_edit::de::from_str(input)?;
            config.validate()?;
            assert!(config.profiles.is_empty());
        }
        Ok(())
    }

    #[test]
    fn opencode_api_requires_a_provider() -> anyhow::Result<()> {
        let mut profile = Profile::new(Agent::OpenCode, AuthMode::Api);
        assert!(profile.validate("work").is_err());
        profile.provider = Some(Provider::OpenAi);
        profile.validate("work")
    }

    #[test]
    fn rejects_provider_outside_opencode_api() {
        use Agent::*;
        for agent in [OpenCode, Codex, ClaudeCode, Kimi, Pi, ZCode] {
            for mode in [AuthMode::Native, AuthMode::Api] {
                let mut profile = Profile::new(agent, mode);
                profile.provider = Some(Provider::OpenAi);
                let valid = matches!((agent, mode), (OpenCode, AuthMode::Api));
                assert_eq!(profile.validate("work").is_ok(), valid, "{agent} {mode}");
            }
        }
    }

    #[test]
    fn rejects_unknown_profile_fields() {
        let input = format!(
            "id = '{}'\nagent = 'codex'\nauth = 'api'\nunknown = 'value'",
            uuid::Uuid::new_v4()
        );
        let error = toml_edit::de::from_str::<Profile>(&input).unwrap_err();
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
            ..Config::default()
        };
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("duplicate ID"));
    }

    #[test]
    fn rejects_blank_profile_names() {
        let config = Config::default();
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
            ..Config::default()
        };
        let error = config.validate_profile_name("work").unwrap_err();
        assert!(error.to_string().contains("already exists"));
        config.validate_profile_name("personal")
    }
}
