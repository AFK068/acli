use std::path::Path;
use std::process::Command;

pub(super) fn command(directory: &Path) -> Command {
    let mut command = Command::new("zcode");
    // ZCode requires both an isolated home and data root, including on Windows.
    command
        .env("HOME", directory)
        .env("USERPROFILE", directory)
        .env("ZCODE_DATA_BASE_DIR", directory)
        .env("ZCODE_STORAGE_DIR", directory.join(".zcode"));
    for name in [
        "ZCODE_PERSONAL_PROVIDER_CONFIG_FILE",
        "ZCODE_BUILTIN_PROVIDER_CONFIG_FILE",
        "ZCODE_BUILTIN_PROVIDER_BUNDLED_CONFIG_FILE",
        "ZCODE_SESSION_DB_PATH",
        "ZCODE_LOG_DIR",
        "ZCODE_API_KEY",
        "ZCODE_MODEL",
        "ZCODE_BASE_URL",
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
    fn isolates_home_credentials_and_sessions_without_changing_working_directory() {
        let directory = Path::new("profile-data");
        let command = command(directory);
        let env: BTreeMap<_, _> = command.get_envs().collect();
        for name in ["HOME", "USERPROFILE", "ZCODE_DATA_BASE_DIR"] {
            assert_eq!(env[OsStr::new(name)], Some(directory.as_os_str()));
        }
        assert_eq!(
            env[OsStr::new("ZCODE_STORAGE_DIR")],
            Some(directory.join(".zcode").as_os_str())
        );
        for name in [
            "ZCODE_PERSONAL_PROVIDER_CONFIG_FILE",
            "ZCODE_SESSION_DB_PATH",
            "ZCODE_API_KEY",
        ] {
            assert_eq!(env[OsStr::new(name)], None);
        }
        assert!(command.get_current_dir().is_none());
    }
}
