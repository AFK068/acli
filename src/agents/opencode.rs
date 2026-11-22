use crate::config::{McpServers, Provider};
use serde_json::json;
use std::path::Path;
use std::process::Command;

pub(super) fn command(
    directory: &Path,
    credentials: Option<(Provider, &str)>,
    servers: &McpServers,
) -> Command {
    let mut command = Command::new("opencode");
    super::environment::clear_provider_credentials(&mut command);
    // OpenCode 2 uses a shared server unless standalone mode is explicit.
    command
        .arg("--standalone")
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("XDG_CONFIG_HOME", directory.join("config"))
        .env("XDG_CACHE_HOME", directory.join("cache"))
        .env("XDG_STATE_HOME", directory.join("state"));
    for name in [
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_AUTH_CONTENT",
        "OPENCODE_DB",
    ] {
        command.env_remove(name);
    }
    let mut config = json!({});
    if let Some((provider, key)) = credentials {
        let variable = match provider {
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::KimiCodePlanCn | Provider::KimiCodePlanGlobal => "KIMI_API_KEY",
        };
        command.env(variable, key);
        config["policies"] = json!([
            {"action":"provider.use", "resource":"*", "effect":"deny"},
            {"action":"provider.use", "resource":provider.to_string(), "effect":"allow"}
        ]);
    }
    for (name, server) in servers {
        let argv: Vec<_> = std::iter::once(&server.command)
            .chain(&server.args)
            .collect();
        config["mcp"]["servers"][super::mcp::shared_name(name)] =
            json!({"type":"local", "command":argv});
    }
    if config != json!({}) {
        command.env("OPENCODE_CONFIG_CONTENT", config.to_string());
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::env;

    use Provider::*;
    const PROVIDERS: [(Provider, &str, &str); 4] = [
        (OpenAi, "openai", "OPENAI_API_KEY"),
        (Anthropic, "anthropic", "ANTHROPIC_API_KEY"),
        (KimiCodePlanCn, "kimi-code-plan-cn", "KIMI_API_KEY"),
        (KimiCodePlanGlobal, "kimi-code-plan-global", "KIMI_API_KEY"),
    ];

    #[test]
    fn isolates_credentials_config_and_server() {
        let directory = Path::new("profile-data");
        let command = command(directory, None, &McpServers::new());
        let vars = env(&command);
        for (variable, child) in [
            ("XDG_DATA_HOME", "data"),
            ("XDG_CONFIG_HOME", "config"),
            ("XDG_CACHE_HOME", "cache"),
            ("XDG_STATE_HOME", "state"),
        ] {
            assert_eq!(vars[variable], Some(directory.join(child).as_os_str()));
        }
        for name in [
            "OPENCODE_DB",
            "OPENCODE_CONFIG_DIR",
            "OPENCODE_AUTH_CONTENT",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
        ] {
            assert_eq!(vars[name], None);
        }
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["--standalone"]);
        assert!(command.get_current_dir().is_none());
    }

    #[test]
    fn limits_credentials_to_the_selected_provider_with_shared_mcp() -> anyhow::Result<()> {
        let servers = McpServers::from([(
            "docs".into(),
            crate::config::McpServer {
                command: "fixture".into(),
                args: vec!["two words".into()],
            },
        )]);
        for (provider, name, variable) in PROVIDERS {
            let command = command(Path::new("profile"), Some((provider, "secret")), &servers);
            let vars = env(&command);
            for key in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "KIMI_API_KEY"] {
                assert_eq!(vars[key], (key == variable).then_some("secret".as_ref()));
            }
            crate::test_support::assert_secret_hidden(&command, "secret");

            let config = vars["OPENCODE_CONFIG_CONTENT"].unwrap().to_str().unwrap();
            let policy: serde_json::Value = serde_json::from_str(config)?;
            assert_eq!(
                policy,
                serde_json::json!({"mcp":{"servers":{"acli-docs":{"type":"local","command":["fixture","two words"]}}},"policies": [
                    {"action":"provider.use", "resource":"*", "effect":"deny"},
                    {"action":"provider.use", "resource":name, "effect":"allow"}
                ]})
            );
        }
        Ok(())
    }
}
