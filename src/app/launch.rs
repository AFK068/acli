use anyhow::Context;
use std::path::Path;
use std::process::Command;

use super::profiles;
use crate::config::{AuthMode, paths};

pub(crate) fn prepare(
    path: &Path,
    name: &str,
    shared: &Path,
    claude_home: &Path,
) -> anyhow::Result<Command> {
    let config = profiles::load(path)?;
    let profile = config.profile(name)?;

    let key = match profile.auth {
        AuthMode::Native => None,
        AuthMode::Api => {
            let key = crate::credentials::load(profile.id)
                .with_context(|| format!("Could not load API key for profile '{name}'"))?;
            anyhow::ensure!(
                !key.trim().is_empty(),
                "API key for profile '{name}' is empty"
            );
            Some(key)
        }
    };

    let directory = paths::profile_dir(path, profile);
    paths::create_dir(&directory)?;
    crate::skills::refresh(shared, claude_home, path, &config)?;
    crate::skills::connect(profile.agent, &directory, shared)?;
    Ok(crate::agents::command(profile, &directory, key.as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, Profile, storage};

    #[test]
    fn skills_installed_in_each_profile_reach_every_harness() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let config = temp.path().join("config.toml");
        let shared = temp.path().join(".agents/skills");
        let claude = temp.path().join(".claude");
        let mut profiles = Vec::new();
        for (agent, local) in [
            (Agent::ClaudeCode, "skills"),
            (Agent::Codex, "skills"),
            (Agent::Kimi, "skills"),
            (Agent::OpenCode, "config/opencode/skills"),
            (Agent::Pi, "skills"),
            (Agent::ZCode, ".zcode/skills"),
        ] {
            let name = agent.to_string();
            let profile = Profile::new(agent, AuthMode::Native);
            let local = paths::profile_dir(&config, &profile).join(local);
            let visible = match agent {
                Agent::ClaudeCode | Agent::ZCode => local.clone(),
                _ => shared.clone(),
            };
            storage::add_profile(&config, &name, profile)?;
            let skill = local.join(&name);
            std::fs::create_dir_all(&skill)?;
            std::fs::write(skill.join("SKILL.md"), &name)?;
            profiles.push((name, visible, skill));
        }
        for (recipient, visible, _) in &profiles {
            prepare(&config, recipient, &shared, &claude)?;
            for (sender, _, _) in &profiles {
                assert_eq!(
                    std::fs::read_to_string(visible.join(sender).join("SKILL.md"))?,
                    *sender,
                    "{sender} -> {recipient}"
                );
            }
        }
        for (_, _, skill) in &profiles {
            std::fs::remove_dir_all(skill)?;
        }
        for (recipient, _, _) in &profiles {
            prepare(&config, recipient, &shared, &claude)?;
        }
        assert!(crate::skills::list(&shared)?.is_empty());
        Ok(())
    }

    #[test]
    fn duplicate_skills_keep_their_originals_and_fall_back_after_removal() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let config = temp.path().join("config.toml");
        let shared = temp.path().join("shared");
        let claude = temp.path().join(".claude");
        for (name, agent) in [("a", Agent::Codex), ("b", Agent::ClaudeCode)] {
            let profile = Profile::new(agent, AuthMode::Native);
            let local = paths::profile_dir(&config, &profile).join("skills/review");
            storage::add_profile(&config, name, profile)?;
            std::fs::create_dir_all(&local)?;
            std::fs::write(local.join("SKILL.md"), name)?;
        }
        prepare(&config, "b", &shared, &claude)?;
        assert_eq!(
            std::fs::read_to_string(shared.join("review/SKILL.md"))?,
            "a"
        );
        let profiles = profiles::load(&config)?;
        let source = paths::profile_dir(&config, profiles.profile("a")?).join("skills/review");
        std::fs::remove_dir_all(source)?;
        prepare(&config, "a", &shared, &claude)?;
        assert_eq!(
            std::fs::read_to_string(shared.join("review/SKILL.md"))?,
            "b"
        );
        Ok(())
    }

    #[test]
    fn reports_missing_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let error = prepare(
            &directory.path().join("config.toml"),
            "missing",
            &directory.path().join("shared"),
            &directory.path().join("claude"),
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "Profile 'missing' not found");
        Ok(())
    }

    #[test]
    fn reports_missing_saved_api_key() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Api);
        storage::add_profile(&path, "work", profile)?;
        let error = prepare(
            &path,
            "work",
            &directory.path().join("shared"),
            &directory.path().join("claude"),
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not load API key for profile 'work'"
        );
        assert!(format!("{error:#}").contains("No API key saved"));
        Ok(())
    }

    #[test]
    fn prepares_profile_directory_without_changing_working_directory() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = Profile::new(Agent::Codex, AuthMode::Native);
        let expected = paths::profile_dir(&path, &profile);
        storage::add_profile(&path, "personal", profile)?;
        std::fs::remove_dir(&expected)?;
        let command = prepare(
            &path,
            "personal",
            &directory.path().join("shared"),
            &directory.path().join("claude"),
        )?;
        assert!(expected.is_dir());
        assert_eq!(command.get_program(), "codex");
        assert!(command.get_current_dir().is_none());
        assert!(
            command.get_envs().any(|(name, value)| {
                name == "CODEX_HOME" && value == Some(expected.as_os_str())
            })
        );
        Ok(())
    }
}
