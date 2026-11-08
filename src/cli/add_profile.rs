use std::io::IsTerminal;

use crate::app::profiles;
use crate::config::{Agent, AuthMode, Profile};

pub(super) fn run(path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "Run 'acli profile add' in an interactive terminal"
    );
    let config = profiles::load(path)?;

    let theme = dialoguer::theme::ColorfulTheme::default();
    let name = dialoguer::Input::<String>::with_theme(&theme)
        .with_prompt("Profile name")
        .validate_with(|name: &String| config.validate_profile_name(name.trim()))
        .interact_text()?;
    let name = name.trim();

    let agents = [("Codex", Agent::Codex), ("Claude Code", Agent::ClaudeCode)];
    let modes = [("Native", AuthMode::Native), ("API key", AuthMode::Api)];
    let Some(agent) = select("Agent", &agents)? else {
        return Ok(());
    };
    let Some(auth) = select("Authentication", &modes)? else {
        return Ok(());
    };
    let key = if auth == AuthMode::Api {
        Some(ask_key()?)
    } else {
        None
    };

    profiles::add(path, name, Profile::new(agent, auth), key.as_deref())?;
    println!("Profile '{name}' saved");
    Ok(())
}

fn select<T: Copy>(prompt: &str, options: &[(&str, T)]) -> anyhow::Result<Option<T>> {
    let labels: Vec<_> = options.iter().map(|(label, _)| *label).collect();
    let selected = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&labels)
        .default(0)
        .interact_opt()?;
    Ok(selected.map(|index| options[index].1))
}

fn ask_key() -> anyhow::Result<String> {
    dialoguer::Password::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("API key (saved in the system credential store)")
        .validate_with(|key: &String| {
            anyhow::ensure!(!key.trim().is_empty(), "API key must not be empty");
            Ok(())
        })
        .interact()
        .map_err(Into::into)
}
