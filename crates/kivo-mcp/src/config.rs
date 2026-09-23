//! An MCP server as KIVO keeps it (TOOLS_AND_CONTROL §9): how to reach it, where it came from,
//! and what the user decided about each of its tools. Secrets (an API key in an env variable, a
//! bearer token) are never stored here: they are names of Credential Manager entries.

use kivo_core::tool::Risk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How KIVO reaches a server.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum Transport {
    /// A program on this PC, spoken to over its stdin and stdout.
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, EnvValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    /// A remote server over Streamable HTTP.
    Http {
        url: String,
        /// A Credential Manager entry holding the `Authorization` header's token, when the
        /// server takes one (an imported header, or a connector's sign-in).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        token: Option<String>,
        /// A Credential Manager entry holding the server's OAuth tokens, once the user signed in
        /// (MCP authorization, INT-04); they are refreshed before they expire.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        oauth: Option<String>,
    },
}

/// An environment variable's value: plain, or kept in Credential Manager under this name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum EnvValue {
    Plain(String),
    Secret(String),
}

/// What the user decided about one of a server's tools.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSetting {
    pub enabled: bool,
    /// The user's own risk for it (lower or higher than the default Medium).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<Risk>,
    /// The hash of the name, description and schema the user approved (TOOL-36): a tool whose
    /// hash no longer matches is switched off until it is reviewed again.
    #[serde(default)]
    pub approved: String,
}

/// An MCP server in KIVO's setup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    /// Letters, digits, `-` and `_`: it is part of every tool's id (`mcp.<id>.<tool>`).
    pub id: String,
    pub name: String,
    pub transport: Transport,
    pub enabled: bool,
    /// The app it was imported from (`claude-desktop`, `cursor`, …) or the connector it belongs
    /// to (`connector:github`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub tools: BTreeMap<String, ToolSetting>,
}

impl McpServer {
    /// A remote server: its tools send data off the PC.
    pub fn remote(&self) -> bool {
        matches!(self.transport, Transport::Http { .. })
    }
}

/// A server id made from any name: lowercase letters, digits and `-`, at most 32 characters.
pub fn server_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
        if id.len() >= 32 {
            break;
        }
    }
    let id = id.trim_end_matches('-').to_owned();
    if id.is_empty() { "server".into() } else { id }
}

/// A tool id KIVO uses for a server's tool: `mcp.<server>.<tool>`, with anything that could
/// look like another namespace (dots, slashes, spaces) replaced, so a server can never name a
/// tool that collides with a KIVO tool (TOOL-41).
pub fn tool_id(server: &str, tool: &str) -> String {
    let clean: String = tool
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    // Brains see ids with `.` as `__`, so a name can't hold `__` of its own.
    let mut clean = clean;
    while clean.contains("__") {
        clean = clean.replace("__", "_");
    }
    format!("mcp.{server}.{clean}")
}

/// Whether an environment variable's name says it holds a secret (an API key, a token, a
/// password): imports move such values into Credential Manager.
pub fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASS",
        "CREDENTIAL",
        "AUTH",
        "PAT",
    ]
    .iter()
    .any(|w| upper.split(['_', '-']).any(|part| part == *w) || upper.ends_with(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_safe_and_namespaced() {
        assert_eq!(server_id("Brave Search!"), "brave-search");
        assert_eq!(server_id("  "), "server");
        assert_eq!(tool_id("github", "create_issue"), "mcp.github.create_issue");
        // A server can't reach into another namespace.
        assert_eq!(tool_id("x", "shell.run"), "mcp.x.shell_run");
        assert_eq!(tool_id("x", "../../etc"), "mcp.x._etc");
        assert_eq!(
            tool_id("x", "a__b"),
            "mcp.x.a_b",
            "no `__`: it stands for `.` on the wire"
        );
    }

    #[test]
    fn secret_names_are_recognized() {
        for secret in [
            "GITHUB_TOKEN",
            "BRAVE_API_KEY",
            "DB_PASSWORD",
            "apiKey",
            "NOTION_SECRET",
        ] {
            assert!(is_secret_name(secret), "{secret}");
        }
        for plain in ["PATH", "NODE_ENV", "ALLOWED_DIR", "LOG_LEVEL"] {
            assert!(!is_secret_name(plain), "{plain}");
        }
    }

    #[test]
    fn servers_round_trip_without_secret_values() {
        let s = McpServer {
            id: "brave".into(),
            name: "brave-search".into(),
            transport: Transport::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "@x/brave".into()],
                env: [(
                    "BRAVE_API_KEY".to_owned(),
                    EnvValue::Secret("mcp.brave.BRAVE_API_KEY".into()),
                )]
                .into(),
                cwd: None,
            },
            enabled: true,
            source: Some("claude-desktop".into()),
            tools: BTreeMap::new(),
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"kind\":\"secret\""), "{json}");
        assert_eq!(serde_json::from_str::<McpServer>(&json).unwrap(), s);
        assert!(!s.remote());
    }
}
