use super::{
    claude,
    links::{link, points_to, unlink},
};
use crate::config::{Agent, Config, paths};
use anyhow::Context;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

type Links = BTreeMap<String, PathBuf>;
const STATE: &str = ".acli-claude-links.json";

pub(crate) fn refresh(
    shared: &Path,
    claude_home: &Path,
    config_path: &Path,
    config: &Config,
) -> anyhow::Result<()> {
    fs::create_dir_all(shared)?;
    let _lock = super::lock(shared)?;
    let mut folders = Vec::new();
    profile_folders(Agent::ClaudeCode, claude_home, &mut folders)?;
    for profile in config.profiles.values() {
        let home = paths::profile_dir(config_path, profile);
        profile_folders(profile.agent, &home, &mut folders)?;
    }
    let mut wanted = Links::new();
    for dir in folders {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with('.')
            && !dir.join(".claude-plugin/plugin.json").is_file()
            && !points_to(&dir, &shared.join(name))
        {
            wanted.entry(name.to_owned()).or_insert(dir);
        }
    }
    publish(shared, wanted)
}

fn profile_folders(agent: Agent, home: &Path, folders: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    super::collect_skills(&home.join(super::profile_skills(agent)), folders)?;
    if matches!(agent, Agent::ClaudeCode) {
        claude::plugins(home, folders)?;
    }
    Ok(())
}

fn publish(shared: &Path, mut wanted: Links) -> anyhow::Result<()> {
    let previous = managed_links(shared)?;
    anyhow::ensure!(
        previous.keys().all(|name| super::valid_name(name)),
        "Invalid managed skill name"
    );

    for entry in fs::read_dir(shared)? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(name) = filename.to_str() else {
            continue;
        };
        if !owns_link(&previous, shared, name) {
            wanted.remove(name); // Preserve user-owned entries.
        } else if previous.get(name) != wanted.get(name) {
            unlink(entry.path())?;
        }
    }
    if wanted != previous {
        let file = tempfile::NamedTempFile::new_in(shared)?;
        serde_json::to_writer(file.as_file(), &wanted)?;
        file.persist(shared.join(STATE))?;
    }
    for (name, source) in wanted {
        let dest = shared.join(&name);
        if !points_to(&dest, &source) {
            link(&source, &dest).with_context(|| format!("Could not share skill '{name}'"))?;
        }
    }
    Ok(())
}

fn managed_links(shared: &Path) -> anyhow::Result<Links> {
    let file = shared.join(STATE);
    if !file.try_exists()? {
        return Ok(Links::new());
    }
    Ok(serde_json::from_slice(&fs::read(file)?)?)
}

pub(super) fn managed(shared: &Path, name: &str) -> anyhow::Result<bool> {
    Ok(owns_link(&managed_links(shared)?, shared, name))
}

fn owns_link(links: &Links, shared: &Path, name: &str) -> bool {
    links
        .get(name)
        .is_some_and(|source| points_to(&shared.join(name), source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestApp, write_skill};

    fn set_plugin(app: &TestApp, version: &str, scope: &str) -> anyhow::Result<()> {
        let plugin = app.claude.join("plugins/cache/example").join(version);
        write_skill(&plugin.join("skills/review"), version)?;
        let registry = serde_json::json!({"version":2, "plugins":{"example@local":[{"scope":scope,"installPath":plugin}]}});
        fs::write(
            app.claude.join("plugins/installed_plugins.json"),
            registry.to_string(),
        )?;
        fs::write(
            app.claude.join("settings.json"),
            r#"{"enabledPlugins":{"example@local":true}}"#,
        )?;
        let config = Config {
            profiles: BTreeMap::new(),
        };
        refresh(&app.shared, &app.claude, &app.config, &config)
    }

    #[test]
    fn plugin_skills_follow_user_installations_and_version_changes() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let skill = app.shared.join("review/SKILL.md");
        for version in ["1", "2"] {
            set_plugin(&app, version, "user")?;
            assert_eq!(fs::read_to_string(&skill)?, version);
        }
        assert!(super::super::remove(&app.shared, "review").is_err());
        set_plugin(&app, "2", "project")?;
        assert!(app.shared.join("review").symlink_metadata().is_err());
        Ok(())
    }

    #[test]
    fn plugin_cannot_replace_a_personal_skill() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        write_skill(&app.shared.join("review"), "personal")?;
        set_plugin(&app, "1", "user")?;
        assert_eq!(
            fs::read_to_string(app.shared.join("review/SKILL.md"))?,
            "personal"
        );
        Ok(())
    }
}
