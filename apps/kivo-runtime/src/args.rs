//! Command-line flags (ARCHITECTURE §1).

/// How the runtime was started, which decides whether and how it launches the app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Args {
    /// Started at sign-in: launch the app in the background (no window).
    pub autostart: bool,
    /// Started by the app, which is already running: supervise it, don't launch it.
    pub from_app: bool,
    /// Never launch or supervise the app (development and tests).
    pub no_app: bool,
    /// Don't start: check that a running runtime answers over IPC, and exit 0 if it does
    /// (the installer smoke test, REL-06).
    pub health: bool,
    /// Started by a browser as KIVO's native messaging host: the caller's origin
    /// (`chrome-extension://<id>/`; TOOL-24). Chrome passes it as the first argument; the host
    /// relays and never starts a runtime.
    pub native_messaging: Option<String>,
    /// Started by a CLI agent as KIVO's MCP server (`--mcp-server --agent <id>`, TOOL-37): relay
    /// the agent's MCP to the running KIVO, nothing else.
    pub mcp_server: Option<String>,
}

impl Args {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--mcp-server" => {
                    parsed.mcp_server.get_or_insert_with(String::new);
                }
                "--agent" => {
                    let agent = args.next().ok_or("--agent needs an agent id")?;
                    parsed.mcp_server = Some(agent);
                }
                "--autostart" => parsed.autostart = true,
                "--from-app" => parsed.from_app = true,
                "--no-app" => parsed.no_app = true,
                "--health" => parsed.health = true,
                "--native-messaging" => {
                    parsed.native_messaging.get_or_insert_with(String::new);
                }
                origin if origin.starts_with("chrome-extension://") => {
                    parsed.native_messaging = Some(origin.to_owned());
                }
                // Chrome adds the calling window on Windows; the host doesn't need it.
                parent if parent.starts_with("--parent-window=") => {}
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(ToString::to_string))
    }

    #[test]
    fn flags_are_parsed_and_unknown_ones_refused() {
        assert_eq!(parse(&[]), Ok(Args::default()));
        let all = parse(&["--autostart", "--from-app", "--no-app"]).unwrap();
        assert!(all.autostart && all.from_app && all.no_app && !all.health);
        assert!(parse(&["--health"]).unwrap().health);
        assert_eq!(parse(&["--nope"]), Err("unknown argument: --nope".into()));
        let mcp = parse(&["--mcp-server", "--agent", "claude-code"]).unwrap();
        assert_eq!(mcp.mcp_server.as_deref(), Some("claude-code"));
        assert!(parse(&["--mcp-server", "--agent"]).is_err());
        let host = parse(&["chrome-extension://abc/", "--parent-window=0"]).unwrap();
        assert_eq!(
            host.native_messaging.as_deref(),
            Some("chrome-extension://abc/")
        );
    }
}
