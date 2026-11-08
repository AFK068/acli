use std::path::Path;
use std::process::Command;

pub(super) fn command(directory: &Path, key: Option<&str>) -> Command {
    let mut command = Command::new("kimi");
    command.env("KIMI_CODE_HOME", directory);

    // KIMI_MODEL_* overrides the saved model and credentials for this launch.
    for name in [
        "KIMI_MODEL_NAME",
        "KIMI_MODEL_API_KEY",
        "KIMI_MODEL_PROVIDER_TYPE",
        "KIMI_MODEL_BASE_URL",
        "KIMI_MODEL_MAX_CONTEXT_SIZE",
        "KIMI_MODEL_CAPABILITIES",
        "KIMI_MODEL_DISPLAY_NAME",
        "KIMI_MODEL_MAX_OUTPUT_SIZE",
        "KIMI_MODEL_REASONING_KEY",
        "KIMI_MODEL_THINKING_EFFORT",
        "KIMI_MODEL_ADAPTIVE_THINKING",
        "KIMI_CODE_CUSTOM_HEADERS",
        "KIMI_CODE_BASE_URL",
        "KIMI_CODE_OAUTH_HOST",
        "KIMI_OAUTH_HOST",
    ] {
        command.env_remove(name);
    }

    if let Some(key) = key {
        command
            .env("KIMI_MODEL_NAME", "kimi-for-coding")
            .env("KIMI_MODEL_PROVIDER_TYPE", "kimi")
            .env("KIMI_MODEL_BASE_URL", "https://api.kimi.com/coding/v1")
            .env("KIMI_MODEL_API_KEY", key);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn native_uses_its_own_login_without_model_overrides() {
        let directory = Path::new("profile-data");
        let command = command(directory, None);
        let env: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[OsStr::new("KIMI_CODE_HOME")],
            Some(directory.as_os_str())
        );
        for name in [
            "KIMI_MODEL_NAME",
            "KIMI_MODEL_API_KEY",
            "KIMI_MODEL_BASE_URL",
            "KIMI_CODE_CUSTOM_HEADERS",
        ] {
            assert_eq!(env[OsStr::new(name)], None);
        }
        assert_eq!(command.get_args().count(), 0);
    }

    #[test]
    fn api_key_uses_kimi_code_endpoint_without_secret_arguments() {
        let command = command(Path::new("profile-data"), Some("test-secret"));
        let env: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[OsStr::new("KIMI_MODEL_API_KEY")],
            Some(OsStr::new("test-secret"))
        );
        assert_eq!(
            env[OsStr::new("KIMI_MODEL_BASE_URL")],
            Some(OsStr::new("https://api.kimi.com/coding/v1"))
        );
        assert_eq!(
            env[OsStr::new("KIMI_MODEL_NAME")],
            Some(OsStr::new("kimi-for-coding"))
        );
        assert!(
            command
                .get_args()
                .all(|arg| !arg.to_string_lossy().contains("test-secret"))
        );
    }
}
