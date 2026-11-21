use crate::config::{Agent, AuthMode, Profile, paths, storage};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) struct TestApp {
    pub root: tempfile::TempDir,
    pub config: PathBuf,
    pub shared: PathBuf,
    pub claude: PathBuf,
}

impl TestApp {
    pub fn new() -> anyhow::Result<Self> {
        let root = tempfile::tempdir()?;
        Ok(Self {
            config: root.path().join("config.toml"),
            shared: root.path().join("shared"),
            claude: root.path().join("claude"),
            root,
        })
    }

    pub fn prepare(&self, name: &str) -> anyhow::Result<Command> {
        crate::app::launch::prepare(&self.config, name, &self.shared, &self.claude)
    }

    pub fn add_native(&self, name: &str, agent: Agent) -> anyhow::Result<PathBuf> {
        let profile = Profile::new(agent, AuthMode::Native);
        let dir = paths::profile_dir(&self.config, &profile);
        storage::add_profile(&self.config, name, profile)?;
        Ok(dir)
    }
}

pub(crate) fn write_skill(dir: &Path, text: &str) -> anyhow::Result<()> {
    fs::create_dir_all(dir)?;
    Ok(fs::write(dir.join("SKILL.md"), text)?)
}

// Indexing this map distinguishes explicitly removed variables from inherited ones.
pub(crate) fn env(command: &Command) -> BTreeMap<&str, Option<&OsStr>> {
    command
        .get_envs()
        .map(|(key, value)| (key.to_str().unwrap(), value))
        .collect()
}

pub(crate) fn assert_secret_hidden(command: &Command, secret: &str) {
    assert!(
        command
            .get_args()
            .all(|arg| !arg.to_string_lossy().contains(secret))
    );
}
