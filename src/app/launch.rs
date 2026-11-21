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

    use crate::test_support::{TestApp, env, write_skill};
    use std::fs;

    fn install_one_skill_per_harness(
        app: &TestApp,
    ) -> anyhow::Result<Vec<(String, std::path::PathBuf)>> {
        let mut recipients = Vec::new();
        for (agent, folder) in [
            (Agent::ClaudeCode, "skills"),
            (Agent::Codex, "skills"),
            (Agent::Kimi, "skills"),
            (Agent::OpenCode, "config/opencode/skills"),
            (Agent::Pi, "skills"),
            (Agent::ZCode, ".zcode/skills"),
        ] {
            let name = agent.to_string();
            let local = app.add_native(&name, agent)?.join(folder);
            write_skill(&local.join(&name), &name)?;
            let visible = match agent {
                Agent::ClaudeCode | Agent::ZCode => local,
                _ => app.shared.clone(),
            };
            recipients.push((name, visible));
        }
        Ok(recipients)
    }

    #[test]
    fn every_harness_sees_skills_from_all_six_profiles() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let recipients = install_one_skill_per_harness(&app)?;
        for (recipient, visible) in &recipients {
            app.prepare(recipient)?;
            for (skill, _) in &recipients {
                assert_eq!(
                    fs::read_to_string(visible.join(skill).join("SKILL.md"))?,
                    *skill
                );
            }
        }
        Ok(())
    }

    #[test]
    fn deleting_the_source_does_not_republish_links_from_receivers() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let source = app
            .add_native("sender", Agent::Codex)?
            .join("skills/review");
        let receiver = app
            .add_native("receiver", Agent::ClaudeCode)?
            .join("skills/review");
        write_skill(&source, "review")?;
        app.prepare("receiver")?;
        assert!(receiver.join("SKILL.md").is_file());
        fs::remove_dir_all(source)?;
        app.prepare("receiver")?;
        assert!(receiver.symlink_metadata().is_err());
        assert!(crate::skills::list(&app.shared)?.is_empty());
        Ok(())
    }

    #[test]
    fn duplicate_skills_fall_back_to_the_remaining_source() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let first = app.add_native("a", Agent::Codex)?.join("skills/review");
        let second = app
            .add_native("b", Agent::ClaudeCode)?
            .join("skills/review");
        write_skill(&first, "a")?;
        write_skill(&second, "b")?;
        let shared = app.shared.join("review/SKILL.md");
        app.prepare("b")?;
        assert_eq!(fs::read_to_string(&shared)?, "a");
        fs::remove_dir_all(first)?;
        app.prepare("a")?;
        assert_eq!(fs::read_to_string(shared)?, "b");
        Ok(())
    }

    #[test]
    fn reports_missing_profile() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        assert_eq!(
            app.prepare("missing").unwrap_err().to_string(),
            "Profile 'missing' not found"
        );
        Ok(())
    }

    #[test]
    fn reports_missing_saved_api_key() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        storage::add_profile(
            &app.config,
            "work",
            Profile::new(Agent::Codex, AuthMode::Api),
        )?;
        let error = app.prepare("work").unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not load API key for profile 'work'"
        );
        assert!(format!("{error:#}").contains("No API key saved"));
        Ok(())
    }

    #[test]
    fn launch_recreates_the_profile_directory_and_keeps_cwd() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let dir = app.add_native("personal", Agent::Codex)?;
        fs::remove_dir(&dir)?;
        let command = app.prepare("personal")?;
        assert!(dir.is_dir());
        assert!(command.get_current_dir().is_none());
        assert_eq!(env(&command)["CODEX_HOME"], Some(dir.as_os_str()));
        Ok(())
    }
}
