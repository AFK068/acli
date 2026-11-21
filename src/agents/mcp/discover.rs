use super::{ZCODE_COMPAT, ZCODE_NATIVE, managed, zcode_uses_compat};
use crate::config::{Agent, Config, McpServer, McpServers, paths};
use anyhow::Context;
use serde_json::{Map, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn user_dirs() -> anyhow::Result<Vec<(Agent, PathBuf)>> {
    let home = std::env::home_dir().context("Missing home directory")?;
    let dir = |var, default: &str| {
        std::env::var_os(var)
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(default))
    };
    Ok(vec![
        (Agent::ClaudeCode, dir("CLAUDE_CONFIG_DIR", "")),
        (Agent::Codex, dir("CODEX_HOME", ".codex")),
        (Agent::Kimi, dir("KIMI_CODE_HOME", ".kimi-code")),
        (
            Agent::OpenCode,
            dir("XDG_CONFIG_HOME", ".config").join("opencode"),
        ),
        (Agent::Pi, dir("PI_CODING_AGENT_DIR", ".pi/agent")),
        (Agent::ZCode, home),
    ])
}

pub(crate) fn shared(
    path: &Path,
    config: &Config,
    target: Option<&str>,
    user_dirs: &[(Agent, PathBuf)],
) -> anyhow::Result<McpServers> {
    let mut servers = config.mcp.clone();
    let mut local = HashSet::new();
    let profiles = config.profiles.iter().map(|(name, profile)| {
        let mut dir = paths::profile_dir(path, profile);
        if matches!(profile.agent, Agent::OpenCode) {
            dir = dir.join("config/opencode");
        }
        (profile.agent, dir, Some(name.as_str()) == target)
    });
    let users = user_dirs
        .iter()
        .map(|(agent, dir)| (*agent, dir.clone(), false));
    for (agent, dir, is_target) in profiles.chain(users) {
        for (name, entry) in entries(agent, &dir)? {
            if managed(&name, &entry) {
                continue;
            }
            if is_target {
                local.insert(name.clone());
            }
            if let Some(server) = portable(agent, entry)
                && server.validate(&name).is_ok()
            {
                servers.entry(name).or_insert(server);
            }
        }
    }
    servers.retain(|name, _| !local.contains(name) && !local.contains(&format!("acli-{name}")));
    Ok(servers)
}

fn entries(agent: Agent, dir: &Path) -> anyhow::Result<Map<String, Value>> {
    let (file, section) = match agent {
        Agent::ClaudeCode => (".claude.json", "/mcpServers"),
        Agent::Kimi | Agent::Pi => ("mcp.json", "/mcpServers"),
        Agent::ZCode if zcode_uses_compat(dir)? => (ZCODE_COMPAT, "/mcpServers"),
        Agent::ZCode => (ZCODE_NATIVE, "/mcp/servers"),
        Agent::Codex => ("config.toml", "/mcp_servers"),
        Agent::OpenCode => {
            let mut servers = Map::new();
            for file in ["opencode.json", "opencode.jsonc"] {
                servers.extend(section_in(&dir.join(file), "/mcp/servers")?);
            }
            return Ok(servers);
        }
    };
    section_in(&dir.join(file), section)
}

fn section_in(path: &Path, section: &str) -> anyhow::Result<Map<String, Value>> {
    if !path.try_exists()? {
        return Ok(Map::new());
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("Could not read '{}'", path.display()))?;
    let mut config: Value = if path.extension().is_some_and(|ext| ext == "toml") {
        toml_edit::de::from_str(&text)
            .with_context(|| format!("Invalid TOML in '{}'", path.display()))?
    } else {
        jsonc_parser::parse_to_serde_value(&text, &Default::default())
            .with_context(|| format!("Invalid JSON in '{}'", path.display()))?
    };
    Ok(config
        .pointer_mut(section)
        .and_then(Value::as_object_mut)
        .map(std::mem::take)
        .unwrap_or_default())
}

// Share only entries whose complete launch configuration fits our command + args format.
fn portable(agent: Agent, mut entry: Value) -> Option<McpServer> {
    let fields = entry.as_object_mut()?;
    let opencode = matches!(agent, Agent::OpenCode);
    let kind = if opencode { "local" } else { "stdio" };
    for (key, default) in [
        ("type", Value::from(kind)),
        ("enabled", Value::from(true)),
        ("disabled", Value::from(false)),
        ("env", Value::Object(Map::new())),
        ("environment", Value::Object(Map::new())),
    ] {
        if fields.remove(key).is_some_and(|value| value != default) {
            return None;
        }
    }
    if opencode {
        let argv = serde_json::from_value(fields.remove("command")?).ok()?;
        if fields.is_empty() {
            return McpServer::from_argv(argv);
        }
        return None;
    }
    serde_json::from_value(entry).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::storage, test_support::TestApp};
    use serde_json::json;

    #[test]
    fn each_profile_shares_its_native_server_without_importing_its_own_copy() -> anyhow::Result<()>
    {
        use Agent::*;
        let app = TestApp::new()?;
        for (agent, file, config) in [
            (
                ClaudeCode,
                ".claude.json",
                r#"{"mcpServers":{"claude-code":{"type":"stdio","command":"run","env":{}}},"projects":{"repo":{"mcpServers":{"private":{"command":"run"}}}}}"#,
            ),
            (Codex, "config.toml", "[mcp_servers.codex]\ncommand='run'"),
            (
                Kimi,
                "mcp.json",
                r#"{"mcpServers":{"kimi":{"command":"run"}}}"#,
            ),
            (Pi, "mcp.json", r#"{"mcpServers":{"pi":{"command":"run"}}}"#),
            (
                OpenCode,
                "config/opencode/opencode.jsonc",
                "{ // comment\n \"mcp\":{\"servers\":{\"opencode\":{\"type\":\"local\",\"command\":[\"run\",\"two words\"],},},},}",
            ),
            (
                ZCode,
                ".agents/mcp.json",
                r#"{"mcpServers":{"z-code":{"command":"run"}}}"#,
            ),
        ] {
            let path = app.add_native(&agent.to_string(), agent)?.join(file);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(path, config)?;
        }
        let config = storage::load(&app.config)?;
        for name in config.profiles.keys() {
            let servers = shared(&app.config, &config, Some(name), &[])?;
            let expected: Vec<_> = config.profiles.keys().filter(|key| *key != name).collect();
            assert_eq!(servers.keys().collect::<Vec<_>>(), expected);
            app.prepare(name)?;
        }
        Ok(())
    }

    #[test]
    fn changing_or_removing_the_original_updates_the_receiver_on_launch() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        let source = app.add_native("work", Agent::Codex)?.join("config.toml");
        let dest = app.add_native("personal", Agent::Pi)?.join("mcp.json");
        for arg in ["first", "two words"] {
            fs::write(
                &source,
                format!("[mcp_servers.docs]\ncommand='run'\nargs=['{arg}']"),
            )?;
            app.prepare("personal")?;
            assert_eq!(
                super::super::read(&dest)?["mcpServers"]["acli-docs"]["args"],
                json!([arg])
            );
        }
        fs::remove_file(source)?;
        app.prepare("personal")?;
        assert_eq!(super::super::read(&dest)?["mcpServers"], json!({}));
        Ok(())
    }

    #[test]
    fn global_servers_respect_local_names_and_profile_specific_settings() -> anyhow::Result<()> {
        let app = TestApp::new()?;
        fs::write(
            app.root.path().join(".claude.json"),
            json!({"mcpServers":{
                "docs":{"command":"run"}, "other":{"command":"run"},
                "private":{"command":"run","env":{"KEY":"secret"}},
                "remote":{"type":"http","url":"https://example.org"}
            }})
            .to_string(),
        )?;
        let receiver = app.add_native("work", Agent::Codex)?;
        fs::write(
            receiver.join("config.toml"),
            "[mcp_servers.docs]\ncommand='local'\nenabled=false",
        )?;
        let dirs = [(Agent::ClaudeCode, app.root.path().into())];
        let servers = shared(
            &app.config,
            &storage::load(&app.config)?,
            Some("work"),
            &dirs,
        )?;
        assert_eq!(servers.keys().collect::<Vec<_>>(), ["other"]);
        Ok(())
    }
}
