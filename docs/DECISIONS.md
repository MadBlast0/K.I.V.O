# KIVO decision log

Decisions made so far, newest first. Each links to the research behind it. When a
decision changes, update the entry and note the date. Do not silently rewrite it.

---

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
