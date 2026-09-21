//! TypeScript types for the UI, generated from the Rust IPC types (ARCHITECTURE §3, ARCH-18).
//!
//! Check (CI):   cargo test -p kivo-ipc --features ts --test ts_bindings
//! Regenerate:   KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings
#![cfg(feature = "ts")]

use kivo_core::capability::{Badge, Capability};
use kivo_core::config::PermissionMode;
use kivo_core::event::{
    CancelReason, DeviceKind, EventKind, IntentPath, PermissionDecision, ProviderEvent, StepStatus,
    SystemEvent, TaskEvent, ToolEvent, TtsStopReason, TurnEvent, TurnSource, UiEvent, VoiceEvent,
};
use kivo_core::tool::{ConfirmSpec, ConfirmedBy, Risk, Strength};
use kivo_core::{Event, EventMeta, ProfileId, SessionState, TaskId, Timestamp, TraceId, TurnId};
use kivo_ipc::infer::Residency;
use kivo_ipc::protocol::{ProtocolVersion, SpeechStatus, StepView, TurnView};
use kivo_ipc::{Link, LinkStatus, RpcError, StateSnapshot, Welcome, method};
use std::path::PathBuf;
use ts_rs::{Config, TS};

fn render() -> String {
    // JSON numbers, not bigint: every 64-bit integer KIVO sends fits in a double.
    let cfg = Config::new().with_large_int("number");
    let declarations = [
        // identifiers and time
        TurnId::decl(&cfg),
        TaskId::decl(&cfg),
        TraceId::decl(&cfg),
        ProfileId::decl(&cfg),
        Timestamp::decl(&cfg),
        // events
        Event::decl(&cfg),
        EventMeta::decl(&cfg),
        EventKind::decl(&cfg),
        VoiceEvent::decl(&cfg),
        TtsStopReason::decl(&cfg),
        TurnEvent::decl(&cfg),
        TurnSource::decl(&cfg),
        IntentPath::decl(&cfg),
        CancelReason::decl(&cfg),
        ToolEvent::decl(&cfg),
        PermissionDecision::decl(&cfg),
        TaskEvent::decl(&cfg),
        StepStatus::decl(&cfg),
        SystemEvent::decl(&cfg),
        DeviceKind::decl(&cfg),
        ProviderEvent::decl(&cfg),
        UiEvent::decl(&cfg),
        // session and protocol
        SessionState::decl(&cfg),
        PermissionMode::decl(&cfg),
        // what KIVO is doing now
        TurnView::decl(&cfg),
        StepView::decl(&cfg),
        SpeechStatus::decl(&cfg),
        Residency::decl(&cfg),
        // permissions and capabilities
        Risk::decl(&cfg),
        Strength::decl(&cfg),
        ConfirmedBy::decl(&cfg),
        ConfirmSpec::decl(&cfg),
        Capability::decl(&cfg),
        Badge::decl(&cfg),
        StateSnapshot::decl(&cfg),
        ProtocolVersion::decl(&cfg),
        Welcome::decl(&cfg),
        RpcError::decl(&cfg),
        // the app's connection to the runtime
        LinkStatus::decl(&cfg),
        Link::decl(&cfg),
    ];
    let mut out = String::from(
        "// Generated from the Rust IPC types by crates/kivo-ipc/tests/ts_bindings.rs. Do not edit.\n\
         // Regenerate: KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings\n",
    );
    for decl in declarations {
        out.push_str("\nexport ");
        out.push_str(&decl);
        out.push('\n');
    }
    // The requests the UI sends through the app (`runtime_request`).
    let methods = [
        ("sessionPause", method::SESSION_PAUSE),
        ("sessionResume", method::SESSION_RESUME),
        ("runtimeQuit", method::RUNTIME_QUIT),
        ("permissionsSetMode", method::PERMISSIONS_SET_MODE),
        ("sessionTalk", method::SESSION_TALK),
        ("sessionSay", method::SESSION_SAY),
        ("sessionCancel", method::SESSION_CANCEL),
        ("sessionStopEverything", method::SESSION_STOP_ALL),
        ("permissionsAnswer", method::PERMISSIONS_ANSWER),
        ("permissionsGrants", method::PERMISSIONS_GRANTS),
        ("permissionsRevoke", method::PERMISSIONS_REVOKE),
        ("capabilitiesGet", method::CAPABILITIES_GET),
        ("capabilitiesSet", method::CAPABILITIES_SET),
        ("activityList", method::ACTIVITY_LIST),
        ("auditList", method::AUDIT_LIST),
        ("modelsList", method::MODELS_LIST),
        ("modelsInstall", method::MODELS_INSTALL),
        ("modelsRemove", method::MODELS_REMOVE),
        ("settingsGet", method::SETTINGS_GET),
        ("settingsSet", method::SETTINGS_SET),
    ];
    out.push_str("\n/** IPC methods the UI can call. */\nexport const Method = {\n");
    for (name, value) in methods {
        out.push_str(&format!("  {name}: \"{value}\",\n"));
    }
    out.push_str("} as const;\n\nexport type Method = (typeof Method)[keyof typeof Method];\n");
    out
}

#[test]
fn generated_typescript_is_up_to_date() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/kivo-app/src/ipc/generated.ts");
    let fresh = render();
    if std::env::var_os("KIVO_WRITE_TS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert!(
        current == fresh,
        "apps/kivo-app/src/ipc/generated.ts is stale; regenerate with:\n  \
         KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings"
    );
}
