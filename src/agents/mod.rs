use crate::config::{Agent, Profile};
use std::path::Path;
use std::process::Command;

mod claude;
mod codex;
mod environment;
mod kimi;
mod opencode;
mod pi;
mod zcode;

// A key selects API authentication; None selects the agent's native login.
pub(crate) fn command(profile: &Profile, directory: &Path, key: Option<&str>) -> Command {
    match profile.agent {
        Agent::Codex => codex::command(directory, key),
        Agent::ClaudeCode => claude::command(directory, key),
        Agent::Kimi => kimi::command(directory, key),
        Agent::OpenCode => opencode::command(directory, profile.provider.zip(key)),
        Agent::Pi => pi::command(directory),
        Agent::ZCode => zcode::command(directory),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_profile_agent() {
        for (agent, program) in [
            (Agent::Codex, "codex"),
            (Agent::ClaudeCode, "claude"),
            (Agent::Kimi, "kimi"),
            (Agent::OpenCode, "opencode"),
            (Agent::Pi, "pi"),
            (Agent::ZCode, "zcode"),
        ] {
            let profile = Profile::new(agent, crate::config::AuthMode::Native);
            let command = command(&profile, Path::new("profile-data"), None);
            assert_eq!(command.get_program(), program);
        }
    }
}
