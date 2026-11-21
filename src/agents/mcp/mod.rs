mod discover;
pub(crate) use discover::{shared, user_dirs};

use crate::config::{Agent, McpServers};
use crate::files;
use anyhow::{Context, ensure};
use serde_json::{Map, Value, json};
use std::{fs, path::Path};

const PREFIX: &str = "acli-";
const ZCODE_NATIVE: &str = ".zcode/cli/config.json";
const ZCODE_COMPAT: &str = ".agents/mcp.json";

pub(super) fn shared_name(name: &str) -> String {
    format!("{PREFIX}{name}")
}

pub(super) fn named(servers: &McpServers) -> Map<String, Value> {
    servers
        .iter()
        .map(|(name, server)| (shared_name(name), json!(server)))
        .collect()
}

// The marker travels with the entry, so cleanup never claims a user's server by name.
const OWNER: &str = "ACLI_MANAGED_MCP";
fn managed(name: &str, server: &Value) -> bool {
    name.starts_with(PREFIX) && server["env"][OWNER].as_str() == Some(name)
}

pub(super) fn sync(agent: Agent, profile: &Path, servers: &McpServers) -> anyhow::Result<()> {
    if !matches!(agent, Agent::Kimi | Agent::Pi | Agent::ZCode) {
        return Ok(());
    }
    let _lock = files::lock(&profile.join(".acli-mcp.lock"))?;
    let mut wanted = named(servers);
    for (name, server) in &mut wanted {
        server["env"] = json!({(OWNER): name});
    }
    if !matches!(agent, Agent::ZCode) {
        return update(&profile.join("mcp.json"), &["mcpServers"], &wanted);
    }

    // ZCode ignores the compatibility file whenever the native section has servers.
    let native = profile.join(ZCODE_NATIVE);
    let compat = profile.join(ZCODE_COMPAT);
    let empty = Map::new();
    if zcode_uses_compat(profile)? {
        update(&compat, &["mcpServers"], &wanted)?;
        update(&native, &["mcp", "servers"], &empty)
    } else {
        for server in wanted.values_mut() {
            server["type"] = json!("stdio");
        }
        update(&native, &["mcp", "servers"], &wanted)?;
        update(&compat, &["mcpServers"], &empty)
    }
}

fn zcode_uses_compat(profile: &Path) -> anyhow::Result<bool> {
    if !profile.join(ZCODE_COMPAT).try_exists()? {
        return Ok(false);
    }
    let native = read(&profile.join(ZCODE_NATIVE))?;
    Ok(!native["mcp"]["servers"]
        .as_object()
        .is_some_and(|entries| entries.iter().any(|(name, entry)| !managed(name, entry))))
}

fn read(path: &Path) -> anyhow::Result<Value> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid JSON in '{}'", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e).with_context(|| format!("Could not read '{}'", path.display())),
    }
}

fn update(path: &Path, keys: &[&str], wanted: &Map<String, Value>) -> anyhow::Result<()> {
    let mut config = read(path)?;
    let mut section = &mut config;
    for key in keys {
        let obj = section
            .as_object_mut()
            .with_context(|| format!("Expected JSON object in '{}'", path.display()))?;
        if wanted.is_empty() && !obj.contains_key(*key) {
            return Ok(());
        }
        section = obj.entry(*key).or_insert_with(|| json!({}));
    }
    let entries = section
        .as_object_mut()
        .with_context(|| format!("Invalid MCP server table in '{}'", path.display()))?;
    let previous = entries.clone();
    entries.retain(|name, entry| !managed(name, entry) || wanted.contains_key(name));
    for (name, server) in wanted {
        if let Some(existing) = entries.get_mut(name) {
            ensure!(
                managed(name, existing),
                "MCP server '{name}' already exists in '{}'",
                path.display()
            );
            existing["command"] = server["command"].clone();
            existing["args"] = server["args"].clone();
        } else {
            entries.insert(name.clone(), server.clone());
        }
    }
    if *entries != previous {
        // Preserve an existing link to a custom configuration file.
        let dest = if path.is_symlink() {
            path.canonicalize()?
        } else {
            path.to_owned()
        };
        files::create_dir(files::parent_dir(&dest))?;
        files::write(&dest, &serde_json::to_string_pretty(&config)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpServer;

    fn fixture() -> McpServers {
        McpServers::from([(
            "docs".into(),
            McpServer {
                command: "run".into(),
                args: vec!["two words".into()],
            },
        )])
    }

    #[test]
    fn file_backed_profiles_share_then_remove_only_their_managed_server() -> anyhow::Result<()> {
        for (agent, file, key) in [
            (Agent::Kimi, "mcp.json", "/mcpServers"),
            (Agent::Pi, "mcp.json", "/mcpServers"),
            (Agent::ZCode, ".zcode/cli/config.json", "/mcp/servers"),
        ] {
            let profile = tempfile::tempdir()?;
            let path = profile.path().join(file);
            sync(agent, profile.path(), &fixture())?;
            let mut config = read(&path)?;
            let entries = config.pointer_mut(key).unwrap();
            assert_eq!(entries["acli-docs"]["args"], json!(["two words"]));
            entries["mine"] = json!({"command":"other"});
            entries["acli-docs"]["enabled"] = json!(false);
            fs::write(&path, config.to_string())?;
            sync(agent, profile.path(), &fixture())?;
            assert_eq!(read(&path)?, config);
            sync(agent, profile.path(), &McpServers::new())?;
            assert_eq!(
                read(&path)?.pointer(key),
                Some(&json!({"mine":{"command":"other"}}))
            );
        }
        Ok(())
    }

    #[test]
    fn zcode_keeps_compatibility_servers_visible() -> anyhow::Result<()> {
        let profile = tempfile::tempdir()?;
        let path = profile.path().join(".agents/mcp.json");
        fs::create_dir(path.parent().unwrap())?;
        fs::write(&path, r#"{"mcpServers":{"mine":{"command":"other"}}}"#)?;
        sync(Agent::ZCode, profile.path(), &fixture())?;
        let config = read(&path)?;
        assert_eq!(config["mcpServers"]["acli-docs"]["command"], "run");
        assert_eq!(config["mcpServers"]["mine"]["command"], "other");
        assert!(!profile.path().join(".zcode/cli/config.json").exists());
        Ok(())
    }

    #[test]
    fn preserves_user_config_on_collisions_and_parse_errors() -> anyhow::Result<()> {
        let profile = tempfile::tempdir()?;
        let path = profile.path().join("mcp.json");
        for original in ["{", r#"{"mcpServers":{"acli-docs":{"command":"mine"}}}"#] {
            fs::write(&path, original)?;
            assert!(sync(Agent::Pi, profile.path(), &fixture()).is_err());
            assert_eq!(fs::read_to_string(&path)?, original);
        }
        Ok(())
    }
}
