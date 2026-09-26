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

export type SystemEvent = { "type": "windowChanged", app: string, title: string, } | { "type": "fullscreenChanged", fullscreen: boolean, } | { "type": "focusModeChanged", on: boolean, } | { "type": "powerChanged", onBattery: boolean, percent: number | null, } | { "type": "deviceChanged", kind: DeviceKind, name: string, } | { "type": "networkChanged", online: boolean, } | { "type": "fileChanged", path: string, } | { "type": "browserChanged", url: string, title: string, } | { "type": "automation", kind: string, element: string, name: string, } | { "type": "shuttingDown" } | { "type": "modelChanged", id: string, 
/**
 * Download progress while downloading.
 */
percent: number | null, installed: boolean, } | { "type": "modelResidency", engine: string, state: string, } | { "type": "speechFallback", slot: string, from: string, to: string | null, message: string, } | { "type": "engineSwitch", slot: string, engine: string, stage: string, message: string | null, } | { "type": "discoveryChanged", section: string, } | { "type": "configChanged" } | { "type": "updateChanged" };

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
 * That app's icon as a `data:image/png` URL, when Windows has one (UX-46).
 */
targetIcon?: string | null, 
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
 * The bottom edge (physical y) of the title bar or tab strip of the window in front, when it
 * sits at the top of its monitor: while only listening, the Island moves below it (UX-14).
 */
titleBarBottom?: number | null, 
/**
 * A control KIVO is showing the user (UX-39, plan §141): the Island moves beside it on its
 * monitor without covering it.
 */
point?: PointTarget, 
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
brain?: BrainChip, 
/**
 * A line the card shows about what KIVO did with the user's data ("Sent a screenshot of
 * VS Code to Claude", CAP-08).
 */
note?: string | null, 
/**
 * The last change can be taken back (UX-43): the Island shows Undo with a ring until
 * `until` (milliseconds since the Unix epoch, about 8 s).
 */
undo?: UndoOffer | null, 
/**
 * A prompt KIVO will type into another AI, waiting for "send" (CONV-15).
 */
draft?: DraftView, 
/**
 * "What can I say?" (UX-44): examples for the app in front, then general ones.
 */
help?: Array<string>, 
/**
 * The task this turn started, for "Open in Tasks".
 */
taskId?: string, 
/**
 * A realtime conversation is open (BRAIN-33): the card's Live chip, its time and End.
 */
live?: LiveView, 
/**
 * A realtime conversation can be started from this card ("Talk live", BRAINS §8).
 */
liveOffer?: boolean, };

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

export type UndoOffer = { 
/**
 * What would be undone ("Move files to Archive").
 */
title: string, 
/**
 * When the Island stops offering it (epoch ms); voice and the toast still work after.
 */
until: number, };

export type DraftView = { 
/**
 * "Claude · terminal · K.I.V.O".
 */
target: string, text: string, };

export type LiveActivity = { id: string, 
/**
 * `timer`, `download`, `agent`, `media` or `task`.
 */
kind: string, title: string, detail: string | null, 
/**
 * 0–1, when known.
 */
progress: number | null, 
/**
 * When it ends (epoch ms), for a timer's countdown.
 */
until: number | null, taskId: string | null, };

export type Offer = { id: string, 
/**
 * `workspace`.
 */
kind: string, text: string, accept: string, decline: string, };

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
plan: boolean, 
/**
 * Windows Hello can confirm this (High risk on a PC with Hello set up, SEC-11).
 */
hello: boolean, 
/**
 * A computer-use step in watch mode (CAP-11): Allow / Skip, with the target highlighted.
 */
watch: boolean, };

export type GrantDuration = "once" | "session" | "day" | "always";

export type Preset = "minimal" | "balanced" | "power-user" | "custom";

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
 * Sensitive capabilities in use right now (CAP-06): `screen` (eye), `input` (hand),
 * `shell` (terminal). The tray and the Island show an indicator for each.
 */
inUse?: Array<string>, 
/**
 * Where the Island goes (UX-13): the setting and the spots it was dragged to.
 */
island?: IslandPlacement, 
/**
 * Ongoing status the collapsed Island shows (UX-15): a timer, a download, an agent.
 */
activities?: Array<LiveActivity>, 
/**
 * Tasks running or waiting now: the tray tooltip and Home count them (UX-56, UX-19).
 */
tasksActive?: number, 
/**
 * Bypass permissions is on until then (epoch ms; `u64::MAX`: until turned off), SEC-03.
 */
bypassUntil?: number, 
/**
 * Something the Island offers outside a turn ("Remember kivo as a workspace?").
 */
offer?: Offer, 
/**
 * Text was selected in the app in front when the text box opened: the Island offers
 * Explain · Rewrite · Translate (UX-42). The text itself stays in the runtime.
 */
hasSelection?: boolean, 
/**
 * KIVO is controlling the screen (computer use, CAP-12): the banner, the frame and KIVO's
 * cursor follow this.
 */
controlling?: ControlView, 
/**
 * Increases with every state change, so a client can tell whether its view is current.
 */
revision: number, };

export type IslandPlacement = { position: OverlayPosition, spots: Array<IslandSpot>, size: IslandSize, 
/**
 * The user's words as they speak.
 */
showTranscript: boolean, showUndo: boolean, 
/**
 * "Say approve or cancel" under questions.
 */
voiceHints: boolean, largeText: boolean, 
/**
 * What KIVO says is shown as text too.
 */
captions: boolean, 
/**
 * Narrator announcements (UX-53).
 */
announcements: boolean, motion: MotionPref, 
/**
 * Island (the pill), or Hidden: sounds only, except a question that needs an answer.
 */
companion: CompanionStyle, 
/**
 * Pill and card, pill only, card only, or off (UX-17).
 */
style: OverlayStyle, 
/**
 * A brief glow along the screen's edges on wake (UX-16; off by default).
 */
wakeGlow: boolean, };

export type IslandSpot = { 
/**
 * The monitor as Windows names it (`\\.\DISPLAY2`).
 */
monitor: string, x: number, y: number, };

export type OverlayPosition = "top-center" | "bottom-center" | "remember-drag";

export type IslandSize = "compact" | "standard" | "large";

export type ProductMode = "normal" | "private" | "offline" | "battery" | "performance" | "gaming" | "presentation";

export type OverlayStyle = "pill-and-card" | "pill-only" | "card-only" | "off";

export type MotionPref = "system" | "full" | "reduced";

export type CompanionStyle = "pill" | "orb" | "character" | "hidden";

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

export type GrantItem = { id: number, tool: string, scope: string | null, createdAt: number, expiresAt: number | null, 
/**
 * A `*` pattern the call's arguments must match (a folder, a command prefix).
 */
pattern: string | null, 
/**
 * Lasts only until KIVO restarts.
 */
sessionOnly: boolean, };

export type ModelItem = { id: string, name: string, 
/**
 * `stt`, `tts`, `vad`, `wake`, `embedding`, `speaker`, `gpuRuntime` (VOICE-50).
 */
kind: string, license: string, attribution: string, source: string, languages: Array<string>, 
/**
 * Download size in bytes.
 */
size: number, 
/**
 * Models it needs that install with it (voice activity, end of turn), by id.
 */
requires: Array<string>, installed: boolean, diskBytes: number, 
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
 * `cpu`, `vulkan`, `cuda`, `npu`.
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
style: string, languages: Array<string>, 
/**
 * `anime` for the Anime voices, empty for everyday ones.
 */
character: string, 
/**
 * Its files are on this PC (a voice added to a model after it was installed comes with the
 * model's update).
 */
ready: boolean, };

export type MeasuredItem = { realTimeFactor: number | null, latencyMs: number | null, wordErrorRate: number | null, 
/**
 * How well it heard the owner's enrollment recordings (VOICE-23).
 */
voiceWordErrorRate?: number, 
/**
 * With background noise added (STT, BENCH-15).
 */
noisyWordErrorRate?: number, 
/**
 * Cancel → silence (TTS, BENCH-15).
 */
cancelMs?: number, 
/**
 * The engine's share of the PC's CPU while it worked, %.
 */
cpuPercent?: number, 
/**
 * The speech worker's memory with the engine loaded (and its GPU memory), MB.
 */
memoryMb?: number, 
/**
 * Unix milliseconds.
 */
measuredAt: number, };

export type SpeechBudget = { 
/**
 * End of speech → final transcript.
 */
sttMs: number, 
/**
 * Text → first audio.
 */
ttsMs: number, 
/**
 * Processing time over audio time.
 */
realTimeFactor: number, 
/**
 * Cancel → silence.
 */
cancelMs: number, wordErrorRate: number, noisyWordErrorRate: number, };

export type BenchmarkReply = { measured: MeasuredItem, budget: SpeechBudget, };

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

export type CapabilityItem = { capability: Capability, label: string, enabled: boolean, default: boolean, badges: Array<Badge>, 
/**
 * When a tool of this capability last ran (epoch ms), for "used 3 min ago" (CAP-04).
 */
lastUsed: number | null, };

export type TaskKind = "plan" | "watch" | "reminder" | "coding" | "routine";

export type TaskStatus = "pending" | "running" | "waiting" | "needsYou" | "paused" | "done" | "failed" | "cancelled" | "interrupted";

export type OnError = { "policy": "stop" } | { "policy": "continue" } | { "policy": "retry", times: number, } | { "policy": "ask" };

export type WindowEvent = "opened" | "closed";

export type WatchSpec = { "type": "time", atMs: number, } | { "type": "processExit", pid: number | null, name: string | null, } | { "type": "folder", path: string, pattern: string | null, } | { "type": "download", folder: string | null, } | { "type": "window", app: string | null, title: string | null, event: WindowEvent, };

export type Criterion = { "type": "commandSucceeds", command: string, cwd: string | null, } | { "type": "fileExists", path: string, } | { "type": "windowExists", title: string, };

export type StepAction = { "type": "tool", tool: string, args: unknown, } | { "type": "watch", watch: WatchSpec, } | { "type": "say", text: string, } | { "type": "ask", prompt: string, profile: string | null, } | { "type": "decide", question: string, options: Array<string>, profile: string | null, } | { "type": "agent", prompt: string, agent: string | null, cwd: string | null, } | { "type": "verify", criterion: Criterion, } | { "type": "delay", ms: number, };

export type Notify = "speak" | "toast" | "silent";

export type TaskGrant = { tool: string, args: unknown, };

export type TaskView = { id: string, title: string, kind: TaskKind, 
/**
 * Who it's for or who made it: "you", a routine's name.
 */
owner: string, status: TaskStatus, steps: Array<TaskStepView>, createdAt: number, updatedAt: number, finishedAt: number | null, result: string | null, error: string | null, 
/**
 * Kept instead of the steps once the task is 90 days old (MEM-02).
 */
summary: string | null, 
/**
 * A step calls a brain (it costs, ROUT-08).
 */
usesAi: boolean, 
/**
 * What the task waits for the user to decide.
 */
question: TaskQuestion | null, routineId: string | null, };

export type TaskStepView = { id: string, title: string, 
/**
 * `pending`, `running`, `waiting`, `needsYou`, `done`, `failed` or `skipped`.
 */
status: string, detail: string | null, startedAt: number | null, finishedAt: number | null, attempts: number, };

export type TaskQuestion = { step: string, text: string, 
/**
 * `retry`, `skip`, `stop`, `allow`, `always`, `deny`.
 */
choices: Array<string>, };

export type Trigger = { "type": "phrase", phrases: Array<string>, lang: string, } | { "type": "hotkey", chord: string, } | { "type": "manual" } | { "type": "schedule", cron: string, tz?: string | null, } | { "type": "event", event: RoutineEvent, };

export type RoutineEvent = { "kind": "appLaunched", app: string, } | { "kind": "usbDevice", name?: string | null, } | { "kind": "network", name: string, } | { "kind": "timeOfDay", time: string, days: Array<number>, } | { "kind": "idle", minutes: number, } | { "kind": "return", awayMinutes: number, } | { "kind": "batteryLow", percent: number, } | { "kind": "afterRoutine", routine: string, };

export type VarKind = "text" | "number";

export type VarDef = { name: string, kind: VarKind, };

export type RoutineStep = { id: string, action: StepAction, onError: OnError, delayMs?: number | null, 
/**
 * Steps in the same group run together; the next group waits for the whole group.
 */
parallelGroup?: string | null, 
/**
 * Asks the user before this step ("Lock the PC?"); a no skips it.
 */
confirm?: boolean, };

export type Routine = { id: string, name: string, description: string, enabled: boolean, triggers: Array<Trigger>, steps: Array<RoutineStep>, variables: Array<VarDef>, 
/**
 * What the user granted when saving it (ROUT-03): each step's tool with its exact
 * arguments.
 */
grants: Array<TaskGrant>, 
/**
 * A starter routine KIVO ships (ROUT-10): its key, so it isn't added twice.
 */
starter?: string | null, };

export type RoutineView = { routine: Routine, containsAi: boolean, customCommand: boolean, 
/**
 * Every step is covered by what was granted when it was saved.
 */
granted: boolean, lastRun: number | null, updatedAt: number, };

export type GrantLine = { tool: string, 
/**
 * "Open Slack", in plain words.
 */
title: string, risk: Risk, capability: Capability, 
/**
 * The capability is off: the step would be refused until it's turned on.
 */
capabilityOff: boolean, };

export type Collision = { phrase: string, 
/**
 * `command` (a built-in command), `routine`, `wakeWord` or `hotkey`.
 */
kind: string, 
/**
 * What it clashes with ("volume up", "Work mode", "Hey Kivo").
 */
with: string, 
/**
 * Blocks saving (an exact clash) or only warns (sounds alike).
 */
blocking: boolean, };

export type RoutineCheck = { collisions: Array<Collision>, grants: Array<GrantLine>, 
/**
 * Problems that stop it from saving (an empty name, an unknown tool).
 */
problems: Array<string>, };

export type ToolItem = { id: string, title: string, description: string, 
/**
 * Its arguments' JSON Schema: the builder generates the form from it.
 */
params: unknown, risk: Risk, capability: Capability, };

export type WorkspaceItem = { id: string, path: string, name: string, instructions: string, 
/**
 * The project's own agent files found there (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`).
 */
agentFiles: Array<string>, preferredAgent: string | null, lastUsed: number, };

export type AgentsOverview = { cli: Array<AgentItem>, 
/**
 * Claude Desktop, ChatGPT, Copilot found in the installed apps (DISC-06).
 */
desktop: Array<DesktopAiItem>, sessions: Array<AgentSessionItem>, };

export type AgentItem = { id: string, name: string, installed: boolean, signedIn: boolean | null, version: string | null, 
/**
 * It can be run in a visible terminal (CONV-14).
 */
terminal: boolean, };

export type DesktopAiItem = { id: string, name: string, 
/**
 * The installed app's id (AUMID or Start-menu path) to open it.
 */
appId: string, };

export type AgentSessionItem = { id: string, agent: string, workspace: string, 
/**
 * `acp` (KIVO runs it) or `terminal` (a visible terminal KIVO started).
 */
kind: string, running: boolean, 
/**
 * Can be continued in a terminal (the agent supports resuming).
 */
resumable: boolean, lastUsed: number, };

export type McpServerView = { id: string, name: string, 
/**
 * `local` (a program on this PC) or `remote`.
 */
kind: string, 
/**
 * The app it was imported from, or `connector:<id>`.
 */
source: string | null, enabled: boolean, 
/**
 * `running`, `stopped`, `connecting`, `error` or `needsReview`.
 */
status: string, error: string | null, tools: Array<McpToolView>, };

export type McpToolView = { name: string, 
/**
 * The server's own description: untrusted, shown for review.
 */
description: string, risk: Risk, enabled: boolean, 
/**
 * `approved`, `new` (not reviewed yet) or `changed` (differs from what was approved).
 */
state: string, };

export type McpFoundView = { app: string, name: string, file: string, servers: Array<string>, 
/**
 * Its servers not imported yet.
 */
new: Array<string>, };

export type ConnectorView = { id: string, name: string, description: string, 
/**
 * What it can reach ("Issues, pull requests and code").
 */
access: string, 
/**
 * `remote` (a remote MCP with sign-in), `local` (ready on this PC, no sign-in) or `builtIn`.
 */
kind: string, 
/**
 * `connected`, `ready`, `available`, `connecting` or `error`.
 */
state: string, 
/**
 * What it's connected as or found through ("gh is signed in as MadBlast0").
 */
detail: string | null, 
/**
 * The MCP server it runs as, once connected.
 */
server: string | null, tools: number, 
/**
 * `local`, `cloud`, `sensitive`.
 */
badges: Array<string>, 
/**
 * When KIVO last used one of its tools (epoch ms, from the audit log).
 */
lastUsed: number | null, };

export type SkillView = { id: string, name: string, description: string, 
/**
 * `kivo`, `claude-code`, `project:<name>`, `import`.
 */
source: string, path: string, enabled: boolean, reviewed: boolean, 
/**
 * It has scripts (they run through the shell tool, under the permission engine).
 */
scripts: boolean, 
/**
 * About how many tokens its name and description add to a request.
 */
tokens: number, };

export type MemoryNoteView = { 
/**
 * Vault-relative (`people/maya.md`).
 */
path: string, 
/**
 * `fact`, `about`, `instructions`, `person`, `workspace`, `topic`, `decisions`, `log`, `note`.
 */
kind: string, title: string, excerpt: string, tags: Array<string>, 
/**
 * Its folder (`workspaces/k-i-v-o/log`), empty at the top.
 */
folder: string, workspace: string | null, 
/**
 * A data class.
 */
sensitivity: string, shareCloud: boolean, updatedAt: number, 
/**
 * Set when a newer fact replaced it.
 */
validUntil: number | null, useCount: number, lastUsedAt: number | null, };

export type MemoryTagView = { tag: string, count: number, };

export type MemoryFolderView = { path: string, 
/**
 * Notes in it and below.
 */
count: number, };

export type MemorySuggestionView = { id: number, text: string, reason: string | null, workspace: string | null, createdAt: number, };

export type MemoryOverview = { 
/**
 * The vault folder.
 */
root: string, notes: Array<MemoryNoteView>, tags: Array<MemoryTagView>, folders: Array<MemoryFolderView>, suggestions: Array<MemorySuggestionView>, 
/**
 * `[[wikilinks]]` between notes, resolved to paths (the graph view, CONV-25).
 */
links: Array<MemoryLinkView>, };

export type PointTarget = { x: number, y: number, width: number, height: number, 
/**
 * The control's name ("Export…").
 */
label: string, };

export type LiveView = { 
/**
 * "Gemini Live".
 */
provider: string, 
/**
 * When it started (epoch ms), for the card's clock.
 */
started: number, 
/**
 * The session ends at this many minutes (the realtime cap), if any.
 */
maxMinutes?: number, };

export type UpdateView = { 
/**
 * The running version.
 */
current: string, 
/**
 * The channel in use (the setting, or the installed build's).
 */
channel: UpdateChannel, 
/**
 * This build can update itself (it was built with KIVO's update key).
 */
enabled: boolean, state: UpdateState, 
/**
 * The update that just installed, until "Got it".
 */
whatsNew?: WhatsNew, };

export type UpdateState = { "kind": "idle" } | { "kind": "checking" } | { "kind": "upToDate", checkedAt: number, } | { "kind": "downloading", version: string, received: number, total: number | null, } | { "kind": "ready", version: string, notes: string, whenIdle: boolean, } | { "kind": "installing", version: string, } | { "kind": "failed", message: string, };

export type WhatsNew = { version: string, from: string, notes: string, };

export type UpdateChannel = "stable" | "beta" | "experimental";

export type UpdateInstall = "ask" | "when-idle";

export type ControlView = { 
/**
 * The app being controlled ("Notes").
 */
app: string, step: number, maxSteps: number, 
/**
 * Estimated cost so far, in US cents.
 */
costCents: number, paused: boolean, frame: ScreenFrame, 
/**
 * KIVO's cursor, where it acts (physical pixels).
 */
cursor: ScreenPoint | null, };

export type ScreenFrame = "off" | "subtle" | "full";

export type ComputerUse = { 
/**
 * Approve each step: always, for the first ten tasks (watch mode), or never.
 */
approve: ApproveSteps, 
/**
 * Wait while the user uses the mouse or keyboard.
 */
"pause-on-mouse": boolean, speed: CuSpeed, "max-steps": number, 
/**
 * Most estimated cost per task, in US cents.
 */
"max-cost-cents": number, "time-limit-seconds": number, 
/**
 * The frame around the screen while KIVO controls it.
 */
frame: ScreenFrame, 
/**
 * A second, KIVO cursor where it acts.
 */
"show-cursor": boolean, 
/**
 * A ring around what it's about to act on.
 */
highlight: boolean, 
/**
 * The Island's controller (step, cost, Pause, Stop).
 */
"island-controls": boolean, };

export type ApproveSteps = "always" | "first-tasks" | "never";

export type CuSpeed = "careful" | "normal" | "fast";

export type InstallPlan = { id: string, name: string, 
/**
 * Already on this PC: nothing to do.
 */
installed: boolean, version: string | null, steps: Array<InstallStepView>, };

export type InstallStepView = { id: string, title: string, 
/**
 * Empty when the user does it (a vendor download).
 */
command: string, why: string, 
/**
 * `pending`, `running`, `done`, `failed`, `needsYou`.
 */
status: string, 
/**
 * The last lines the command printed.
 */
output: string, };

export type InstallView = { id: string, name: string, 
/**
 * `running`, `done`, `failed`, `cancelled`, `needsYou`.
 */
status: string, steps: Array<InstallStepView>, 
/**
 * The version it answers with once verified.
 */
version: string | null, error: string | null, };

export type MemoryLinkView = { from: string, to: string, };

export type SetupAdvice = { online: boolean, metered: boolean, ramMb: number, 
/**
 * The GPU with the most memory of its own.
 */
gpu: string | null, onBattery: boolean, 
/**
 * A permission mode (`auto`).
 */
mode: string, 
/**
 * A performance profile (`auto`, `battery`).
 */
performance: string, 
/**
 * A privacy mode (`cloud`, `local`).
 */
privacy: string, 
/**
 * The brain to connect first: a discovered id or `openrouter`; none when one is connected or
 * none can answer.
 */
brain: string | null, 
/**
 * A local server's address, for connecting it.
 */
brainUrl: string | null, 
/**
 * Download the speech models now (not on a metered connection).
 */
downloadNow: boolean, 
/**
 * Why, one line each, in the user's language.
 */
reasons: Array<string>, };

export type MemoryNoteDetail = { note: MemoryNoteView, 
/**
 * The whole file, front-matter included.
 */
markdown: string, 
/**
 * Its `[[links]]`.
 */
links: Array<string>, 
/**
 * Notes linking to it.
 */
backlinks: Array<string>, 
/**
 * Other facts about the same thing (older or newer).
 */
history: Array<MemoryNoteView>, supersededBy: string | null, };

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
  sessionUndo: "session.undo",
  permissionsAnswer: "permissions.answer",
  permissionsGrants: "permissions.grants",
  permissionsRevoke: "permissions.revoke",
  capabilitiesGet: "capabilities.get",
  capabilitiesSet: "capabilities.set",
  capabilitiesPreset: "capabilities.preset",
  browserStatus: "browser.status",
  activityList: "activity.list",
  activityExport: "activity.export",
  auditList: "audit.list",
  modelsList: "models.list",
  modelsInstall: "models.install",
  modelsRemove: "models.remove",
  modelsPause: "models.pause",
  modelsCancel: "models.cancel",
  modelsSetDefault: "models.setDefault",
  settingsGet: "settings.get",
  settingsSet: "settings.set",
  modeSet: "mode.set",
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
  soundsImport: "sounds.import",
  soundsClear: "sounds.clear",
  voiceEngines: "voice.engines",
  voiceRecommend: "voice.recommend",
  voiceSwitch: "voice.switch",
  voiceSetKey: "voice.setKey",
  voiceDeleteKey: "voice.deleteKey",
  voicePreview: "voice.preview",
  voiceMicCheck: "voice.micCheck",
  voiceDevices: "voice.devices",
  voiceAdvanced: "voice.advanced",
  voiceTrySample: "voice.trySample",
  voiceTest: "voice.test",
  voiceBenchmark: "voice.benchmark",
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
  brainsSetActive: "brains.setActive",
  brainsReasoning: "brains.reasoning",
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
  chatContinue: "chat.continue",
  chatBranch: "chat.branch",
  chatExport: "chat.export",
  chatSearch: "chat.search",
  chatMisroute: "chat.misroute",
  memoryPreferences: "memory.preferences",
  memorySetPreference: "memory.setPreference",
  memoryDeletePreference: "memory.deletePreference",
  tasksList: "tasks.list",
  tasksGet: "tasks.get",
  tasksCancel: "tasks.cancel",
  tasksPause: "tasks.pause",
  tasksResume: "tasks.resume",
  tasksAnswer: "tasks.answer",
  tasksDelete: "tasks.delete",
  tasksClear: "tasks.clearFinished",
  routinesList: "routines.list",
  routinesSave: "routines.save",
  routinesDelete: "routines.delete",
  routinesRun: "routines.run",
  routinesEnable: "routines.enable",
  routinesCheck: "routines.check",
  routinesTools: "routines.tools",
  routinesDraft: "routines.draft",
  routinesExport: "routines.export",
  routinesImport: "routines.import",
  routinesToSkill: "routines.toSkill",
  installsPlan: "installs.plan",
  installsStart: "installs.start",
  installsStatus: "installs.status",
  installsCancel: "installs.cancel",
  installsTools: "installs.tools",
  sessionComputerPause: "session.computerPause",
  sessionLive: "session.live",
  updatesStatus: "updates.status",
  updatesCheck: "updates.check",
  updatesInstall: "updates.install",
  updatesSeen: "updates.seen",
  aboutNotices: "about.notices",
  agentsOverview: "agents.overview",
  agentsOpenInTerminal: "agents.openInTerminal",
  agentsStart: "agents.start",
  workspacesList: "workspaces.list",
  workspacesRemember: "workspaces.remember",
  workspacesForget: "workspaces.forget",
  workspacesExportAgentsMd: "workspaces.exportAgentsMd",
  workspacesSetAgent: "workspaces.setAgent",
  instructionsGet: "instructions.get",
  instructionsSet: "instructions.set",
  permissionsBypass: "permissions.bypass",
  draftAnswer: "draft.answer",
  sessionHelp: "session.help",
  sessionMissed: "session.missed",
  selectionAction: "selection.action",
  offerAnswer: "offer.answer",
  mcpList: "mcp.list",
  mcpFound: "mcp.found",
  mcpImport: "mcp.import",
  mcpAdd: "mcp.add",
  mcpRemove: "mcp.remove",
  mcpEnable: "mcp.enable",
  mcpApprove: "mcp.approve",
  mcpSetTool: "mcp.setTool",
  mcpShare: "mcp.share",
  connectorsList: "connectors.list",
  connectorsConnect: "connectors.connect",
  connectorsDisconnect: "connectors.disconnect",
  skillsList: "skills.list",
  skillsEnable: "skills.enable",
  skillsImport: "skills.import",
  skillsRead: "skills.read",
  skillsRemove: "skills.remove",
  extensionsRefresh: "extensions.refresh",
  memoryOverview: "memory.overview",
  memoryNote: "memory.note",
  memorySave: "memory.save",
  memoryRemember: "memory.remember",
  memoryRememberTurn: "memory.rememberTurn",
  memoryMeta: "memory.meta",
  memoryDelete: "memory.delete",
  memoryForget: "memory.forget",
  memoryExport: "memory.export",
  memorySuggestion: "memory.suggestion",
  memoryTidy: "memory.tidy",
  memoryOpen: "memory.open",
  memoryWhy: "memory.why",
  performanceStatus: "performance.status",
  setupRecommend: "setup.recommend",
  diagnosticsRun: "diagnostics.run",
  diagnosticsBundle: "diagnostics.bundle",
  diagnosticsSave: "diagnostics.save",
  settingsExport: "settings.export",
  settingsImport: "settings.import",
  settingsReset: "settings.reset",
  systemOpenUrl: "system.openUrl",
} as const;

export type Method = (typeof Method)[keyof typeof Method];
