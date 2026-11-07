use anyhow::Context;
use std::io::Write;

use super::{Config, Profile, paths};

pub(crate) fn load(path: &std::path::Path) -> anyhow::Result<Config> {
    let _lock = lock(path)?;
    let (config, mut document) = read_config(path)?;
    let migrated = insert_missing_ids(&config, &mut document);
    paths::create_profile_dirs(path, &config)?;
    if migrated {
        write(path, &document.to_string())?;
    }
    Ok(config)
}

pub(crate) fn add_profile(
    path: &std::path::Path,
    name: &str,
    profile: Profile,
) -> anyhow::Result<()> {
    let _lock = lock(path)?;
    let (mut config, mut document) = read_config(path)?;
    config.validate_profile_name(name)?;
    insert_profile(&mut document, name, &profile)?;
    config.profiles.insert(name.to_owned(), profile);
    config.validate()?;

    insert_missing_ids(&config, &mut document);
    paths::create_profile_dirs(path, &config)?;
    write(path, &document.to_string())
}

fn read_config(path: &std::path::Path) -> anyhow::Result<(Config, toml_edit::DocumentMut)> {
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

fn insert_missing_ids(config: &Config, document: &mut toml_edit::DocumentMut) -> bool {
    let mut changed = false;
    for (name, profile) in &config.profiles {
        let Some(entry) = document
            .get_mut("profiles")
            .and_then(|profiles| profiles.get_mut(name))
        else {
            continue;
        };
        if entry.get("id").is_none() {
            entry["id"] = toml_edit::value(profile.id.to_string());
            changed = true;
        }
    }
    changed
}

fn insert_profile(
    document: &mut toml_edit::DocumentMut,
    name: &str,
    profile: &Profile,
) -> anyhow::Result<()> {
    document
        .entry("profiles")
        .or_insert(toml_edit::table())
        .as_table_like_mut()
        .context("Expected profiles to be a TOML table")?
        .insert(name, toml_edit::Item::Table(profile_table(profile)));
    Ok(())
}

fn profile_table(profile: &Profile) -> toml_edit::Table {
    let mut table = toml_edit::Table::new();
    table["id"] = toml_edit::value(profile.id.to_string());
    table["agent"] = toml_edit::value(profile.agent.to_string());
    table["auth"] = toml_edit::value(profile.auth.to_string());
    if let Some(key) = &profile.api_key_env {
        table["api_key_env"] = toml_edit::value(key);
    }
    table
}

fn read(path: &std::path::Path) -> anyhow::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(input) => Ok(input),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => {
            Err(error).with_context(|| format!("Could not read configuration '{}'", path.display()))
        }
    }
}

fn write(path: &std::path::Path, contents: &str) -> anyhow::Result<()> {
    // Replace the original only after the entire new file has been written.
    let mut file = tempfile::NamedTempFile::new_in(paths::parent_dir(path))?;
    file.write_all(contents.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)
        .with_context(|| format!("Could not save configuration '{}'", path.display()))?;
    Ok(())
}

fn lock(path: &std::path::Path) -> anyhow::Result<std::fs::File> {
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

    fn subscription_profile() -> Profile {
        Profile::new(Agent::Codex, AuthMode::Subscription, None)
    }

    #[test]
    fn creates_config_and_preserves_existing_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("acli/config.toml");
        assert!(load(&path)?.profiles.is_empty());
        assert!(!path.exists());

        add_profile(&path, "personal", subscription_profile())?;
        add_profile(
            &path,
            "work.test",
            Profile::new(Agent::ClaudeCode, AuthMode::Api, Some("WORK_KEY".into())),
        )?;

        let config = load(&path)?;
        assert_eq!(config.profiles.len(), 2);
        assert!(config.get_profile("personal").is_some());
        let work = config.get_profile("work.test").expect("new profile");
        assert!(matches!(work.agent, Agent::ClaudeCode));
        assert_eq!(work.api_key_env.as_deref(), Some("WORK_KEY"));
        let personal = config.get_profile("personal").unwrap();
        assert_ne!(personal.id, work.id);
        for profile in config.profiles.values() {
            assert!(profile_dir(&path, profile).is_dir());
        }
        let reloaded = load(&path)?;
        assert_eq!(reloaded.get_profile("work.test").unwrap().id, work.id);
        Ok(())
    }

    #[test]
    fn migrates_legacy_ids_once_and_preserves_comments() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for input in [
            "# Keep this\n[profiles.old]\nagent = 'codex'\nauth = 'subscription'\n",
            "# Keep this\nprofiles = { old = { agent = 'codex', auth = 'subscription' } }\n",
        ] {
            std::fs::write(&path, input)?;
            let config = load(&path)?;
            let profile = config.get_profile("old").unwrap();
            let saved = std::fs::read_to_string(&path)?;
            assert!(saved.contains(&profile.id.to_string()));
            assert!(saved.contains("# Keep this"));
            assert!(profile_dir(&path, profile).is_dir());
            assert_eq!(load(&path)?.get_profile("old").unwrap().id, profile.id);
            assert_eq!(std::fs::read_to_string(&path)?, saved);
        }
        Ok(())
    }

    #[test]
    fn renaming_a_profile_keeps_its_directory_and_data() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "old", subscription_profile())?;
        let config = load(&path)?;
        let old = config.get_profile("old").unwrap();
        let old_dir = profile_dir(&path, old);
        std::fs::write(old_dir.join("history.txt"), "saved history")?;

        let mut document: toml_edit::DocumentMut = std::fs::read_to_string(&path)?.parse()?;
        let profiles = document["profiles"].as_table_mut().unwrap();
        let profile = profiles.remove("old").unwrap();
        profiles.insert("renamed", profile);
        std::fs::write(&path, document.to_string())?;

        let renamed = load(&path)?;
        let renamed = renamed.get_profile("renamed").unwrap();
        assert_eq!(renamed.id, old.id);
        assert_eq!(profile_dir(&path, renamed), old_dir);
        assert_eq!(
            std::fs::read_to_string(old_dir.join("history.txt"))?,
            "saved history"
        );
        Ok(())
    }

    #[test]
    fn rejects_invalid_and_duplicate_ids_without_rewriting_config() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for id in [
            "not-a-uuid",
            "../../outside",
            "00000000-0000-0000-0000-000000000000",
        ] {
            let input =
                format!("[profiles.bad]\nid = '{id}'\nagent = 'codex'\nauth = 'subscription'\n");
            std::fs::write(&path, &input)?;
            assert!(load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path)?, input);
        }
        assert!(!directory.path().join("profiles").exists());

        let id = uuid::Uuid::new_v4();
        let input = format!(
            "[profiles.one]\nid = '{id}'\nagent = 'codex'\nauth = 'subscription'\n[profiles.two]\nid = '{id}'\nagent = 'claude-code'\nauth = 'subscription'\n"
        );
        std::fs::write(&path, &input)?;
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, input);

        std::fs::remove_file(&path)?;
        let first = subscription_profile();
        let mut second = subscription_profile();
        second.id = first.id;
        add_profile(&path, "first", first)?;
        let saved = std::fs::read(&path)?;
        assert!(add_profile(&path, "second", second).is_err());
        assert_eq!(std::fs::read(&path)?, saved);
        Ok(())
    }

    #[test]
    fn concurrent_loads_and_additions_keep_ids_and_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "[profiles.old]\nagent = 'codex'\nauth = 'subscription'\n",
        )?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|index| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || -> anyhow::Result<uuid::Uuid> {
                    barrier.wait();
                    let config = load(&path)?;
                    let id = config.get_profile("old").unwrap().id;
                    add_profile(&path, &format!("new-{index}"), subscription_profile())?;
                    Ok(id)
                })
            })
            .collect();
        let ids: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<anyhow::Result<_>>()?;
        assert_eq!(ids[0], ids[1]);
        let config = load(&path)?;
        assert_eq!(config.profiles.len(), 3);
        assert_eq!(config.get_profile("old").unwrap().id, ids[0]);
        Ok(())
    }

    #[test]
    fn directory_creation_failure_does_not_save_new_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        std::fs::write(directory.path().join("profiles"), "not a directory")?;
        assert!(add_profile(&path, "new", subscription_profile()).is_err());
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn preserves_comments_and_unknown_settings() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for input in [
            "# My settings\ncustom = true\n[profiles.personal]\nagent = 'codex' # keep this\nauth = 'subscription'\n",
            "# My settings\ncustom = true\nprofiles = { personal = { agent = 'codex', auth = 'subscription' } } # keep this\n",
        ] {
            std::fs::write(&path, input)?;
            add_profile(&path, "second", subscription_profile())?;
            let saved = std::fs::read_to_string(&path)?;
            assert!(saved.contains("# My settings"));
            assert!(saved.contains("# keep this"));
            assert!(saved.contains("custom = true"));
            assert_eq!(load(&path)?.profiles.len(), 2);
        }
        Ok(())
    }

    #[test]
    fn refuses_duplicate_names_without_changing_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "work", subscription_profile())?;
        let before = std::fs::read(&path)?;
        let error = add_profile(&path, "work", subscription_profile()).unwrap_err();
        assert!(error.to_string().contains("already exists"));
        assert_eq!(std::fs::read(&path)?, before);
        Ok(())
    }

    #[test]
    fn refuses_invalid_config_and_invalid_new_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let malformed = "[profiles.work";
        std::fs::write(&path, malformed)?;
        assert!(add_profile(&path, "new", subscription_profile()).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, malformed);

        let missing = directory.path().join("new/config.toml");
        assert!(add_profile(&missing, "   ", subscription_profile()).is_err());
        assert!(
            add_profile(
                &missing,
                "new",
                Profile::new(Agent::Codex, AuthMode::Api, None)
            )
            .is_err()
        );
        assert!(!missing.exists());
        assert!(load(&directory.path().join("new")).is_err());
        Ok(())
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse("[profiles.work").is_err());
    }
}
