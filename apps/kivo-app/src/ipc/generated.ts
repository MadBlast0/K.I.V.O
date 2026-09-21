// Generated from the Rust IPC types by crates/kivo-ipc/tests/ts_bindings.rs. Do not edit.
// Regenerate: KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings

export type TurnId = string;

export type TaskId = string;

export type TraceId = string;

export type ProfileId = string;

export type Timestamp = { 
/**
 * Microseconds since this process started. Monotonic: use it for latency, never for display.
 */
monoUs: number, 
/**
 * Milliseconds since the Unix epoch (UTC). For display and storage.
 */
wallMs: number, };

export type Event = { meta: EventMeta, } & ({ "group": "voice", "event": VoiceEvent } | { "group": "turn", "event": TurnEvent } | { "group": "tool", "event": ToolEvent } | { "group": "task", "event": TaskEvent } | { "group": "system", "event": SystemEvent } | { "group": "provider", "event": ProviderEvent } | { "group": "ui", "event": UiEvent });

export type EventMeta = { ts: Timestamp, turnId?: TurnId | null, taskId?: TaskId | null, traceId: TraceId, };

export type EventKind = { "group": "voice", "event": VoiceEvent } | { "group": "turn", "event": TurnEvent } | { "group": "tool", "event": ToolEvent } | { "group": "task", "event": TaskEvent } | { "group": "system", "event": SystemEvent } | { "group": "provider", "event": ProviderEvent } | { "group": "ui", "event": UiEvent };

export type VoiceEvent = { "type": "wakeDetected", wordId: string, score: number, } | { "type": "speechStarted" } | { "type": "partialTranscript", text: string, stableLen: number, } | { "type": "finalTranscript", text: string, confidence: number | null, } | { "type": "speechEnded" } | { "type": "ttsStarted" } | { "type": "ttsStopped", reason: TtsStopReason, } | { "type": "bargeIn" };

export type TtsStopReason = "finished" | "cancelled" | "bargeIn";

export type TurnEvent = { "type": "started", source: TurnSource, } | { "type": "intentDetected", path: IntentPath, intent: string, } | { "type": "brainSelected", profile: string, provider: string, model: string, reason: string, } | { "type": "completed" } | { "type": "cancelled", reason: CancelReason, } | { "type": "failed", message: string, };

export type TurnSource = "wakeWord" | "pushToTalk" | "typed" | "followUp" | "routine";

export type IntentPath = "fastPath" | "brain" | "eventTask";

export type CancelReason = "userVoice" | "userButton" | "hotkey" | "emergencyStop" | "bargeIn" | "timeout" | "providerFailure" | "shutdown";

export type ToolEvent = { "type": "requested", callId: string, toolId: string, } | { "type": "permissionDecided", callId: string, decision: PermissionDecision, } | { "type": "started", callId: string, } | { "type": "progress", callId: string, message: string, } | { "type": "completed", callId: string, } | { "type": "failed", callId: string, message: string, };

export type PermissionDecision = "allow" | "confirm" | "deny";

export type TaskEvent = { "type": "created", name: string, } | { "type": "stepChanged", step: string, status: StepStatus, } | { "type": "waiting", reason: string, } | { "type": "completed" } | { "type": "cancelled", reason: CancelReason, } | { "type": "failed", message: string, };

export type StepStatus = "pending" | "running" | "done" | "failed" | "skipped";

export type SystemEvent = { "type": "windowChanged", app: string, title: string, } | { "type": "fullscreenChanged", fullscreen: boolean, } | { "type": "focusModeChanged", on: boolean, } | { "type": "powerChanged", onBattery: boolean, percent: number | null, } | { "type": "deviceChanged", kind: DeviceKind, name: string, } | { "type": "networkChanged", online: boolean, } | { "type": "fileChanged", path: string, } | { "type": "browserChanged", url: string, title: string, };

export type DeviceKind = "microphone" | "speaker";

export type ProviderEvent = { "type": "healthChanged", provider: string, healthy: boolean, } | { "type": "rateLimited", provider: string, retryAfterMs: number | null, } | { "type": "authFailed", provider: string, };

export type UiEvent = { "type": "overlayShown" } | { "type": "overlayHidden" } | { "type": "userConfirmed", callId: string, } | { "type": "userCancelled" };

export type SessionState = "idle" | "listening" | "thinking" | "acting" | "speaking" | "followUp" | "interrupted" | "paused" | "awaitingConfirmation" | "error";

export type StateSnapshot = { session: SessionState, 
/**
 * Increases with every state change, so a client can tell whether its view is current.
 */
revision: number, };

export type ProtocolVersion = { major: number, minor: number, };

export type Welcome = { protocolVersion: ProtocolVersion, runtimeVersion: string, snapshot: StateSnapshot, };

export type RpcError = { code: number, message: string, };
