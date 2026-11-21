use serde::{Deserialize, Serialize};
use toml_edit::{Array, InlineTable};

pub(crate) type McpServers = std::collections::BTreeMap<String, McpServer>;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpServer {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl McpServer {
    pub fn from_argv(argv: Vec<String>) -> Option<Self> {
        let mut args = argv.into_iter();
        Some(Self {
            command: args.next()?,
            args: args.collect(),
        })
    }

    pub fn validate(&self, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)),
            "MCP server names may contain letters, numbers, hyphens and underscores"
        );
        anyhow::ensure!(
            !self.command.trim().is_empty(),
            "Missing command for MCP server '{name}'"
        );
        anyhow::ensure!(
            !self.command.contains('\0') && !self.args.iter().any(|arg| arg.contains('\0')),
            "Invalid NUL in MCP server '{name}'"
        );
        Ok(())
    }

    pub fn table(&self) -> InlineTable {
        let mut table = InlineTable::new();
        table.insert("command", self.command.as_str().into());
        table.insert("args", Array::from_iter(&self.args).into());
        table
    }
}
