//! The permission engine (SECURITY §1.1–2, SEC-01/05/06/07): `authorize(call, context)` decides
//! Allow, Confirm or Deny for every tool call. An Allow carries a `Permit`, the only thing the tool
//! executor accepts, and permits can only be made here (TOOL-02): AI output can never bypass this.
//!
//! Order: hard limits (every mode, even Bypass) → Bypass → guest → irreversible → the mode table,
//! tightened for tainted or AI-initiated turns.

use kivo_core::capability::{Capability, CapabilitySettings};
use kivo_core::config::PermissionMode;
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

/// "Always allow" for a tool, optionally only for one target (SEC-08).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub tool: String,
    /// A target id the grant is limited to (an app id, a folder); `None` for any.
    pub scope: Option<String>,
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
        return deny(
            DenyCode::EmergencyStop,
            "Stopped. KIVO won't continue this request.",
        );
    }
    if !cx.capabilities.enabled(spec.capability) {
        return deny(
            DenyCode::CapabilityOff(spec.capability),
            format!("{} is off. Turn it on?", spec.capability.label()),
        );
    }
    for target in &call.targets {
        match target {
            Target::App { id, name }
                if cx
                    .limits
                    .blocked_apps
                    .iter()
                    .any(|b| b == id || b.eq_ignore_ascii_case(name)) =>
            {
                return deny(
                    DenyCode::BlockedApp,
                    format!("KIVO isn't allowed to act on {name}."),
                );
            }
            Target::Destination {
                address,
                provenance: Provenance::Untrusted,
            } => {
                return deny(
                    DenyCode::UntrustedDestination,
                    format!(
                        "{address} came from content KIVO read, not from you, so it won't send anything there."
                    ),
                );
            }
            _ => {}
        }
    }

    // 2. Bypass: the user's explicit, time-limited override (SEC-03); owners only.
    if cx.mode == PermissionMode::Bypass && cx.session == SessionKind::Owner {
        return Decision::Allow(Permit::new(call, ConfirmedBy::Policy));
    }

    let risk = spec.risk;
    let tainted =
        matches!(cx.taint, Taint::Tainted(_)) || call.initiated_by != Initiator::UserDirect;
    let confirm = |strength: Strength, why: &str, plan: bool| {
        Decision::Confirm(confirm_spec(spec, call, cx, strength, why, plan))
    };

    // 3. Guests: reads and low-risk actions only, and low risk asks (SECURITY §2 table).
    if cx.session == SessionKind::Guest {
        return match risk {
            Risk::Safe => Decision::Allow(Permit::new(call, ConfirmedBy::Policy)),
            Risk::Low => confirm(
                Strength::Normal,
                "KIVO doesn't recognize this voice.",
                false,
            ),
            Risk::Medium | Risk::High => deny(
                DenyCode::GuestNotAllowed,
                "Only the owner can ask KIVO to do that.",
            ),
        };
    }

    // 4. High always confirms; irreversible actions confirm up front (UX §8.1).
    if risk == Risk::High {
        let why = if tainted {
            "This can't be undone, and it was requested by the AI or after reading other content."
        } else {
            "This is a high-risk action."
        };
        return confirm(Strength::Strong, why, cx.mode == PermissionMode::Plan);
    }
    if spec.reversibility == Reversibility::Irreversible {
        return confirm(
            Strength::Normal,
            "This can't be undone.",
            cx.mode == PermissionMode::Plan,
        );
    }

    let read_only = spec
        .side_effects
        .iter()
        .all(|e| matches!(e, SideEffect::None | SideEffect::LocalRead));
    let granted = cx.grants.iter().any(|g| {
        g.tool == call.tool
            && g.scope
                .as_ref()
                .is_none_or(|scope| call.targets.iter().any(|t| target_id(t) == Some(scope)))
    });
    let allow = |by| Decision::Allow(Permit::new(call, by));

    // 5. Tainted or AI-initiated turns: medium risk asks unless granted (SECURITY §2).
    if tainted && risk == Risk::Medium {
        return if granted {
            allow(ConfirmedBy::Grant)
        } else {
            confirm(
                Strength::Normal,
                "The AI asked for this, not you directly.",
                false,
            )
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
                confirm(
                    Strength::Normal,
                    "You asked KIVO to check with you before doing things.",
                    false,
                )
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
                confirm(
                    Strength::Normal,
                    "In Accept edits, KIVO checks before commands and settings.",
                    false,
                )
            }
        }
        PermissionMode::Plan => {
            if read_only || risk == Risk::Safe {
                allow(ConfirmedBy::Policy)
            } else {
                confirm(
                    Strength::Normal,
                    "In Plan first, nothing changes until you approve the plan.",
                    true,
                )
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

/// Turns the user's answer to `spec` into a permit (SEC-10, CONVERSATION §7).
pub fn confirmed(spec: &ConfirmSpec, call: &ToolCall, answer: Answer) -> Result<Permit, Denial> {
    let refuse = |message: &str| Denial {
        code: DenyCode::NotConfirmed,
        message: message.into(),
    };
    if spec.call_id != call.id || spec.tool != call.tool {
        return Err(refuse("That approval was for a different action."));
    }
    match answer {
        Answer::Deny => Err(refuse("Cancelled.")),
        // High risk needs an on-screen click or Windows Hello; a spoken "yes" is not enough.
        Answer::Allow {
            by: ConfirmedBy::Voice,
        } if spec.strength == Strength::Strong => Err(refuse(
            "For this one, please click Allow or confirm with Windows Hello.",
        )),
        Answer::Allow {
            by: by @ (ConfirmedBy::Click | ConfirmedBy::Voice | ConfirmedBy::Hello),
        } => Ok(Permit::new(call, by)),
        Answer::Allow { .. } => Err(refuse("That isn't a way to approve an action.")),
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
        (Taint::Tainted(sources), _) if !sources.is_empty() => {
            format!("Requested after reading {}", sources.join(", "))
        }
        (_, Initiator::UserDirect) => "You asked".into(),
        (_, Initiator::Brain) => "Suggested by the AI".into(),
        (_, Initiator::Task) => "Part of a background task".into(),
        (_, Initiator::Mcp) => "Requested by a connected tool".into(),
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
    }

    impl Env {
        fn new() -> Self {
            Self {
                caps: CapabilitySettings::default(),
                limits: HardLimits::default(),
                grants: Vec::new(),
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
}
