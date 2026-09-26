//! Brain profiles and routing (BRAINS §5, BRAIN-20/21/22). Routing is deterministic and explains
//! itself: the rules run in order — the user's explicit choice, the task class, the data's
//! class, being offline, then health and preference — and each decision carries the one-line
//! reason the card shows ("Coding · Claude Code — because this looked like a coding task").
//! Failover moves on only within the same privacy class; otherwise KIVO asks.

use crate::catalog::{self, CatalogEntry, Tier};
use crate::types::{NormalizedError, PrivacyClass, ProviderKind};
use serde::{Deserialize, Serialize};

/// A provider and one of its models.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfilePrivacy {
    Cloud,
    /// Local when a local brain is available, else cloud.
    LocalPreferred,
    /// Local only; never the cloud.
    StrictPrivate,
}

/// Which tools a profile may use (BRAIN-27 applies within this).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "tools")]
pub enum ToolScope {
    All,
    None,
    Only(Vec<String>),
}

/// BRAINS §5 `Profile`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub tier: Tier,
    /// The brain the user chose for it; `None` lets KIVO pick among connected brains.
    pub primary: Option<ModelRef>,
    pub fallbacks: Vec<ModelRef>,
    pub privacy: ProfilePrivacy,
    /// Tokens per request (CONV-04); `None` uses the model class default.
    pub max_context: Option<u32>,
    pub allowed_tools: ToolScope,
    pub system_prompt_addendum: String,
    /// Overrides the user's persona for this profile (BRAIN-38).
    pub persona: Option<String>,
    /// Voice requests on this profile open a realtime conversation (BRAINS §8), when the
    /// Realtime voice capability is on.
    #[serde(default)]
    pub realtime: bool,
    pub built_in: bool,
}

impl Profile {
    fn built_in(id: &str, name: &str, tier: Tier, privacy: ProfilePrivacy) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tier,
            primary: None,
            fallbacks: Vec::new(),
            privacy,
            max_context: None,
            allowed_tools: ToolScope::All,
            system_prompt_addendum: String::new(),
            persona: None,
            realtime: false,
            built_in: true,
        }
    }
}

/// The built-in profiles (BRAIN-20): Default, Fast, Smart, Coding, Private, Offline, Cheap.
pub fn built_in_profiles() -> Vec<Profile> {
    vec![
        Profile::built_in("default", "Default", Tier::Default, ProfilePrivacy::Cloud),
        Profile::built_in("fast", "Fast", Tier::Fast, ProfilePrivacy::Cloud),
        Profile::built_in("smart", "Smart", Tier::Smart, ProfilePrivacy::Cloud),
        Profile::built_in("coding", "Coding", Tier::Coding, ProfilePrivacy::Cloud),
        Profile::built_in(
            "private",
            "Private",
            Tier::Default,
            ProfilePrivacy::StrictPrivate,
        ),
        Profile::built_in(
            "offline",
            "Offline",
            Tier::Default,
            ProfilePrivacy::StrictPrivate,
        ),
        Profile::built_in("cheap", "Cheap", Tier::Cheap, ProfilePrivacy::Cloud),
    ]
}

/// A connected brain as routing sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Available {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    pub free: bool,
    /// Its last health check passed (or it hasn't failed yet).
    pub healthy: bool,
    /// Its live model list, when known.
    pub models: Vec<String>,
}

/// What the request looks like (rule 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskClass {
    Chat,
    Coding,
}

/// Words that mark a coding task.
const CODING: &[&str] = &[
    "code",
    "coding",
    "bug",
    "compile",
    "compiler",
    "stack trace",
    "stacktrace",
    "exception",
    "function",
    "refactor",
    "unit test",
    "tests",
    "repo",
    "repository",
    "commit",
    "pull request",
    "rust",
    "python",
    "typescript",
    "javascript",
    "cargo",
    "npm",
    "error",
    "script",
    "debug",
    "api",
    "regex",
    "sql",
    "class",
    "method",
];

pub fn classify_task(text: &str) -> TaskClass {
    let lower = format!(" {} ", text.to_lowercase());
    let hits = CODING
        .iter()
        .filter(|w| {
            lower.contains(&format!(" {w} "))
                || lower.contains(&format!(" {w},"))
                || lower.contains(&format!(" {w}."))
                || lower.contains(&format!(" {w}?"))
        })
        .count();
    // One word alone is weak ("the error margin"); a pointed one isn't ("this error").
    let pointed = ["this", "my", "that", "the following"].iter().any(|d| {
        [
            "error",
            "bug",
            "exception",
            "stack trace",
            "function",
            "code",
            "script",
        ]
        .iter()
        .any(|w| lower.contains(&format!(" {d} {w}")))
    });
    if hits >= 2 || lower.contains("```") || (hits >= 1 && pointed) {
        TaskClass::Coding
    } else {
        TaskClass::Chat
    }
}

#[derive(Clone, Debug)]
pub struct RouteRequest<'a> {
    pub text: &'a str,
    /// A profile chosen for this request (Chat's brain switcher).
    pub profile: Option<&'a str>,
    pub default_profile: &'a str,
    /// `Sensitive` or above (SECURITY §6): cloud brains are out.
    pub sensitive: bool,
    pub offline: bool,
    /// The privacy mode and the Cloud AI capability allow cloud brains at all.
    pub cloud_allowed: bool,
    /// The agent the user chose for the current workspace (CONVERSATION §4): coding requests go
    /// to it first while it's healthy.
    pub workspace_agent: Option<&'a str>,
}

/// Where a request goes, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub profile: String,
    pub target: ModelRef,
    pub target_name: String,
    pub kind: ProviderKind,
    pub privacy: PrivacyClass,
    /// Next in line, same privacy class only (BRAIN-22).
    pub fallbacks: Vec<ModelRef>,
    /// The card's line: "Coding · Claude Code — because this looked like a coding task".
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("no brain is connected yet")]
    NoBrain,
    #[error("this needs a brain on this PC, and none is running")]
    NeedsLocal,
    #[error("{0} isn't connected")]
    NotConnected(String),
    #[error("{0} isn't available right now")]
    Unavailable(String),
}

fn allowed(a: &Available, privacy: ProfilePrivacy, req: &RouteRequest<'_>) -> bool {
    let local = a.privacy == PrivacyClass::Local;
    let cloud_ok = req.cloud_allowed && !req.sensitive && !req.offline;
    match privacy {
        ProfilePrivacy::StrictPrivate => local,
        ProfilePrivacy::Cloud | ProfilePrivacy::LocalPreferred => local || cloud_ok,
    }
}

fn model_of(a: &Available, tier: Tier) -> String {
    catalog::entry(&a.id)
        .and_then(|e| e.model_for(tier, &a.models))
        .or_else(|| a.models.first().cloned())
        .unwrap_or_else(|| "default".into())
}

/// Candidates for a profile, best first: its chosen brain, then connected brains in the order
/// that suits the tier (agents for coding, free ones for cheap, local first where preferred).
fn candidates<'a>(
    profile: &Profile,
    available: &'a [Available],
    req: &RouteRequest<'_>,
) -> Vec<(&'a Available, String)> {
    let mut out: Vec<(&Available, String)> = Vec::new();
    let mut push = |a: &'a Available, model: String| {
        if !out.iter().any(|(x, m)| x.id == a.id && *m == model) {
            out.push((a, model));
        }
    };
    for chosen in profile.primary.iter().chain(&profile.fallbacks) {
        if let Some(a) = available.iter().find(|a| a.id == chosen.provider) {
            push(a, chosen.model.clone());
        }
    }
    let rank = |a: &Available| -> (u8, u8, usize) {
        let catalog_order = catalog::CATALOG
            .iter()
            .position(|e| e.id == a.id)
            .unwrap_or(usize::MAX);
        let local_first = u8::from(
            matches!(profile.privacy, ProfilePrivacy::LocalPreferred)
                && a.privacy != PrivacyClass::Local,
        );
        let kind = |order: [ProviderKind; 3]| {
            u8::try_from(order.iter().position(|k| *k == a.kind).unwrap_or(3)).unwrap_or(3)
        };
        let preference = match profile.tier {
            // Coding goes to an agent when one is connected.
            Tier::Coding => kind([ProviderKind::Cli, ProviderKind::Api, ProviderKind::Local]),
            Tier::Cheap => {
                u8::from(!a.free) * 3
                    + kind([ProviderKind::Local, ProviderKind::Api, ProviderKind::Cli])
            }
            // Conversation: API brains answer best, local ones next; an agent session last.
            Tier::Default | Tier::Smart | Tier::Fast => {
                kind([ProviderKind::Api, ProviderKind::Local, ProviderKind::Cli])
            }
        };
        (local_first, preference, catalog_order)
    };
    let mut rest: Vec<&Available> = available.iter().collect();
    rest.sort_by_key(|a| rank(a));
    for a in rest {
        push(a, model_of(a, profile.tier));
    }
    out.retain(|(a, _)| allowed(a, profile.privacy, req));
    out
}

fn reason_for(profile: &Profile, target: &Available, why: &str) -> String {
    format!("{} · {} — {why}", profile.name, target.name)
}

/// Picks the brain for a request, in the rule order of BRAINS §5.
pub fn route(
    req: &RouteRequest<'_>,
    profiles: &[Profile],
    available: &[Available],
) -> Result<Route, RouteError> {
    if available.is_empty() {
        return Err(RouteError::NoBrain);
    }
    let profile_by_id = |id: &str| profiles.iter().find(|p| p.id == id).cloned();
    let default = profile_by_id(req.default_profile)
        .or_else(|| profile_by_id("default"))
        .unwrap_or_else(|| built_in_profiles().remove(0));

    // 1. The user's explicit choice: a brain named in the request, or a profile picked for it.
    if let Some((entry, said)) = catalog::named_in(req.text) {
        return explicit(entry, &said, &default, available, req);
    }
    let (profile, why) = if let Some(p) = req.profile.and_then(profile_by_id) {
        (p, "you chose it".to_owned())
    // 3. Sensitive data stays on this PC (checked before the task class: privacy wins).
    } else if req.sensitive {
        (
            profile_by_id("private").unwrap_or(default.clone()),
            "because it contains sensitive data".to_owned(),
        )
    // 4. Offline.
    } else if req.offline {
        (
            profile_by_id("offline").unwrap_or(default.clone()),
            "because this PC is offline".to_owned(),
        )
    // 2. The task class.
    } else if classify_task(req.text) == TaskClass::Coding {
        (
            profile_by_id("coding").unwrap_or(default.clone()),
            "because this looked like a coding task".to_owned(),
        )
    } else {
        (default.clone(), "your default brain".to_owned())
    };

    // 5–6. Health, then the profile's preferences; for coding, the workspace's own agent first.
    let mut options = candidates(&profile, available, req);
    let mut why = why;
    if profile.id == "coding"
        && let Some(agent) = req.workspace_agent
        && let Some(at) = options.iter().position(|(a, _)| a.id == agent && a.healthy)
    {
        let chosen = options.remove(at);
        options.insert(0, chosen);
        why = "the agent you chose for this workspace".to_owned();
    }
    let healthy: Vec<&(&Available, String)> = options.iter().filter(|(a, _)| a.healthy).collect();
    let Some((target, model)) = healthy.first().copied() else {
        return Err(
            if matches!(profile.privacy, ProfilePrivacy::StrictPrivate)
                || req.sensitive
                || req.offline
                || (options.is_empty() && !req.cloud_allowed)
            {
                RouteError::NeedsLocal
            } else {
                RouteError::Unavailable(profile.name.clone())
            },
        );
    };
    let skipped = options.iter().take_while(|(a, _)| !a.healthy).count();
    let why = if skipped > 0 && why == "your default brain" {
        format!("{} isn't available right now", options[0].0.name)
    } else {
        why
    };
    Ok(Route {
        profile: profile.id.clone(),
        target: ModelRef {
            provider: target.id.clone(),
            model: model.clone(),
        },
        target_name: target.name.clone(),
        kind: target.kind,
        privacy: target.privacy,
        fallbacks: healthy
            .iter()
            .skip(1)
            .filter(|(a, _)| a.privacy == target.privacy)
            .map(|(a, m)| ModelRef {
                provider: a.id.clone(),
                model: m.clone(),
            })
            .collect(),
        reason: reason_for(&profile, target, &why),
    })
}

fn explicit(
    entry: &CatalogEntry,
    said: &str,
    default: &Profile,
    available: &[Available],
    req: &RouteRequest<'_>,
) -> Result<Route, RouteError> {
    // "Claude" may be connected as the API or through Claude Code: take whichever is here.
    let family: Vec<&Available> = available
        .iter()
        .filter(|a| {
            a.id == entry.id
                || catalog::entry(&a.id).is_some_and(|e| {
                    // The same family: "claude" covers the API and Claude Code.
                    e.names.iter().any(|n| {
                        entry
                            .names
                            .iter()
                            .any(|m| n == m || n.starts_with(&format!("{m} ")))
                    }) || (entry.privacy == PrivacyClass::Local && e.privacy == PrivacyClass::Local)
                })
        })
        .collect();
    let target = family
        .iter()
        .find(|a| a.id == entry.id)
        .or_else(|| family.first())
        .copied()
        .ok_or_else(|| RouteError::NotConnected(entry.name.to_owned()))?;
    if target.privacy == PrivacyClass::Cloud && (req.sensitive || !req.cloud_allowed) {
        return Err(RouteError::NeedsLocal);
    }
    if !target.healthy {
        return Err(RouteError::Unavailable(target.name.clone()));
    }
    let tier = if classify_task(req.text) == TaskClass::Coding {
        Tier::Coding
    } else {
        default.tier
    };
    Ok(Route {
        profile: default.id.clone(),
        target: ModelRef {
            provider: target.id.clone(),
            model: model_of(target, tier),
        },
        target_name: target.name.clone(),
        kind: target.kind,
        privacy: target.privacy,
        fallbacks: Vec::new(),
        reason: format!("{} — because you asked for {said}", target.name),
    })
}

/// BRAIN-22: after `error`, the next brain to try — only for failures another provider may not
/// have, and only in the same privacy class. `None` means ask the user.
pub fn failover<'a>(route: &'a Route, error: &NormalizedError) -> Option<&'a ModelRef> {
    if !error.fails_over() {
        return None;
    }
    route.fallbacks.first()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain(id: &str, healthy: bool) -> Available {
        let e = catalog::entry(id).unwrap();
        Available {
            id: id.into(),
            name: e.name.into(),
            kind: e.kind,
            privacy: e.privacy,
            free: e.free.is_some(),
            healthy,
            models: Vec::new(),
        }
    }

    fn request(text: &str) -> RouteRequest<'_> {
        RouteRequest {
            text,
            profile: None,
            default_profile: "default",
            sensitive: false,
            offline: false,
            cloud_allowed: true,
            workspace_agent: None,
        }
    }

    fn all() -> Vec<Available> {
        vec![
            brain("anthropic", true),
            brain("claude-code", true),
            brain("ollama", true),
        ]
    }

    #[test]
    fn a_plain_question_goes_to_the_default_brain_with_its_reason() {
        let r = route(
            &request("what's the capital of Peru?"),
            &built_in_profiles(),
            &all(),
        )
        .unwrap();
        assert_eq!(r.target.provider, "anthropic");
        assert_eq!(r.target.model, "claude-sonnet-5");
        assert_eq!(r.reason, "Default · Anthropic — your default brain");
    }

    #[test]
    fn coding_goes_to_the_coding_agent() {
        let r = route(
            &request("explain this error in my Rust function"),
            &built_in_profiles(),
            &all(),
        )
        .unwrap();
        assert_eq!(r.target.provider, "claude-code");
        assert_eq!(
            r.reason,
            "Coding · Claude Code — because this looked like a coding task"
        );
    }

    #[test]
    fn a_workspaces_own_agent_takes_its_coding_requests_while_healthy() {
        let text = "explain this error in my Rust function";
        let mut available = all();
        available.push(brain("codex", true));
        let mut req = request(text);
        req.workspace_agent = Some("codex");
        let r = route(&req, &built_in_profiles(), &available).unwrap();
        assert_eq!(r.target.provider, "codex");
        assert!(
            r.reason.ends_with("the agent you chose for this workspace"),
            "{}",
            r.reason
        );
        // Other requests ignore it, and an unhealthy agent falls back to the usual order.
        req.text = "what's the capital of Peru?";
        assert_eq!(
            route(&req, &built_in_profiles(), &available)
                .unwrap()
                .target
                .provider,
            "anthropic"
        );
        req.text = text;
        available.last_mut().unwrap().healthy = false;
        assert_eq!(
            route(&req, &built_in_profiles(), &available)
                .unwrap()
                .target
                .provider,
            "claude-code"
        );
    }

    #[test]
    fn sensitive_and_offline_requests_stay_on_this_pc() {
        let mut req = request("my card is 4111 1111 1111 1111, is it valid?");
        req.sensitive = true;
        let r = route(&req, &built_in_profiles(), &all()).unwrap();
        assert_eq!(
            (r.target.provider.as_str(), r.privacy),
            ("ollama", PrivacyClass::Local)
        );
        assert!(r.reason.contains("sensitive data"));
        let cloud_only = vec![brain("anthropic", true)];
        assert_eq!(
            route(&req, &built_in_profiles(), &cloud_only),
            Err(RouteError::NeedsLocal)
        );

        let mut offline = request("hello");
        offline.offline = true;
        assert_eq!(
            route(&offline, &built_in_profiles(), &all())
                .unwrap()
                .target
                .provider,
            "ollama"
        );
    }

    #[test]
    fn the_privacy_mode_rules_out_the_cloud() {
        let mut req = request("hello");
        req.cloud_allowed = false;
        let r = route(&req, &built_in_profiles(), &all()).unwrap();
        assert_eq!(r.target.provider, "ollama");
        assert_eq!(
            route(&req, &built_in_profiles(), &[brain("anthropic", true)]),
            Err(RouteError::NeedsLocal)
        );
    }

    #[test]
    fn an_explicit_choice_wins_and_says_so() {
        let r = route(
            &request("use Claude for this: plan my week"),
            &built_in_profiles(),
            &all(),
        )
        .unwrap();
        assert_eq!(r.target.provider, "anthropic");
        assert_eq!(r.reason, "Anthropic — because you asked for Claude");
        let r = route(
            &request("ask the local model"),
            &built_in_profiles(),
            &all(),
        )
        .unwrap();
        assert_eq!(r.target.provider, "ollama");
        assert_eq!(
            route(
                &request("use gemini for this"),
                &built_in_profiles(),
                &all()
            ),
            Err(RouteError::NotConnected("Google Gemini".into()))
        );
        // Only Claude Code is connected: "use Claude" goes there.
        let r = route(
            &request("use Claude for this"),
            &built_in_profiles(),
            &[brain("claude-code", true)],
        )
        .unwrap();
        assert_eq!(r.target.provider, "claude-code");
    }

    #[test]
    fn an_unhealthy_brain_is_skipped_and_failover_keeps_the_privacy_class() {
        let brains = vec![
            brain("anthropic", false),
            brain("openai", true),
            brain("gemini", true),
            brain("ollama", true),
        ];
        let r = route(&request("hello"), &built_in_profiles(), &brains).unwrap();
        assert_eq!(r.target.provider, "openai");
        assert_eq!(
            r.reason,
            "Default · OpenAI — Anthropic isn't available right now"
        );
        assert_eq!(
            r.fallbacks
                .iter()
                .map(|f| f.provider.as_str())
                .collect::<Vec<_>>(),
            ["gemini"],
            "the local brain is a different privacy class"
        );
        assert_eq!(
            failover(&r, &NormalizedError::RateLimited { retry_after: None })
                .map(|f| f.provider.as_str()),
            Some("gemini")
        );
        assert_eq!(
            failover(&r, &NormalizedError::Auth),
            None,
            "a refused key isn't another provider's problem"
        );
    }

    #[test]
    fn a_profile_with_a_chosen_brain_uses_it_and_cheap_prefers_free() {
        let mut profiles = built_in_profiles();
        profiles[0].primary = Some(ModelRef {
            provider: "ollama".into(),
            model: "llama3.2:3b".into(),
        });
        let r = route(&request("hello"), &profiles, &all()).unwrap();
        assert_eq!(
            (r.target.provider.as_str(), r.target.model.as_str()),
            ("ollama", "llama3.2:3b")
        );
        let brains = vec![brain("anthropic", true), brain("openrouter", true)];
        let mut req = request("hi");
        req.profile = Some("cheap");
        let r = route(&req, &built_in_profiles(), &brains).unwrap();
        assert_eq!(r.target.provider, "openrouter");
        assert_eq!(r.target.model, "meta-llama/llama-3.3-70b-instruct:free");
    }

    #[test]
    fn no_brain_at_all_is_its_own_answer() {
        assert_eq!(
            route(&request("hi"), &built_in_profiles(), &[]),
            Err(RouteError::NoBrain)
        );
    }

    #[test]
    fn coding_tasks_are_recognized() {
        assert_eq!(
            classify_task("why does cargo fail to compile this function"),
            TaskClass::Coding
        );
        assert_eq!(classify_task("explain this error"), TaskClass::Coding);
        assert_eq!(classify_task("tell me a joke"), TaskClass::Chat);
        assert_eq!(
            classify_task("what's the error margin in polls"),
            TaskClass::Chat
        );
    }
}
