//! The wire protocol (ARCHITECTURE §3): JSON-RPC 2.0 messages in length-prefixed frames.
//!
//! A connection starts with `hello` (carrying the session token and protocol version). The
//! reply is a full `StateSnapshot`; after that the runtime pushes `event` notifications, and a
//! fresh `snapshot` notification whenever the client may have missed events.

use kivo_core::config::PermissionMode;
use kivo_core::event::{StepStatus, TurnSource};
use kivo_core::tool::ConfirmSpec;
use kivo_core::{Event, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Bumped on breaking changes (major) and additions (minor). Clients with another major
/// version are refused with a clear error (for example after a partial update).
/// 1.1 added the permission mode and "Island hidden" to the snapshot, and `permissions.setMode`.
/// 1.2 added the live turn (transcript, steps, answer, confirmation), capabilities, Activity,
/// speech models and the requests the Island and the Control Center make.
pub const PROTOCOL_VERSION: ProtocolVersion = ProtocolVersion { major: 1, minor: 2 };

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

pub mod method {
    /// Client → runtime, first message: authenticate and get the initial state.
    pub const HELLO: &str = "hello";
    /// Client → runtime: a liveness check.
    pub const PING: &str = "ping";
    /// Client → runtime: the current state snapshot.
    pub const STATE: &str = "state.get";
    /// Runtime → client notification: one event from the bus.
    pub const EVENT: &str = "event";
    /// Runtime → client notification: a full snapshot (on every state change, and after events
    /// were missed).
    pub const SNAPSHOT: &str = "snapshot";
    /// Client → runtime: stop listening (the mic is released); only from Ready or a follow-up.
    pub const SESSION_PAUSE: &str = "session.pause";
    /// Client → runtime: listen again after a pause.
    pub const SESSION_RESUME: &str = "session.resume";
    /// Client → runtime: quit KIVO (UX §1). The runtime announces `ShuttingDown`, then stops.
    pub const RUNTIME_QUIT: &str = "runtime.quit";
    /// Runtime → client notification: the microphone level, `{ "level": 0.0–1.0 }`, about 30 times
    /// a second while listening (ARCHITECTURE §3).
    pub const LEVELS: &str = "levels";
    /// Client → runtime: switch the permission mode (`{ "mode": "plan" }`). Only the user does
    /// this, from the UI or the hotkey, never a tool or voice alone (SECURITY §1.1).
    pub const PERMISSIONS_SET_MODE: &str = "permissions.setMode";
    /// Client → runtime: start listening, as the Talk button does (`{}`).
    pub const SESSION_TALK: &str = "session.talk";
    /// Client → runtime: a typed request (`{ "text": "open chrome" }`, UX §8).
    pub const SESSION_SAY: &str = "session.say";
    /// Client → runtime: stop the current turn (Esc, the Island's Stop button).
    pub const SESSION_CANCEL: &str = "session.cancel";
    /// Client → runtime: stop everything (SEC-25).
    pub const SESSION_STOP_ALL: &str = "session.stopEverything";
    /// Client → runtime: answer a confirmation
    /// (`{ "callId": …, "answer": "allow" | "deny", "always": bool }`, SEC-10).
    pub const PERMISSIONS_ANSWER: &str = "permissions.answer";
    /// Client → runtime: the capability toggles (CAPABILITIES §1).
    pub const CAPABILITIES_GET: &str = "capabilities.get";
    /// Client → runtime: turn a capability on or off (`{ "capability": …, "on": bool }`).
    pub const CAPABILITIES_SET: &str = "capabilities.set";
    /// Client → runtime: a page of the Activity timeline (`{ "before": id?, "limit": n }`).
    pub const ACTIVITY_LIST: &str = "activity.list";
    /// Client → runtime: the speech models on this PC and what can be downloaded (DIST-12).
    pub const MODELS_LIST: &str = "models.list";
    /// Client → runtime: download a model (`{ "id": "moonshine-base-en" }`).
    pub const MODELS_INSTALL: &str = "models.install";
    /// Client → runtime: delete a downloaded model (`{ "id": … }`).
    pub const MODELS_REMOVE: &str = "models.remove";
    /// Client → runtime: the settings the Control Center edits.
    pub const SETTINGS_GET: &str = "settings.get";
    /// Client → runtime: change settings (merged into the current values).
    pub const SETTINGS_SET: &str = "settings.set";
    /// Client → runtime: the "always allow" grants in force (SEC-08).
    pub const PERMISSIONS_GRANTS: &str = "permissions.grants";
    /// Client → runtime: revoke one grant (`{ "id": 3 }`).
    pub const PERMISSIONS_REVOKE: &str = "permissions.revoke";
    /// Client → runtime: the audit log, newest first (`{ "limit": n }`, SECURITY §7).
    pub const AUDIT_LIST: &str = "audit.list";
    /// Client → runtime: the Control Center window was closed (`{}`). The reply says whether KIVO
    /// keeps running (`{ "keepRunning": true }`); the first time, the runtime shows the one-time
    /// notice (UX §1).
    pub const UI_WINDOW_CLOSED: &str = "ui.windowClosed";
    /// Runtime → client notification: a model download's progress.
    pub const MODEL_PROGRESS: &str = "models.progress";
    /// Wake words (VOICE §4): list, check a phrase, add or edit, delete, turn on/off or tune,
    /// hear it spoken, try it, record samples, tune from them, and a false-alarm test.
    pub const WAKE_LIST: &str = "wake.list";
    pub const WAKE_CHECK: &str = "wake.check";
    pub const WAKE_SAVE: &str = "wake.save";
    pub const WAKE_DELETE: &str = "wake.delete";
    pub const WAKE_SET: &str = "wake.set";
    pub const WAKE_HEAR: &str = "wake.hear";
    pub const WAKE_TRY: &str = "wake.try";
    pub const WAKE_SAMPLE: &str = "wake.sample";
    pub const WAKE_TUNE: &str = "wake.tune";
    pub const WAKE_FALSE_ALARMS: &str = "wake.falseAlarms";
    /// Your voice (VOICE §5): status, start (with consent), record a prompt, finish, cancel, and
    /// delete all voice data.
    pub const VOICE_ID_STATUS: &str = "voiceId.status";
    pub const VOICE_ID_START: &str = "voiceId.start";
    pub const VOICE_ID_RECORD: &str = "voiceId.record";
    pub const VOICE_ID_FINISH: &str = "voiceId.finish";
    pub const VOICE_ID_CANCEL: &str = "voiceId.cancel";
    pub const VOICE_ID_DELETE: &str = "voiceId.delete";
    /// Settings → Sounds: play a set's cues (VOICE-27).
    pub const SOUNDS_PREVIEW: &str = "sounds.preview";
    /// Choosing speech engines (VOICE §11): the registry, profiles and current choice; the
    /// recommendation; a safe switch (progress as `engineSwitch` events); a voice preview.
    pub const VOICE_ENGINES: &str = "voice.engines";
    pub const VOICE_RECOMMEND: &str = "voice.recommend";
    pub const VOICE_SWITCH: &str = "voice.switch";
    pub const VOICE_PREVIEW: &str = "voice.preview";
    /// Onboarding's microphone check (UX-33) and a recognizer's "Try sample" (UX-60).
    pub const VOICE_MIC_CHECK: &str = "voice.micCheck";
    pub const VOICE_TRY_SAMPLE: &str = "voice.trySample";
}

/// The name the desktop app gives in `hello`; the runtime supervises the client with this name.
pub const APP_CLIENT: &str = "kivo-app";

/// `hello` parameters.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hello {
    pub protocol_version: ProtocolVersion,
    /// Who is connecting, e.g. "kivo-app 0.1.0".
    pub client: String,
    /// The contents of the session token file.
    pub token: String,
}

/// `hello` result.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Welcome {
    pub protocol_version: ProtocolVersion,
    pub runtime_version: String,
    pub snapshot: StateSnapshot,
}

/// Everything a UI needs to render from scratch (ARCHITECTURE §3: full snapshot, then deltas).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub session: SessionState,
    /// How much KIVO may do without asking (SECURITY §1.1).
    pub mode: PermissionMode,
    /// "Hide Island for 1 hour" (UX §1): the Island shows nothing until this is cleared.
    pub island_hidden: bool,
    /// What KIVO is working on now: what it heard, what it is doing and what it will say
    /// (UX §2). `None` between turns.
    pub turn: Option<TurnView>,
    /// Whether speech recognition is ready, downloading or missing.
    pub speech: SpeechStatus,
    /// Another app owns the push-to-talk keys, so KIVO can't use them (VOICE-41). The Control
    /// Center offers a rebind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hotkey_conflict: Option<String>,
    /// Increases with every state change, so a client can tell whether its view is current.
    pub revision: u64,
}

/// The turn the Island is showing (UX §2 card contents).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnView {
    pub id: String,
    pub source: TurnSource,
    /// What KIVO has heard so far, or the typed request.
    pub transcript: String,
    /// The transcript won't change any more.
    pub transcript_final: bool,
    /// What KIVO is doing, in order.
    pub steps: Vec<StepView>,
    /// What KIVO says back.
    pub answer: Option<String>,
    /// Why it couldn't be done, in plain words.
    pub error: Option<String>,
    /// A decision waiting for the user (SEC-10).
    pub confirm: Option<ConfirmSpec>,
    /// The app the action is aimed at, for the Island's leading icon (UX §8.1).
    pub target_app: Option<String>,
    /// A capability this request needed that is off (CAP-02): the Island offers to turn it on.
    pub capability_off: Option<kivo_core::Capability>,
    /// A fullscreen app or Focus is on: the Island hides or shrinks to a dot, and KIVO only
    /// uses sounds (UX §2, UX-11).
    pub quiet: Option<QuietIsland>,
    /// The centre of the window in front when the request began (physical pixels): the Island
    /// appears on that window's monitor (UX-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub anchor: Option<ScreenPoint>,
    /// Someone other than the enrolled owner is talking: a guest turn (UX-08, VOICE-22).
    #[serde(default)]
    pub guest: bool,
    /// Listening for a follow-up without the wake word for this many seconds from when it
    /// appears (UX-45: the Island's ring counts down).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub follow_up: Option<u8>,
    /// KIVO is listening for a spoken answer to the decision (CONV-26): the mic ring and the
    /// voice hints show.
    #[serde(default)]
    pub answering: bool,
    /// The user said "wait": the card stays, "Waiting for you", with no timeout (UX-08).
    #[serde(default)]
    pub waiting: bool,
}

/// A point on the desktop, in physical pixels.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

/// How the Island behaves over a fullscreen app or during Focus (Settings → Island).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuietIsland {
    Hidden,
    Tiny,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    /// The tool call's id.
    pub id: String,
    /// What it does, in the user's words ("Open Google Chrome").
    pub title: String,
    pub status: StepStatus,
    /// A short result or reason ("Volume 30%", "Chrome isn't installed").
    pub detail: Option<String>,
}

/// Whether KIVO can hear: the speech model is a download (DIST-12).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum SpeechStatus {
    Ready,
    /// The speech model is downloading.
    Downloading {
        percent: u8,
    },
    /// It isn't downloaded yet.
    #[default]
    Missing,
    /// The engine failed; the message is safe to show.
    Failed {
        message: String,
    },
}

/// One entry of the Activity timeline (UX-20).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: i64,
    /// Unix milliseconds.
    pub ts: i64,
    pub turn_id: Option<String>,
    /// `transcript`, `tool`, `reply`, `setting`, `stop`.
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    /// `done`, `failed`, `cancelled`, `denied`, `unhandled`.
    pub status: String,
}

/// One row of the audit log (SECURITY §7, Activity → Audit).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditItem {
    pub ts: i64,
    pub turn_id: Option<String>,
    pub tool: String,
    pub args_summary: String,
    pub risk: String,
    pub decision: String,
    pub confirmed_by: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// An "always allow" grant (SEC-08).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantItem {
    pub id: i64,
    pub tool: String,
    pub scope: Option<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
}

/// A speech model KIVO can install (Voice → Models on this PC, DIST-13).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelItem {
    pub id: String,
    pub name: String,
    /// `stt`, `tts`, `vad`, `wake`, `embedding`.
    pub kind: String,
    pub license: String,
    pub attribution: String,
    pub source: String,
    pub languages: Vec<String>,
    /// Download size in bytes.
    pub size: u64,
    pub installed: bool,
    pub disk_bytes: u64,
    /// 0–100 while downloading.
    pub downloading: Option<u8>,
    /// Loaded or not right now (PLAN-02): `unloaded`, `warming`, `warm`, `active`, `idle`,
    /// `unloading`; `None` until the worker reports it.
    #[serde(default)]
    pub residency: Option<String>,
}

/// A speech engine in the registry, as the Voice page and onboarding show it (VOICE-42).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechEngineItem {
    pub id: String,
    pub name: String,
    /// `stt` or `tts`.
    pub slot: String,
    /// `recommended`, `lightweight`, `highAccuracy`, `multilingual`, `natural`, `expressive`.
    pub profiles: Vec<String>,
    /// `local`, `cloud` or `hybrid`.
    pub privacy: String,
    pub license: String,
    pub commercial_use: bool,
    /// BCP-47 primary tags; `*` for any.
    pub languages: Vec<String>,
    pub streaming: bool,
    /// `cpu`, `directMl`, `cuda`, `npu`.
    pub devices: Vec<String>,
    pub download_mb: u32,
    pub ram_mb: u32,
    /// The model it downloads, if any.
    pub model: Option<String>,
    /// Ready to use: its model is on this PC, or it needs none.
    pub ready: bool,
    /// It handles the primary language (VOICE-48).
    pub fits_language: bool,
    pub voices: Vec<VoiceItem>,
    /// KIVO's measurement on this PC; `None` reads "Not benchmarked by KIVO".
    pub measured: Option<MeasuredItem>,
}

/// A voice of a TTS engine.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceItem {
    pub id: String,
    pub name: String,
    /// `female`, `male` or empty.
    pub style: String,
    pub languages: Vec<String>,
}

/// KIVO's own benchmark of an engine on this PC.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredItem {
    pub real_time_factor: Option<f64>,
    pub latency_ms: Option<f64>,
    pub word_error_rate: Option<f64>,
    /// Unix milliseconds.
    pub measured_at: i64,
}

/// One curated profile and the engine behind it for the primary language (VOICE-43).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileItem {
    pub slot: String,
    pub profile: String,
    /// `None`: "Not available yet", or none for this language (`otherLanguagesOnly`).
    pub engine: Option<String>,
    pub other_languages_only: bool,
}

/// Everything the speech choosers need (`voice.engines`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechChoices {
    /// The primary language.
    pub language: String,
    pub engines: Vec<SpeechEngineItem>,
    pub profiles: Vec<ProfileItem>,
    /// The recognizer KIVO listens with, and the voice engine and voice it speaks with.
    pub stt: String,
    pub tts: String,
    pub tts_voice: String,
}

/// The recommendation for this PC (`voice.recommend`, VOICE-44).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationItem {
    /// `low`, `mid` or `high`.
    pub tier: String,
    pub stt_engine: Option<String>,
    pub stt_fallback: Option<String>,
    pub tts_engine: String,
    pub tts_fallback: Option<String>,
    pub threads: u32,
    pub reason: String,
}

/// A capability toggle (CAPABILITIES §1).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityItem {
    pub capability: kivo_core::Capability,
    pub label: String,
    pub enabled: bool,
    pub default: bool,
    pub badges: Vec<kivo_core::capability::Badge>,
}

/// A JSON-RPC 2.0 request id (numbers only; KIVO's clients never send strings).
pub type RequestId = u64;

/// One JSON-RPC message on the wire.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub jsonrpc: JsonRpcV2,
    pub method: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub jsonrpc: JsonRpcV2,
    pub id: RequestId,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Result(Value),
    Error(RpcError),
}

/// The literal `"2.0"`; anything else is rejected while parsing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsonRpcV2;

impl Serialize for JsonRpcV2 {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str("2.0")
    }
}

impl<'de> Deserialize<'de> for JsonRpcV2 {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = String::deserialize(d)?;
        if v == "2.0" {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("jsonrpc must be \"2.0\""))
        }
    }
}

/// A JSON-RPC error. `message` is safe to show the user.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} ({code})")]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcError {
    pub const PARSE: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL: i32 = -32603;
    /// Wrong or missing session token.
    pub const UNAUTHORIZED: i32 = -32001;
    /// Different protocol major version.
    pub const INCOMPATIBLE: i32 = -32002;
    /// The request is valid but does not fit the current state; the message says why.
    pub const REFUSED: i32 = -32010;
    /// A speech engine in `kivo-infer` failed; the message is safe to show and speak.
    pub const ENGINE: i32 = -32020;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn method_not_found(method: &str) -> Self {
        Self::new(Self::METHOD_NOT_FOUND, format!("unknown method {method}"))
    }

    pub fn invalid_params(detail: impl std::fmt::Display) -> Self {
        Self::new(
            Self::INVALID_PARAMS,
            format!("invalid parameters: {detail}"),
        )
    }
}

impl Request {
    pub fn new(id: RequestId, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            method: method.to_owned(),
            params,
        }
    }
}

impl Notification {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            method: method.to_owned(),
            params,
        }
    }

    /// An `event` notification carrying one bus event.
    pub fn event(event: &Event) -> Result<Self, serde_json::Error> {
        Ok(Self::new(method::EVENT, serde_json::to_value(event)?))
    }
}

impl Response {
    pub fn ok(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Result(result),
        }
    }

    pub fn err(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: JsonRpcV2,
            id,
            outcome: Outcome::Error(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Message, serde_json::Error> {
        serde_json::from_value(v)
    }

    #[test]
    fn parses_each_message_kind() {
        let req = parse(json!({"jsonrpc":"2.0","id":7,"method":"ping"})).unwrap();
        assert_eq!(req, Message::Request(Request::new(7, "ping", Value::Null)));
        let note = parse(json!({"jsonrpc":"2.0","method":"event","params":{"x":1}})).unwrap();
        assert!(matches!(note, Message::Notification(n) if n.method == "event"));
        let ok = parse(json!({"jsonrpc":"2.0","id":7,"result":"pong"})).unwrap();
        assert_eq!(ok, Message::Response(Response::ok(7, json!("pong"))));
        let err = parse(
            json!({"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"unknown method x"}}),
        )
        .unwrap();
        assert_eq!(
            err,
            Message::Response(Response::err(7, RpcError::method_not_found("x")))
        );
    }

    #[test]
    fn rejects_malformed_messages() {
        for bad in [
            json!({"jsonrpc":"1.0","id":1,"method":"ping"}),
            json!({"id":1,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":"a","method":"ping"}),
            json!({"jsonrpc":"2.0","id":1,"method":"ping","extra":true}),
            json!({"jsonrpc":"2.0","id":1}),
        ] {
            assert!(parse(bad.clone()).is_err(), "{bad}");
        }
    }

    #[test]
    fn serializes_responses_in_the_standard_shape() {
        let v = serde_json::to_value(Response::err(
            3,
            RpcError::new(RpcError::UNAUTHORIZED, "wrong token"),
        ))
        .unwrap();
        assert_eq!(
            v,
            json!({"jsonrpc":"2.0","id":3,"error":{"code":-32001,"message":"wrong token"}})
        );
    }
}
