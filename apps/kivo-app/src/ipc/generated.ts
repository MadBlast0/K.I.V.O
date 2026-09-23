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
percent: number | null, installed: boolean, } | { "type": "modelResidency", engine: string, state: string, } | { "type": "speechFallback", slot: string, from: string, to: string | null, message: string, } | { "type": "engineSwitch", slot: string, engine: string, stage: string, message: string | null, };

export type DeviceKind = "microphone" | "speaker";

export type ProviderEvent = { "type": "healthChanged", provider: string, healthy: boolean, } | { "type": "rateLimited", provider: string, retryAfterMs: number | null, } | { "type": "authFailed", provider: string, } | { "type": "discoveryChanged", section: string, } | { "type": "usageRecorded", provider: string, cost: number | null, } | { "type": "threadChanged", thread: string, };

export type UiEvent = { "type": "controlCenterRequested", page: string | null, } | { "type": "overlayShown" } | { "type": "overlayHidden" } | { "type": "userConfirmed", callId: string, } | { "type": "userCancelled" };

export type SessionState = "idle" | "listening" | "thinking" | "acting" | "speaking" | "followUp" | "interrupted" | "paused" | "awaitingConfirmation" | "error";

export type PermissionMode = "ask" | "accept-edits" | "plan" | "auto" | "bypass";

export type SoundCue = "listen-start" | "listen-stop" | "done" | "error" | "thinking" | "hangup" | "question" | "approved" | "cancelled" | "notification";

export type SoundSet = "soft" | "glass" | "pulse" | "wood" | "minimal" | "custom";

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
anchor?: ScreenPoint, 
/**
 * Someone other than the enrolled owner is talking: a guest turn (UX-08, VOICE-22).
 */
guest: boolean, 
/**
 * Listening for a follow-up without the wake word for this many seconds from when it
 * appears (UX-45: the Island's ring counts down).
 */
followUp?: number, 
/**
 * KIVO is listening for a spoken answer to the decision (CONV-26): the mic ring and the
 * voice hints show.
 */
answering: boolean, 
/**
 * The user said "wait": the card stays, "Waiting for you", with no timeout (UX-08).
 */
waiting: boolean, 
/**
 * The brain answering and why (PLAN-17): the card's chip, with the reason on hover.
 */
brain?: BrainChip, };

export type BrainChip = { 
/**
 * The brain's name ("Claude Code").
 */
name: string, 
/**
 * The profile's name ("Coding").
 */
profile: string, 
/**
 * "Coding · Claude Code — because this looked like a coding task".
 */
reason: string, 
/**
 * It runs on this PC.
 */
local: boolean, 
/**
 * Estimated dollars so far; `None` when hidden or unknown (always an estimate).
 */
cost?: number, 
/**
 * A spending-limit warning ("80% of your daily limit").
 */
warning?: string, 
/**
 * Context used by this request, in tokens, and the budget (the context meter, UX-21).
 */
contextUsed: number, contextBudget: number, };

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
residency: string | null, 
/**
 * `notInstalled`, `downloading`, `installing`, `paused`, `ready`, `updateAvailable` or
 * `error` (UX-61).
 */
state: string, 
/**
 * Why the last download failed, in plain words.
 */
error?: string, 
/**
 * The speech engine KIVO uses now runs on this model.
 */
inUse: boolean, };

export type SpeechEngineItem = { id: string, name: string, 
/**
 * `stt` or `tts`.
 */
slot: string, 
/**
 * `recommended`, `lightweight`, `highAccuracy`, `multilingual`, `natural`, `expressive`.
 */
profiles: Array<string>, 
/**
 * `local`, `cloud` or `hybrid`.
 */
privacy: string, license: string, commercialUse: boolean, 
/**
 * BCP-47 primary tags; `*` for any.
 */
languages: Array<string>, streaming: boolean, 
/**
 * `cpu`, `directMl`, `cuda`, `npu`.
 */
devices: Array<string>, downloadMb: number, ramMb: number, 
/**
 * The model it downloads, if any.
 */
model: string | null, 
/**
 * Ready to use: its model is on this PC, or it needs none.
 */
ready: boolean, 
/**
 * It handles the primary language (VOICE-48).
 */
fitsLanguage: boolean, voices: Array<VoiceItem>, 
/**
 * KIVO's measurement on this PC; `None` reads "Not benchmarked by KIVO".
 */
measured: MeasuredItem | null, };

export type VoiceItem = { id: string, name: string, 
/**
 * `female`, `male` or empty.
 */
style: string, languages: Array<string>, };

export type MeasuredItem = { realTimeFactor: number | null, latencyMs: number | null, wordErrorRate: number | null, 
/**
 * How well it heard the owner's enrollment recordings (VOICE-23).
 */
voiceWordErrorRate?: number, 
/**
 * Unix milliseconds.
 */
measuredAt: number, };

export type ProfileItem = { slot: string, profile: string, 
/**
 * `None`: "Not available yet", or none for this language (`otherLanguagesOnly`).
 */
engine: string | null, otherLanguagesOnly: boolean, };

export type SpeechChoices = { 
/**
 * The primary language.
 */
language: string, engines: Array<SpeechEngineItem>, profiles: Array<ProfileItem>, 
/**
 * The recognizer KIVO listens with, and the voice engine and voice it speaks with.
 */
stt: string, tts: string, ttsVoice: string, };

export type RecommendationItem = { 
/**
 * `low`, `mid` or `high`.
 */
tier: string, sttEngine: string | null, sttFallback: string | null, ttsEngine: string, ttsFallback: string | null, threads: number, reason: string, };

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
  modelsPause: "models.pause",
  modelsCancel: "models.cancel",
  modelsSetDefault: "models.setDefault",
  settingsGet: "settings.get",
  settingsSet: "settings.set",
  wakeList: "wake.list",
  wakeCheck: "wake.check",
  wakeSave: "wake.save",
  wakeDelete: "wake.delete",
  wakeSet: "wake.set",
  wakeHear: "wake.hear",
  wakeTry: "wake.try",
  wakeSample: "wake.sample",
  wakeTune: "wake.tune",
  wakeFalseAlarms: "wake.falseAlarms",
  voiceIdStatus: "voiceId.status",
  voiceIdStart: "voiceId.start",
  voiceIdRecord: "voiceId.record",
  voiceIdFinish: "voiceId.finish",
  voiceIdCancel: "voiceId.cancel",
  voiceIdDelete: "voiceId.delete",
  soundsPreview: "sounds.preview",
  voiceEngines: "voice.engines",
  voiceRecommend: "voice.recommend",
  voiceSwitch: "voice.switch",
  voicePreview: "voice.preview",
  voiceMicCheck: "voice.micCheck",
  voiceDevices: "voice.devices",
  voiceAdvanced: "voice.advanced",
  voiceTrySample: "voice.trySample",
  voiceVocabulary: "voice.vocabulary",
  voiceAddWord: "voice.addWord",
  voiceRemoveWord: "voice.removeWord",
  brainsCatalog: "brains.catalog",
  brainsList: "brains.list",
  brainsCheck: "brains.check",
  brainsConnect: "brains.connect",
  brainsDisconnect: "brains.disconnect",
  brainsSetKey: "brains.setKey",
  brainsTestKey: "brains.testKey",
  brainsSignIn: "brains.signIn",
  brainsSetDefault: "brains.setDefault",
  brainsSaveProfile: "brains.saveProfile",
  brainsDeleteProfile: "brains.deleteProfile",
  brainsDiscovery: "brains.discovery",
  brainsRefresh: "brains.refresh",
  brainsViewed: "brains.viewed",
  brainsSetWorkspace: "brains.setWorkspace",
  brainsContext: "brains.context",
  usageSummary: "usage.summary",
  usageSetLimits: "usage.setLimits",
  usageSetCaps: "usage.setCaps",
  usageSetPrice: "usage.setPrice",
  usageExport: "usage.export",
  chatThreads: "chat.threads",
  chatThread: "chat.thread",
  chatNew: "chat.new",
  chatUpdate: "chat.update",
  chatDelete: "chat.delete",
  chatSend: "chat.send",
  chatCompact: "chat.compact",
  chatSearch: "chat.search",
  chatMisroute: "chat.misroute",
  memoryPreferences: "memory.preferences",
  memorySetPreference: "memory.setPreference",
  memoryDeletePreference: "memory.deletePreference",
} as const;

export type Method = (typeof Method)[keyof typeof Method];
