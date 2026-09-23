# KIVO decision log

Decisions made so far, newest first. Each links to the research behind it. When a
decision changes, update the entry and note the date. Do not silently rewrite it.

---

## 2026-09-23 — M5 build

| Topic | Decision |
|---|---|
| **Tasks are graphs in SQLite** | A task is its spec (steps with dependencies, error policy, grants, success checks, timeout, how to tell the user) plus its steps' state, in `tasks` and `task_steps`. Ready steps run together; each task has its own cancellation token under KIVO's. After a crash, a task that was running is marked Interrupted and never resumed on its own; a watcher that was only waiting is armed again. Finished tasks keep their steps for 90 days, then only a summary (MEM-02) |
| **Task grants are exact** | A task (a plan, a routine) runs with the grants it got at creation: each a tool with its exact arguments (`${name}` for a routine's variable). A task's call outside them is refused with `NotGranted` in every mode, Bypass included; a covered call runs without asking. Hard limits still come first |
| **Watchers are events, not polls** | Process exit is a kernel wait on the process handle (with a cancel event); folders and downloads use the `notify` crate (ReadDirectoryChangesW), with a download done once it stops growing and isn't a partial file; windows use UIA window events; times are stored and armed again after a restart, and a missed time is reported on the next start. No brain is called while a watcher waits (M5-X2 counts the calls) |
| **Proactive speech** | `notifier::decide` is a pure function of the situation (presence, fullscreen, quiet hours, how urgent, the source's setting) and when KIVO last spoke up: at most one spoken interruption per 10 minutes, the rest toasts or queued for "what did I miss?". "In a call" means another app uses the microphone, read from Windows' capability-access registry (read-only); presence also uses idle time and the lock screen |
| **Starter routines without Focus** | Windows has no documented API for turning on Focus / Do Not Disturb, muting other apps' notifications or choosing the default microphone. The starters use what exists: Work mode opens VS Code and the browser, Break pauses media and locks (asks first), Meeting unmutes the mic and opens the calendar, Goodnight sleeps (asks first), Focus for {minutes} pauses media and sets a reminder. All start disabled; turning one on shows its grants. ROUTINES §6 is updated |
| **Routine phrases before the grammar** | A routine's phrases are matched before the built-in commands, so the user's own words win; a custom command (one phrase, one tool) runs as a direct action, a routine as a task. Collisions are checked on save: an exact clash with a command, another routine, a wake word or a hotkey blocks saving; a phrase that sounds like one (the wake word confusability list) warns |
| **Terminal agents** | Windows Terminal (`wt -d <folder> --title <agent · folder>`) with the agent's command. Mode flags come from the app registry and are used only if the installed version's `--help` lists them; otherwise the launch is refused, not guessed. A bypass/yolo launch is High risk. Prompts are pasted into the session's window (found by its title) with the clipboard and Enter; `agents.send_prompt` is irreversible, so it is always a Draft card first. Replies are read from the window's UIA text and marked untrusted |
| **The Draft card** | The decision card of `agents.send_prompt` carries the draft (where it goes and the text). The text changes by voice ("add …", "remove the last sentence", "read it back", parsed without AI) or in the Island's field; "send" approves. Nothing is typed before that |
| **The planner is a tool** | `tasks.propose_plan` takes the steps (tool calls with dependencies) and success checks. It always asks (it hands out permissions); its risk is its highest step's; the card lists the steps first. Approving grants exactly those calls and checks, and the plan runs as a task |
| **Checking coding work** | After an agent's coding work, KIVO runs the project's own check: a `tests:` / `build:` line in the workspace's notes, else the project's markers (Cargo, package.json scripts with the lockfile's runner, pytest, go, dotnet, Maven, Gradle). It runs through `shell.run` and the permission engine. If it fails, the agent gets the failure once more; KIVO says it's fixed only when the check passes. The work is a Coding task in Tasks |
| **Bypass permissions** | Only from the Permissions page's dialog (the tray opens it), for 15 minutes, 1 hour (default) or until turned off, optionally behind Windows Hello; never stored, so a restart ends it. The tray icon gets a red ring and the Island a BYPASS chip; every action is audited as usual. Guests keep guest rules, and hard limits hold |
| **Workspaces and instructions** | Instructions live in SQLite and are mirrored to Markdown (`instructions\global.md`, `workspaces\<name>\instructions.md` in KIVO's data folder), watched with `notify` and read back when edited. A folder KIVO works in (a command's folder, an agent's, the folder open in VS Code) is offered once per run: "Remember … as a workspace?". Paths compare case-insensitively (migration 8). A project's CLAUDE.md / AGENTS.md / GEMINI.md are read, never written; "Export as AGENTS.md" writes a KIVO section and keeps the rest |
| **VS Code's folder** | The open folder is read from VS Code's own window state (`%APPDATA%\Code\User\globalStorage\storage.json`, read-only) and matched to the window's title; nothing is injected into VS Code |
| **Live activities** | Tasks and media commands publish activities in the state snapshot; the Island shows the first one collapsed, with a count. Over a fullscreen app, a game or a presentation (`SHQueryUserNotificationState`) they don't show; this is checked whenever KIVO's state changes, not polled, so one already showing stays until the next change. The media activity appears only after a media request (no background media polling) |
| **The selection shortcut** | Ctrl+Shift+Space reads the selection in the app in front (UIA TextPattern, never a password field) before the Island takes focus, and keeps it in the runtime for two minutes; the UI only learns that there is one, and offers Explain · Rewrite · Translate. The text goes to the brain as untrusted, fenced content. Without UI Automation, the clipboard is the fallback |
| **Every Island button by voice** | "Try again", "that's not what I meant", "turn it on", "open my tasks / routines / the Control Center" act on the last finished turn; a spoken yes or no answers an offer; F1 in the Island is "what can I say?" |
| **Jump list** | `ICustomDestinationList` user tasks start KIVO with `--action <name>`; the single running instance does it (Routines, New conversation, Pause listening, Stop everything) |
| **Smaller dev builds** | `[profile.dev]` keeps line tables only, no debug info for dependencies and no incremental cache. `target/` had grown to about 240 GB and filled the owner's disk (one runtime copy per integration test, full debug info for the `windows` and ONNX crates, and a second target folder). One target folder only |

## 2026-09-23 — M4 build

| Topic | Decision |
|---|---|
| **UI Automation on its own thread** | The `uiautomation` crate (MIT/Apache) on one thread in the COM multithreaded apartment, which owns every element KIVO holds; callers send it jobs and wait at most 12 s, and IUIAutomation2's provider timeouts (2 s connect, 8 s per call) keep one hung app from blocking the thread. Element references are `"<window>:<runtime id>"`, so a caller can check the window's app against the per-app lists before acting; an evicted reference is found again by searching its window for the runtime id |
| **The dummy app is a workspace crate** | `testenv/app` (`kivo-test-app.exe`) is plain Win32 controls, whose control ids are their AutomationIds. `cargo test --workspace` builds it next to the tests. It never takes focus (`SW_SHOWNOACTIVATE`), and its Export only writes into the `--out` folder a test gives it, so the UIA journey runs beside the user's work without touching it |
| **Tools assess their own call's risk** | SECURITY §3's escalation lives with each tool (`Tool::assess`), because only the tool knows its arguments: files are High in protected places (the OS, program files, app data, the profile root, a drive root) or over 20 files; the shell's risk comes from its parser; `apps.cli` from the registry verb; input is Low when the user asked directly. The permission engine decides on that assessment plus its own rule for egress: personal or sensitive data leaving the device is High, and refused in Strict Private |
| **Destination binding, precisely** | A destination is the user's when their words for the task contain it or its host; one that appears after untrusted content entered the turn is Untrusted and always refused; any other (a link the brain knew on a clean turn) is the system's, fine for opening a page but refused for anything that sends data (`data_egress`). This keeps "open the Rust docs" working while an address from a web page can never receive the user's data |
| **Tainted turns: fewer tools, and only offered ones run** | After untrusted content enters a turn (a window, a page, a file, the clipboard, command output), the next requests offer at most 8 tools and none that send, delete, spend, run commands or power off. The engine also refuses any tool the brain names that wasn't offered in that round, tainted or not |
| **Plan first: one card per round** | In Plan first, a brain round's changes (every call the engine would confirm) become one plan card; approving it grants permits for exactly those calls (`approve_plan`), reads run as they come, and a plan with a High step needs a click or Windows Hello |
| **Windows Hello** | `UserConsentVerifier` through `IUserConsentVerifierInterop`, parented to the window in front. High-risk cards offer "Confirm with Windows Hello" when Hello is set up; a spoken "approve" starts Hello, which must finish it; a failed Hello approves nothing |
| **Grants: pattern and duration** | "Always for…" asks how long: this session (until KIVO restarts; session grants from earlier runs are removed at start), 24 hours, or always. A grant can carry a `*` pattern over the call's arguments (a folder, `git *`). Migration 7 adds both columns |
| **Input belongs to Computer use** | The `input.*` tools (the last tier of the ladder) sit under the Computer use capability, which is off by default; the full computer-use loop (CAP-09–13) is still M8. Tests never send synthetic input to the desktop: the Windows `SendInput` code is covered by tests of the event lists it builds, and every rule around it (window still in front and unmoved, never a password field, the risk) is tested through the tool |
| **Output device switch** | Windows has no documented API for changing the system's default output, and KIVO uses documented APIs only. `audio.set_output` moves KIVO's own voice to the device and opens Windows' output settings for the rest. TOOLS_AND_CONTROL §3 is updated |
| **Brightness through WMI** | The built-in panel's brightness (`WmiMonitorBrightness`); external monitors report "can't be changed from Windows". Tests read the real brightness but never change it |
| **Files: scoped, recycled** | File tools act inside the allowed folders (the user's own folders and the workspace by default) and never in private folders; every path passes the guard (absolute, no `..`, no device or network namespace, no alternate data stream). Search uses the Windows index through the documented `search-ms:` protocol, else a bounded walk. Delete goes to the Recycle Bin through `IFileOperation` and is confirmed up front (KIVO can't restore it itself) |
| **Commands in Job Objects** | Every command starts suspended, joins its own Job Object (kill on close, a memory cap, a hard CPU cap), and only then runs, so nothing it starts escapes; timeouts, cancellation and the emergency stop kill the whole tree. By default the shell capability runs read-only commands only; secrets reach a command as per-call environment variables by handle and are scrubbed from its output |
| **The browser extension and its host** | A Chromium MV3 extension (Chrome, Edge, Brave) with a fixed key (id `ohaefaipbiknncgdjdpkjgaiocpnlloc`; the private key stays out of the repository). Its native messaging host is `kivo-runtime` itself, started by the browser with the extension's origin: it checks the origin, reads the session token, connects to the runtime's user-only `…-browser` pipe and relays frames — Chrome's framing is the same as KIVO's IPC (u32 length + JSON), 1 MiB per message. The host is registered per user for the three browsers only while "Browser: read & act on pages" is on. Until the extension is in the stores, it is loaded unpacked from the folder the Permissions page shows |
| **KIVO's own browser (CDP)** | `chromiumoxide` drives Chrome, Edge or Brave already on the PC in a KIVO-managed profile folder, never the user's; it never downloads a browser, keeps certificate errors as errors, and reads pages with the same functions as the extension |
| **The UIA fallback for browsers** | Without the extension, the active tab's address comes from the browser window's address bar (UIA) and the page text from its UIA tree |
| **App registry as data** | `crates/kivo-tools/apps/*.toml` (compiled in) plus the user's `%APPDATA%\KIVO\apps\*.toml`, which win over a core entry with the same id. Entries hold program and AUMID matches, the tiers to try first, CLI verbs with their own risk, URI verbs and UIA hints. `apps.open_uri` opens only schemes an entry declares; `apps.cli` runs only declared verbs, with PowerShell-quoted arguments |
| **The router is a tool** | `control.act` walks the capability ladder for an app (its registry preferences first) and says which tier did it; a tier whose capability is off is skipped, a password field stops it |
| **Capability presets and last use** | Presets decide the everyday capabilities only; consent-driven ones (the microphone, voice recognition, remote access, agents, MCP servers, integrations) keep the user's choice. "Used … ago" comes from the audit log |
| **Undo in the Island** | The last undoable change is offered for 8 s in the Island and as a Control Center toast; "Kivo, undo that" (a grammar command) and the toast still work for 10 minutes |
| **Island placement** | Top center (default), bottom center, or where the user dragged it: dragging the card remembers the spot for that monitor (and switches to Remember drag). While only listening, a title bar or tab strip under the Island (DWM's caption-button height, else the system metrics) moves it just below |
| **App icons** | The Island's leading icon while acting is the app's own shell icon (`IShellItemImageFactory`), sent as a small PNG data URL |
| **Plugin interface drafted** | `crates/kivo-tools/wit/kivo-plugin.wit` (Component Model): `tool-spec` mirrors `ToolSpec` field for field, host imports are one per grant, and every call still goes through the permission engine; a test keeps the WIT and the Rust types in step. No WASM runs before plugins (post-M9) |
| **Password fields: refused before asking** | Every tool that can type declares a `Tool::hard_limit` pre-check (read-only lookups: the element, the focused field, the router's target); the engine runs it before the permission engine and denies with `DenyCode::HardLimit`, so no mode — Bypass included — ever shows a question such as "Type … into a field" for a password field. `run` still checks again. Found by the TOOL-40 suite, where Ask mode first asked and only then refused |
| **Tool hints cover the M4 namespaces** | Brain requests offer at most 20 tools, chosen by keyword hints per namespace plus description overlap; the hints now include `uia`, `control`, `input`, `shell`, `context` and `audio`. A tool the brain names that wasn't offered in that round is refused, so a request whose words point nowhere gets the best-matching tools only |
| **The shell parser reads inside brackets** | Script blocks `{…}`, parentheses, `$(…)`/`@(…)` (also inside double quotes, which PowerShell expands) are assessed as commands of their own; en and em dashes count as `-`; `iex(…)`, `InvokeScript`/`NewScriptBlock` are High; `git … --output` writes. Expressions that only read (`$_.Length -gt 5`) stay Low, so filters keep working in the read-only shell |
| **The untrusted fence can't be closed from inside** | `ContextItem::render` defuses any `<untrusted`/`</untrusted` in the content, in any case and spacing, and strips quotes and angle brackets from the source label |

## 2026-09-23 — M3 build

| Topic | Decision |
|---|---|
| **Own adapters on `reqwest`, not `genai`** | BRAIN-10's "`genai` for breadth" is replaced by KIVO's own adapters: Anthropic and Gemini natively, and one OpenAI-compatible adapter for OpenAI, OpenRouter, Groq, Mistral, DeepSeek, xAI, custom services and local servers. They share one streaming HTTP layer (`reqwest` with native TLS, cancel within 100 ms) and one contract test suite against recorded responses. That gives prompt caching, tool calls, reasoning and usage in one vocabulary without a second abstraction to keep in step |
| **Brain settings split** | `[brains]` in kivo.toml holds connections (catalog id, address, key *handle*), the default profile, persona, the reasoning log, price refresh and the session timings. User-made profiles, spending limits, task caps and price overrides are kept by the runtime in the store (`app_meta`), since they are structured data the Brains and Usage pages edit, not hand-edited config |
| **Speaking can move to Acting** | A brain's streamed answer starts speaking before it may call a tool ("Sure, opening Chrome"), so the session machine gains Speaking → Acting and Speaking → AwaitingConfirmation (ARCHITECTURE §4.2) |
| **Semantic stage (BRAIN-03)** | all-MiniLM-L6-v2 (sentence-transformers, Apache-2.0), the 23 MB quantized ONNX export pinned at its Hugging Face revision with sha256, run by KIVO's own WordPiece tokenizer on `ort`. It is a model the user chooses (Voice → Models), never downloaded unasked. Exemplars are data (`grammar/<lang>/exemplars.toml`); a paraphrase is accepted at cosine ≥ 0.72 (0.66 when the three nearest agree or no other command is within 0.15) and 0.05 ahead of any other command, and only when the grammar resolves its slots. Measured on this PC: paraphrases score 0.69–0.89, unrelated requests ≤ 0.54. Power actions (restart, shut down) have no exemplars, and "… and …/then" requests go to a brain (BRAIN-04) |
| **A named brain is a request for it** | "Use Claude", "ask Gemini CLI", "tell Claude Code …" or "Claude, …" choose a brain; a mere mention doesn't ("open Google Chrome" isn't a request for Gemini). Found by an end-to-end test where a corrected request containing "Google Chrome" was routed to Gemini |
| **Agents' requests in KIVO's permission engine (BRAIN-15)** | Each ACP permission kind maps to a KIVO action (`agent.read`, `agent.edit`, …): read, search, think and fetch are low risk; edit and move medium; delete and execute medium and irreversible, so they are confirmed up front. As AI-initiated actions, medium ones ask unless "always allow" was given. Declining tells the agent no; its turn continues (unlike KIVO's own actions, where a decline ends the turn) |
| **Health checked around use (DISC-14)** | A stale health check runs in the background when a request starts, so it never adds latency; a failure marks the brain at once and re-checks it; and before saying no brain is available, KIVO re-checks the failing ones, so a passing outage isn't reported |
| **Same topic, simply (CONV-01)** | A voice request joins the last voice thread from the past 30 minutes when it shares a content word with its title or last messages, or leans on it ("and tomorrow?", "what about …"). "Kivo, new topic" always starts a new one |
| **Chat attachments** | Text files only (code, notes, CSV, JSON…), up to 200 KB in all, fenced as untrusted data in the request (SECURITY §4). Images and PDFs arrive with screen and file tools (M4/M7) |
| **CLI agents found, the ACP registry in the catalog (DISC-04)** | Detection checks KIVO's own catalog of CLI agents and their ACP entry points (`CLI_TOOLS`, versioned with KIVO), which mirrors the ACP registry; it does not fetch the registry during detection. An installed CLI whose ACP adapter is missing (Claude Code, Codex) shows the adapter's install command |
| **Speech speed and expressiveness (UX-61)** | Speaking speed works on all three voices (the Windows voices' SpeakingRate, Kokoro's and Supertonic's speed inputs). None of them exposes expressiveness, so the Voice page says so instead of showing a control that does nothing |
| **Your voice picks the recogniser (VOICE-23)** | After enrollment, each installed recogniser transcribes the eight prompts in a separate worker; one that is 5 points of WER better than the hardware's choice becomes the recommendation. Moonshine takes no hotwords, so the user's words go to the brain for transcript repair, and to engines with hotwords or a prompt when they arrive (Whisper, M8) |
| **M3-X1 measured later** | "First audio ≤ 1.2 s on a cloud brain" needs the owner's cloud account and the benchmark runs, deferred with the other benchmarks. The end-to-end test shows speech starting while the brain is still answering; `kivo-bench brain` (BENCH-09) will measure it |

## 2026-09-22 — M1 build

| Topic | Decision |
|---|---|
| **Speech engines on `ort`** | `kivo-infer` and the runtime's VAD run on `ort` (ONNX Runtime, MIT, linked statically), not the sherpa-onnx crate: sherpa's static library contains espeak-ng (GPL-3.0). Moonshine Base (MIT, English) is the M1 speech-to-text model, loaded from `.ort` files; partial transcripts come from re-transcribing the audio so far every 0.5 s. Silero VAD v6 (MIT, 2.3 MB) is bundled in `assets/models`; everything larger is downloaded by the model manager from Hugging Face with sha256 checks |
| **`kivo-infer` binary** | A separate `kivo-infer.exe` (the ARCHITECTURE §7 layout and DIST-01 sidecar list) run as `kivo-infer serve`, speaking the UI protocol's framed JSON-RPC over stdin/stdout. It starts on demand when a model is wanted, is restarted with backoff, and a lost worker fails only the current turn |
| **Earcons generated, not Kenney** | KIVO's Soft cues are synthesized at startup (rounded two-note tones, each under 300 ms) instead of shipping the Kenney CC0 placeholder files: no licence to track, no files to load, and the motif is KIVO's own (VOICE-24/29) |
| **Earcon gating** | While a cue plays, *voice detection* ignores the microphone (cue + 50 ms), so a chime never counts as speech (VOICE-25); *recognition* still receives every sample. Measured: dropping the cue's 310 ms cut the first word of "Open Chrome" when the user speaks right after pressing the key. Echo cancellation (M2) removes the chime from the recognized audio |
| **System voice first** | Spoken replies use the Windows voices (WinRT `SpeechSynthesizer`) by default, through the same streaming `TtsEngine` trait and mixer; Kokoro is the second engine, downloaded when the user picks it (VOICE-09) |
| **Kokoro's phonemizer** | KIVO's own Rust port of misaki's lexicon path (Apache-2.0): the US gold and silver dictionaries (downloaded with the model, pinned by commit and sha256), misaki's stress and -s/-ed/-ing rules, English number words; unknown short capitals are spelled, other unknown words use the public-domain NRL letter-to-sound rules instead of misaki's neural fallback. No espeak anywhere; no part-of-speech tagger, so heteronyms take their default reading |
| **Activity recorder** | Activity and Audit rows are written by a recorder the turn engine calls, not by an event-bus subscriber (ARCHITECTURE §4.1, ARCH-23): the rows need the permission decision, the step's arguments and the reply text, which the bus events don't carry, and writing in order with the turn keeps the audit hash chain simple |
| **Failures are spoken in-process** | When the speech worker is what failed (crash, slow start), the failure message is spoken by Windows' voice inside the runtime, so a turn never fails silently (ARCH-09) |
| **Speech engine choice (owner brief)** | Users choose among 2–4 curated STT and TTS profiles (not a model list), with the engine as secondary detail, honest "Not benchmarked by KIVO" labels, previews, safe switching that keeps the last working engine, visible fallbacks and Local/Cloud labels; engines live in a registry so adding one needs no UI changes (VOICE §11, VOICE-42–49, UX-60–62, BENCH-15). Supertonic 3 and Moonshine Streaming are candidates to evaluate, not locked choices |
| **Hands-free voice: measured choices (M2)** | Built on KIVO's own `ort` code: the keyword spotter (a port of sherpa-onnx's decoder, matching its keyword encodings and test detections), CAM++ (matching sherpa's embeddings), Smart Turn v3 (matching Whisper's features) and WebRTC AEC3 (`sonora`). Changes from the spec, each from a measurement: (1) recognition starts just before the wake phrase **begins**, not 300 ms before it ends, and the phrase is removed by word or by sound alignment ("A key vo mute" → "mute"): the spotter's word boundaries are approximate and a clipped "…vo, mute" was often not recognized; (2) "Hey Kivo" listens for its pronunciation variants (Kivo/Keyvo/Kevo/Kiva) and the sensitivity slider maps to boost 1.0–2.5 (sherpa's 1.0 lost the word on Windows' voice); (3) the speaker check uses the whole request at end of speech, not the wake word, and the profile is the joined enrollment audio plus clips of 2 s or more, scored against their centroid (sub-second clips gave noisy embeddings); (4) echo cancellation runs while KIVO's voice plays (and 500 ms after), not during cues, which it blunted at the start of requests; cues keep the VAD's own gating; (5) Smart Turn and Silero VAD install with any speech model; the keyword model when hands-free is turned on; CAM++ when the user enrolls |
| **Echo cancellation (VOICE-30)** | KIVO cancels its own voice with WebRTC AEC3 (`sonora`) on its output mix, not with the Windows OS AEC: that needs the capture stream in the Communications category, and by default Windows then lowers every other app's sound by 80% whenever the microphone is open — which, hands-free, is always. AEC3 removes 38.5 dB of KIVO's echo, but with headphones (no echo path) it erased 99% of the user's first half-second while KIVO spoke, so a short barge-in ("stop", "open Chrome") was lost. An echo-path detector (the loudness envelopes of what was played and what was heard, at delays up to 250 ms) now steps the canceller aside when KIVO's sound doesn't reach the microphone, and looks again when the output device changes. With speakers, barge-in waits until the canceller has settled (it removes at least 10 dB of KIVO's voice; about a second, once per output device): before that, KIVO's own voice interrupted it in testing. Only KIVO's own sound is removed; other apps' audio isn't |
| **"Stop" as a request (VOICE-19)** | Talking over KIVO starts a barge-in (300 ms of speech) before the stop-word spotter finishes "Kivo, stop", so a request that is only a stop word ("stop", "cancel", "never mind", "ruko", "bas"…, with or without "Kivo") ends the turn quietly instead of being answered; barge-in also silences whatever KIVO was playing. Both paths stop KIVO |
| **Voice hints use the buttons' words (CONV-26)** | While KIVO waits for a spoken answer, the Island's hint and the spoken prompt name the buttons' own words — "allow", "always allow" (when offered), "deny", "wait" — which the decision grammar understands, instead of a separate vocabulary |
| **Setup and Settings at M2 (UX-33, VOICE-27)** | First launch opens setup until it is finished or skipped (`general.onboarded`). M2 builds the Welcome and Voice phases plus a "Your setup" summary (UX-60's confirm step); the brain and control phases follow in M3/M7. Settings shows Sounds now; its other tabs come with UX-31 (M7) |
| **Speech engines evaluated (VOICE-46)** | **Supertonic 3** (Supertone, OpenRAIL-M; 31 languages, 10 voices, ONNX on CPU, 16 files, pinned) passes: KIVO's own port of Supertone's Rust example loads in 0.9 s and speaks at RTF 0.25 on the owner's PC (English and Hindi checked), so it is the **Multilingual** voice profile. **Moonshine Streaming** is not added: its publisher ships only safetensors, and converting it would make KIVO a model publisher (DECISIONS "Model sources"). **Moonshine sizes and languages** are added from the sherpa-onnx author's pinned ONNX exports: Tiny English (MIT) is the **Lightweight** recognizer; Base Spanish, Japanese, Chinese, Arabic, Ukrainian, Vietnamese and Tiny Korean, Japanese are **Multilingual** under the non-commercial Moonshine Community License, labelled before download. One Moonshine code path reads each export's shape and inputs from the model; round trips (Supertonic speaks, Moonshine hears) pass exactly for Base/Tiny English, Base Spanish and Tiny Japanese. High-accuracy STT (Whisper, Parakeet) and Expressive TTS read "Not available yet" until they are added (VOICE-10/11, M8) |
| **Model sources and catalog (owner)** | KIVO downloads each model straight from its publisher (Hugging Face or GitHub releases) at a pinned revision with sha256 checks. There is **no KIVO mirror**. The catalog is built into the app, so new models arrive with app updates. The UI follows the Handy pattern (MIT, handy.computer): recommended profile cards, then an "All models" list with Download / Use / Remove. Scores come only from KIVO's measurements or are labelled as the publisher's |
| **No training (owner)** | KIVO trains no models: it downloads published models and runs them. "Hey Kivo" uses the Apache-2.0 open-vocabulary keyword spotter (sherpa-onnx's GigaSpeech Zipformer, run by KIVO's own `ort` code because sherpa's library contains GPL espeak-ng); VOICE-13 (trained model) and VOICE-17 (Enhance training) are dropped. Wake detection is two-stage without a trained verifier: the spotter per word, then the speaker check on the whole request (VOICE-14) |
| **No models in the installer (owner)** | KIVO ships **no models**, not even small ones: Silero VAD moved from the bundle to the model manager (pinned upstream commit, same sha256), and the "Hey Kivo", keyword-spotting and speaker models will be downloads too. Nothing downloads unasked: the runtime no longer fetches the default speech model at startup; Home says speech isn't set up and opens the Voice page, and onboarding (UX-60) will offer the choice. A manifest's `requires` installs what a model needs with it (speech recognition brings Silero). Supersedes the bundling in DIST-02 and the "first run downloads" wording of DIST-15 |
| **Installer and release at M1** | Packaging is configured, and verified on the first `v*` tag, not by local release builds (owner: no release builds unless asked). `release.yml` builds, runs `scripts/smoke-install.ps1` (install with `/STARTUP`, IPC health via `kivo-runtime --health`, upgrade while running, uninstall), and only then creates a **draft** release with checksums. Startup stays off by default (asked in onboarding); `/STARTUP` registers it for deployments. `latest.json` moves to the updater (DIST-07, M9), since there is nothing to sign it with before then |
| **Resource rules at M1 exit** | Plan §128 reviewed (PLAN-16): (1) nothing loaded at idle, 0 ms CPU over 20 s, 67 MB; (2) screenshots only on request; (3) deterministic commands never reach a model (the grammar); (4) no polling where events exist: the detection thread now sleeps until a command, the worker's recognition loop waits for messages, downloads stop through the shutdown token; *exceptions*: a held push-to-talk key is checked every 15 ms because Windows sends no release event, the speaker checks once a second (only while open) whether to close, and `apps.restart` checks every 250 ms (at most 15 s) that the app has closed; (5) no MCP; (6) engines load only when their id changes; (7) the waveform draws at 30 fps only while sound plays; (8) *exception*: the microphone opens per request instead of staying open, for privacy (KIVO never holds the mic while not listening; opening costs a few ms, measured in the end-to-end tests); (9) cancellation reaches every layer in ≤ 35 ms; (10) recognition and speech both stream |
| **M1 scope moves** | Parts of M1 items that need later features moved to the items that build them: the first launch on onboarding (UX-01 → UX-33, M2), Home's "Listening for Hey Kivo" (the wake word, VOICE-13, M2), the running-task count and Home's Running list (UX-56/UX-19 → UX-24, M5), the Chat page's mode picker (SEC-04 → UX-21, M3), the About licence list (DIST-13 → UX-31, M7), the diagnostics bundle (DIST-15 → ARCH-40, M8), the rest of the Voice page beyond models and the voice choice (UX-23, M3) |
| **Hotkeys: keyboard-hook fallback** | When another app owns a combination, KIVO catches it first with a low-level keyboard hook and takes those keys (so the other app doesn't act too), and Home says so with a rebind prompt (VOICE-41) |
| **Auto means Medium runs** | SECURITY §1.1 (the owner's mode table) is authoritative over the older §2 profile table where they differ: in Auto, Safe/Low/Medium run on clean, user-initiated turns; tainted or AI-initiated Medium actions still ask; High always asks except in Bypass |

## 2026-09-21 — Implementation start

| Topic | Decision |
|---|---|
| **UI implementation** | React 19 + TypeScript + Vite + Tauri v2. Interactive primitives are **Base UI** used directly, styled by KIVO's own `k-` component CSS on design tokens (no shadcn layer; it added nothing the tokens don't cover). Tailwind v4 is installed for utilities. Motion for springs, Lucide icons behind semantic names ([DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md)) |
| **Title bar** | The native title bar is off. KIVO draws one bar: the sidebar brand row plus a drag strip with Windows-style caption buttons (owner request) |
| **Zero warnings** | tsc, Vite, rustc and clippy must stay free of warnings (owner request) |
| **UI linter: Oxlint, not ESLint** | TypeScript 7 (the native compiler) has no JavaScript API, so typescript-eslint cannot run (it supports TS < 6.1). Oxlint (VoidZero, same team as Vite) parses TypeScript itself, includes the React Hooks and jsx-a11y rules, and its type-aware mode runs on TypeScript 7's engine (`oxlint-tsgolint`). Style rules turned off: `prefer-tag-over-role`, `consistent-return`, `no-array-index-key` (they conflict with deliberate patterns). Prettier stays, at 120 columns |
| **IPC transport** | tokio's named pipes (Windows) and Unix sockets instead of the `interprocess` crate: tokio exposes the security settings SECURITY §9 needs directly (reject remote clients, first-instance creation, a raw security descriptor), and it is already a dependency. The pipe and token file get a protected DACL granting only the user's SID (verified by reading it back in tests) |
| **TypeScript types** | `ts-rs` (not specta): stable, supports every serde attribute the IPC types use; behind a `ts` cargo feature so shipped binaries never compile it |
| **Toolchain pin** | `rust-toolchain.toml` pins Rust 1.97.0 so CI and local builds match; the MSRV stays `rust-version` 1.90 |
| **Build tracking** | Every buildable requirement has an ID and a status mark in its spec's `## Build checklist`; the plan's §168 maps every plan section to those IDs; ROADMAP lists each milestone's docs and items, regenerated with `pnpm docs:sync`. Rules in [README.md](README.md) (owner request) |
| **Onboarding by milestone** | Steps 1–5 (voice) in M2, step 6 (brain) in M3, steps 7–11 in M7 |
| **Audio backend** | WASAPI directly (windows-rs), shared mode, event-driven, float with AUTOCONVERTPCM. The cpal fallback in VOICE-01 is dropped: cpal uses WASAPI on Windows, so it can't cover a WASAPI failure, and AUTOCONVERTPCM already makes float work on every device. Measured: 0.05% CPU to hold the mic open |
| **Benchmark counters** | GPU load and power come from Windows performance counters (`GPU Engine` utilization, `Energy Meter` RAPL package power) instead of PresentMon: no extra tool to install, and they work on the machines KIVO targets. PresentMon stays an option for per-frame timing |
| **No redraw per refresh** | Measured on a 165 Hz laptop: anything that redraws every display refresh is expensive in WebView2 (a looping CSS filter animation on Home and a requestAnimationFrame waveform cost ~0.8 CPU cores and +12 W). Rule: no infinite CSS animations in the Control Center, animation loops use a fixed rate (the waveform draws at 30 fps), and the overlay window fits the Island. Result: +1.1 W while the Island animates |
| **Kokoro phonemizer** | sherpa-onnx's TTS phonemizes with espeak-ng (GPL-3.0), so sherpa's Kokoro is used only inside `kivo-bench`, which is never distributed. KIVO's shipped Kokoro (VOICE-09, M1) needs a permissive phonemizer (misaki-style lexicon + rules, as VOICE §3 already says); a hard requirement, checked against DIST-17 |
| **Speech engine runtime (M0)** | The engine comparisons run through sherpa-onnx 1.13.8 (Apache-2.0) on ONNX Runtime, CPU, 4 threads: one runtime for Moonshine v2, Parakeet TDT v3, Whisper turbo, Kokoro, keyword spotting and Silero VAD. Whether `kivo-infer` (M1) links sherpa-onnx for ASR/VAD/KWS depends on building it without espeak-ng; otherwise it uses `ort` directly (VOICE §3 names both) |
| **Wake-word data** | No user ever records anything for KIVO to work: the wake benchmark and the built-in "Hey Kivo" model use synthetic speech (many TTS voices and speeds through room echo and noise), as openWakeWord does; enrollment stays optional. The low-end tier is emulated on the reference PC (4 cores, 8 GB limits), labelled approximate (owner, 2026-09-22) |
| **License checks (M0)** | `uiautomation` 0.25.1 (UIA, M4): Apache-2.0 ✓. Smart Turn v3 (endpointing, M2): BSD-2-Clause, commercial use allowed, keep the notice ✓. `interprocess`: not used; KIVO's pipes are tokio's with a DACL tested directly ("IPC transport") ✓. New in M0: sherpa-onnx Apache-2.0 (benchmark only; its espeak-ng is GPL-3.0, see "Kokoro phonemizer"), sonora BSD-3-Clause, claxon Apache-2.0; `deny.toml` also allows ISC and CDLA-Permissive-2.0 (both permissive; via libloading, rustls-webpki, webpki-roots). `cargo deny check` passes |
| **Benchmarks deferred** | Owner decision (2026-09-22): the speech-engine benchmark runs (BENCH-02–05, the low-tier run, the engine-defaults update) wait until after the build milestones. The suites and `pnpm bench:data` are ready; M1 starts with the provisional engine defaults of VOICE §3 |
| **Island geometry** | Owner review of the Island found small misalignments; measured fixes (2026-09-21): the waveform shows at the designed 44 × 18 px (it rendered at 88 × 36), row insets are symmetric and trailing items concentric with the pill ends, and expanded content aligns with the label column (36 px) instead of 16 px. The mockup and DESIGN_SYSTEM were updated to match |
| **Mockup files** | The round-1 and wake-concept mockups were removed by the owner (kept in git history); the reference is [kivo-app.html](design/mockups/kivo-app.html) |
| **GitHub Actions only on version tags** | Owner decision: workflows (`ci.yml`, `codeql.yml`, later `release.yml`) run only when a `v*` tag is pushed, or by hand; nothing runs on pushes, PRs or a schedule. release-please, the PR-title check and Dependabot update PRs were removed (Dependabot alerts stay on). Versions start at `0.0.0` and are set with `pnpm release:version X.Y.Z`; the owner tags. Checks run locally before every push. Supersedes the release-please part of "Releases" below ([RELEASE.md](architecture/RELEASE.md)) |

## 2026-09-21 — Feature additions (round 2)

| Topic | Decision |
|---|---|
| **Capabilities** | An iOS-style **Capabilities** page: every capability can be toggled, and a disabled one is removed entirely. Presets: Minimal / Balanced / Power user / Custom. Active-use indicators. **Computer use is off by default**, with watch mode and per-task step and cost caps. See [CAPABILITIES.md](architecture/CAPABILITIES.md) |
| **Screen awareness** | On request only. UIA + local OCR first; cloud vision only if allowed |
| **Costs** | Track usage and estimated cost for everything. **Limits are optional and user-set** (none by default), with user-chosen warning thresholds and an at-limit action (ask / cheaper / local / block). Per-task caps for computer use, agents and realtime |
| **Realtime voice** | An optional speech-to-speech conversation mode (OpenAI Realtime, Gemini Live). The fast path and permissions stay in front. Off by default |
| **Routines** | User-defined custom commands and routines (phrase, hotkey, schedule and event triggers) built on the Task engine. No AI needed, with optional AI steps. See [ROUTINES.md](architecture/ROUTINES.md) |
| **Companion** | Every style is available and switchable: Pill (default), Orb, Character (Rive), Hidden |
| **Personas** | Calm (default), Friendly, Witty, Custom. They never change safety wording |
| **Local LLM** | **Not bundled.** Users connect their own local servers (Ollama, LM Studio, llama.cpp) |
| **Integrations** | All major apps, planned in order: Google, Microsoft, Spotify/media, dev tools, then others. Local means first, then vendor MCP servers, then native OAuth connectors with **bring-your-own client ID** where the platform restricts it (Gmail/Drive restricted scopes need CASA; Spotify dev mode allows 5 users). *Superseded 2026-09-21 by **No user keys** below: no bring-your-own client IDs* |
| **Plugins** | WASM Component Model (wasmtime + WIT), with capabilities granted by linked imports. Post-MVP |
| **Remote access** | Phone pairing with QR, an E2E-encrypted channel, an untrusted relay, and a separate permission principal. Post-MVP |
| **Design** | Three directions are mocked up (mockup since removed; in git history). **Owner choice pending**. *Superseded 2026-09-21: round 1 rejected; Island chosen (see Overlay concept and Mockup v3)* |
| **Name** | **Keep "KIVO"** (owner decision). The owner accepts the risk from existing "KIVO"/Kivo.ai/kivo.io uses ([research §8](research/features-and-extensions/REPORT.md)). A registry check (USPTO/EUIPO/WIPO/India) is recommended before the first public release |
| **No user keys** | Users never paste API keys or register developer apps. Connectors work like Claude Desktop's "Connect" flow: remote MCP with DCR/CIMD, KIVO-owned OAuth apps, or local means ([INTEGRATIONS_AND_PLUGINS.md §0](architecture/INTEGRATIONS_AND_PLUGINS.md)). Brains prefer CLI-agent logins, OpenRouter OAuth and local models; direct API keys are optional (advanced) |
| **Design round 1** | Rejected by the owner (too generic). Round 2 focuses on the **wake-up moment**, with four interaction concepts (kivo-wake-concepts.html, since removed; in git history): Native, Line, Island, Halo |
| **Overlay concept** | **Island (03)**: a black capsule at the top center that morphs into a card, then a compact live activity. It supersedes the bottom-center pill placement; bottom center stays available as a setting. The Control Center, onboarding and brand are designed next in the same language ([UX.md §2](architecture/UX.md)) |
| **Control Center layout** | **Sidebar with groups (A)**, **minimal Home (B)**, one comfortable density (no density option), **Blue** accent, with **ink primary buttons** (black in light mode, white in dark mode). The accent is for status, selection and focus only ([kivo-app.html](design/mockups/kivo-app.html)) |
| **Appearance settings** | Settings → Appearance lets users change the theme, the accent color (7 presets + custom), text size, transparency effects and motion. Settings → Island covers position, size, live transcript, live activities, wake glow and auto-hide |
| **Onboarding v2** | 10 screens in 5 phases (Welcome · Voice · Brain · Control · Ready) *(superseded: 11 steps, see **Onboarding** below)*, one decision per screen, recommended choices preselected, optional steps skippable, and an interactive "Try it" finish |
| **Permission modes** | Ask every time / Accept edits / Plan first / **Auto (default)** / Bypass permissions (explicit, time-limited, audited, BYPASS chip). Hard limits apply in every mode ([SECURITY.md §1.1](architecture/SECURITY.md)) |
| **Computer use UX** | Island controller (Pause/Stop, step and cost), target highlight, action callout with Allow/Skip in watch mode, optional KIVO cursor, and a configurable frame (Off/Subtle/Full) and limits ([CAPABILITIES.md §4.1](architecture/CAPABILITIES.md)) |
| **Conversation conveniences** | Undo in the Island, "What can I say?", follow-up without the wake word, target-app icon, Ctrl+K palette ([UX.md §8.1](architecture/UX.md)) |
| **People profiles** | Planned for after the MVP. Data is scoped by `profile_id` from M0 |
| **Mockup v3** | One Island-based theme: system font, macOS-style grouped lists, ink buttons, black Island surfaces (palette, toasts, onboarding welcome), accent only for state. Onboarding Finish opens the Control Center. [kivo-app.html](design/mockups/kivo-app.html) |
| **Startup wording** | "Open KIVO when Windows starts" (matches Windows' "Startup apps"), not "Start when I sign in" |
| **Voice confirmations** | Every Island decision can be answered by voice ("approve / cancel / wait / change it / why?"), with a local grammar (EN + HI), a listen window without the wake word, and a question earcon. High risk: voice triggers Windows Hello ([CONVERSATION.md §7](architecture/CONVERSATION.md)) |
| **Sound sets** | Soft (default), Glass, Pulse, Wood, Minimal, Custom; per-cue toggles; a separate notification sound |
| **Context** | A per-model context budget, with running-summary compaction, recall, and prompt caching; full history kept locally ([CONVERSATION.md §2](architecture/CONVERSATION.md)) |
| **Instructions** | KIVO keeps global and per-workspace instructions in its own data folder as editable Markdown. It reads project `CLAUDE.md`/`AGENTS.md` but never writes them unless asked |
| **Memory v2** | Built-in local knowledge graph (SQLite) with a Markdown mirror. Capture defaults to **Suggest**, with opt-in automatic workspace notes; no passive PC monitoring. KIVO shares memory with the agents it launches through its MCP server (with permission). Third-party memory MCPs are optional connectors, not bundled |
| **Driving other AIs** | ACP sessions (preferred), visible terminal agents (Windows Terminal + UIA typing), desktop AI apps (Claude Desktop, ChatGPT) via UIA, all with a voice-editable prompt draft before sending. Launching an agent in bypass/yolo mode is High risk |
| **Free options** | Local models, Gemini CLI (free Google sign-in, about 1,000/day), Codex with ChatGPT Free (small), OpenRouter free models. Labelled "Free" in the UI; KIVO never pays for users' AI |
| **Local model manager** | Download and delete speech-to-text, text-to-speech and embedding models in-app (Voice → Models on this PC) |
| **Sessions** | KIVO uses sessions internally and users never have to manage them: automatic continue/new, with summaries. Chat offers power users session management (new, pin, rename, search, context meter, compact) ([CONVERSATION.md §0](architecture/CONVERSATION.md)) |
| **Context layers** | System prompt → About me → Workspace → Live context → Memory → Skills index → Tools (lazy) → Conversation. About 1.5k tokens to start, cached after the first message (~90% cheaper), zero for the fast path. Settings → Context shows sizes and cost and previews what the AI sees ([CONVERSATION.md §8](architecture/CONVERSATION.md)) |
| **Skills** | Support the open Agent Skills standard (`SKILL.md`), with progressive loading. Skills from outside are reviewed before they're enabled ([CONVERSATION.md §9](architecture/CONVERSATION.md)) |
| **Onboarding** | Adds an optional "Connect your apps and tools" step (connectors, browser extension, MCP, skills), for 11 steps in total |
| **Releases** | GitHub Actions: version tags (set by hand since 2026-09-21; see "GitHub Actions only on version tags") → `tauri-action` matrix producing a Windows NSIS `.exe` + `.msi` (x64 and ARM64), a macOS universal `.dmg`, and Linux `.AppImage`/`.deb`/`.rpm`. Stable/Beta/Experimental channels, SBOM, smoke-install tests. Mac/Linux ship as previews until the ports land ([RELEASE.md](architecture/RELEASE.md)) |
| **Memory defaults** | **Suggest + Workspace notes both on** (each can be switched off). An Obsidian-compatible Markdown vault (front-matter tags, wikilinks, folders per type), a note detail level (Brief/Standard/Detailed), and an automatic tidy job (merge, condense, cap). A built-in memory MCP serves the vault to agents ([CONVERSATION.md §6](architecture/CONVERSATION.md)) |
| **Mac/Linux releases** | Built in CI, not published until the ports are done |
| **Navigation** | Grouped into 13 items with in-page tabs: Home, Chat, Tasks, Activity, Routines · Brains (Brains/Context), Agents, Voice, Extensions (Connectors/MCP/Skills) · Permissions (Mode/Capabilities/Privacy), Memory, Usage · Settings (General/Island & sounds/Performance/Diagnostics/About). *Tab lists superseded by **Settings structure** and **Extensions page** below* |
| **Settings structure** | Settings tabs: General (startup, language, profiles-later, export/import/reset) · Appearance (theme, accent, text size, animations, transparency) · Island (style, placement, what it shows, behavior) · Sounds · Notifications (speak out loud, quiet hours, sources) · Accessibility · Shortcuts · Performance · Diagnostics · About (updates, licenses). Personality moved to Voice |
| **Theme** | **Light by default**, with Light / Dark / System (follows Windows) |
| **Extensions page** | The Installed/Browse redesign was **reverted** at the owner's request. It keeps the tabbed layout (the explainer cards were removed later on 2026-09-21), now with four tabs: **Connectors · MCP servers · Plugins · Skills** (Plugins added). Every tab follows one pattern: a one-line description with actions, then grouped lists with counts (Connected / Built in / Available, Servers / Tools, Installed / Waiting for review) |
| **Discovery** | KIVO detects existing CLI agents, local model servers, desktop AI apps, signed-in CLIs (e.g. GitHub via `gh`), installed apps, MCP servers configured in other apps (Claude Desktop, Claude Code, Cursor, VS Code, Codex, Gemini CLI; imported, never modified) and skills folders. It suggests them and the user enables them. Supported CLIs can be installed in-app with consent. Every discovery section has "Checked … · Refresh" ([DISCOVERY.md](architecture/DISCOVERY.md)) |
| **Data freshness** | Event-driven first: pushed over IPC for live state, file watchers for configs/skills/memory, checks on page open with a max age, polling only while visible, daily signed catalogs, update check every 6 h. No background work while the Control Center is closed beyond what the runtime needs ([DISCOVERY.md §3](architecture/DISCOVERY.md)) |
| **Permissions and Extensions** | Mode / Capabilities (presets explained) / Privacy (where requests go, what always stays local, your data, what went to the cloud today). Extensions opens with a plain-language explainer of Connectors vs MCP servers vs Skills; adding an MCP server is a guided link-or-program flow with browser sign-in |
| **GitHub repo** | Applied: private vulnerability reporting, Dependabot alerts + security updates, secret scanning + push protection, description/topics, wiki off, squash-only merges + delete branch on merge, Discussions on, a ruleset on `main` (no force-push, no deletion) |

## 2026-09-21 — Owner answers (pre-M0)

| Topic | Decision |
|---|---|
| **License** | KIVO will be **open source**. Permissive (Apache-2.0/MIT) or copyleft (GPL/AGPL) is **undecided**. Until it is decided, the default dependency graph stays **permissive-compatible, with no GPL** (enforced by `cargo deny`), so both paths remain open. GPL components (espeak-ng, Piper) stay separately downloaded add-ons. Model attribution (CC-BY) and use restrictions (OpenRAIL) are shown in the app |
| **Cloud brains (M3)** | **All four:** Anthropic, OpenAI, Google Gemini, OpenRouter (plus the OpenAI-compatible local adapter) |
| **CLI agents (M3)** | **All three:** Claude Code (`claude-agent-acp`), Gemini CLI (`--acp`), Codex (`codex-acp`, with the App Server as a later richer adapter) |
| **Languages** | **Designed for all languages from day one** and implemented one at a time, each with proper testing. Order: **English → Hindi + Punjabi** (the owner can test these, including Hindi–English code-mixing, "Hinglish") → European languages → Japanese/Chinese/Korean → Arabic and other right-to-left languages. Languages the owner doesn't speak need native-speaker testers before they are marked supported |
| **Code signing** | Decide later. Dev and alpha builds are unsigned |
| **Low-end benchmark machine** | None available. M0 uses a **VM limited to 4 cores / 8 GB with no GPU**, and its results are labelled approximate. The owner's Windows 11 PC is the mid/high data point |
| **Emergency stop** | **Ctrl+Alt+Shift+Esc** confirmed |

## 2026-09-21 — Architecture baseline (Phase 0 specs)

**Decision:** Adopt the draft specs in [architecture/](architecture/ARCHITECTURE.md) as the Phase 0
baseline. Key points:

- **Processes:**
  - `kivo-runtime` is the per-user core. It owns the tray, audio, wake word, routing, tools,
    permissions and store.
  - `kivo-app` is the Tauri UI and can be restarted independently.
  - `kivo-infer` runs the supervised STT/TTS/embedding workers.
- **IPC:** a named pipe (user-only DACL, reject remote clients, session token) carrying JSON-RPC
  2.0, with TypeScript types generated from Rust.
- **State:** the runtime is authoritative. There is a typed event bus (tokio broadcast), and
  cancellation-token trees per turn and per task (cancel → silence ≤ 100 ms).
- **Storage:**
  - TOML config (versioned, migrated);
  - SQLite (rusqlite, WAL) holding activity, audit (hash-chained), tasks, memory (FTS5, with
    sqlite-vec later);
  - secrets in Credential Manager (`keyring`), and voice data encrypted with DPAPI.
- **Brains:**
  - own `BrainProvider` trait, with `genai` for breadth plus native adapters where needed;
  - an OpenAI-compatible adapter for local servers;
  - an **ACP client** for CLI agents;
  - a **KIVO MCP server** exposing KIVO tools to agents.
- **Intent:** a grammar fast path, then semantic exemplar matching, then the brain. Routing is
  deterministic and gives a reason.
- **Computer control:** the capability ladder, UIA on a dedicated MTA thread, a browser extension +
  native messaging, CDP only on a KIVO profile, Windows.Media.Ocr, SendInput as the last resort.
- **Security:** risk tiers × taint × profile policy. High risk always needs an on-screen or Windows
  Hello confirmation. Destination binding. Emergency stop on Ctrl+Alt+Shift+Esc (proposed).
- **Distribution:** NSIS per-user primary, MSI for IT, MSIX later; minisign-signed updates with
  rollback; models downloaded on demand with sha256 manifests; no GPL in the default dependency
  graph.
- **Roadmap:** vertical milestones M0–M9 ([ROADMAP.md](ROADMAP.md)).

Research: [architecture-and-platform/REPORT.md](research/architecture-and-platform/REPORT.md)

## 2026-09-21 — Custom wake words

**Decision:** Users can add, edit, rename, re-record, set per-word sensitivity for,
enable/disable and delete their own wake words, with several active at once. "Hey Kivo" is
the default.

- Flow: type the phrase → validator rates it Good / Fair / Risky (syllables, phonemes,
  confusability, collisions) → TTS plays it back so the user can adjust the pronunciation
  spelling → it works immediately via open-vocabulary keyword spotting → the user records 3–5
  samples to tune the threshold and build a second-stage verifier → optional background
  "Enhance this wake word" training.

Research: [voice-pipeline-engines/REPORT.md](research/voice-pipeline-engines/REPORT.md) §1

## 2026-09-21 — Voice pipeline direction (proposed defaults, pending benchmarks)

**Decision:** An all-local, permissively licensed pipeline on ONNX Runtime (`ort`) +
sherpa-onnx, with every engine behind a provider trait and the user choosing among tiers.
Defaults are **provisional** until measured on a low-end CPU-only Windows laptop.

| Stage | Proposed default | Alternatives offered |
|---|---|---|
| Wake word | Trained "Hey Kivo" model (openWakeWord pipeline, KIVO-owned) | sherpa-onnx KWS for custom words |
| Wake verification | Two-stage: verifier + CAM++ speaker check | — |
| VAD | Silero v6 | — |
| Echo cancellation | Windows 11 OS AEC where available, else WebRTC AEC3 (in-process) | — |
| STT | Moonshine v2 streaming (EN) / Parakeet TDT v3 int8 (multilingual) | Whisper turbo, Voxtral, Windows AI Speech, Apple SpeechAnalyzer, cloud (Deepgram Flux, AssemblyAI, OpenAI) |
| Turn detection | Silero pause + Pipecat Smart Turn v3 | Cloud engine's own endpointing |
| TTS | Kokoro-82M | Supertonic/Piper (Instant), Chatterbox (Expressive), system voices, cloud |

- **Excluded:** Picovoice (enterprise-only since 2026-06-30), openWakeWord's non-commercial
  pretrained models, GPL espeak-ng in the core, and Windows Narrator natural voices.
- **Speaker verification is a convenience filter only.** It never authorizes risky actions.
- **Enrollment:** 8 prompts (30–45 s) after explicit biometric consent. Stored locally and
  encrypted, and can be deleted. The profile grows from high-confidence matches.
- **STT personalization:** pick the engine by the user's measured WER, add custom vocabulary,
  and let the LLM repair transcripts. Per-user fine-tuning comes later.

Research: [voice-pipeline-engines/REPORT.md](research/voice-pipeline-engines/REPORT.md)

## 2026-09-21 — Platform strategy

**Decision:** Windows 11 is the primary, fully polished target. Windows 10 is
supported with graceful fallbacks. macOS and Linux are architected for from day
one, but ship later.

- The Rust runtime keeps all OS-specific code behind platform traits (audio I/O,
  echo cancellation, hotkeys, overlay window behavior, UI automation, app launching,
  notifications), with one implementation per OS. Core logic never calls Win32 directly.
- Tauri v2 already runs on all three desktop OSes, so the UI is shared.
- Windows 10 fallbacks: no Mica (solid or acrylic surface instead); no
  `SetEchoCancellationRenderEndpoint` (use software AEC plus gating the mic during chimes and TTS).
- Known hard parts, for later: macOS needs a non-activating `NSPanel`, the
  Accessibility (AX) API for UI control, and mic and accessibility permission prompts.
  On Linux under Wayland, global hotkeys go through the GlobalShortcuts portal,
  always-on-top overlays are compositor-dependent (layer-shell), and UI control goes through AT-SPI.

## 2026-09-21 — App presence and lifecycle

**Decision:** A tray-resident app with two surfaces: the main window (Control Center)
and a floating voice overlay.

- First launch opens the main window with onboarding. A launch at sign-in
  (opt-in, `--autostart`) starts hidden in the tray.
- Closing hides KIVO to the tray by default ("On close, keep KIVO running"), the same
  for X, Alt+F4 and the taskbar. The first close shows a one-time toast.
- A relaunch or a tray left-click always shows the main window, never the overlay.
- Single instance is enforced in both the Tauri app and the Rust runtime.

Research: [voice-ui-and-app-presence/REPORT.md](research/voice-ui-and-app-presence/REPORT.md)

## 2026-09-21 — Voice overlay

**Decision:** A pill that expands into a card, plus an optional edge glow.

- The pill sits at bottom center of the active monitor, is draggable, never takes
  focus, and draws zero frames while idle. *(Superseded 2026-09-21: the Island at top center;
  bottom center remains a setting. See "Overlay concept".)*
- The card grows out of the pill. It shows the live transcript, the streamed answer,
  action steps and confirmations, and accepts typed input.
- Edge glow: a short accent (about 400 ms) on wake, **off by default**, active
  monitor only. It is auto-disabled under reduced motion, Focus mode and fullscreen.
  It ships only after power measurements are acceptable.
- Overlay style setting: Pill + card / Pill only / Card only / Off.

## 2026-09-21 — Activation

**Decision:** The wake word "Hey Kivo" is the primary trigger. **Ctrl+Space** is
hold-to-talk (push-to-talk).

- Hotkey registration conflicts (for example IME switching on CJK layouts) are
  detected, and the user is prompted to rebind.
- Onboarding includes **voice enrollment**, like Siri's setup: the user speaks a few
  prompted phrases, which are used for:
  - wake-word personalization and speaker verification (respond to the owner's voice);
  - STT accuracy for the user's accent and speech patterns, where the engine supports it.
- Enrollment data stays local, is encrypted, and can be viewed and deleted.
- Both classic (non-LLM) and AI-model-based options are offered for the wake word and
  STT, and the user picks one. Defaults are chosen by measured speed and accuracy.
  (Engines researched; see "Voice pipeline direction" above.)

## 2026-09-21 — UI stack

**Decision:** React + shadcn/ui (Base UI) + Tailwind v4 + Motion, with the Inter or
Geist font. *(Superseded 2026-09-21: the system font, and Base UI styled by KIVO's own CSS
instead of shadcn. See "Implementation start".)* Voice visuals are adapted from ElevenLabs UI (MIT) and optionally the
LiveKit aura shader (Apache-2.0). The overlay shell, state machine and audio-level
bridge are custom.

## 2026-09-23 — M6 build

| Topic | Decision |
|---|---|
| **rmcp for MCP** | The official Rust SDK `rmcp` (Apache-2.0) is KIVO's MCP client and server: stdio child processes (`.cmd` shims run through `cmd /c`, no console window) and Streamable HTTP, with its OAuth support. The new `kivo-mcp` crate wraps it; the runtime never calls rmcp directly |
| **Tools are reviewed by hash** | Each MCP tool's approval is a SHA-256 of its name, description and input schema. A new or changed tool is shown on the Extensions page and not registered until the user approves it; `tools/list_changed` re-lists and compares. Tool ids are `mcp.<server>.<tool>` with names sanitized and `__` collapsed; default risk Medium (lowerable per tool); descriptions are labelled as the server's and capped at 1,000 characters, results at 64,000 and treated as untrusted. The approval card uses KIVO's own words, never the server's |
| **KIVO's MCP server is a bridge** | `kivo-runtime --mcp-server --agent <id>` speaks MCP on stdio and relays each call over the local IPC to the running runtime, so every call goes through the same permission engine (initiator `Mcp`); a call that would ask is refused while no turn is running. It is passed in each ACP session's MCP server list. Only agents the user turns on (Agents → Memory) get it, and it offers only the memory tools |
| **Sensitive memory** | Memories under `personal.*`, About me and past conversations are marked sensitive and removed from what agents see unless "Include personal memories" is on. CONV-24's memory tools work over the M5 memory (preferences, notes, conversations) until the M7 vault |
| **Connector catalog** | `crates/kivo-mcp/connectors.toml`: remote servers only where the publisher documents an official MCP endpoint with OAuth (GitHub, Notion, Linear, Atlassian, Stripe); local ones that need no sign-in (GitHub via `gh auth status`, Spotify, VS Code, the browser extension) and built-in media. Sentry and Asana left out until their endpoints are confirmed. Sign-in uses OAuth 2.1 with PKCE and dynamic client registration on a loopback redirect; tokens go to Credential Manager; KIVO never asks for a password or key |
| **Imports copy, never modify** | Other apps' MCP setups (Claude Desktop incl. the Store build, Claude Code, Cursor, VS Code, Codex, Gemini CLI) are read and copied; their files are never written. Secret-looking environment values move to Credential Manager. Every imported tool waits for review |
| **Skills trust** | Skills in KIVO's own folder are on; skills found in `~/.claude/skills` or a project's `.claude/skills` and imported folders or zips are off until reviewed. Zip import rejects paths outside the folder. Only a skill's name and description go with a request; `skills.load` (always offered while skills are on) loads its body; its scripts run through `shell.run` and ask like any command |
| **Plugins after 1.0** | The Plugins tab says plugins come after 1.0 (INT-06+ are Post); MCP servers and skills extend KIVO until then |

## 2026-09-24 — M7 build

| Topic | Decision |
|---|---|
| **The vault is the memory** | `crates/kivo-memory`: Markdown notes with front-matter (unknown keys kept), wikilinks and tags in `%APPDATA%\KIVO\memory\`. The files are the source of truth; SQLite (migration 10) is only the index, rebuilt by hash, and an edit made in Obsidian or any editor wins. Writes are atomic through `.kivo/tmp`. Secrets are removed before anything is written |
| **Instructions live in the vault** | "About me" is `about-me.md` and each workspace's instructions `workspaces/<slug>/instructions.md`; the M5 instruction files are moved there once, front-matter stripped when sent |
| **Memory budget** | ≤ 150 tokens per memory and 400 per request (MEM-08; CONVERSATION §8's "≤ 600" is the assembler's cap for the layer); an agent handoff gets ≤ 300. Guests get no memory tools and no recall |
| **Settings file** | Export writes `kivo.toml` plus routines to Downloads; import never brings grants or Bypass and keeps this PC's device choices; reset asks first |
| **Setup recommends, never decides** | `setup.recommend` preselects the brain, mode (Auto, the default), performance profile and privacy mode from RAM, GPU, battery, the network (`GetNetworkConnectivityHint`: offline, metered), agents and local servers found, and brains connected; each shown with its reason and changeable on the same screen. Metered connections don't start downloads |
| **Context settings** | A `[context]` section: layer switches (About me, workspace, memories, skills), the live fields sent, auto-compaction and its threshold (default: only when full), "Start each conversation fresh" (a new voice session never joins an earlier thread). They live on Brains → Context rather than a Settings tab, beside the budget they affect. The preview builds the next request with no side effects and redacts secrets; its cost estimate counts only the start-up layers |
| **Privacy by source and labels** | Two kinds of folder: `tools.private-folders` are never touched; `privacy.sensitive-folders` ("Keep on this PC") are readable but only a local brain sees their content. User labels ("Project Falcon" → sensitive) raise text's class. A request that asks to stay local ("…locally", "confidential") is routed like sensitive data. A tool result that is private is replaced, for a cloud brain, by a note saying it stayed on the PC, and Activity records it |
| **Custom privacy** | Custom has four switches: cloud brains (on), personal details to cloud (off), cloud speech (off), cloud screen (off). Sensitive data never goes to the cloud in any mode |
| **Contrast** | Text tokens are ≥ 4.5:1 on every surface: `--text-3` darkened from the mockup (light `#6E6E73`, dark `#8E8E93`), text-safe `--acc-text` and status `-text` tokens, light Amber `#BF7C00` for 3:1 as a control; a custom accent's text colour is computed. `contrast.test.ts` reads `tokens.css`. Windows contrast themes use system colours |
| **Mica** | Only on Windows 11 (build ≥ 22000) with transparency on; Windows 10 stays solid |
| **Hidden companion** | The Hidden Island style shows nothing but questions that need an answer; everything else is sounds and Activity |
| **UI tests on the dev build** | Playwright connects over CDP to the dev app's own WebView2 (`pnpm dev:e2e` opens a debugging port on 127.0.0.1 only); no browser is downloaded and the tests are read-only against the owner's KIVO |
| **Low-memory mode** | Speech models unload right after each request |
