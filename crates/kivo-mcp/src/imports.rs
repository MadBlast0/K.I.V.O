//! MCP setups other apps already have (DISCOVERY §1.2, DISC-09): Claude Desktop (including the
//! Store build), Claude Code (the user's `~/.claude.json` and projects' `.mcp.json`), Cursor, VS
//! Code (the user's `mcp.json` or `settings.json`, and workspaces' `.vscode/mcp.json`), Codex
//! (`~/.codex/config.toml`) and Gemini CLI (`~/.gemini/settings.json`).
//!
//! Files are only ever read. An import copies a server into KIVO's own setup; values that look
//! like secrets (an API key in an env variable, a bearer header) are returned separately so the
//! caller puts them into Credential Manager, and the copied server only names the entry.

use crate::config::{EnvValue, McpServer, Transport, is_secret_name, server_id};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Where to look.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// The user's profile folder (`~`).
    pub home: PathBuf,
    /// `%APPDATA%`.
    pub appdata: PathBuf,
    /// `%LOCALAPPDATA%`.
    pub local_appdata: PathBuf,
    /// Project folders KIVO knows (remembered workspaces), for `.mcp.json` and `.vscode/mcp.json`.
    pub projects: Vec<PathBuf>,
}

impl Places {
    /// This user's folders.
    pub fn user() -> Self {
        let env = |k: &str| std::env::var_os(k).map(PathBuf::from).unwrap_or_default();
        Self {
            home: env("USERPROFILE"),
            appdata: env("APPDATA"),
            local_appdata: env("LOCALAPPDATA"),
            projects: Vec::new(),
        }
    }
}

/// A server found in another app, ready to import.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub server: McpServer,
    /// (Credential Manager entry, value) pairs for the secrets the server needs.
    pub secrets: Vec<(String, String)>,
}

/// One app's (or project's) MCP setup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// `claude-desktop`, `claude-code`, `cursor`, `vscode`, `codex`, `gemini-cli`.
    pub app: String,
    /// "Claude Desktop", "Claude Code · kivo".
    pub name: String,
    pub file: PathBuf,
    pub servers: Vec<Imported>,
}

/// Every MCP setup found on this PC, in a stable order. Unreadable or malformed files are
/// skipped (they are someone else's).
pub fn find_all(places: &Places) -> Vec<Found> {
    let mut out = Vec::new();
    for (app, name, file) in files(places) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let servers = match parse(&app, &text) {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        out.push(Found {
            app,
            name,
            file,
            servers,
        });
    }
    out
}

/// The files to read, as (app, name shown, path). Also the paths to watch (DISC-16).
pub fn files(places: &Places) -> Vec<(String, String, PathBuf)> {
    candidates(places)
        .into_iter()
        .filter(|(_, _, path)| path.is_file())
        .collect()
}

/// Every place another app keeps its MCP setup, whether or not the file exists yet (the
/// watcher looks at their folders, so a setup made later is found too).
pub fn candidates(places: &Places) -> Vec<(String, String, PathBuf)> {
    let mut out = Vec::new();
    let mut push = |app: &str, name: String, path: PathBuf| {
        out.push((app.to_owned(), name, path));
    };
    push(
        "claude-desktop",
        "Claude Desktop".into(),
        places
            .appdata
            .join("Claude")
            .join("claude_desktop_config.json"),
    );
    // The Microsoft Store build keeps its config inside its package folder.
    if let Ok(packages) = std::fs::read_dir(places.local_appdata.join("Packages")) {
        for p in packages.flatten() {
            if p.file_name().to_string_lossy().starts_with("Claude_") {
                push(
                    "claude-desktop",
                    "Claude Desktop".into(),
                    p.path()
                        .join("LocalCache")
                        .join("Roaming")
                        .join("Claude")
                        .join("claude_desktop_config.json"),
                );
            }
        }
    }
    push(
        "claude-code",
        "Claude Code".into(),
        places.home.join(".claude.json"),
    );
    push(
        "cursor",
        "Cursor".into(),
        places.home.join(".cursor").join("mcp.json"),
    );
    push(
        "vscode",
        "VS Code".into(),
        places.appdata.join("Code").join("User").join("mcp.json"),
    );
    push(
        "vscode",
        "VS Code".into(),
        places
            .appdata
            .join("Code")
            .join("User")
            .join("settings.json"),
    );
    push(
        "codex",
        "Codex".into(),
        places.home.join(".codex").join("config.toml"),
    );
    push(
        "gemini-cli",
        "Gemini CLI".into(),
        places.home.join(".gemini").join("settings.json"),
    );
    for project in &places.projects {
        let label = project.file_name().map_or_else(
            || project.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        push(
            "claude-code",
            format!("Claude Code · {label}"),
            project.join(".mcp.json"),
        );
        push(
            "vscode",
            format!("VS Code · {label}"),
            project.join(".vscode").join("mcp.json"),
        );
    }
    out
}

/// The servers in one app's file.
pub fn parse(app: &str, text: &str) -> Option<Vec<Imported>> {
    if app == "codex" {
        let doc: toml::Value = toml::from_str(text).ok()?;
        let table = doc.get("mcp_servers")?.as_table()?;
        let json = serde_json::to_value(table).ok()?;
        return Some(servers(app, json.as_object()?));
    }
    let doc: Value = serde_json::from_str(&strip_jsonc(text)).ok()?;
    let map = match app {
        // VS Code: `servers` in mcp.json, `mcp.servers` in settings.json.
        "vscode" => doc
            .get("servers")
            .or_else(|| doc.get("mcp").and_then(|m| m.get("servers")))
            .or_else(|| doc.get("mcp.servers")),
        _ => doc.get("mcpServers"),
    }
    .and_then(Value::as_object);
    let mut out = map.map(|m| servers(app, m)).unwrap_or_default();
    // Claude Code keeps per-project servers inside ~/.claude.json too.
    if app == "claude-code"
        && let Some(projects) = doc.get("projects").and_then(Value::as_object)
    {
        for project in projects.values() {
            if let Some(m) = project.get("mcpServers").and_then(Value::as_object) {
                for s in servers(app, m) {
                    if !out.iter().any(|o| o.server.id == s.server.id) {
                        out.push(s);
                    }
                }
            }
        }
    }
    Some(out)
}

fn servers(app: &str, map: &Map<String, Value>) -> Vec<Imported> {
    map.iter()
        .filter_map(|(name, entry)| server(app, name, entry.as_object()?))
        .collect()
}

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

fn server(app: &str, name: &str, e: &Map<String, Value>) -> Option<Imported> {
    let id = server_id(name);
    let mut secrets = Vec::new();
    let mut secret = |key: &str, value: String| -> String {
        let entry = format!("mcp.{id}.{key}");
        secrets.push((entry.clone(), value));
        entry
    };
    // `disabled: true` in the other app stays off in KIVO too.
    let enabled = !e.get("disabled").and_then(Value::as_bool).unwrap_or(false);
    let url = text(e.get("url"))
        .or_else(|| text(e.get("httpUrl")))
        .or_else(|| text(e.get("serverUrl")));
    let transport = if let Some(url) = url {
        let header = e
            .get("headers")
            .and_then(Value::as_object)
            .and_then(|h| {
                h.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
            })
            .and_then(|(_, v)| v.as_str())
            .map(|v| v.strip_prefix("Bearer ").unwrap_or(v).to_owned());
        let token = header
            .filter(|t| !t.is_empty() && !t.contains("${"))
            .map(|t| secret("token", t));
        Transport::Http {
            url,
            token,
            oauth: None,
        }
    } else {
        let command = text(e.get("command"))?;
        let args = e
            .get("args")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let mut env = BTreeMap::new();
        if let Some(vars) = e.get("env").and_then(Value::as_object) {
            for (k, v) in vars {
                let value = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => continue,
                    other => other.to_string(),
                };
                // A reference to another variable or an input prompt stays as it is.
                let is_reference = value.contains("${") || value.starts_with('$');
                let stored = if is_secret_name(k) && !value.is_empty() && !is_reference {
                    EnvValue::Secret(secret(k, value))
                } else {
                    EnvValue::Plain(value)
                };
                env.insert(k.clone(), stored);
            }
        }
        Transport::Stdio {
            command,
            args,
            env,
            cwd: text(e.get("cwd")),
        }
    };
    Some(Imported {
        server: McpServer {
            id,
            name: name.to_owned(),
            transport,
            enabled,
            source: Some(app.to_owned()),
            tools: BTreeMap::new(),
        },
        secrets,
    })
}

/// JSON with comments and trailing commas (VS Code's settings) as plain JSON. Strings are kept
/// as they are.
pub fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for n in chars.by_ref() {
                    if last == '*' && n == '/' {
                        break;
                    }
                    last = n;
                }
            }
            _ => out.push(c),
        }
    }
    // Trailing commas before } or ].
    let mut cleaned = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            cleaned.push(c);
            if c == '\\' && i + 1 < bytes.len() {
                cleaned.push(bytes[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            cleaned.push(c);
        } else if c == ',' {
            let next = bytes[i + 1..].iter().find(|n| !n.is_whitespace());
            if !matches!(next, Some('}' | ']')) {
                cleaned.push(c);
            }
        } else {
            cleaned.push(c);
        }
        i += 1;
    }
    cleaned
}

/// A file's fingerprint (its bytes), to prove an import left it untouched (M6-X2).
pub fn fingerprint(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_DESKTOP: &str = r#"{
      "mcpServers": {
        "filesystem": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "D:\\work"] },
        "brave-search": { "command": "npx", "args": ["-y", "@x/brave"], "env": { "BRAVE_API_KEY": "BSA-123", "LOG_LEVEL": "info" } },
        "old": { "command": "old.exe", "disabled": true }
      }
    }"#;

    #[test]
    fn claude_desktop_servers_and_their_secrets() {
        let servers = parse("claude-desktop", CLAUDE_DESKTOP).unwrap();
        assert_eq!(servers.len(), 3);
        let brave = servers
            .iter()
            .find(|s| s.server.id == "brave-search")
            .unwrap();
        let Transport::Stdio { env, .. } = &brave.server.transport else {
            panic!("stdio");
        };
        assert_eq!(
            env["BRAVE_API_KEY"],
            EnvValue::Secret("mcp.brave-search.BRAVE_API_KEY".into())
        );
        assert_eq!(env["LOG_LEVEL"], EnvValue::Plain("info".into()));
        assert_eq!(
            brave.secrets,
            [(
                "mcp.brave-search.BRAVE_API_KEY".to_owned(),
                "BSA-123".to_owned()
            )]
        );
        // The secret's value appears nowhere in the copied server.
        assert!(
            !serde_json::to_string(&brave.server)
                .unwrap()
                .contains("BSA-123")
        );
        assert!(
            !servers
                .iter()
                .find(|s| s.server.id == "old")
                .unwrap()
                .server
                .enabled
        );
        assert_eq!(brave.server.source.as_deref(), Some("claude-desktop"));
    }

    #[test]
    fn remote_servers_with_bearer_headers() {
        let cursor = r#"{ "mcpServers": { "linear": { "url": "https://mcp.linear.app/mcp", "headers": { "Authorization": "Bearer lin_abc" } } } }"#;
        let s = &parse("cursor", cursor).unwrap()[0];
        assert_eq!(
            s.server.transport,
            Transport::Http {
                url: "https://mcp.linear.app/mcp".into(),
                token: Some("mcp.linear.token".into()),
                oauth: None,
            }
        );
        assert_eq!(
            s.secrets,
            [("mcp.linear.token".to_owned(), "lin_abc".to_owned())]
        );
        assert!(s.server.remote());
        let gemini = r#"{ "mcpServers": { "docs": { "httpUrl": "https://example.com/mcp" } } }"#;
        assert!(matches!(
            parse("gemini-cli", gemini).unwrap()[0].server.transport,
            Transport::Http { token: None, .. }
        ));
    }

    #[test]
    fn claude_code_projects_vscode_jsonc_and_codex_toml() {
        let claude = r#"{ "mcpServers": { "a": { "type": "stdio", "command": "a" } },
          "projects": { "D:\\kivo": { "mcpServers": { "b": { "command": "b" }, "a": { "command": "dup" } } } } }"#;
        let ids: Vec<String> = parse("claude-code", claude)
            .unwrap()
            .into_iter()
            .map(|s| s.server.id)
            .collect();
        assert_eq!(ids, ["a", "b"]);
        let vscode = r#"{
          // VS Code's settings allow comments
          "editor.fontSize": 14,
          "mcp": { "servers": { "gh": { "type": "http", "url": "https://api.githubcopilot.com/mcp/", }, }, },
        }"#;
        let gh = &parse("vscode", vscode).unwrap()[0];
        assert_eq!(gh.server.id, "gh");
        let codex = "model = \"o3\"\n[mcp_servers.fs]\ncommand = \"npx\"\nargs = [\"-y\", \"fs\"]\n[mcp_servers.fs.env]\nGITHUB_TOKEN = \"ghp_x\"\n";
        let fs = &parse("codex", codex).unwrap()[0];
        assert_eq!(fs.server.id, "fs");
        assert_eq!(fs.secrets.len(), 1);
        // Malformed files are skipped, not fatal.
        assert!(parse("cursor", "{ nope").is_none());
    }

    #[test]
    fn found_on_disk_and_never_changed() {
        let dir = tempfile::tempdir().unwrap();
        let places = Places {
            home: dir.path().join("home"),
            appdata: dir.path().join("roaming"),
            local_appdata: dir.path().join("local"),
            projects: vec![dir.path().join("kivo")],
        };
        let desktop = places.appdata.join("Claude");
        std::fs::create_dir_all(&desktop).unwrap();
        let file = desktop.join("claude_desktop_config.json");
        std::fs::write(&file, CLAUDE_DESKTOP).unwrap();
        let store = places
            .local_appdata
            .join("Packages")
            .join("Claude_pzs8sxrjxfjjc")
            .join("LocalCache")
            .join("Roaming")
            .join("Claude");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(
            store.join("claude_desktop_config.json"),
            r#"{"mcpServers":{"s":{"command":"s"}}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(places.projects[0].join(".vscode")).unwrap();
        std::fs::write(
            places.projects[0].join(".vscode").join("mcp.json"),
            r#"{"servers":{"p":{"command":"p"}}}"#,
        )
        .unwrap();
        let before = fingerprint(&file);
        let found = find_all(&places);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["Claude Desktop", "Claude Desktop", "VS Code · kivo"]
        );
        assert_eq!(found[0].servers.len(), 3);
        assert_eq!(fingerprint(&file), before, "the original is never changed");
    }
}
