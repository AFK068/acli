use std::path::Path;
use std::process::Command;

pub(super) fn command(directory: &Path) -> Command {
    let mut command = Command::new("pi");
    super::environment::clear_provider_credentials(&mut command);
    command
        .env("PI_CODING_AGENT_DIR", directory)
        .env_remove("PI_CODING_AGENT_SESSION_DIR");
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsStr;

    #[test]
    fn isolates_saved_accounts_and_does_not_use_shell_api_keys() {
        let directory = Path::new("profile-data");
        let command = command(directory);
        let env: BTreeMap<_, _> = command.get_envs().collect();
        assert_eq!(
            env[OsStr::new("PI_CODING_AGENT_DIR")],
            Some(directory.as_os_str())
        );
        for name in [
            "PI_CODING_AGENT_SESSION_DIR",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "KIMI_API_KEY",
        ] {
            assert_eq!(env[OsStr::new(name)], None);
        }
        assert!(command.get_current_dir().is_none());
    }
}
