use crate::config::{Agent, McpServers, Profile};
use std::path::Path;
use std::process::Command;

mod claude;
mod codex;
mod environment;
mod kimi;
pub(crate) mod mcp;
mod opencode;
mod pi;
mod zcode;

// A key selects API authentication; None selects the agent's native login.
pub(crate) fn command(
    profile: &Profile,
    directory: &Path,
    key: Option<&str>,
    servers: &McpServers,
) -> anyhow::Result<Command> {
    mcp::sync(profile.agent, directory, servers)?;
    Ok(match profile.agent {
        Agent::ClaudeCode => claude::command(directory, key, servers),
        Agent::Codex => codex::command(directory, key, servers),
        Agent::OpenCode => opencode::command(directory, profile.provider.zip(key), servers),
        Agent::Pi => pi::command(directory),
        Agent::Kimi => kimi::command(directory, key),
        Agent::ZCode => zcode::command(directory),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_profile_agent() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        for (agent, program) in [
            (Agent::Codex, "codex"),
            (Agent::ClaudeCode, "claude"),
            (Agent::Kimi, "kimi"),
            (Agent::OpenCode, "opencode"),
            (Agent::Pi, "pi"),
            (Agent::ZCode, "zcode"),
        ] {
            let profile = Profile::new(agent, crate::config::AuthMode::Native);
            let command = command(&profile, dir.path(), None, &McpServers::new())?;
            assert_eq!(command.get_program(), program);
        }
        Ok(())
    }
}
