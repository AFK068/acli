use anyhow::Context;
use std::io::Write;
use std::path::Path;

use super::{Config, Profile, paths};

pub(crate) fn load(path: &Path) -> anyhow::Result<Config> {
    parse(&read(path)?).with_context(|| format!("Invalid configuration '{}'", path.display()))
}

pub(crate) fn add_profile(path: &Path, name: &str, profile: Profile) -> anyhow::Result<()> {
    let _lock = lock(path)?;
    let (config, mut document) = read_config(path)?;
    config.validate_new_profile(name, &profile)?;
    profiles_table(&mut document)?.insert(name, toml_edit::Item::Table(profile_table(&profile)));
    paths::create_dir(&paths::profile_dir(path, &profile))?;
    write(path, &document.to_string())
}

pub(crate) fn remove_profile(path: &Path, name: &str) -> anyhow::Result<Profile> {
    let _lock = lock(path)?;
    let (mut config, mut document) = read_config(path)?;
    let profile = config
        .profiles
        .remove(name)
        .with_context(|| format!("Profile '{name}' not found"))?;
    profiles_table(&mut document)?.remove(name);
    write(path, &document.to_string())?;
    Ok(profile)
}

pub(crate) fn rename_profile(path: &Path, name: &str, new_name: &str) -> anyhow::Result<()> {
    let _lock = lock(path)?;
    let (config, mut document) = read_config(path)?;
    config.profile(name)?;
    config.validate_profile_name(new_name)?;

    let profiles = profiles_table(&mut document)?;
    let profile = profiles
        .remove(name)
        .with_context(|| format!("Profile '{name}' not found"))?;
    profiles.insert(new_name, profile);
    write(path, &document.to_string())
}

fn read_config(path: &Path) -> anyhow::Result<(Config, toml_edit::DocumentMut)> {
    let input = read(path)?;
    let config =
        parse(&input).with_context(|| format!("Invalid configuration '{}'", path.display()))?;
    Ok((config, input.parse()?))
}

fn parse(input: &str) -> anyhow::Result<Config> {
    let config: Config = toml::from_str(input)?;
    config.validate()?;
    Ok(config)
}

fn profiles_table(
    document: &mut toml_edit::DocumentMut,
) -> anyhow::Result<&mut dyn toml_edit::TableLike> {
    document
        .entry("profiles")
        .or_insert(toml_edit::table())
        .as_table_like_mut()
        .context("Expected profiles to be a TOML table")
}

fn profile_table(profile: &Profile) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["id"] = toml_edit::value(profile.id.to_string());
    table["agent"] = toml_edit::value(profile.agent.to_string());
    table["auth"] = toml_edit::value(profile.auth.to_string());
    if let Some(provider) = profile.provider {
        table["provider"] = toml_edit::value(provider.to_string());
    }
    table
}

fn read(path: &Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(input) => Ok(input),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => {
            Err(error).with_context(|| format!("Could not read configuration '{}'", path.display()))
        }
    }
}

fn write(path: &Path, contents: &str) -> anyhow::Result<()> {
    // Replace the original only after the entire new file has been written.
    let mut file = tempfile::NamedTempFile::new_in(paths::parent_dir(path))?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Could not save configuration '{}'", path.display()))?;
    Ok(())
}

fn lock(path: &Path) -> anyhow::Result<std::fs::File> {
    // Lock a separate file: replacing config.toml must not release the lock.
    paths::create_dir(paths::parent_dir(path))?;
    let lock_path = path.with_extension("lock");
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Could not open lock '{}'", lock_path.display()))?;
    file.lock().context("Could not lock configuration")?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Agent, AuthMode, paths::profile_dir};

    fn native_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Native)
    }

    use crate::test_support::TestApp;
    use std::fs;

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
        for profile in [
            invalid,
            Profile::new(Agent::Pi, AuthMode::Api),
            Profile::new(Agent::ZCode, AuthMode::Api),
        ] {
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
