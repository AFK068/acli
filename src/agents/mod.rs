use crate::config::{Agent, Profile};
use std::path::Path;
use std::process::Command;

mod claude;
mod codex;

pub(crate) fn command(profile: &Profile, directory: &Path, key: Option<&str>) -> Command {
    match profile.agent {
        Agent::Codex => codex::command(&profile.auth, directory, key),
        Agent::ClaudeCode => claude::command(&profile.auth, directory, key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthMode;

    #[test]
    fn selects_profile_agent() {
        for (agent, program) in [(Agent::Codex, "codex"), (Agent::ClaudeCode, "claude")] {
            let profile = Profile::new(agent, AuthMode::Subscription);
            let command = command(&profile, Path::new("profile-data"), None);
            assert_eq!(command.get_program(), program);
        }
    }
}
