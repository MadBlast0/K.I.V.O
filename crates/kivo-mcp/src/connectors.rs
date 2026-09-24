//! The connector directory (INTEGRATIONS §0–1, INT-02/03): each connector is a remote MCP server
//! that signs in in the browser, a way KIVO already has on this PC (a signed-in CLI, an app, the
//! browser extension), or built in. Every connector's tools are namespaced ToolSpecs covered by the
//! capability toggles and the permission engine (a remote connector's are `mcp.<id>.*`).

use serde::Deserialize;

/// How a connector works.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A vendor's remote MCP server, with sign-in.
    Remote,
    /// Found on this PC; no account.
    Local,
    /// Always there.
    BuiltIn,
    /// Signs in with KIVO's own OAuth app (INT-05): Google, Microsoft.
    Native,
}

/// One connector in the directory.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connector {
    pub id: String,
    pub name: String,
    pub description: String,
    /// What it can reach, shown before connecting (the scope list, INT-02).
    pub access: String,
    pub kind: Kind,
    /// The remote MCP endpoint.
    #[serde(default)]
    pub url: Option<String>,
    /// How a local one is found: `gh` (the GitHub CLI signed in), `app` (the app installed),
    /// `browser` (KIVO's extension installed).
    #[serde(default)]
    pub detect: Option<String>,
    /// The app registry entry a local one works through.
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub badges: Vec<String>,
    /// A native connector's OAuth provider (`google`, `microsoft`), the one scope it asks for,
    /// and the prefix of its tools.
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub tools: Option<String>,
}

#[derive(Deserialize)]
struct Catalog {
    connector: Vec<Connector>,
}

/// The directory that ships with KIVO.
pub fn catalog() -> Vec<Connector> {
    toml::from_str::<Catalog>(include_str!("../connectors.toml"))
        .map(|c| c.connector)
        .unwrap_or_default()
}

/// The account `gh auth status` says is signed in to github.com, if any. Both the old
/// ("Logged in to github.com as X") and the new ("Logged in to github.com account X") wording.
pub fn gh_account(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.split("Logged in to github.com").nth(1)?;
        let rest = rest
            .trim_start()
            .strip_prefix("account")
            .or_else(|| rest.trim_start().strip_prefix("as"))?;
        let name: String = rest
            .trim()
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_directory_loads_with_endpoints_for_remote_connectors() {
        let all = catalog();
        assert!(all.len() >= 8);
        for c in &all {
            match c.kind {
                Kind::Remote => {
                    let url = c.url.as_deref().expect(&c.id);
                    assert!(url.starts_with("https://"), "{url}");
                }
                Kind::Local => assert!(c.detect.is_some(), "{}", c.id),
                Kind::Native => assert!(
                    c.provider.is_some() && c.scope.is_some() && c.tools.is_some(),
                    "{}",
                    c.id
                ),
                Kind::BuiltIn => {}
            }
            assert!(
                !c.access.is_empty(),
                "every connector says what it can reach"
            );
        }
        let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"github") && ids.contains(&"github-cli"));
    }

    #[test]
    fn a_signed_in_gh_is_recognized() {
        let new = "github.com\n  ✓ Logged in to github.com account MadBlast0 (keyring)\n  - Active account: true";
        assert_eq!(gh_account(new).as_deref(), Some("MadBlast0"));
        let old = "github.com\n  ✓ Logged in to github.com as octo-cat (oauth_token)";
        assert_eq!(gh_account(old).as_deref(), Some("octo-cat"));
        assert_eq!(
            gh_account("You are not logged into any GitHub hosts."),
            None
        );
    }
}
