use std::io::IsTerminal;

use crate::app::profiles;
use crate::config::{Agent, AuthMode, Profile, Provider, storage};

pub(super) fn run(path: &std::path::Path) -> anyhow::Result<()> {
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal(),
        "Run 'acli profile add' in an interactive terminal"
    );
    let config = storage::load(path)?;

    let theme = dialoguer::theme::ColorfulTheme::default();
    let name = dialoguer::Input::<String>::with_theme(&theme)
        .with_prompt("Profile name")
        .validate_with(|name: &String| config.validate_profile_name(name.trim()))
        .interact_text()?;
    let name = name.trim();

    let agents = [
        ("Codex", Agent::Codex),
        ("Claude Code", Agent::ClaudeCode),
        ("Kimi Code", Agent::Kimi),
        ("OpenCode", Agent::OpenCode),
        ("Pi", Agent::Pi),
        ("ZCode (Z.ai)", Agent::ZCode),
    ];
    let modes = [("Native", AuthMode::Native), ("API key", AuthMode::Api)];
    let Some(agent) = select(&theme, "Agent", &agents)? else {
        return Ok(());
    };
    let auth = if agent.supports_api_key() {
        let Some(auth) = select(&theme, "Authentication", &modes)? else {
            return Ok(());
        };
        auth
    } else {
        println!("Sign in inside the agent after running this profile.");
        AuthMode::Native
    };
    let mut profile = Profile::new(agent, auth);
    if profile.requires_provider() {
        let providers = [
            ("OpenAI", Provider::OpenAi),
            ("Anthropic", Provider::Anthropic),
            ("Kimi Code (kimi.com)", Provider::KimiCodePlanCn),
            ("Kimi Code (kimi.ai)", Provider::KimiCodePlanGlobal),
        ];
        let Some(provider) = select(&theme, "Provider", &providers)? else {
            return Ok(());
        };
        profile.provider = Some(provider);
    }
    let key = if auth == AuthMode::Api {
        if matches!(agent, Agent::Kimi) {
            println!("Use a Kimi Code API key (api.kimi.com), not a Moonshot Platform key.");
        }
        Some(ask_key(&theme)?)
    } else {
        None
    };

    profiles::add(path, name, profile, key.as_deref())?;
    println!("Profile '{name}' saved");
    Ok(())
}

fn select<T: Copy>(
    theme: &dyn dialoguer::theme::Theme,
    prompt: &str,
    options: &[(&str, T)],
) -> anyhow::Result<Option<T>> {
    let labels: Vec<_> = options.iter().map(|(label, _)| *label).collect();
    let selected = dialoguer::Select::with_theme(theme)
        .with_prompt(prompt)
        .items(&labels)
        .default(0)
        .interact_opt()?;
    Ok(selected.map(|index| options[index].1))
}

fn ask_key(theme: &dyn dialoguer::theme::Theme) -> anyhow::Result<String> {
    Ok(dialoguer::Password::with_theme(theme)
        .with_prompt("API key (saved in the system credential store)")
        .validate_with(|key: &String| {
            anyhow::ensure!(!key.trim().is_empty(), "API key must not be empty");
            Ok(())
        })
        .interact()?)
}
