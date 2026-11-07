use crate::config::AuthMode;
use std::path::Path;
use std::process::Command;

pub(super) fn command(auth: &AuthMode, directory: &Path, key: Option<&str>) -> Command {
    let mut command = Command::new("claude");
    command.env("CLAUDE_CONFIG_DIR", directory);

    for name in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_REFRESH_TOKEN",
        "CLAUDE_CODE_OAUTH_SCOPES",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
    ] {
        command.env_remove(name);
    }

    match auth {
        AuthMode::Subscription => {
            command.args(["--settings", r#"{"forceLoginMethod":"claudeai"}"#]);
        }
        AuthMode::Api => {
            command.args(["--settings", r#"{"forceLoginMethod":"console"}"#]);
            if let Some(key) = key {
                command.env("ANTHROPIC_API_KEY", key);
            }
        }
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn subscription_uses_its_own_login_and_directory() {
        let directory = Path::new("profile-data");
        let command = command(&AuthMode::Subscription, directory, None);
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[OsStr::new("CLAUDE_CONFIG_DIR")],
            Some(directory.as_os_str())
        );
        assert_eq!(environment[OsStr::new("ANTHROPIC_API_KEY")], None);
        assert!(
            command
                .get_args()
                .any(|arg| arg == r#"{"forceLoginMethod":"claudeai"}"#)
        );
    }

    #[test]
    fn api_key_is_passed_in_environment_only() {
        let command = command(
            &AuthMode::Api,
            Path::new("profile-data"),
            Some("test-secret"),
        );
        let environment: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            environment[OsStr::new("ANTHROPIC_API_KEY")],
            Some(OsStr::new("test-secret"))
        );
        assert_eq!(environment[OsStr::new("ANTHROPIC_AUTH_TOKEN")], None);
        assert_eq!(environment[OsStr::new("CLAUDE_CODE_OAUTH_TOKEN")], None);
        assert!(
            command
                .get_args()
                .all(|arg| !arg.to_string_lossy().contains("test-secret"))
        );
    }
}
