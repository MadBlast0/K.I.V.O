//! The tool registry and executor (TOOLS_AND_CONTROL §1, TOOL-01/02/03; CAPABILITIES §2, CAP-01).
//!
//! - A tool whose capability is off is **not registered**: it doesn't appear in the available
//!   list and can't be looked up, for any caller.
//! - `execute` takes a `Permit` from the permission engine. Only `kivo-security` can make one, and
//!   it must name this exact call, so nothing runs without `authorize()` (type-level).
//! - Every call has the tool's timeout and the turn's cancellation; errors carry a user-readable
//!   message and a code, and raw OS detail goes to the log only.

use kivo_core::capability::CapabilitySettings;
use kivo_core::text;
use kivo_core::tool::{Provenance, Risk, ToolCall, ToolError, ToolErrorCode, ToolResult, ToolSpec};
use kivo_security::Permit;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// What a tool produced: data for the brain and Activity, and a short sentence to say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub data: Value,
    /// Spoken and shown in the Island ("Opening Chrome.").
    pub say: String,
    /// Where the data came from when it is someone else's content (a page, a window, the
    /// clipboard, a file, command output): it is `Untrusted` and taints the turn (SECURITY §4).
    pub source: Option<String>,
    /// A PNG for a vision brain (`screen.look`); held in memory, never saved.
    pub image: Option<Vec<u8>>,
}

impl Output {
    pub fn new(say: impl Into<String>, data: Value) -> Self {
        Self {
            data,
            say: say.into(),
            source: None,
            image: None,
        }
    }

    /// Marks the data as untrusted content from `source`.
    #[must_use]
    pub fn untrusted(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// A tool implementation. `run` is blocking; the executor runs it off the async threads.
pub trait Tool: Send + Sync {
    fn spec(&self) -> &ToolSpec;
    fn run(&self, args: &Value) -> Result<Output, ToolError>;
    /// `run` for tools that can stop midway (a command, a long search): `cancel` fires on the
    /// turn's cancellation, the emergency stop and the timeout.
    fn run_cancellable(
        &self,
        args: &Value,
        _cancel: &CancellationToken,
    ) -> Result<Output, ToolError> {
        self.run(args)
    }
    /// The risk of this particular call (SECURITY §3): arguments can raise it (a protected
    /// folder, a destructive command) or, for input, a direct user request can lower it. The
    /// permission engine decides on this, never on the tool's own say-so at run time.
    fn assess(&self, _args: &Value, _initiator: kivo_core::tool::Initiator) -> Risk {
        self.spec().risk
    }
    /// A hard limit this call runs into (SECURITY §1.1, e.g. typing into a password field),
    /// checked before the permission engine so no one is ever asked to approve it; `run` checks
    /// again. Read-only lookups only.
    fn hard_limit(&self, _args: &Value) -> Result<(), ToolError> {
        Ok(())
    }
    /// What this call acts on (the app behind a window or element, a folder, a destination),
    /// for the hard limits and the confirmation card.
    fn targets(&self, _args: &Value) -> Vec<kivo_core::tool::Target> {
        Vec::new()
    }
    /// Takes back what `run` did, from the data it returned (UX §8.1). Only tools declared
    /// `Reversibility::Undoable` have one.
    fn undo(&self, _data: &Value) -> Result<Output, ToolError> {
        Err(ToolError::new(
            ToolErrorCode::Unsupported,
            text::t("error.cantUndo"),
        ))
    }
}

/// KIVO's tools: the built-in ones, fixed at start, and the ones that come and go with MCP
/// servers and connectors (`mcp.<server>.*`), replaced whenever a server's tools change.
#[derive(Default)]
pub struct Registry {
    tools: Vec<Arc<dyn Tool>>,
    live: std::sync::RwLock<Vec<Arc<dyn Tool>>>,
}

impl Registry {
    pub fn new(tools: Vec<Arc<dyn Tool>>) -> Self {
        Self {
            tools,
            live: std::sync::RwLock::default(),
        }
    }

    /// Replaces the live tools whose id starts with `prefix` (`mcp.github.`) with `tools`. A
    /// live tool can never replace a built-in one: ids already taken are dropped.
    pub fn set_live(&self, prefix: &str, tools: Vec<Arc<dyn Tool>>) {
        let mut live = self
            .live
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        live.retain(|t| !t.spec().id.starts_with(prefix));
        for t in tools {
            let id = &t.spec().id;
            if id.starts_with(prefix)
                && !self.tools.iter().any(|b| &b.spec().id == id)
                && !live.iter().any(|l| &l.spec().id == id)
            {
                live.push(t);
            }
        }
    }

    /// Every tool now, built-in first.
    fn every(&self) -> Vec<Arc<dyn Tool>> {
        let live = self
            .live
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.tools.iter().chain(live.iter()).cloned().collect()
    }

    /// Specs of the tools that exist with these capability settings (CAP-01).
    pub fn available(&self, capabilities: &CapabilitySettings) -> impl Iterator<Item = ToolSpec> {
        let platform = current_platform();
        let caps = capabilities.clone();
        self.every()
            .into_iter()
            .map(|t| t.spec().clone())
            .filter(move |s| caps.enabled(s.capability) && s.platforms.contains(&platform))
    }

    /// A registered tool (`None` if unknown, not for this OS, or its capability is off).
    pub fn get(&self, id: &str, capabilities: &CapabilitySettings) -> Option<Arc<dyn Tool>> {
        self.every().into_iter().find(|t| {
            t.spec().id == id
                && capabilities.enabled(t.spec().capability)
                && t.spec().platforms.contains(&current_platform())
        })
    }

    /// Every tool, whatever the capabilities.
    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.every()
    }

    /// Every tool's spec, whatever the capabilities (the routine builder's catalog).
    pub fn all_specs(&self) -> Vec<ToolSpec> {
        self.every().iter().map(|t| t.spec().clone()).collect()
    }

    /// The spec of any known tool, registered or not (to explain *why* it isn't available: CAP-02).
    pub fn known(&self, id: &str) -> Option<ToolSpec> {
        self.every()
            .iter()
            .map(|t| t.spec())
            .find(|s| s.id == id)
            .cloned()
    }
}

fn current_platform() -> kivo_core::tool::Platform {
    if cfg!(windows) {
        kivo_core::tool::Platform::Windows
    } else if cfg!(target_os = "macos") {
        kivo_core::tool::Platform::MacOs
    } else {
        kivo_core::tool::Platform::Linux
    }
}

/// Runs a permitted call with its timeout and cancellation.
pub async fn execute(
    tool: Arc<dyn Tool>,
    call: &ToolCall,
    permit: Permit,
    cancel: &CancellationToken,
) -> (ToolResult, Option<Output>) {
    let args = call.args.clone();
    // The tool's own token: cancelled with the turn, and on timeout, so a blocking tool that
    // watches it (a command) stops instead of running on unseen.
    let own = cancel.child_token();
    let stop = own.clone();
    let _guard = own.clone().drop_guard();
    run_permitted(tool, call, permit, cancel, move |t| {
        t.run_cancellable(&args, &stop)
    })
    .await
}

/// Undoes an earlier run of `tool` from the data it returned. The undo is a call of its own
/// (`call`: the same tool, `{"undo": <data>}`), authorized like any other.
pub async fn undo(
    tool: Arc<dyn Tool>,
    call: &ToolCall,
    permit: Permit,
    cancel: &CancellationToken,
) -> (ToolResult, Option<Output>) {
    let data = call.args["undo"].clone();
    run_permitted(tool, call, permit, cancel, move |t| t.undo(&data)).await
}

async fn run_permitted(
    tool: Arc<dyn Tool>,
    call: &ToolCall,
    permit: Permit,
    cancel: &CancellationToken,
    work: impl FnOnce(&dyn Tool) -> Result<Output, ToolError> + Send + 'static,
) -> (ToolResult, Option<Output>) {
    let started = Instant::now();
    let finish = |status: Result<Value, ToolError>, output: Option<Output>| {
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let provenance = if output.as_ref().is_some_and(|o| o.source.is_some()) {
            Provenance::Untrusted
        } else {
            Provenance::System
        };
        (
            ToolResult {
                status,
                provenance,
                duration_ms,
            },
            output,
        )
    };
    if !permit.covers(call) || tool.spec().id != call.tool {
        // A permit for another call: a bug, never user-reachable. Refuse.
        tracing::error!(
            tool = call.tool,
            permit = permit.tool(),
            "permit does not match the call"
        );
        return finish(
            Err(ToolError::new(
                ToolErrorCode::AccessDenied,
                text::t("error.notApproved"),
            )),
            None,
        );
    }
    let timeout = Duration::from_millis(tool.spec().timeout_ms);
    let runner = Arc::clone(&tool);
    let job = tokio::task::spawn_blocking(move || work(runner.as_ref()));
    let outcome = tokio::select! {
        joined = job => match joined {
            Ok(result) => result,
            Err(e) => Err(ToolError::new(ToolErrorCode::Failed, text::t("error.failed")).with_detail(e.to_string())),
        },
        () = tokio::time::sleep(timeout) => Err(ToolError::new(ToolErrorCode::Timeout, text::t("error.timeout"))),
        () = cancel.cancelled() => Err(ToolError::new(ToolErrorCode::Cancelled, text::t("reply.cancelled"))),
    };
    match outcome {
        Ok(output) => finish(Ok(output.data.clone()), Some(output)),
        Err(e) => {
            if let Some(detail) = &e.detail {
                tracing::warn!(tool = call.tool, code = ?e.code, detail, "tool failed");
            }
            finish(Err(e), None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_core::capability::Capability;
    use kivo_core::config::PermissionMode;
    use kivo_core::tool::{CapabilityTier, Initiator, Platform, Reversibility, Risk, SideEffect};
    use kivo_security::{Context, Decision, HardLimits, SessionKind, Taint, authorize};
    use serde_json::json;

    struct Slow {
        spec: ToolSpec,
        delay: Duration,
    }

    impl Tool for Slow {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        fn run(&self, _args: &Value) -> Result<Output, ToolError> {
            std::thread::sleep(self.delay);
            Ok(Output::new("Done.", json!({"ok": true})))
        }
    }

    fn spec(id: &str, capability: Capability, timeout_ms: u64) -> ToolSpec {
        ToolSpec {
            id: id.into(),
            description: String::new(),
            title: "Test".into(),
            params: json!({}),
            result: json!({}),
            risk: Risk::Low,
            side_effects: vec![SideEffect::LocalWrite],
            data_egress: false,
            timeout_ms,
            cancellable: true,
            tier: CapabilityTier::OsApi,
            platforms: vec![Platform::Windows, Platform::MacOs, Platform::Linux],
            reversibility: Reversibility::NotApplicable,
            capability,
        }
    }

    fn call(id: &str, tool: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            tool: tool.into(),
            args: json!({}),
            initiated_by: Initiator::UserDirect,
            targets: Vec::new(),
        }
    }

    fn permit(tool: &Arc<dyn Tool>, call: &ToolCall) -> Permit {
        let caps = CapabilitySettings::default();
        let limits = HardLimits::default();
        let tools = kivo_core::config::Tools::default();
        let cx = Context {
            mode: PermissionMode::Auto,
            session: SessionKind::Owner,
            taint: Taint::Clean,
            capabilities: &caps,
            limits: &limits,
            grants: &[],
            tools: &tools,
            privacy: kivo_core::config::PrivacyMode::Cloud,
            assessed: None,
            task_grants: None,
        };
        match authorize(tool.spec(), call, &cx) {
            Decision::Allow(p) => p,
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn live_tools_come_and_go_and_never_replace_built_in_ones() {
        let slow = |id: &str| -> Arc<dyn Tool> {
            Arc::new(Slow {
                spec: spec(id, Capability::AppsAndWindows, 100),
                delay: Duration::ZERO,
            })
        };
        let registry = Registry::new(vec![slow("mcp.x.taken")]);
        let caps = CapabilitySettings::default();
        registry.set_live(
            "mcp.x.",
            vec![slow("mcp.x.a"), slow("mcp.x.taken"), slow("apps.launch")],
        );
        let ids: Vec<String> = registry.available(&caps).map(|s| s.id).collect();
        assert_eq!(
            ids,
            ["mcp.x.taken", "mcp.x.a"],
            "no duplicates, nothing outside its prefix"
        );
        assert!(registry.get("mcp.x.a", &caps).is_some());
        registry.set_live("mcp.x.", Vec::new());
        assert!(registry.get("mcp.x.a", &caps).is_none());
        assert_eq!(registry.all_specs().len(), 1);
    }

    #[tokio::test]
    async fn tools_of_a_disabled_capability_are_not_registered() {
        let registry = Registry::new(vec![
            Arc::new(Slow {
                spec: spec("a.x", Capability::AppsAndWindows, 100),
                delay: Duration::ZERO,
            }),
            Arc::new(Slow {
                spec: spec("s.x", Capability::Shell, 100),
                delay: Duration::ZERO,
            }),
        ]);
        let mut caps = CapabilitySettings::default();
        let ids =
            |caps: &CapabilitySettings| registry.available(caps).map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(&caps), ["a.x"], "shell is off by default");
        assert!(registry.get("s.x", &caps).is_none());
        assert!(
            registry.known("s.x").is_some(),
            "known, to explain why it's unavailable"
        );
        caps.set(Capability::AppsAndWindows, false);
        assert!(ids(&caps).is_empty());
    }

    #[tokio::test]
    async fn an_undo_is_a_permitted_call_of_its_own() {
        let tool: Arc<dyn Tool> = Arc::new(Slow {
            spec: spec("a.x", Capability::AppsAndWindows, 1000),
            delay: Duration::ZERO,
        });
        let mut undo_call = call("c2", "a.x");
        undo_call.args = json!({ "undo": { "ok": true } });
        let p = permit(&tool, &undo_call);
        let (result, _) = undo(Arc::clone(&tool), &undo_call, p, &CancellationToken::new()).await;
        // `Slow` declares no undo: the executor reports that plainly.
        assert_eq!(result.status.unwrap_err().code, ToolErrorCode::Unsupported);
        let other = call("c3", "a.x");
        let wrong = permit(&tool, &other);
        let (result, _) = undo(tool, &undo_call, wrong, &CancellationToken::new()).await;
        assert_eq!(
            result.status.unwrap_err().code,
            ToolErrorCode::AccessDenied,
            "a permit for another call doesn't cover an undo"
        );
    }

    #[tokio::test]
    async fn a_permit_runs_only_its_own_call() {
        let tool: Arc<dyn Tool> = Arc::new(Slow {
            spec: spec("a.x", Capability::AppsAndWindows, 1000),
            delay: Duration::ZERO,
        });
        let first = call("c1", "a.x");
        let p = permit(&tool, &first);
        let (result, output) = execute(
            Arc::clone(&tool),
            &call("c2", "a.x"),
            p,
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(result.status.unwrap_err().code, ToolErrorCode::AccessDenied);
        assert!(output.is_none());
        let p = permit(&tool, &first);
        let (result, output) = execute(tool, &first, p, &CancellationToken::new()).await;
        assert_eq!(result.status.unwrap(), json!({"ok": true}));
        assert_eq!(output.unwrap().say, "Done.");
    }

    #[tokio::test]
    async fn timeouts_and_cancellation_end_the_wait_promptly() {
        let slow: Arc<dyn Tool> = Arc::new(Slow {
            spec: spec("a.x", Capability::AppsAndWindows, 50),
            delay: Duration::from_secs(2),
        });
        let c = call("c1", "a.x");
        let (result, _) = execute(
            Arc::clone(&slow),
            &c,
            permit(&slow, &c),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(result.status.unwrap_err().code, ToolErrorCode::Timeout);

        let slow: Arc<dyn Tool> = Arc::new(Slow {
            spec: spec("a.x", Capability::AppsAndWindows, 10_000),
            delay: Duration::from_secs(2),
        });
        let cancel = CancellationToken::new();
        let canceller = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            canceller.cancel();
        });
        let started = Instant::now();
        let (result, _) = execute(Arc::clone(&slow), &c, permit(&slow, &c), &cancel).await;
        assert_eq!(result.status.unwrap_err().code, ToolErrorCode::Cancelled);
        assert!(
            started.elapsed() < Duration::from_millis(100 + 20),
            "cancel reached the tool layer in ≤ 100 ms"
        );
    }
}
