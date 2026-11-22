use super::collect_skills;
use crate::files;
use anyhow::{Context, ensure};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn directory() -> anyhow::Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|p| !p.is_empty()) {
        return Ok(std::path::absolute(path)?);
    }
    Ok(std::env::home_dir()
        .context("Missing home directory")?
        .join(".claude"))
}

#[derive(Deserialize)]
struct Registry {
    version: u32,
    plugins: BTreeMap<String, Vec<Installation>>,
}
#[derive(Deserialize)]
struct Installation {
    scope: String,
    #[serde(rename = "installPath")]
    path: PathBuf,
}

pub(super) fn plugins(home: &Path, folders: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    let path = home.join("plugins/installed_plugins.json");
    let Some(registry) = files::read_text(&path)? else {
        return Ok(());
    };
    let registry: Registry = serde_json::from_str(&registry)
        .with_context(|| format!("Invalid Claude plugin registry: {}", path.display()))?;
    ensure!(
        registry.version == 2,
        "Unsupported Claude plugin registry version {}",
        registry.version
    );
    let Some(settings) = files::read_text(&home.join("settings.json"))? else {
        return Ok(());
    };
    let settings: serde_json::Value = serde_json::from_str(&settings)?;
    for (id, installs) in registry.plugins {
        if settings["enabledPlugins"][&id].as_bool() != Some(true) {
            continue;
        }
        for install in installs {
            if install.scope == "user" && install.path.is_absolute() {
                plugin_skills(&install.path, folders)?;
            }
        }
    }
    Ok(())
}

fn plugin_skills(plugin: &Path, folders: &mut Vec<PathBuf>) -> anyhow::Result<()> {
    collect_skills(&plugin.join("skills"), folders)?;
    let manifest = plugin.join(".claude-plugin/plugin.json");
    if !manifest.is_file() {
        return Ok(());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&fs::read(manifest)?)?;
    let plugin_root = plugin.canonicalize()?;
    let extra = &manifest["skills"];
    let paths = extra
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(std::slice::from_ref(extra));
    for value in paths {
        let Some(relative) = value.as_str() else {
            continue;
        };
        let path = plugin.join(relative);
        if !path.is_dir() || !path.canonicalize()?.starts_with(&plugin_root) {
            continue;
        }
        if path.join("SKILL.md").is_file() {
            folders.push(path);
        } else {
            collect_skills(&path, folders)?;
        }
    }
    Ok(())
}
