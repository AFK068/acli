use crate::files;
use anyhow::Context;
use std::path::Path;
use toml_edit::{DocumentMut, ImDocument};

use super::{Config, McpServer, Profile, paths};

pub(crate) fn load(path: &Path) -> anyhow::Result<Config> {
    decode(path, read_document(path)?)
}

pub(crate) fn add_profile(path: &Path, name: &str, profile: Profile) -> anyhow::Result<()> {
    edit(path, |config, document| {
        config.validate_new_profile(name, &profile)?;
        let profile_table = toml_edit::ser::to_document(&profile)?.into_table();
        table(document, "profiles")?.insert(name, toml_edit::Item::Table(profile_table));
        files::create_dir(&paths::profile_dir(path, &profile))
    })
}

pub(crate) fn remove_profile(path: &Path, name: &str) -> anyhow::Result<Profile> {
    edit(path, |mut config, document| {
        let profile = config
            .profiles
            .remove(name)
            .with_context(|| format!("Profile '{name}' not found"))?;
        table(document, "profiles")?.remove(name);
        Ok(profile)
    })
}

pub(crate) fn rename_profile(path: &Path, name: &str, new_name: &str) -> anyhow::Result<()> {
    edit(path, |config, document| {
        config.profile(name)?;
        config.validate_profile_name(new_name)?;
        let profiles = table(document, "profiles")?;
        let profile = profiles
            .remove(name)
            .with_context(|| format!("Profile '{name}' not found"))?;
        profiles.insert(new_name, profile);
        Ok(())
    })
}

pub(crate) fn update_mcp(
    path: &Path,
    name: &str,
    server: Option<&McpServer>,
) -> anyhow::Result<()> {
    edit(path, |_, document| {
        let servers = table(document, "mcp")?;
        if let Some(server) = server {
            server.validate(name)?;
            anyhow::ensure!(
                !servers.contains_key(name),
                "MCP server '{name}' already exists"
            );
            servers.insert(name, toml_edit::value(server.table()));
        } else {
            servers
                .remove(name)
                .with_context(|| format!("No MCP server '{name}' in the acli configuration"))?;
        }
        Ok(())
    })
}

fn edit<T>(
    path: &Path,
    update: impl FnOnce(Config, &mut DocumentMut) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    files::create_dir(files::parent_dir(path))?;
    // Lock a separate file so replacing the configuration does not release the lock.
    let _lock = files::lock(&path.with_extension("lock"))?;
    let document = read_document(path)?;
    let config = decode(path, document.clone())?;
    let mut document = document.into_mut();
    let result = update(config, &mut document)?;
    files::write(path, &document.to_string())?;
    Ok(result)
}

fn read_document(path: &Path) -> anyhow::Result<ImDocument<String>> {
    let input = files::read_text(path)?.unwrap_or_default();
    ImDocument::parse(input).with_context(|| invalid_config(path))
}

fn decode(path: &Path, document: ImDocument<String>) -> anyhow::Result<Config> {
    let config: Config =
        toml_edit::de::from_document(document).with_context(|| invalid_config(path))?;
    config.validate().with_context(|| invalid_config(path))?;
    Ok(config)
}

fn invalid_config(path: &Path) -> String {
    format!("Invalid configuration '{}'", path.display())
}

fn table<'a>(
    document: &'a mut DocumentMut,
    key: &str,
) -> anyhow::Result<&'a mut dyn toml_edit::TableLike> {
    document
        .entry(key)
        .or_insert(toml_edit::table())
        .as_table_like_mut()
        .with_context(|| format!("Expected {key} to be a TOML table"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, AuthMode, paths::profile_dir};
    use crate::test_support::TestApp;
    use std::fs;

    fn native_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Native)
    }

    #[test]
    fn shared_mcp_round_trips_without_overwriting_existing_settings() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        fs::write(&app.config, "# keep\ncustom = true\n")?;
        let server = McpServer {
            command: "test-server".into(),
            args: vec!["--path".into(), "two words".into()],
        };
        update_mcp(&app.config, "docs", Some(&server))?;
        assert_eq!(load(&app.config)?.mcp["docs"].args, server.args);
        assert!(update_mcp(&app.config, "docs", Some(&server)).is_err());
        update_mcp(&app.config, "docs", None)?;
        assert!(load(&app.config)?.mcp.is_empty());
        assert!(fs::read_to_string(&app.config)?.contains("# keep\ncustom = true"));
        Ok(())
    }

    #[test]
    fn saving_a_profile_creates_its_directory_and_persists_its_id() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let profile = native_profile();
        let id = profile.id;
        let dir = profile_dir(&app.config, &profile);
        add_profile(&app.config, "work.test", profile)?;
        assert_eq!(load(&app.config)?.profile("work.test")?.id, id);
        assert!(dir.is_dir());
        Ok(())
    }

    #[test]
    fn loading_a_missing_config_has_no_side_effects() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = app.root.path().join("missing/config.toml");
        assert!(load(&path)?.profiles.is_empty());
        assert!(!path.parent().unwrap().exists());
        assert!(load(app.root.path()).is_err());
        Ok(())
    }

    #[test]
    fn loading_existing_config_does_not_change_files() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let input = include_str!("../../config.example.toml");
        fs::write(&app.config, input)?;
        load(&app.config)?;
        assert_eq!(fs::read_to_string(&app.config)?, input);
        assert_eq!(fs::read_dir(app.root.path())?.count(), 1);
        Ok(())
    }

    #[test]
    fn removal_keeps_other_profiles_and_agent_data() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let history = app.add_native("work.test", Agent::Codex)?.join("history");
        app.add_native("personal", Agent::Codex)?;
        fs::write(&history, "saved history")?;
        let before = load(&app.config)?;
        let removed = remove_profile(&app.config, "work.test")?;
        let after = load(&app.config)?;
        assert_eq!(removed.id, before.profile("work.test")?.id);
        assert!(after.profile("work.test").is_err());
        assert_eq!(
            after.profile("personal")?.id,
            before.profile("personal")?.id
        );
        assert_eq!(fs::read_to_string(history)?, "saved history");
        Ok(())
    }

    #[test]
    fn renaming_keeps_the_same_data_directory() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let dir = app.add_native("old", Agent::Codex)?;
        fs::write(dir.join("history"), "saved")?;
        rename_profile(&app.config, "old", "new")?;
        let config = load(&app.config)?;
        assert!(config.profile("old").is_err());
        let new_dir = profile_dir(&app.config, config.profile("new")?);
        assert_eq!(new_dir, dir);
        assert_eq!(fs::read_to_string(new_dir.join("history"))?, "saved");
        Ok(())
    }

    #[test]
    fn invalid_names_leave_saved_profiles_untouched() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        app.add_native("old", Agent::Codex)?;
        app.add_native("taken", Agent::Codex)?;
        let before = fs::read(path)?;
        for name in ["taken", "old", " ", ""] {
            assert!(rename_profile(path, "old", name).is_err());
        }
        assert!(rename_profile(path, "missing", "new").is_err());
        assert!(remove_profile(path, "missing").is_err());
        assert!(add_profile(path, "old", native_profile()).is_err());
        assert_eq!(fs::read(path)?, before);
        Ok(())
    }

    #[test]
    fn invalid_existing_config_is_never_rewritten() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        for input in [
            "[profiles.bad",
            "[profiles.old]\nagent='codex'\nauth='native'",
            "profiles = { old = { agent='codex', auth='native' } }",
            "[profiles.old]\nid='../../outside'\nagent='codex'\nauth='native'",
        ] {
            fs::write(&app.config, input)?;
            assert!(load(&app.config).is_err());
            assert!(add_profile(&app.config, "new", native_profile()).is_err());
            assert!(remove_profile(&app.config, "old").is_err());
            assert_eq!(fs::read_to_string(&app.config)?, input);
        }
        assert!(!app.root.path().join("profiles").exists());
        Ok(())
    }

    #[test]
    fn invalid_new_profiles_create_no_data() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let mut invalid = native_profile();
        invalid.id = uuid::Uuid::nil();
        let error = add_profile(&app.config, "work", invalid).unwrap_err();
        assert!(error.to_string().contains("empty ID"));
        for agent in [Agent::Pi, Agent::ZCode] {
            let profile = Profile::new(agent, AuthMode::Api);
            assert!(add_profile(&app.config, "work", profile).is_err());
        }
        assert!(add_profile(&app.config, " ", native_profile()).is_err());
        assert!(!app.config.exists());
        assert!(!app.root.path().join("profiles").exists());
        Ok(())
    }

    #[test]
    fn duplicate_profile_id_leaves_config_unchanged() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        app.add_native("first", Agent::Codex)?;
        let mut second = native_profile();
        second.id = load(&app.config)?.profile("first")?.id;
        let before = fs::read(&app.config)?;
        assert!(add_profile(&app.config, "second", second).is_err());
        assert_eq!(fs::read(&app.config)?, before);
        Ok(())
    }

    #[test]
    fn concurrent_writers_preserve_all_profiles() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let path = &app.config;
        app.add_native("old", Agent::Codex)?;
        let id = load(path)?.profile("old")?.id;
        let ready = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for name in ["one", "two"] {
                let ready = &ready;
                scope.spawn(move || {
                    ready.wait();
                    assert_eq!(load(path).unwrap().profile("old").unwrap().id, id);
                    add_profile(path, name, native_profile()).unwrap();
                });
            }
        });
        let config = load(path)?;
        assert_eq!(config.profile("old")?.id, id);
        assert_eq!(config.profiles.len(), 3);
        Ok(())
    }

    #[test]
    fn edits_preserve_comments_and_unknown_settings() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let id = uuid::Uuid::new_v4();
        for table in [
            format!("[profiles.old] # keep\nid='{id}'\nagent='codex'\nauth='native'"),
            format!("profiles = {{ old = {{ id='{id}', agent='codex', auth='native' }} }} # keep"),
        ] {
            fs::write(&app.config, format!("custom = true\n{table}\n"))?;
            app.add_native("other", Agent::Codex)?;
            rename_profile(&app.config, "old", "work.test")?;
            remove_profile(&app.config, "other")?;
            let saved = fs::read_to_string(&app.config)?;
            assert!(saved.contains("custom = true"));
            assert!(saved.contains("# keep"));
            assert_eq!(load(&app.config)?.profile("work.test")?.id, id);
        }
        Ok(())
    }

    #[test]
    fn directory_creation_failure_does_not_save_profile() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        fs::write(app.root.path().join("profiles"), "not a directory")?;
        assert!(add_profile(&app.config, "new", native_profile()).is_err());
        assert!(!app.config.exists());
        Ok(())
    }
}
