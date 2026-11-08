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

    #[test]
    fn saves_and_loads_every_agent_with_its_own_directory() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for agent in [Agent::Codex, Agent::ClaudeCode, Agent::Kimi] {
            let name = agent.to_string();
            let profile = Profile::new(agent, AuthMode::Native);
            let id = profile.id;
            let expected = profile_dir(&path, &profile);
            add_profile(&path, &name, profile)?;
            let config = load(&path)?;
            let loaded = config.profile(&name)?;
            assert_eq!(loaded.agent.to_string(), name);
            assert_eq!(loaded.id, id);
            assert_eq!(profile_dir(&path, loaded), expected);
            assert!(expected.is_dir());
        }
        Ok(())
    }

    #[test]
    fn loading_configuration_does_not_write_files_or_create_directories() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let missing = directory.path().join("missing/config.toml");
        assert!(load(&missing)?.profiles.is_empty());
        assert!(!missing.parent().unwrap().exists());

        let path = directory.path().join("config.toml");
        let input = include_str!("../../config.example.toml");
        std::fs::write(&path, input)?;
        assert_eq!(load(&path)?.profiles.len(), 2);
        assert_eq!(std::fs::read_to_string(&path)?, input);
        assert!(!path.with_extension("lock").exists());
        assert!(!directory.path().join("profiles").exists());
        Ok(())
    }

    #[test]
    fn removes_only_selected_profile_and_keeps_agent_data() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let profile = native_profile();
        let id = profile.id;
        let data = profile_dir(&path, &profile).join("history.txt");
        std::fs::write(&path, "# Keep this comment\ncustom = true\n")?;
        add_profile(&path, "work.test", profile)?;
        add_profile(&path, "personal", native_profile())?;
        let remaining_id = load(&path)?.profile("personal")?.id;
        std::fs::write(&data, "history")?;

        assert_eq!(remove_profile(&path, "work.test")?.id, id);

        let config = load(&path)?;
        assert_eq!(config.profiles.len(), 1);
        assert_eq!(config.profile("personal")?.id, remaining_id);
        let saved = std::fs::read_to_string(&path)?;
        assert!(saved.contains("# Keep this comment"));
        assert!(saved.contains("custom = true"));
        assert_eq!(std::fs::read_to_string(data)?, "history");
        Ok(())
    }

    #[test]
    fn missing_profile_does_not_change_configuration() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let error = remove_profile(&path, "missing").unwrap_err();
        assert_eq!(error.to_string(), "Profile 'missing' not found");
        assert!(!path.exists());
        add_profile(&path, "personal", native_profile())?;
        let saved = std::fs::read(&path)?;
        assert!(remove_profile(&path, "missing").is_err());
        assert_eq!(std::fs::read(&path)?, saved);
        Ok(())
    }

    #[test]
    fn removal_does_not_overwrite_invalid_configuration() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let input = "[profiles.work\n";
        std::fs::write(&path, input)?;
        assert!(remove_profile(&path, "work").is_err());
        assert_eq!(std::fs::read_to_string(&path)?, input);
        Ok(())
    }

    #[test]
    fn creates_config_and_preserves_existing_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("acli/config.toml");
        assert!(load(&path)?.profiles.is_empty());
        assert!(!path.exists());

        add_profile(&path, "personal", native_profile())?;
        add_profile(
            &path,
            "work.test",
            Profile::new(Agent::ClaudeCode, AuthMode::Api),
        )?;

        let config = load(&path)?;
        assert_eq!(config.profiles.len(), 2);
        let work = config.profile("work.test")?;
        assert!(matches!(work.agent, Agent::ClaudeCode));
        assert_eq!(work.auth, AuthMode::Api);
        let personal = config.profile("personal")?;
        assert_ne!(personal.id, work.id);
        for profile in config.profiles.values() {
            assert!(profile_dir(&path, profile).is_dir());
        }
        let reloaded = load(&path)?;
        assert_eq!(reloaded.profile("work.test")?.id, work.id);
        Ok(())
    }

    #[test]
    fn rejects_profiles_without_ids_without_changing_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for input in [
            "# Keep this\n[profiles.old]\nagent = 'codex'\nauth = 'native'\n",
            "# Keep this\nprofiles = { old = { agent = 'codex', auth = 'native' } }\n",
        ] {
            std::fs::write(&path, input)?;
            let error = load(&path).err().expect("missing ID must be rejected");
            assert!(format!("{error:#}").contains("missing field `id`"));
            assert!(add_profile(&path, "new", native_profile()).is_err());
            assert!(remove_profile(&path, "old").is_err());
            assert_eq!(std::fs::read_to_string(&path)?, input);
        }
        Ok(())
    }

    #[test]
    fn renaming_a_profile_keeps_its_directory_and_data() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "old", native_profile())?;
        let config = load(&path)?;
        let old = config.profile("old")?;
        let old_dir = profile_dir(&path, old);
        std::fs::write(old_dir.join("history.txt"), "saved history")?;

        rename_profile(&path, "old", "renamed")?;

        let renamed = load(&path)?;
        assert!(renamed.profile("old").is_err());
        let renamed = renamed.profile("renamed")?;
        assert_eq!(renamed.id, old.id);
        assert_eq!(profile_dir(&path, renamed), old_dir);
        assert_eq!(
            std::fs::read_to_string(old_dir.join("history.txt"))?,
            "saved history"
        );
        Ok(())
    }

    #[test]
    fn rename_rejects_missing_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let error = rename_profile(&path, "missing", "new").unwrap_err();
        assert_eq!(error.to_string(), "Profile 'missing' not found");
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn rename_rejects_taken_names_without_changing_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "work", native_profile())?;
        add_profile(&path, "personal", native_profile())?;
        let saved = std::fs::read(&path)?;
        for name in ["personal", "work"] {
            let error = rename_profile(&path, "work", name).unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("Profile '{name}' already exists")
            );
            assert_eq!(std::fs::read(&path)?, saved);
        }
        Ok(())
    }

    #[test]
    fn rename_rejects_blank_names_without_changing_file() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        add_profile(&path, "work", native_profile())?;
        let saved = std::fs::read(&path)?;
        for name in ["", " \t\n"] {
            let error = rename_profile(&path, "work", name).unwrap_err();
            assert_eq!(error.to_string(), "Profile name must not be empty");
            assert_eq!(std::fs::read(&path)?, saved);
        }
        Ok(())
    }

    #[test]
    fn rename_preserves_toml_comments_and_other_profiles() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        let id = uuid::Uuid::new_v4();
        for input in [
            format!(
                "# My settings\ncustom = true\n[profiles.old] # Profile comment\nid = '{id}'\nagent = 'codex' # Agent comment\nauth = 'native'\n"
            ),
            format!(
                "# My settings\ncustom = true\nprofiles = {{ old = {{ id = '{id}', agent = 'codex', auth = 'native' }} }} # Profile comment\n"
            ),
        ] {
            std::fs::write(&path, input)?;
            add_profile(&path, "other", native_profile())?;
            let other_id = load(&path)?.profile("other")?.id;
            rename_profile(&path, "old", "work.test")?;
            let config = load(&path)?;
            assert_eq!(config.profiles.len(), 2);
            assert!(config.profile("old").is_err());
            assert_eq!(config.profile("work.test")?.id, id);
            assert_eq!(config.profile("other")?.id, other_id);
            let saved = std::fs::read_to_string(&path)?;
            assert!(saved.contains("# My settings"));
            assert!(saved.contains("# Profile comment"));
            assert!(saved.contains("custom = true"));
        }
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
            let input = format!("[profiles.bad]\nid = '{id}'\nagent = 'codex'\nauth = 'native'\n");
            std::fs::write(&path, &input)?;
            assert!(load(&path).is_err());
            assert_eq!(std::fs::read_to_string(&path)?, input);
        }
        assert!(!directory.path().join("profiles").exists());

        let id = uuid::Uuid::new_v4();
        let input = format!(
            "[profiles.one]\nid = '{id}'\nagent = 'codex'\nauth = 'native'\n[profiles.two]\nid = '{id}'\nagent = 'claude-code'\nauth = 'native'\n"
        );
        std::fs::write(&path, &input)?;
        assert!(load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, input);

        std::fs::remove_file(&path)?;
        let first = native_profile();
        let mut second = native_profile();
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
        add_profile(&path, "old", native_profile())?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|index| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || -> anyhow::Result<uuid::Uuid> {
                    barrier.wait();
                    let config = load(&path)?;
                    let id = config.profile("old")?.id;
                    add_profile(&path, &format!("new-{index}"), native_profile())?;
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
        assert_eq!(config.profile("old")?.id, ids[0]);
        Ok(())
    }

    #[test]
    fn directory_creation_failure_does_not_save_new_profile() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        std::fs::write(directory.path().join("profiles"), "not a directory")?;
        assert!(add_profile(&path, "new", native_profile()).is_err());
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn preserves_comments_and_unknown_settings() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("config.toml");
        for input in [
            "# My settings\ncustom = true\n[profiles.personal]\nid = '8d8ed9a1-9c7e-4ad3-a69a-6e9dc9a05745'\nagent = 'codex' # keep this\nauth = 'native'\n",
            "# My settings\ncustom = true\nprofiles = { personal = { id = '8d8ed9a1-9c7e-4ad3-a69a-6e9dc9a05745', agent = 'codex', auth = 'native' } } # keep this\n",
        ] {
            std::fs::write(&path, input)?;
            add_profile(&path, "second", native_profile())?;
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
        add_profile(&path, "work", native_profile())?;
        let before = std::fs::read(&path)?;
        let error = add_profile(&path, "work", native_profile()).unwrap_err();
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
        assert!(add_profile(&path, "new", native_profile()).is_err());
        assert_eq!(std::fs::read_to_string(&path)?, malformed);

        let missing = directory.path().join("new/config.toml");
        assert!(add_profile(&missing, "   ", native_profile()).is_err());
        let mut invalid = native_profile();
        invalid.id = uuid::Uuid::nil();
        assert!(add_profile(&missing, "new", invalid).is_err());
        assert!(!missing.exists());
        assert!(load(&directory.path().join("new")).is_err());
        Ok(())
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!(parse("[profiles.work").is_err());
    }
}
