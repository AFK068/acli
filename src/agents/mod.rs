use crate::config::Agent;
use std::path::Path;
use std::process::Command;

mod claude;
mod codex;

// A key selects API authentication; None selects the agent's native login.
pub(crate) fn command(agent: Agent, directory: &Path, key: Option<&str>) -> Command {
    match agent {
        Agent::Codex => codex::command(directory, key),
        Agent::ClaudeCode => claude::command(directory, key),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_profile_agent() {
        for (agent, program) in [(Agent::Codex, "codex"), (Agent::ClaudeCode, "claude")] {
            let command = command(agent, Path::new("profile-data"), None);
            assert_eq!(command.get_program(), program);
        }
    }
}
