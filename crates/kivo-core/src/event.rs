//! The typed event model (ARCHITECTURE §4.1). Every event carries metadata; the payload is one of
//! the groups below. Events cross the IPC boundary as JSON:
//!
//! ```json
//! { "meta": { "ts": {…}, "turnId": "…", "traceId": "…" },
//!   "group": "voice", "event": { "type": "partialTranscript", "text": "open chr" } }
//! ```

use crate::ids::{TaskId, TraceId, TurnId};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// Something that happened, with who/when metadata.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub meta: EventMeta,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    /// An event stamped now, with a fresh trace id.
    pub fn new(kind: EventKind) -> Self {
        Self {
            meta: EventMeta::now(),
            kind,
        }
    }

    pub fn in_turn(mut self, turn: TurnId) -> Self {
        self.meta.turn_id = Some(turn);
        self
    }

    pub fn in_task(mut self, task: TaskId) -> Self {
        self.meta.task_id = Some(task);
        self
    }

    pub fn traced(mut self, trace: TraceId) -> Self {
        self.meta.trace_id = trace;
        self
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMeta {
    pub ts: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    pub trace_id: TraceId,
}

impl EventMeta {
    pub fn now() -> Self {
        Self {
            ts: Timestamp::now(),
            turn_id: None,
            task_id: None,
            trace_id: TraceId::new(),
        }
    }
}

/// The event groups of ARCHITECTURE §4.1.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "group", content = "event", rename_all = "camelCase")]
pub enum EventKind {
    Voice(VoiceEvent),
    Turn(TurnEvent),
    Tool(ToolEvent),
    Task(TaskEvent),
    System(SystemEvent),
    Provider(ProviderEvent),
    Ui(UiEvent),
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum VoiceEvent {
    WakeDetected {
        word_id: String,
        score: f32,
    },
    SpeechStarted,
    /// Live, may still change. `stable_len` bytes at the start will no longer change (plan §31).
    PartialTranscript {
        text: String,
        stable_len: usize,
    },
    FinalTranscript {
        text: String,
        confidence: Option<f32>,
    },
    SpeechEnded,
    TtsStarted,
    TtsStopped {
        reason: TtsStopReason,
    },
    BargeIn,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TtsStopReason {
    Finished,
    Cancelled,
    BargeIn,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TurnEvent {
    Started {
        source: TurnSource,
    },
    IntentDetected {
        path: IntentPath,
        intent: String,
    },
    /// `reason` is the one-line explanation the card shows (plan §145).
    BrainSelected {
        profile: String,
        provider: String,
        model: String,
        reason: String,
    },
    Completed,
    Cancelled {
        reason: CancelReason,
    },
    /// `message` is user-readable (plan §146); technical detail goes to the log.
    Failed {
        message: String,
    },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnSource {
    WakeWord,
    PushToTalk,
    Typed,
    FollowUp,
    Routine,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentPath {
    FastPath,
    Brain,
    EventTask,
}

/// Why a turn or task was cancelled (plan §43).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelReason {
    UserVoice,
    UserButton,
    Hotkey,
    EmergencyStop,
    BargeIn,
    Timeout,
    ProviderFailure,
    Shutdown,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ToolEvent {
    Requested {
        call_id: String,
        tool_id: String,
    },
    PermissionDecided {
        call_id: String,
        decision: PermissionDecision,
    },
    Started {
        call_id: String,
    },
    Progress {
        call_id: String,
        message: String,
    },
    Completed {
        call_id: String,
    },
    Failed {
        call_id: String,
        message: String,
    },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionDecision {
    Allow,
    Confirm,
    Deny,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TaskEvent {
    Created { name: String },
    StepChanged { step: String, status: StepStatus },
    Waiting { reason: String },
    Completed,
    Cancelled { reason: CancelReason },
    Failed { message: String },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepStatus {
    Pending,
    Running,
    Done,
    Failed,
    Skipped,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SystemEvent {
    WindowChanged {
        app: String,
        title: String,
    },
    FullscreenChanged {
        fullscreen: bool,
    },
    FocusModeChanged {
        on: bool,
    },
    PowerChanged {
        on_battery: bool,
        percent: Option<u8>,
    },
    DeviceChanged {
        kind: DeviceKind,
        name: String,
    },
    NetworkChanged {
        online: bool,
    },
    FileChanged {
        path: String,
    },
    BrowserChanged {
        url: String,
        title: String,
    },
    /// The runtime is quitting (tray or Control Center "Quit KIVO"); UIs close too (UX §1).
    ShuttingDown,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceKind {
    Microphone,
    Speaker,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ProviderEvent {
    HealthChanged {
        provider: String,
        healthy: bool,
    },
    RateLimited {
        provider: String,
        retry_after_ms: Option<u64>,
    },
    AuthFailed {
        provider: String,
    },
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UiEvent {
    /// Show the Control Center, optionally on a page ("settings", "activity", …): tray clicks,
    /// "Open KIVO", or a second launch.
    ControlCenterRequested {
        page: Option<String>,
    },
    OverlayShown,
    OverlayHidden,
    UserConfirmed {
        call_id: String,
    },
    UserCancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_to_the_documented_shape() {
        let turn = TurnId::new();
        let e = Event::new(EventKind::Voice(VoiceEvent::PartialTranscript {
            text: "open chr".into(),
            stable_len: 5,
        }))
        .in_turn(turn);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["group"], "voice");
        assert_eq!(
            v["event"],
            json!({ "type": "partialTranscript", "text": "open chr", "stableLen": 5 })
        );
        assert_eq!(v["meta"]["turnId"], turn.to_string());
        assert!(v["meta"].get("taskId").is_none(), "absent ids are omitted");
        assert!(v["meta"]["ts"]["monoUs"].is_u64());
    }

    #[test]
    fn round_trips_every_group() {
        let events = [
            EventKind::Voice(VoiceEvent::TtsStopped {
                reason: TtsStopReason::BargeIn,
            }),
            EventKind::Turn(TurnEvent::Cancelled {
                reason: CancelReason::EmergencyStop,
            }),
            EventKind::Tool(ToolEvent::PermissionDecided {
                call_id: "c1".into(),
                decision: PermissionDecision::Confirm,
            }),
            EventKind::Task(TaskEvent::StepChanged {
                step: "run tests".into(),
                status: StepStatus::Running,
            }),
            EventKind::System(SystemEvent::PowerChanged {
                on_battery: true,
                percent: Some(42),
            }),
            EventKind::Provider(ProviderEvent::RateLimited {
                provider: "anthropic".into(),
                retry_after_ms: None,
            }),
            EventKind::Ui(UiEvent::UserConfirmed {
                call_id: "c1".into(),
            }),
        ];
        for kind in events {
            let e = Event::new(kind).in_task(TaskId::new());
            let back: Event = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
            assert_eq!(back, e);
        }
    }
}
