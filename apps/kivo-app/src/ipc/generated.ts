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

export type SystemEvent = { "type": "windowChanged", app: string, title: string, } | { "type": "fullscreenChanged", fullscreen: boolean, } | { "type": "focusModeChanged", on: boolean, } | { "type": "powerChanged", onBattery: boolean, percent: number | null, } | { "type": "deviceChanged", kind: DeviceKind, name: string, } | { "type": "networkChanged", online: boolean, } | { "type": "fileChanged", path: string, } | { "type": "browserChanged", url: string, title: string, } | { "type": "shuttingDown" } | { "type": "modelChanged", id: string, 
/**
 * Download progress while downloading.
 */
percent: number | null, installed: boolean, } | { "type": "modelResidency", engine: string, state: string, };

export type DeviceKind = "microphone" | "speaker";

export type ProviderEvent = { "type": "healthChanged", provider: string, healthy: boolean, } | { "type": "rateLimited", provider: string, retryAfterMs: number | null, } | { "type": "authFailed", provider: string, };

export type UiEvent = { "type": "controlCenterRequested", page: string | null, } | { "type": "overlayShown" } | { "type": "overlayHidden" } | { "type": "userConfirmed", callId: string, } | { "type": "userCancelled" };

export type SessionState = "idle" | "listening" | "thinking" | "acting" | "speaking" | "followUp" | "interrupted" | "paused" | "awaitingConfirmation" | "error";

export type PermissionMode = "ask" | "accept-edits" | "plan" | "auto" | "bypass";

export type TurnView = { id: string, source: TurnSource, 
/**
 * What KIVO has heard so far, or the typed request.
 */
transcript: string, 
/**
 * The transcript won't change any more.
 */
transcriptFinal: boolean, 
/**
 * What KIVO is doing, in order.
 */
steps: Array<StepView>, 
/**
 * What KIVO says back.
 */
answer: string | null, 
/**
 * Why it couldn't be done, in plain words.
 */
error: string | null, 
/**
 * A decision waiting for the user (SEC-10).
 */
confirm: ConfirmSpec | null, 
/**
 * The app the action is aimed at, for the Island's leading icon (UX §8.1).
 */
targetApp: string | null, 
/**
 * A capability this request needed that is off (CAP-02): the Island offers to turn it on.
 */
capabilityOff: Capability | null, 
/**
 * A fullscreen app or Focus is on: the Island hides or shrinks to a dot, and KIVO only
 * uses sounds (UX §2, UX-11).
 */
quiet: QuietIsland | null, 
/**
 * The centre of the window in front when the request began (physical pixels): the Island
 * appears on that window's monitor (UX-06).
 */
anchor?: ScreenPoint, };

export type StepView = { 
/**
 * The tool call's id.
 */
id: string, 
/**
 * What it does, in the user's words ("Open Google Chrome").
 */
title: string, status: StepStatus, 
/**
 * A short result or reason ("Volume 30%", "Chrome isn't installed").
 */
detail: string | null, };

export type SpeechStatus = { "state": "ready" } | { "state": "downloading", percent: number, } | { "state": "missing" } | { "state": "failed", message: string, };

export type QuietIsland = "hidden" | "tiny";

export type ScreenPoint = { x: number, y: number, };

export type Residency = "unloaded" | "warming" | "warm" | "active" | "idle" | "unloading";

export type Risk = "safe" | "low" | "medium" | "high";

export type Strength = "normal" | "strong";

export type ConfirmedBy = "policy" | "grant" | "click" | "voice" | "hello";

export type ConfirmSpec = { callId: string, tool: string, 
/**
 * The exact action in plain words ("Shut down the computer").
 */
action: string, 
/**
 * What it acts on, if anything ("Google Chrome").
 */
target: string | null, 
/**
 * Why KIVO is asking.
 */
why: string, 
/**
 * Where the request came from ("You asked", "Requested after reading example.com").
 */
provenance: string, risk: Risk, strength: Strength, 
/**
 * Offer "Always for …" (not for High risk or guests).
 */
allowAlways: boolean, 
/**
 * The mode is Plan first: approving grants exactly this step.
 */
plan: boolean, };

export type Capability = "mic-listening" | "push-to-talk" | "speak-responses" | "apps-and-windows" | "system-controls" | "power-actions" | "files-read" | "files-modify" | "clipboard" | "browser-open-links" | "browser-pages" | "browser-autonomous" | "ui-automation" | "screen-awareness" | "computer-use" | "shell" | "background-tasks" | "routines" | "memory" | "cloud-brains" | "realtime-voice" | "cli-agents" | "mcp-servers" | "integrations" | "speaker-recognition" | "remote-access" | "notifications";

export type Badge = "local" | "cloud" | "costly" | "sensitive";

export type StateSnapshot = { session: SessionState, 
/**
 * How much KIVO may do without asking (SECURITY §1.1).
 */
mode: PermissionMode, 
/**
 * "Hide Island for 1 hour" (UX §1): the Island shows nothing until this is cleared.
 */
islandHidden: boolean, 
/**
 * What KIVO is working on now: what it heard, what it is doing and what it will say
 * (UX §2). `None` between turns.
 */
turn: TurnView | null, 
/**
 * Whether speech recognition is ready, downloading or missing.
 */
speech: SpeechStatus, 
/**
 * Another app owns the push-to-talk keys, so KIVO can't use them (VOICE-41). The Control
 * Center offers a rebind.
 */
hotkeyConflict?: string | null, 
/**
 * Increases with every state change, so a client can tell whether its view is current.
 */
revision: number, };

export type ActivityItem = { id: number, 
/**
 * Unix milliseconds.
 */
ts: number, turnId: string | null, 
/**
 * `transcript`, `tool`, `reply`, `setting`, `stop`.
 */
kind: string, title: string, detail: string | null, 
/**
 * `done`, `failed`, `cancelled`, `denied`, `unhandled`.
 */
status: string, };

export type AuditItem = { ts: number, turnId: string | null, tool: string, argsSummary: string, risk: string, decision: string, confirmedBy: string | null, result: string | null, error: string | null, };

export type GrantItem = { id: number, tool: string, scope: string | null, createdAt: number, expiresAt: number | null, };

export type ModelItem = { id: string, name: string, 
/**
 * `stt`, `tts`, `vad`, `wake`, `embedding`.
 */
kind: string, license: string, attribution: string, source: string, languages: Array<string>, 
/**
 * Download size in bytes.
 */
size: number, installed: boolean, diskBytes: number, 
/**
 * 0–100 while downloading.
 */
downloading: number | null, 
/**
 * Loaded or not right now (PLAN-02): `unloaded`, `warming`, `warm`, `active`, `idle`,
 * `unloading`; `None` until the worker reports it.
 */
residency: string | null, };

export type CapabilityItem = { capability: Capability, label: string, enabled: boolean, default: boolean, badges: Array<Badge>, };

export type ProtocolVersion = { major: number, minor: number, };

export type Welcome = { protocolVersion: ProtocolVersion, runtimeVersion: string, snapshot: StateSnapshot, };

export type RpcError = { code: number, message: string, };

export type LinkStatus = "connecting" | "connected" | "reconnecting" | "incompatible";

export type Link = { status: LinkStatus, 
/**
 * The connected runtime's version.
 */
runtimeVersion: string | null, 
/**
 * The last state received; kept while reconnecting so the UI can show what it last knew.
 */
snapshot: StateSnapshot | null, 
/**
 * Why the link is not connected, when there is something to say.
 */
message: string | null, };

/** IPC methods the UI can call. */
export const Method = {
  sessionPause: "session.pause",
  sessionResume: "session.resume",
  runtimeQuit: "runtime.quit",
  permissionsSetMode: "permissions.setMode",
  sessionTalk: "session.talk",
  sessionSay: "session.say",
  sessionCancel: "session.cancel",
  sessionStopEverything: "session.stopEverything",
  permissionsAnswer: "permissions.answer",
  permissionsGrants: "permissions.grants",
  permissionsRevoke: "permissions.revoke",
  capabilitiesGet: "capabilities.get",
  capabilitiesSet: "capabilities.set",
  activityList: "activity.list",
  auditList: "audit.list",
  modelsList: "models.list",
  modelsInstall: "models.install",
  modelsRemove: "models.remove",
  settingsGet: "settings.get",
  settingsSet: "settings.set",
} as const;

export type Method = (typeof Method)[keyof typeof Method];
