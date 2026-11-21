use std::path::Path;
use std::process::Command;

const API_PROVIDER: &str = concat!(
    r#"model_providers.acli={name="OpenAI","#,
    r#"base_url="https://api.openai.com/v1","#,
    r#"wire_api="responses",env_key="ACLI_API_KEY","#,
    "requires_openai_auth=false}",
);

pub(super) fn command(directory: &Path, key: Option<&str>) -> Command {
    let mut command = Command::new("codex");
    command
        .env("CODEX_HOME", directory)
        .args(["-c", r#"cli_auth_credentials_store="file""#]);

    for name in [
        "OPENAI_API_KEY",
        "OPENAI_BASE_URL",
        "CODEX_API_KEY",
        "ACLI_API_KEY",
    ] {
        command.env_remove(name);
    }

    if let Some(key) = key {
        command
            .args([
                "-c",
                r#"model_provider="acli""#,
                "-c",
                r#"forced_login_method="api""#,
                "-c",
                API_PROVIDER,
            ])
            .env("ACLI_API_KEY", key);
    } else {
        command.args([
            "-c",
            r#"model_provider="openai""#,
            "-c",
            r#"forced_login_method="chatgpt""#,
        ]);
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
        assert_eq!(vars["CODEX_HOME"], Some(directory.as_os_str()));
        assert_eq!(vars["OPENAI_API_KEY"], None);
        assert!(
            command
                .get_args()
                .any(|arg| arg == r#"forced_login_method="chatgpt""#)
        );
    }

    #[test]
    fn api_key_is_passed_in_environment_only() {
        let command = command(Path::new("profile-data"), Some("test-secret"));
        assert_eq!(env(&command)["ACLI_API_KEY"], Some("test-secret".as_ref()));
        assert!(
            command
                .get_args()
                .any(|arg| arg == r#"forced_login_method="api""#)
        );
        crate::test_support::assert_secret_hidden(&command, "test-secret");
    }
}
