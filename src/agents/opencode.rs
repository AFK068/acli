use std::path::Path;
use std::process::Command;

pub(super) fn command(directory: &Path) -> Command {
    let mut command = Command::new("opencode");
    super::environment::clear_provider_credentials(&mut command);
    // OpenCode 2 uses a shared server unless standalone mode is explicit.
    command
        .arg("--standalone")
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_CACHE_HOME", directory.join("cache"))
        .env("XDG_STATE_HOME", directory.join("state"));
    for name in [
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_AUTH_CONTENT",
        "OPENCODE_DB",
    ] {
        command.env_remove(name);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn isolates_credentials_config_and_server() {
        let directory = Path::new("profile-data");
        let command = command(directory);
        let env: BTreeMap<_, _> = command.get_envs().collect();
        for (variable, child) in [
            ("XDG_DATA_HOME", "data"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
        ] {
            assert_eq!(
                env[OsStr::new(variable)],
                Some(directory.join(child).as_os_str())
            );
        }
        for name in [
            "OPENCODE_DB",
            "OPENCODE_CONFIG_DIR",
            "OPENCODE_AUTH_CONTENT",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
        ] {
            assert_eq!(env[OsStr::new(name)], None);
        }
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["--standalone"]);
        assert!(command.get_current_dir().is_none());
    }
}
