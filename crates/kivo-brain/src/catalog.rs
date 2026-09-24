//! The brains KIVO knows how to connect (BRAINS §4, CONVERSATION §3): how each signs in (no-key
//! routes first, BRAIN-17), whether it has a free option (CONV-08), where it listens, and which of
//! its models serve each routing tier. Model ids age, so a provider's live model list always wins:
//! a tier's preferences are tried in order among the models the provider actually offers.

use crate::types::{PrivacyClass, ProviderKind};
use serde::{Deserialize, Serialize};

/// What a profile asks of a model (BRAINS §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tier {
    Default,
    Fast,
    Smart,
    Cheap,
    Coding,
}

/// How the user connects a brain, most preferred first (BRAIN-17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SignIn {
    /// The CLI's own login flow (subscription or account), launched from KIVO.
    CliLogin,
    /// OAuth PKCE that hands KIVO a user-controlled key (OpenRouter).
    OAuth,
    /// Found on this PC; nothing to sign in to.
    Local,
    /// "Advanced: use your own API key".
    ApiKey,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    pub sign_in: SignIn,
    /// A free option exists (CONV-08), with the note the UI shows.
    pub free: Option<&'static str>,
    /// API base (OpenAI-compatible adapters) or empty.
    pub base_url: &'static str,
    /// For CLI agents: the program and its ACP arguments.
    pub agent: Option<(&'static str, &'static [&'static str])>,
    /// Model preferences per tier, best first.
    pub tiers: &'static [(Tier, &'static [&'static str])],
    /// Words that name it in "use Claude for this".
    pub names: &'static [&'static str],
}

impl CatalogEntry {
    /// The best model for `tier` among `offered` (the provider's live list); without a list,
    /// the catalog's first preference. `None` when the list has none of them and isn't empty
    /// (then the first offered model is used by the caller).
    pub fn model_for(&self, tier: Tier, offered: &[String]) -> Option<String> {
        let prefs = self
            .tiers
            .iter()
            .find(|(t, _)| *t == tier)
            .or_else(|| self.tiers.iter().find(|(t, _)| *t == Tier::Default))
            .map(|(_, m)| *m)
            .unwrap_or_default();
        if offered.is_empty() {
            return prefs.first().map(|m| (*m).to_owned());
        }
        prefs
            .iter()
            .find(|m| offered.iter().any(|o| o == *m))
            .map(|m| (*m).to_owned())
            .or_else(|| offered.first().cloned())
    }
}

const ANTHROPIC: &[(Tier, &[&str])] = &[
    (Tier::Default, &["claude-sonnet-5", "claude-sonnet-4-5"]),
    (
        Tier::Fast,
        &["claude-haiku-4-5-20251001", "claude-haiku-4-5"],
    ),
    (
        Tier::Smart,
        &["claude-opus-5-5", "claude-opus-4-1", "claude-sonnet-5"],
    ),
    (
        Tier::Cheap,
        &["claude-haiku-4-5-20251001", "claude-haiku-4-5"],
    ),
    (Tier::Coding, &["claude-sonnet-5", "claude-opus-5-5"]),
];
const OPENAI: &[(Tier, &[&str])] = &[
    (Tier::Default, &["gpt-5", "gpt-5-mini"]),
    (Tier::Fast, &["gpt-5-mini", "gpt-5-nano"]),
    (Tier::Smart, &["gpt-5", "gpt-5-pro"]),
    (Tier::Cheap, &["gpt-5-nano", "gpt-5-mini"]),
    (Tier::Coding, &["gpt-5-codex", "gpt-5"]),
];
const GEMINI: &[(Tier, &[&str])] = &[
    (Tier::Default, &["gemini-2.5-flash", "gemini-flash-latest"]),
    (Tier::Fast, &["gemini-2.5-flash-lite", "gemini-2.5-flash"]),
    (Tier::Smart, &["gemini-2.5-pro", "gemini-pro-latest"]),
    (Tier::Cheap, &["gemini-2.5-flash-lite", "gemini-2.5-flash"]),
    (Tier::Coding, &["gemini-2.5-pro", "gemini-2.5-flash"]),
];
const OPENROUTER: &[(Tier, &[&str])] = &[
    (Tier::Default, &["openrouter/auto"]),
    (Tier::Fast, &["openrouter/auto"]),
    (
        Tier::Smart,
        &["anthropic/claude-sonnet-5", "openrouter/auto"],
    ),
    (
        Tier::Cheap,
        &[
            "meta-llama/llama-3.3-70b-instruct:free",
            "deepseek/deepseek-chat-v3.1:free",
        ],
    ),
    (
        Tier::Coding,
        &["anthropic/claude-sonnet-5", "openrouter/auto"],
    ),
];
const AGENT: &[(Tier, &[&str])] = &[(Tier::Default, &["default"])];
/// Local servers: whatever they have loaded; no preference.
const LOCAL: &[(Tier, &[&str])] = &[];

pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "claude-code",
        name: "Claude Code",
        kind: ProviderKind::Cli,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::CliLogin,
        free: None,
        base_url: "",
        agent: Some(("claude-agent-acp", &[])),
        tiers: AGENT,
        names: &["claude code"],
    },
    CatalogEntry {
        id: "gemini-cli",
        name: "Gemini CLI",
        kind: ProviderKind::Cli,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::CliLogin,
        free: Some("Free with a Google account: about 1,000 requests a day"),
        base_url: "",
        agent: Some(("gemini", &["--acp"])),
        tiers: AGENT,
        names: &["gemini cli"],
    },
    CatalogEntry {
        id: "codex",
        name: "Codex",
        kind: ProviderKind::Cli,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::CliLogin,
        free: Some("Included with ChatGPT, including a small Free allowance"),
        base_url: "",
        agent: Some(("codex-acp", &[])),
        tiers: AGENT,
        names: &["codex"],
    },
    CatalogEntry {
        id: "copilot-cli",
        name: "GitHub Copilot CLI",
        kind: ProviderKind::Cli,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::CliLogin,
        free: Some("Included with GitHub Copilot, including Copilot Free"),
        base_url: "",
        agent: Some(("copilot", &["--acp"])),
        tiers: AGENT,
        names: &["copilot cli", "github copilot"],
    },
    CatalogEntry {
        id: "opencode",
        name: "OpenCode",
        kind: ProviderKind::Cli,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::CliLogin,
        free: None,
        base_url: "",
        agent: Some(("opencode", &["acp"])),
        tiers: AGENT,
        names: &["opencode"],
    },
    CatalogEntry {
        id: "openrouter",
        name: "OpenRouter",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::OAuth,
        free: Some("Free models, rate limited"),
        base_url: "https://openrouter.ai/api/v1",
        agent: None,
        tiers: OPENROUTER,
        names: &["openrouter"],
    },
    CatalogEntry {
        id: "ollama",
        name: "Ollama",
        kind: ProviderKind::Local,
        privacy: PrivacyClass::Local,
        sign_in: SignIn::Local,
        free: Some("Free and private: runs on this PC"),
        base_url: "http://127.0.0.1:11434/v1",
        agent: None,
        tiers: LOCAL,
        names: &["ollama", "local model", "local"],
    },
    CatalogEntry {
        id: "lmstudio",
        name: "LM Studio",
        kind: ProviderKind::Local,
        privacy: PrivacyClass::Local,
        sign_in: SignIn::Local,
        free: Some("Free and private: runs on this PC"),
        base_url: "http://127.0.0.1:1234/v1",
        agent: None,
        tiers: LOCAL,
        names: &["lm studio", "lmstudio"],
    },
    CatalogEntry {
        id: "llamacpp",
        name: "llama.cpp server",
        kind: ProviderKind::Local,
        privacy: PrivacyClass::Local,
        sign_in: SignIn::Local,
        free: Some("Free and private: runs on this PC"),
        base_url: "http://127.0.0.1:8080/v1",
        agent: None,
        tiers: LOCAL,
        names: &["llama.cpp", "llama cpp"],
    },
    CatalogEntry {
        id: "anthropic",
        name: "Anthropic",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: crate::anthropic::BASE_URL,
        agent: None,
        tiers: ANTHROPIC,
        names: &["claude", "anthropic"],
    },
    CatalogEntry {
        id: "openai",
        name: "OpenAI",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: "https://api.openai.com/v1",
        agent: None,
        tiers: OPENAI,
        names: &["gpt", "chatgpt", "openai"],
    },
    CatalogEntry {
        id: "gemini",
        name: "Google Gemini",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: crate::gemini::BASE_URL,
        agent: None,
        tiers: GEMINI,
        names: &["gemini", "google"],
    },
    CatalogEntry {
        id: "groq",
        name: "Groq",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: Some("A free tier with rate limits"),
        base_url: "https://api.groq.com/openai/v1",
        agent: None,
        tiers: &[
            (Tier::Default, &["llama-3.3-70b-versatile"]),
            (Tier::Fast, &["llama-3.1-8b-instant"]),
        ],
        names: &["groq"],
    },
    CatalogEntry {
        id: "mistral",
        name: "Mistral",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: "https://api.mistral.ai/v1",
        agent: None,
        tiers: &[
            (Tier::Default, &["mistral-medium-latest"]),
            (Tier::Fast, &["mistral-small-latest"]),
        ],
        names: &["mistral"],
    },
    CatalogEntry {
        id: "deepseek",
        name: "DeepSeek",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: "https://api.deepseek.com/v1",
        agent: None,
        tiers: &[
            (Tier::Default, &["deepseek-chat"]),
            (Tier::Smart, &["deepseek-reasoner"]),
        ],
        names: &["deepseek"],
    },
    CatalogEntry {
        id: "xai",
        name: "xAI",
        kind: ProviderKind::Api,
        privacy: PrivacyClass::Cloud,
        sign_in: SignIn::ApiKey,
        free: None,
        base_url: "https://api.x.ai/v1",
        agent: None,
        tiers: &[(Tier::Default, &["grok-4"]), (Tier::Fast, &["grok-4-fast"])],
        names: &["grok", "xai"],
    },
];

/// How a CLI agent is found, signed in and installed (DISCOVERY §1.1). Vendors change package
/// names, so this is data, versioned with KIVO, never code paths.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliTool {
    /// The catalog entry it belongs to.
    pub id: &'static str,
    /// The CLI's own command names.
    pub commands: &'static [&'static str],
    /// ACP adapter programs, when the CLI doesn't speak ACP itself (empty: it does, with the
    /// entry's `agent` arguments).
    pub adapters: &'static [&'static str],
    /// Files (under the home folder) that exist once the CLI is signed in. Only their existence
    /// is checked; they are never read (DISC-03).
    pub signed_in_files: &'static [&'static str],
    /// The install command shown to the user before it runs (with consent).
    pub install: &'static str,
    /// The adapter's install command, when one is needed.
    pub adapter_install: &'static str,
    /// The CLI's own sign-in, launched in a terminal from KIVO (BRAIN-17).
    pub login: &'static [&'static str],
}

pub const CLI_TOOLS: &[CliTool] = &[
    CliTool {
        id: "claude-code",
        commands: &["claude"],
        adapters: &["claude-agent-acp", "claude-code-acp"],
        signed_in_files: &[".claude/.credentials.json"],
        install: "npm install -g @anthropic-ai/claude-code",
        adapter_install: "npm install -g @zed-industries/claude-agent-acp",
        login: &["claude", "/login"],
    },
    CliTool {
        id: "gemini-cli",
        commands: &["gemini"],
        adapters: &[],
        signed_in_files: &[".gemini/oauth_creds.json", ".gemini/google_accounts.json"],
        install: "npm install -g @google/gemini-cli",
        adapter_install: "",
        login: &["gemini"],
    },
    CliTool {
        id: "codex",
        commands: &["codex"],
        adapters: &["codex-acp"],
        signed_in_files: &[".codex/auth.json"],
        install: "npm install -g @openai/codex",
        adapter_install: "npm install -g @zed-industries/codex-acp",
        login: &["codex", "login"],
    },
    CliTool {
        id: "copilot-cli",
        commands: &["copilot"],
        adapters: &[],
        // It keeps its sign-in in Windows' credential store: the ACP handshake says whether it
        // needs one.
        signed_in_files: &[],
        install: "npm install -g @github/copilot",
        adapter_install: "",
        login: &["copilot", "/login"],
    },
    CliTool {
        id: "opencode",
        commands: &["opencode"],
        adapters: &[],
        signed_in_files: &[".local/share/opencode/auth.json"],
        install: "npm install -g opencode-ai",
        adapter_install: "",
        login: &["opencode", "auth", "login"],
    },
];

pub fn cli_tool(id: &str) -> Option<&'static CliTool> {
    CLI_TOOLS.iter().find(|t| t.id == id)
}

pub fn entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

/// Words before a brain's name that make it the user's choice ("use Claude", "ask Gemini CLI",
/// "tell Claude Code to …", "try the local model").
const ASKS: &[&str] = &[
    "use",
    "using",
    "ask",
    "with",
    "via",
    "through",
    "tell",
    "try",
    "let",
    "have",
    "switch to",
    "hand this to",
    "give this to",
    "send this to",
    "in",
];

/// The brain the user asks for in `text` ("use Claude for this", "ask Gemini CLI", "Claude, …"),
/// with the words they used for it, as they wrote them. A name only counts when the user
/// addresses or asks for that brain: "open Google Chrome" is not a request for Gemini. Longer
/// names win.
pub fn named_in(text: &str) -> Option<(&'static CatalogEntry, String)> {
    let lower = text.to_lowercase();
    let lower = lower
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_start_matches("hey kivo")
        .trim_start_matches("kivo")
        .trim_start_matches(|c: char| !c.is_alphanumeric());
    let offset = text.to_lowercase().len() - lower.len();
    let padded = format!(" {lower} ");
    // Lower-casing can change byte lengths outside ASCII: fall back to the lower-case words.
    let original = format!(" {} ", text.get(offset..).unwrap_or(lower));
    CATALOG
        .iter()
        .flat_map(|e| e.names.iter().map(move |n| (e, *n)))
        .filter_map(|(e, n)| {
            let mut from = 0;
            while let Some(found) = padded[from..].find(&format!(" {n}")) {
                let at = from + found;
                let end = at + 1 + n.len();
                let after = padded[end..].chars().next().unwrap_or(' ');
                from = at + 1;
                if !matches!(after, ' ' | ',' | '.' | '?' | '!' | ':') {
                    continue;
                }
                // Addressed at the start ("Claude, explain …").
                let addressed = at == 0 && matches!(after, ',' | ':');
                // Asked for: a trigger right before, allowing an article ("try the local model").
                let before = padded[..at].trim_end();
                let before = before
                    .strip_suffix(" the")
                    .or_else(|| before.strip_suffix(" my"))
                    .unwrap_or(before);
                let asked = ASKS
                    .iter()
                    .any(|a| before == *a || before.ends_with(&format!(" {a}")));
                if addressed || asked {
                    return Some((e, n, at));
                }
            }
            None
        })
        .max_by_key(|(_, n, _)| n.len())
        .map(|(e, n, at)| {
            let said = original
                .get(at + 1..at + 1 + n.len())
                .filter(|s| s.eq_ignore_ascii_case(n))
                .unwrap_or(n);
            (e, said.to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_picks_the_best_model_the_provider_offers() {
        let a = entry("anthropic").unwrap();
        assert_eq!(
            a.model_for(Tier::Fast, &[]).as_deref(),
            Some("claude-haiku-4-5-20251001")
        );
        let offered = vec![
            "claude-sonnet-4-5".to_owned(),
            "claude-haiku-4-5".to_owned(),
        ];
        assert_eq!(
            a.model_for(Tier::Default, &offered).as_deref(),
            Some("claude-sonnet-4-5")
        );
        let other = vec!["some-new-model".to_owned()];
        assert_eq!(
            a.model_for(Tier::Smart, &other).as_deref(),
            Some("some-new-model")
        );
    }

    #[test]
    fn brains_are_recognized_by_name_in_a_request() {
        assert_eq!(
            named_in("use Claude for this").map(|(e, _)| e.id),
            Some("anthropic")
        );
        assert_eq!(
            named_in("ask Claude Code to fix it").map(|(e, _)| e.id),
            Some("claude-code")
        );
        assert_eq!(
            named_in("try the local model").map(|(e, _)| e.id),
            Some("ollama")
        );
        assert_eq!(named_in("what's the weather").map(|(e, _)| e.id), None);
        assert_eq!(
            named_in("ask gemini cli, please").map(|(e, _)| e.id),
            Some("gemini-cli")
        );
        assert_eq!(
            named_in("Claude, explain this").map(|(e, s)| (e.id, s)),
            Some(("anthropic", "Claude".to_owned()))
        );
        assert_eq!(
            named_in("tell Claude Code to also add a test").map(|(e, _)| e.id),
            Some("claude-code")
        );
        // A mention isn't a request for that brain.
        assert_eq!(named_in("open Google Chrome").map(|(e, _)| e.id), None);
        assert_eq!(named_in("what does GPT stand for").map(|(e, _)| e.id), None);
        assert_eq!(
            named_in("is there local weather news").map(|(e, _)| e.id),
            None
        );
    }

    #[test]
    fn free_options_are_labelled_and_keys_come_last() {
        let free: Vec<_> = CATALOG
            .iter()
            .filter(|e| e.free.is_some())
            .map(|e| e.id)
            .collect();
        for id in ["gemini-cli", "codex", "openrouter", "ollama"] {
            assert!(free.contains(&id), "{id}");
        }
        let first_key = CATALOG
            .iter()
            .position(|e| e.sign_in == SignIn::ApiKey)
            .unwrap();
        assert!(
            CATALOG[..first_key]
                .iter()
                .all(|e| e.sign_in != SignIn::ApiKey),
            "no-key options come first"
        );
        assert!(
            CATALOG[first_key..]
                .iter()
                .all(|e| e.sign_in == SignIn::ApiKey)
        );
    }
}
