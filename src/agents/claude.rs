use std::path::Path;
use std::process::Command;

pub(super) fn command(directory: &Path, key: Option<&str>) -> Command {
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

    if let Some(key) = key {
        command
            .args(["--settings", r#"{"forceLoginMethod":"console"}"#])
            .env("ANTHROPIC_API_KEY", key);
    } else {
        command.args(["--settings", r#"{"forceLoginMethod":"claudeai"}"#]);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::env;

    #[test]
    fn native_uses_its_own_login_and_directory() {
        let directory = Path::new("profile-data");
        let command = command(directory, None);
        let vars = env(&command);
        assert_eq!(vars["CLAUDE_CONFIG_DIR"], Some(directory.as_os_str()));
        assert_eq!(vars["ANTHROPIC_API_KEY"], None);
        assert!(
            command
                .get_args()
                .any(|arg| arg == r#"{"forceLoginMethod":"claudeai"}"#)
        );
    }

    #[test]
    fn api_key_is_passed_in_environment_only() {
        let command = command(Path::new("profile-data"), Some("test-secret"));
        let vars = env(&command);
        assert_eq!(vars["ANTHROPIC_API_KEY"], Some("test-secret".as_ref()));
        assert_eq!(vars["ANTHROPIC_AUTH_TOKEN"], None);
        assert_eq!(vars["CLAUDE_CODE_OAUTH_TOKEN"], None);
        assert!(
            command
                .get_args()
                .any(|arg| arg == r#"{"forceLoginMethod":"console"}"#)
        );
        crate::test_support::assert_secret_hidden(&command, "test-secret");
    }
}
