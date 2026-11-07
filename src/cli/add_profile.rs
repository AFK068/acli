use crate::config::{Agent, AuthMode, Profile, storage};
use std::io::IsTerminal;

pub(super) fn run(path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "Run 'acli profile add' in an interactive terminal"
    );
    let config = storage::load(path)?;
    let theme = dialoguer::theme::ColorfulTheme::default();

    let name: String = dialoguer::Input::with_theme(&theme)
        .with_prompt("Profile name")
        .validate_with(|name: &String| config.validate_profile_name(name.trim()))
        .interact_text()?;
    let name = name.trim();

    let Some(agent) = select("Agent", &["Codex", "Claude Code"])? else {
        return Ok(());
    };
    let agent = match agent {
        0 => Agent::Codex,
        _ => Agent::ClaudeCode,
    };

    let Some(auth) = select("Authentication", &["Subscription", "API key"])? else {
        return Ok(());
    };
    let auth = match auth {
        0 => AuthMode::Subscription,
        _ => AuthMode::Api,
    };

    let api_key_env = if auth == AuthMode::Api {
        Some(ask_key_env()?)
    } else {
        None
    };

    let profile = Profile::new(agent, auth, api_key_env);
    storage::add_profile(path, name, profile)?;
    println!("Profile '{name}' saved to {}", path.display());
    Ok(())
}

fn ask_key_env() -> anyhow::Result<String> {
    let theme = dialoguer::theme::ColorfulTheme::default();
    let name: String = dialoguer::Input::with_theme(&theme)
        .with_prompt("API key environment variable name (not the key)")
        .validate_with(|name: &String| {
            if name.trim().is_empty() {
                Err("Variable name must not be empty")
            } else {
                Ok(())
            }
        })
        .interact_text()?;
    Ok(name.trim().to_owned())
}

fn select(prompt: &str, options: &[&str]) -> anyhow::Result<Option<usize>> {
    let theme = dialoguer::theme::ColorfulTheme::default();
    Ok(dialoguer::Select::with_theme(&theme)
        .with_prompt(prompt)
        .items(options)
        .default(0)
        .interact_opt()?)
}
