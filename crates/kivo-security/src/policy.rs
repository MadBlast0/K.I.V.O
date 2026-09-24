//! The permission engine (SECURITY §1.1–2, SEC-01/05/06/07): `authorize(call, context)` decides
//! Allow, Confirm or Deny for every tool call. An Allow carries a `Permit`, the only thing the tool
//! executor accepts, and permits can only be made here (TOOL-02): AI output can never bypass this.
//!
//! Order: hard limits (every mode, even Bypass: the stop, capabilities, blocked apps and the
//! per-capability app scopes, destination binding, private data in Strict Private) → the risk of
//! this call (the tool's assessment of its arguments, raised for sensitive data leaving the
//! device) → Bypass → guest → irreversible → the mode table, tightened for tainted or
//! AI-initiated turns.

use crate::classify::{DataClass, classify};
use kivo_core::capability::{Capability, CapabilitySettings};
use kivo_core::config::{AppScope, PermissionMode, PrivacyMode, Tools};
use kivo_core::text;
use kivo_core::tool::{
    ConfirmSpec, ConfirmedBy, Initiator, Provenance, Reversibility, Risk, SideEffect, Strength,
    Target, ToolCall, ToolSpec,
};
use serde::{Deserialize, Serialize};

/// Owner or guest (VOICE §5: unknown voices get a guest session).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionKind {
    Owner,
    Guest,
}

/// Whether untrusted content has entered the turn (SECURITY §4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Taint {
    Clean,
    /// Where the untrusted content came from ("example.com", "clipboard").
    Tainted(Vec<String>),
}

/// "Always allow" for a tool, optionally only for one target and for arguments matching a
/// pattern, for a while (once, this session, 24 h or always; SEC-08). Expired grants are never
/// handed to the engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub tool: String,
    /// A target id the grant is limited to (an app id, a folder); `None` for any.
    pub scope: Option<String>,
    /// A `*` pattern one of the call's arguments must match (`C:\Users\me\Documents\*`,
    /// `git *`); `None` for any arguments.
    #[serde(default)]
    pub pattern: Option<String>,
    /// The exact arguments this grant is for (a routine's or a task's step, ROUT-03, SEC-09).
    /// A string holding `${name}` stands for whatever a variable is filled with at run time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

impl Grant {
    /// "Always allow" for a tool, for any target and arguments.
    pub fn tool(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            scope: None,
            pattern: None,
            args: None,
        }
    }

    /// A grant for exactly this call's arguments (task and routine grants).
    pub fn exact(tool: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            args: Some(args),
            ..Self::tool(tool)
        }
    }

    /// Whether this grant covers `call`.
    pub fn covers(&self, call: &ToolCall) -> bool {
        self.tool == call.tool
            && self
                .scope
                .as_ref()
                .is_none_or(|scope| call.targets.iter().any(|t| target_id(t) == Some(scope)))
            && self
                .pattern
                .as_ref()
                .is_none_or(|p| strings(&call.args).iter().any(|v| glob(p, v)))
            && self
                .args
                .as_ref()
                .is_none_or(|template| args_match(template, &call.args))
    }
}

/// Whether `actual` is `template`, where a `${name}` inside a template string stands for any text
/// (and a template string that is only `${name}` also for a number or a boolean).
pub fn args_match(template: &serde_json::Value, actual: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (template, actual) {
        (Value::String(t), a) if is_variable(t) => {
            matches!(a, Value::String(_) | Value::Number(_) | Value::Bool(_))
        }
        (Value::String(t), Value::String(a)) if t.contains("${") => {
            glob(&variables_as_stars(t), a) && !a.contains('\n')
        }
        (Value::Object(t), Value::Object(a)) => {
            t.len() == a.len()
                && t.iter()
                    .all(|(k, tv)| a.get(k).is_some_and(|av| args_match(tv, av)))
        }
        (Value::Array(t), Value::Array(a)) => {
            t.len() == a.len() && t.iter().zip(a).all(|(tv, av)| args_match(tv, av))
        }
        (t, a) => t == a,
    }
}

fn is_variable(s: &str) -> bool {
    s.starts_with("${")
        && s.ends_with('}')
        && s[2..s.len() - 1]
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
}

/// `open ${file} now` → `open * now`, for matching (and a literal `*` in the template is kept).
fn variables_as_stars(template: &str) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(end) => {
                out.push('*');
                rest = &rest[start + end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Every string in a call's arguments.
fn strings(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a.iter().flat_map(strings).collect(),
        serde_json::Value::Object(o) => o.values().flat_map(strings).collect(),
        _ => Vec::new(),
    }
}

/// Case-insensitive `*` wildcard match of the whole text.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether an app list entry names this app: its id, its name, its program's name, or a
/// `*pattern*` over those (CAP-07).
pub fn app_matches(entry: &str, id: &str, name: &str) -> bool {
    let stem = std::path::Path::new(id)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let entry = entry.trim();
    if entry.is_empty() {
        return false;
    }
    [id, name, stem.as_str()]
        .iter()
        .filter(|c| !c.is_empty())
        .any(|c| {
            if entry.contains('*') {
                glob(entry, c)
            } else {
                c.eq_ignore_ascii_case(entry)
            }
        })
}

/// The per-app list for a capability, if it has one (UIA, screen awareness, computer use).
pub fn app_scope(tools: &Tools, capability: Capability) -> Option<&AppScope> {
    match capability {
        Capability::UiAutomation => Some(&tools.ui_automation_apps),
        Capability::ScreenAwareness => Some(&tools.screen_apps),
        Capability::ComputerUse => Some(&tools.computer_use_apps),
        _ => None,
    }
}

/// Destination binding (SECURITY §4): an address is the user's only when their own words for
/// this task contain it (or its host); one that appeared while the turn was tainted is untrusted;
/// otherwise it is the system's (a link the brain knew).
pub fn bind(address: &str, user_text: &[String], tainted: bool) -> Provenance {
    let a = address.trim().to_lowercase();
    let host = a
        .split_once("://")
        .map_or(a.as_str(), |(_, r)| r)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_owned();
    let said = user_text.iter().any(|t| {
        let t = t.to_lowercase();
        (!a.is_empty() && t.contains(&a)) || (host.len() > 3 && t.contains(&host))
    });
    if said {
        Provenance::User
    } else if tainted {
        Provenance::Untrusted
    } else {
        Provenance::System
    }
}

/// Limits no mode or brain can lift (SECURITY §1.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HardLimits {
    /// The emergency stop fired for this turn.
    pub stopped: bool,
    /// Apps KIVO must never act on, by id or name.
    pub blocked_apps: Vec<String>,
}

pub struct Context<'a> {
    pub mode: PermissionMode,
    pub session: SessionKind,
    pub taint: Taint,
    pub capabilities: &'a CapabilitySettings,
    pub limits: &'a HardLimits,
    pub grants: &'a [Grant],
    /// The per-capability options (app scopes, CAP-07).
    pub tools: &'a Tools,
    pub privacy: PrivacyMode,
    /// The tool's own assessment of this call's risk from its arguments (SECURITY §3); the
    /// declared risk when `None`.
    pub assessed: Option<Risk>,
    /// A background task's grants (SEC-09): what the user approved when the task was created.
    /// A task's own calls (`Initiator::Task`) never go beyond them.
    pub task_grants: Option<&'a [Grant]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "capability")]
pub enum DenyCode {
    EmergencyStop,
    CapabilityOff(Capability),
    BlockedApp,
    UntrustedDestination,
    GuestNotAllowed,
    NotConfirmed,
    /// The privacy mode keeps this on the device (SECURITY §6).
    Privacy,
    /// A limit no mode lifts, found by the tool itself before anyone is asked (typing into a
    /// password field, SECURITY §1.1).
    HardLimit,
    /// A background task asked for something it wasn't granted when it was created (SEC-09).
    NotGranted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct Denial {
    pub code: DenyCode,
    /// Safe to show and speak.
    pub message: String,
}

/// Proof that one specific call was authorized. It can't be built, cloned or deserialized outside
/// this crate, and the executor checks it names the call it is given.
#[derive(Debug, PartialEq, Eq)]
pub struct Permit {
    call_id: String,
    tool: String,
    confirmed_by: ConfirmedBy,
}

impl Permit {
    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    pub fn tool(&self) -> &str {
        &self.tool
    }

    pub fn confirmed_by(&self) -> ConfirmedBy {
        self.confirmed_by
    }

    /// True when this permit was issued for `call`.
    pub fn covers(&self, call: &ToolCall) -> bool {
        self.call_id == call.id && self.tool == call.tool
    }

    fn new(call: &ToolCall, by: ConfirmedBy) -> Self {
        Self {
            call_id: call.id.clone(),
            tool: call.tool.clone(),
            confirmed_by: by,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    Allow(Permit),
    Confirm(ConfirmSpec),
    Deny(Denial),
}

impl Decision {
    /// For events and the audit log.
    pub fn kind(&self) -> kivo_core::event::PermissionDecision {
        use kivo_core::event::PermissionDecision as D;
        match self {
            Self::Allow(_) => D::Allow,
            Self::Confirm(_) => D::Confirm,
            Self::Deny(_) => D::Deny,
        }
    }
}

fn deny(code: DenyCode, message: impl Into<String>) -> Decision {
    Decision::Deny(Denial {
        code,
        message: message.into(),
    })
}

/// Decides one call (SEC-06).
pub fn authorize(spec: &ToolSpec, call: &ToolCall, cx: &Context<'_>) -> Decision {
    // 1. Hard limits, in every mode (SEC-05).
    if cx.limits.stopped {
        return deny(DenyCode::EmergencyStop, text::t("policy.stopped"));
    }
    if !cx.capabilities.enabled(spec.capability) {
        return deny(
            DenyCode::CapabilityOff(spec.capability),
            text::tf(
                "policy.capabilityOff",
                &[("capability", &spec.capability.label())],
            ),
        );
    }
    let scope = app_scope(cx.tools, spec.capability);
    let outbound = spec.data_egress
        || spec
            .side_effects
            .iter()
            .any(|e| matches!(e, SideEffect::ExternalComms | SideEffect::Financial));
    for target in &call.targets {
        match target {
            Target::App { id, name }
                if cx
                    .limits
                    .blocked_apps
                    .iter()
                    .any(|b| app_matches(b, id, name))
                    || scope.is_some_and(|s| {
                        s.block.iter().any(|b| app_matches(b, id, name))
                            || (!s.allow.is_empty()
                                && !s.allow.iter().any(|a| app_matches(a, id, name)))
                    }) =>
            {
                return deny(
                    DenyCode::BlockedApp,
                    text::tf("policy.blockedApp", &[("name", name)]),
                );
            }
            // Untrusted destinations never; and data goes out only to what the user named.
            Target::Destination {
                address,
                provenance,
            } if *provenance == Provenance::Untrusted
                || (spec.data_egress && *provenance != Provenance::User) =>
            {
                return deny(
                    DenyCode::UntrustedDestination,
                    text::tf("policy.untrustedDestination", &[("address", address)]),
                );
            }
            _ => {}
        }
    }
    // Personal or sensitive data leaving the device: never in Strict Private, High otherwise.
    let class = if outbound {
        classify(&call.args.to_string())
    } else {
        DataClass::Public
    };
    if outbound && class >= DataClass::Personal && cx.privacy == PrivacyMode::StrictPrivate {
        return deny(DenyCode::Privacy, text::t("policy.privateData"));
    }

    // Task calls: exactly what was approved when the task was created, in every mode (SEC-09).
    // The approval was the confirmation (a click, with every step shown), so High steps the
    // user approved run; anything else is refused, never asked, since no one may be watching.
    if call.initiated_by == Initiator::Task {
        return if cx
            .task_grants
            .is_some_and(|g| g.iter().any(|g| g.covers(call)))
        {
            Decision::Allow(Permit::new(call, ConfirmedBy::Grant))
        } else {
            deny(
                DenyCode::NotGranted,
                text::tf("policy.taskNotGranted", &[("action", &spec.title)]),
            )
        };
    }

    // 2. Bypass: the user's explicit, time-limited override (SEC-03); owners only.
    if cx.mode == PermissionMode::Bypass && cx.session == SessionKind::Owner {
        return Decision::Allow(Permit::new(call, ConfirmedBy::Policy));
    }

    let mut risk = cx.assessed.unwrap_or(spec.risk);
    if outbound && class >= DataClass::Personal {
        risk = Risk::High;
    }
    let spec = &ToolSpec {
        risk,
        ..spec.clone()
    };
    let tainted =
        matches!(cx.taint, Taint::Tainted(_)) || call.initiated_by != Initiator::UserDirect;
    // `why` is a key under `policy.` in the text catalog.
    // In Plan first every change waits for the plan (SECURITY §1.1).
    let confirm = |strength: Strength, why: &str, plan: bool| {
        let why = text::t(&format!("policy.{why}"));
        let plan = plan || cx.mode == PermissionMode::Plan;
        Decision::Confirm(confirm_spec(spec, call, cx, strength, &why, plan))
    };

    // 3. Guests: reads and low-risk actions only, and low risk asks (SECURITY §2 table). A guest
    // reads and writes no memory at all (MEM-07).
    if cx.session == SessionKind::Guest {
        if spec.capability == Capability::Memory {
            return deny(DenyCode::GuestNotAllowed, text::t("policy.guestDenied"));
        }
        return match risk {
            Risk::Safe => Decision::Allow(Permit::new(call, ConfirmedBy::Policy)),
            Risk::Low => confirm(Strength::Normal, "guestLow", false),
            Risk::Medium | Risk::High => {
                deny(DenyCode::GuestNotAllowed, text::t("policy.guestDenied"))
            }
        };
    }

    // 4. High always confirms; irreversible actions confirm up front (UX §8.1).
    if risk == Risk::High {
        let why = if tainted { "highTainted" } else { "high" };
        return confirm(Strength::Strong, why, cx.mode == PermissionMode::Plan);
    }
    if spec.reversibility == Reversibility::Irreversible {
        return confirm(
            Strength::Normal,
            "irreversible",
            cx.mode == PermissionMode::Plan,
        );
    }

    let read_only = spec
        .side_effects
        .iter()
        .all(|e| matches!(e, SideEffect::None | SideEffect::LocalRead));
    let granted = cx.grants.iter().any(|g| g.covers(call));
    let allow = |by| Decision::Allow(Permit::new(call, by));

    // 5. Tainted or AI-initiated turns: medium risk asks unless granted (SECURITY §2).
    if tainted && risk == Risk::Medium {
        return if granted {
            allow(ConfirmedBy::Grant)
        } else {
            confirm(Strength::Normal, "aiAsked", false)
        };
    }

    // 6. The mode (SECURITY §1.1).
    match cx.mode {
        PermissionMode::Auto | PermissionMode::Bypass => allow(ConfirmedBy::Policy),
        PermissionMode::Ask => {
            if read_only || risk == Risk::Safe {
                allow(ConfirmedBy::Policy)
            } else if granted {
                allow(ConfirmedBy::Grant)
            } else {
                confirm(Strength::Normal, "askMode", false)
            }
        }
        PermissionMode::AcceptEdits => {
            let edits = matches!(
                spec.capability,
                Capability::AppsAndWindows | Capability::FilesModify
            );
            if risk <= Risk::Low || (risk == Risk::Medium && edits) || granted {
                allow(if granted {
                    ConfirmedBy::Grant
                } else {
                    ConfirmedBy::Policy
                })
            } else {
                confirm(Strength::Normal, "acceptEditsMode", false)
            }
        }
        PermissionMode::Plan => {
            if read_only || risk == Risk::Safe {
                allow(ConfirmedBy::Policy)
            } else {
                confirm(Strength::Normal, "planMode", true)
            }
        }
    }
}

/// How the user answered a confirmation card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Answer {
    Allow { by: ConfirmedBy },
    Deny,
}

/// Approving a plan (Plan first, SECURITY §1.1): one answer grants exactly the planned steps,
/// nothing more. Voice can't approve a plan with a High-risk step.
pub fn approve_plan(
    steps: &[(ConfirmSpec, ToolCall)],
    answer: Answer,
) -> Result<Vec<Permit>, Denial> {
    let refuse = |key: &str| Denial {
        code: DenyCode::NotConfirmed,
        message: text::t(key),
    };
    let by = match answer {
        Answer::Deny => return Err(refuse("reply.cancelled")),
        Answer::Allow { by } => by,
    };
    if !matches!(
        by,
        ConfirmedBy::Click | ConfirmedBy::Voice | ConfirmedBy::Hello
    ) {
        return Err(refuse("policy.notAWay"));
    }
    let strong = steps.iter().any(|(s, _)| s.strength == Strength::Strong);
    if strong && by == ConfirmedBy::Voice {
        return Err(refuse("policy.voiceNotEnough"));
    }
    steps
        .iter()
        .map(|(spec, call)| {
            if spec.call_id != call.id || spec.tool != call.tool {
                Err(refuse("policy.differentAction"))
            } else {
                Ok(Permit::new(call, by))
            }
        })
        .collect()
}

/// Turns the user's answer to `spec` into a permit (SEC-10, CONVERSATION §7).
pub fn confirmed(spec: &ConfirmSpec, call: &ToolCall, answer: Answer) -> Result<Permit, Denial> {
    // `key` is under `policy.` in the text catalog, except `reply.cancelled`.
    let refuse = |key: &str| Denial {
        code: DenyCode::NotConfirmed,
        message: text::t(key),
    };
    if spec.call_id != call.id || spec.tool != call.tool {
        return Err(refuse("policy.differentAction"));
    }
    match answer {
        Answer::Deny => Err(refuse("reply.cancelled")),
        // High risk needs an on-screen click or Windows Hello; a spoken "yes" is not enough.
        Answer::Allow {
            by: ConfirmedBy::Voice,
        } if spec.strength == Strength::Strong => Err(refuse("policy.voiceNotEnough")),
        Answer::Allow {
            by: by @ (ConfirmedBy::Click | ConfirmedBy::Voice | ConfirmedBy::Hello),
        } => Ok(Permit::new(call, by)),
        Answer::Allow { .. } => Err(refuse("policy.notAWay")),
    }
}

fn target_id(target: &Target) -> Option<&String> {
    match target {
        Target::App { id, .. } => Some(id),
        Target::Window { app_id, .. } => Some(app_id),
        Target::Destination { address, .. } => Some(address),
    }
}

fn target_name(target: &Target) -> String {
    match target {
        Target::App { name, .. } => name.clone(),
        Target::Window { title, .. } => title.clone(),
        Target::Destination { address, .. } => address.clone(),
    }
}

fn confirm_spec(
    spec: &ToolSpec,
    call: &ToolCall,
    cx: &Context<'_>,
    strength: Strength,
    why: &str,
    plan: bool,
) -> ConfirmSpec {
    let provenance = match (&cx.taint, call.initiated_by) {
        (Taint::Tainted(sources), _) if !sources.is_empty() => text::tf(
            "policy.provenance.read",
            &[("sources", &sources.join(", "))],
        ),
        (_, Initiator::UserDirect) => text::t("policy.provenance.user"),
        (_, Initiator::Brain) => text::t("policy.provenance.brain"),
        (_, Initiator::Task) => text::t("policy.provenance.task"),
        (_, Initiator::Mcp) => text::t("policy.provenance.mcp"),
    };
    ConfirmSpec {
        call_id: call.id.clone(),
        tool: call.tool.clone(),
        action: render_title(&spec.title, &call.args),
        target: call.targets.first().map(target_name),
        why: why.into(),
        provenance,
        risk: spec.risk,
        strength,
        allow_always: spec.risk < Risk::High && cx.session == SessionKind::Owner,
        plan,
        hello: false,
        watch: false,
    }
}

/// The watch-mode card for one computer-use step (CAP-11): approved by a click or Enter; by
/// voice only when the step is Low risk. No "always".
pub fn watch_card(spec: &ToolSpec, call: &ToolCall) -> ConfirmSpec {
    ConfirmSpec {
        call_id: call.id.clone(),
        tool: call.tool.clone(),
        action: render_title(&spec.title, &call.args),
        target: call.targets.first().map(target_name),
        why: text::t("policy.watchMode"),
        provenance: text::t("policy.provenance.task"),
        risk: spec.risk,
        strength: if spec.risk <= Risk::Low {
            Strength::Normal
        } else {
            Strength::Strong
        },
        allow_always: false,
        plan: false,
        hello: false,
        watch: true,
    }
}

/// Fills `{arg}` placeholders from the call's arguments (`{app}` uses the app's name).
pub fn render_title(title: &str, args: &serde_json::Value) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let key = &rest[start + 1..start + end];
        let value = &args[key];
        match value {
            serde_json::Value::String(s) => out.push_str(s),
            serde_json::Value::Number(n) => out.push_str(&n.to_string()),
            serde_json::Value::Object(o) => {
                if let Some(name) = o.get("name").and_then(|n| n.as_str()) {
                    out.push_str(name);
                }
            }
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::tool::{CapabilityTier, Platform};
    use serde_json::json;

    fn spec(risk: Risk, effects: &[SideEffect], capability: Capability) -> ToolSpec {
        ToolSpec {
            id: "x.tool".into(),
            description: String::new(),
            title: "Do it to {app}".into(),
            params: json!({}),
            result: json!({}),
            risk,
            side_effects: effects.to_vec(),
            data_egress: false,
            timeout_ms: 1000,
            cancellable: true,
            tier: CapabilityTier::OsApi,
            platforms: vec![Platform::Windows],
            reversibility: Reversibility::NotApplicable,
            capability,
        }
    }

    fn call(initiated_by: Initiator) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            tool: "x.tool".into(),
            args: json!({"app": {"id": "chrome", "name": "Google Chrome"}}),
            initiated_by,
            targets: vec![Target::App {
                id: "chrome".into(),
                name: "Google Chrome".into(),
            }],
        }
    }

    struct Env {
        caps: CapabilitySettings,
        limits: HardLimits,
        grants: Vec<Grant>,
        tools: Tools,
        privacy: PrivacyMode,
        assessed: Option<Risk>,
        task_grants: Option<Vec<Grant>>,
    }

    impl Env {
        fn new() -> Self {
            Self {
                caps: CapabilitySettings::default(),
                limits: HardLimits::default(),
                grants: Vec::new(),
                tools: Tools::default(),
                privacy: PrivacyMode::Cloud,
                assessed: None,
                task_grants: None,
            }
        }
        fn cx(&self, mode: PermissionMode, session: SessionKind, taint: Taint) -> Context<'_> {
            Context {
                mode,
                session,
                taint,
                capabilities: &self.caps,
                limits: &self.limits,
                grants: &self.grants,
                tools: &self.tools,
                privacy: self.privacy,
                assessed: self.assessed,
                task_grants: self.task_grants.as_deref(),
            }
        }
    }

    fn outcome(d: &Decision) -> &'static str {
        match d {
            Decision::Allow(_) => "allow",
            Decision::Confirm(c) if c.strength == Strength::Strong => "confirm!",
            Decision::Confirm(_) => "confirm",
            Decision::Deny(_) => "deny",
        }
    }

    /// The SECURITY §2 table for the default mode, by risk: clean user turn, tainted or
    /// AI-initiated turn, guest.
    #[test]
    fn the_default_policy_table() {
        let env = Env::new();
        let write = [SideEffect::LocalWrite];
        let rows = [
            (Risk::Safe, ["allow", "allow", "allow"]),
            (Risk::Low, ["allow", "allow", "confirm"]),
            (Risk::Medium, ["allow", "confirm", "deny"]),
            (Risk::High, ["confirm!", "confirm!", "deny"]),
        ];
        for (risk, expected) in rows {
            let s = spec(risk, &write, Capability::AppsAndWindows);
            let clean = authorize(
                &s,
                &call(Initiator::UserDirect),
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean),
            );
            let by_ai = authorize(
                &s,
                &call(Initiator::Brain),
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean),
            );
            let tainted = authorize(
                &s,
                &call(Initiator::UserDirect),
                &env.cx(
                    PermissionMode::Auto,
                    SessionKind::Owner,
                    Taint::Tainted(vec!["example.com".into()]),
                ),
            );
            let guest = authorize(
                &s,
                &call(Initiator::UserDirect),
                &env.cx(PermissionMode::Auto, SessionKind::Guest, Taint::Clean),
            );
            assert_eq!(outcome(&clean), expected[0], "{risk:?} clean");
            assert_eq!(outcome(&by_ai), expected[1], "{risk:?} AI-initiated");
            assert_eq!(outcome(&tainted), expected[1], "{risk:?} tainted");
            assert_eq!(outcome(&guest), expected[2], "{risk:?} guest");
        }
    }

    /// MEM-07: a guest can't even search the owner's memory, though searching is Safe.
    #[test]
    fn guests_get_no_memory() {
        let env = Env::new();
        let search = spec(Risk::Safe, &[SideEffect::LocalRead], Capability::Memory);
        let guest = authorize(
            &search,
            &call(Initiator::UserDirect),
            &env.cx(PermissionMode::Auto, SessionKind::Guest, Taint::Clean),
        );
        assert_eq!(outcome(&guest), "deny");
        let owner = authorize(
            &search,
            &call(Initiator::UserDirect),
            &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean),
        );
        assert_eq!(outcome(&owner), "allow");
    }

    #[test]
    fn each_mode_asks_for_what_it_promises() {
        let env = Env::new();
        let user = call(Initiator::UserDirect);
        let decide = |mode, risk, effects: &[SideEffect], cap| {
            outcome(&authorize(
                &spec(risk, effects, cap),
                &user,
                &env.cx(mode, SessionKind::Owner, Taint::Clean),
            ))
        };
        let (read, write) = ([SideEffect::LocalRead], [SideEffect::LocalWrite]);
        use PermissionMode as M;
        // Ask every time: every non-read action asks.
        assert_eq!(
            decide(M::Ask, Risk::Safe, &read, Capability::SystemControls),
            "allow"
        );
        assert_eq!(
            decide(M::Ask, Risk::Low, &write, Capability::AppsAndWindows),
            "confirm"
        );
        // Accept edits: app/window and file actions run; commands and settings ask.
        assert_eq!(
            decide(
                M::AcceptEdits,
                Risk::Medium,
                &write,
                Capability::AppsAndWindows
            ),
            "allow"
        );
        assert_eq!(
            decide(
                M::AcceptEdits,
                Risk::Medium,
                &write,
                Capability::SystemControls
            ),
            "confirm"
        );
        // Plan first: reads run, anything else waits for plan approval.
        let plan = authorize(
            &spec(Risk::Low, &write, Capability::AppsAndWindows),
            &user,
            &env.cx(M::Plan, SessionKind::Owner, Taint::Clean),
        );
        assert!(matches!(plan, Decision::Confirm(ref c) if c.plan));
        // High always confirms except in Bypass.
        for mode in [M::Ask, M::AcceptEdits, M::Plan, M::Auto] {
            assert_eq!(
                decide(mode, Risk::High, &write, Capability::PowerActions),
                "confirm!",
                "{mode:?}"
            );
        }
        assert_eq!(
            decide(M::Bypass, Risk::High, &write, Capability::PowerActions),
            "allow"
        );
    }

    #[test]
    fn hard_limits_hold_in_every_mode_including_bypass() {
        let mut env = Env::new();
        let s = spec(Risk::Safe, &[SideEffect::None], Capability::AppsAndWindows);
        let user = call(Initiator::UserDirect);
        env.limits.blocked_apps = vec!["chrome".into()];
        let d = authorize(
            &s,
            &user,
            &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean),
        );
        assert!(matches!(
            d,
            Decision::Deny(Denial {
                code: DenyCode::BlockedApp,
                ..
            })
        ));

        env.limits.blocked_apps.clear();
        env.caps.set(Capability::AppsAndWindows, false);
        let d = authorize(
            &s,
            &user,
            &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean),
        );
        assert!(
            matches!(&d, Decision::Deny(Denial { code: DenyCode::CapabilityOff(Capability::AppsAndWindows), message }) if message == "Apps & windows is off. Turn it on?")
        );

        env.caps.set(Capability::AppsAndWindows, true);
        env.limits.stopped = true;
        let d = authorize(
            &s,
            &user,
            &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean),
        );
        assert!(matches!(
            d,
            Decision::Deny(Denial {
                code: DenyCode::EmergencyStop,
                ..
            })
        ));

        env.limits.stopped = false;
        let mut send = user;
        send.targets = vec![Target::Destination {
            address: "x@evil.test".into(),
            provenance: Provenance::Untrusted,
        }];
        let d = authorize(
            &s,
            &send,
            &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean),
        );
        assert!(matches!(
            d,
            Decision::Deny(Denial {
                code: DenyCode::UntrustedDestination,
                ..
            })
        ));
    }

    #[test]
    fn grants_lift_medium_asks_but_never_high() {
        let mut env = Env::new();
        env.grants = vec![Grant {
            tool: "x.tool".into(),
            scope: Some("chrome".into()),
            pattern: None,
            args: None,
        }];
        let brain = call(Initiator::Brain);
        let tainted = || Taint::Tainted(vec!["mail".into()]);
        let medium = spec(
            Risk::Medium,
            &[SideEffect::LocalWrite],
            Capability::AppsAndWindows,
        );
        assert!(matches!(
            authorize(&medium, &brain, &env.cx(PermissionMode::Auto, SessionKind::Owner, tainted())),
            Decision::Allow(ref p) if p.confirmed_by() == ConfirmedBy::Grant
        ));
        let high = spec(
            Risk::High,
            &[SideEffect::Destructive],
            Capability::PowerActions,
        );
        assert_eq!(
            outcome(&authorize(
                &high,
                &brain,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, tainted())
            )),
            "confirm!"
        );
        // A grant scoped to another app doesn't apply.
        env.grants[0].scope = Some("spotify".into());
        assert_eq!(
            outcome(&authorize(
                &medium,
                &brain,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, tainted())
            )),
            "confirm"
        );
    }

    #[test]
    fn a_spoken_yes_is_not_enough_for_high_risk() {
        let env = Env::new();
        let user = call(Initiator::UserDirect);
        let high = spec(
            Risk::High,
            &[SideEffect::Destructive],
            Capability::PowerActions,
        );
        let Decision::Confirm(card) = authorize(
            &high,
            &user,
            &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean),
        ) else {
            panic!("High must confirm");
        };
        assert_eq!(card.action, "Do it to Google Chrome");
        assert_eq!(card.provenance, "You asked");
        assert!(!card.allow_always);
        assert!(
            confirmed(
                &card,
                &user,
                Answer::Allow {
                    by: ConfirmedBy::Voice
                }
            )
            .is_err()
        );
        assert!(confirmed(&card, &user, Answer::Deny).is_err());
        let permit = confirmed(
            &card,
            &user,
            Answer::Allow {
                by: ConfirmedBy::Click,
            },
        )
        .unwrap();
        assert!(permit.covers(&user));
        let mut other = call(Initiator::UserDirect);
        other.id = "c2".into();
        assert!(!permit.covers(&other), "a permit is for one call only");
        assert!(
            confirmed(
                &card,
                &other,
                Answer::Allow {
                    by: ConfirmedBy::Click
                }
            )
            .is_err()
        );
    }

    #[test]
    fn titles_fill_in_names_and_numbers() {
        assert_eq!(
            render_title("Open {app}", &json!({"app": {"id": "c", "name": "Chrome"}})),
            "Open Chrome"
        );
        assert_eq!(
            render_title("Set the volume to {number}%", &json!({"number": 30})),
            "Set the volume to 30%"
        );
        assert_eq!(
            render_title("Lock the computer", &json!({})),
            "Lock the computer"
        );
    }

    #[test]
    fn app_scopes_block_password_managers_banking_and_windows_security_by_default() {
        let mut env = Env::new();
        env.caps.set(Capability::ScreenAwareness, true);
        let uia = spec(
            Risk::Safe,
            &[SideEffect::LocalRead],
            Capability::UiAutomation,
        );
        let on = |id: &str, name: &str| {
            let mut c = call(Initiator::Brain);
            c.targets = vec![Target::App {
                id: id.into(),
                name: name.into(),
            }];
            c
        };
        for (id, name) in [
            (r"C:\Program Files\KeePassXC\KeePassXC.exe", "KeePassXC"),
            (
                r"C:\Users\me\AppData\Local\1Password\app\8\1Password.exe",
                "1Password",
            ),
            (r"C:\Apps\MyBankApp.exe", "MyBankApp"),
            (r"C:\Windows\SystemApps\SecHealthUI.exe", "SecHealthUI"),
        ] {
            let d = authorize(
                &uia,
                &on(id, name),
                &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean),
            );
            assert!(
                matches!(
                    d,
                    Decision::Deny(Denial {
                        code: DenyCode::BlockedApp,
                        ..
                    })
                ),
                "{name}"
            );
        }
        assert!(matches!(
            authorize(
                &uia,
                &on(r"C:\Windows\notepad.exe", "notepad"),
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Allow(_)
        ));
        // An allow list: only those apps.
        env.tools.ui_automation_apps.allow = vec!["notepad".into()];
        assert!(matches!(
            authorize(
                &uia,
                &on(r"C:\x\Code.exe", "Code"),
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Deny(_)
        ));
        // Other capabilities aren't scoped by these lists.
        let apps = spec(
            Risk::Low,
            &[SideEffect::LocalWrite],
            Capability::AppsAndWindows,
        );
        assert!(matches!(
            authorize(
                &apps,
                &on(r"C:\x\KeePassXC.exe", "KeePassXC"),
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Allow(_)
        ));
    }

    #[test]
    fn destinations_are_bound_to_the_users_own_words() {
        let words = vec!["email the report to sam@example.com".to_owned()];
        assert_eq!(bind("sam@example.com", &words, true), Provenance::User);
        assert_eq!(bind("x@evil.test", &words, true), Provenance::Untrusted);
        assert_eq!(
            bind("https://github.com/kivo", &[], false),
            Provenance::System
        );
        let open = vec!["open github.com please".to_owned()];
        assert_eq!(
            bind("https://www.github.com/x", &open, true),
            Provenance::User
        );

        let env = Env::new();
        let mut send = spec(
            Risk::Medium,
            &[SideEffect::ExternalComms],
            Capability::AppsAndWindows,
        );
        send.data_egress = true;
        let mut c = call(Initiator::Brain);
        c.targets = vec![Target::Destination {
            address: "https://docs.example".into(),
            provenance: Provenance::System,
        }];
        // Outbound to somewhere the user didn't name: denied even in Bypass.
        assert!(matches!(
            authorize(
                &send,
                &c,
                &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Deny(Denial {
                code: DenyCode::UntrustedDestination,
                ..
            })
        ));
        c.targets = vec![Target::Destination {
            address: "https://docs.example".into(),
            provenance: Provenance::User,
        }];
        assert!(!matches!(
            authorize(
                &send,
                &c,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Deny(_)
        ));
        // Opening a link isn't sending: a known link on a clean turn is fine.
        let open_link = spec(
            Risk::Low,
            &[SideEffect::LocalWrite],
            Capability::BrowserOpenLinks,
        );
        c.targets = vec![Target::Destination {
            address: "https://github.com".into(),
            provenance: Provenance::System,
        }];
        assert!(matches!(
            authorize(
                &open_link,
                &c,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Allow(_)
        ));
    }

    #[test]
    fn arguments_raise_the_risk_and_private_data_stays_home_in_strict_private() {
        let mut env = Env::new();
        let files = spec(
            Risk::Medium,
            &[SideEffect::Destructive],
            Capability::FilesModify,
        );
        let user = call(Initiator::UserDirect);
        // The tool assessed a protected path: High confirms strongly even in Auto.
        env.assessed = Some(Risk::High);
        assert_eq!(
            outcome(&authorize(
                &files,
                &user,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            )),
            "confirm!"
        );
        env.assessed = None;
        let mut send = spec(
            Risk::Low,
            &[SideEffect::ExternalComms],
            Capability::AppsAndWindows,
        );
        send.data_egress = true;
        let mut c = call(Initiator::UserDirect);
        c.targets.clear();
        c.args = json!({ "text": "my card is 4111 1111 1111 1111" });
        assert_eq!(
            outcome(&authorize(
                &send,
                &c,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            )),
            "confirm!"
        );
        env.privacy = PrivacyMode::StrictPrivate;
        assert!(matches!(
            authorize(
                &send,
                &c,
                &env.cx(PermissionMode::Bypass, SessionKind::Owner, Taint::Clean)
            ),
            Decision::Deny(Denial {
                code: DenyCode::Privacy,
                ..
            })
        ));
        // Input the user asked for directly can be assessed lower than declared.
        env.privacy = PrivacyMode::Cloud;
        env.assessed = Some(Risk::Low);
        let input = spec(
            Risk::Medium,
            &[SideEffect::LocalWrite],
            Capability::ComputerUse,
        );
        env.caps.set(Capability::ComputerUse, true);
        assert_eq!(
            outcome(&authorize(
                &input,
                &user,
                &env.cx(PermissionMode::Ask, SessionKind::Owner, Taint::Clean)
            )),
            "confirm"
        );
        assert_eq!(
            outcome(&authorize(
                &input,
                &user,
                &env.cx(
                    PermissionMode::AcceptEdits,
                    SessionKind::Owner,
                    Taint::Clean
                )
            )),
            "allow"
        );
    }

    #[test]
    fn grants_match_argument_patterns() {
        assert!(glob(
            "C:\\Users\\me\\Documents\\*",
            "c:\\users\\me\\documents\\a\\b.txt"
        ));
        assert!(!glob(
            "C:\\Users\\me\\Documents\\*",
            "C:\\Users\\me\\Desktop\\b.txt"
        ));
        assert!(glob("git *", "git status"));
        assert!(!glob("git *", "gitk"));
        let g = Grant {
            tool: "shell.run".into(),
            scope: None,
            pattern: Some("git *".into()),
            args: None,
        };
        let mut c = call(Initiator::Brain);
        c.tool = "shell.run".into();
        c.args = json!({ "command": "git status" });
        assert!(g.covers(&c));
        c.args = json!({ "command": "Remove-Item x" });
        assert!(!g.covers(&c));
    }

    #[test]
    fn a_plan_is_approved_as_exactly_its_steps() {
        let env = Env::new();
        let medium = spec(
            Risk::Medium,
            &[SideEffect::LocalWrite],
            Capability::AppsAndWindows,
        );
        let mut steps = Vec::new();
        for i in 0..2 {
            let mut c = call(Initiator::Brain);
            c.id = format!("step-{i}");
            let Decision::Confirm(card) = authorize(
                &medium,
                &c,
                &env.cx(PermissionMode::Plan, SessionKind::Owner, Taint::Clean),
            ) else {
                panic!("Plan first asks");
            };
            assert!(card.plan);
            steps.push((card, c));
        }
        let permits = approve_plan(
            &steps,
            Answer::Allow {
                by: ConfirmedBy::Click,
            },
        )
        .unwrap();
        assert_eq!(permits.len(), 2);
        assert!(permits[0].covers(&steps[0].1) && permits[1].covers(&steps[1].1));
        assert!(
            !permits[0].covers(&steps[1].1),
            "each permit is for its own step only"
        );
        assert!(approve_plan(&steps, Answer::Deny).is_err());
        // A plan with a High step needs more than a spoken yes.
        let high = spec(
            Risk::High,
            &[SideEffect::Destructive],
            Capability::PowerActions,
        );
        let mut c = call(Initiator::Brain);
        c.id = "step-h".into();
        let Decision::Confirm(card) = authorize(
            &high,
            &c,
            &env.cx(PermissionMode::Plan, SessionKind::Owner, Taint::Clean),
        ) else {
            panic!()
        };
        steps.push((card, c));
        assert!(
            approve_plan(
                &steps,
                Answer::Allow {
                    by: ConfirmedBy::Voice
                }
            )
            .is_err()
        );
        assert_eq!(
            approve_plan(
                &steps,
                Answer::Allow {
                    by: ConfirmedBy::Hello
                }
            )
            .unwrap()
            .len(),
            3
        );
    }

    /// SEC-09: a task's calls run exactly as approved at creation, in every mode, and nothing
    /// beyond — not asked, refused.
    #[test]
    fn tasks_never_exceed_their_grants() {
        let mut env = Env::new();
        let mut c = call(Initiator::Task);
        c.args = json!({ "app": "chrome" });
        let high = spec(
            Risk::High,
            &[SideEffect::LocalWrite],
            Capability::AppsAndWindows,
        );
        for mode in [
            PermissionMode::Ask,
            PermissionMode::Auto,
            PermissionMode::Plan,
            PermissionMode::Bypass,
        ] {
            env.task_grants = None;
            assert_eq!(
                outcome(&authorize(
                    &high,
                    &c,
                    &env.cx(mode, SessionKind::Owner, Taint::Clean)
                )),
                "deny",
                "{mode:?}"
            );
            env.task_grants = Some(vec![Grant::exact("x.tool", json!({ "app": "chrome" }))]);
            assert_eq!(
                outcome(&authorize(
                    &high,
                    &c,
                    &env.cx(mode, SessionKind::Owner, Taint::Clean)
                )),
                "allow",
                "{mode:?}"
            );
            let mut other = c.clone();
            other.args = json!({ "app": "notepad" });
            assert_eq!(
                outcome(&authorize(
                    &high,
                    &other,
                    &env.cx(mode, SessionKind::Owner, Taint::Clean)
                )),
                "deny",
                "{mode:?}: other arguments"
            );
        }
        // The hard limits still come first.
        env.limits.stopped = true;
        assert_eq!(
            outcome(&authorize(
                &high,
                &c,
                &env.cx(PermissionMode::Auto, SessionKind::Owner, Taint::Clean)
            )),
            "deny"
        );
    }

    #[test]
    fn exact_argument_grants_fill_variables_only() {
        let g = Grant::exact(
            "timer.start",
            json!({ "minutes": "${minutes}", "label": "Focus for ${minutes} min", "sound": true }),
        );
        let mut c = call(Initiator::Task);
        c.tool = "timer.start".into();
        c.args = json!({ "minutes": 25, "label": "Focus for 25 min", "sound": true });
        assert!(g.covers(&c));
        c.args = json!({ "minutes": 25, "label": "Focus for 25 min", "sound": false });
        assert!(!g.covers(&c), "a fixed argument changed");
        c.args = json!({ "minutes": 25, "label": "Focus for 25 min", "sound": true, "extra": 1 });
        assert!(!g.covers(&c), "an argument was added");
        c.args = json!({ "minutes": { "evil": 1 }, "label": "Focus for 25 min", "sound": true });
        assert!(
            !g.covers(&c),
            "a variable is text or a number, not an object"
        );
        c.args = json!({ "minutes": 5, "label": "Something else", "sound": true });
        assert!(!g.covers(&c));
    }
}
