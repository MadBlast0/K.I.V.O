# KIVO roadmap

Status: v1, 2026-09-21. This refines the master plan's §155 phases into **vertical milestones**,
per plan §154 ("user speaks → KIVO understands → performs one useful action → speaks back").
Each milestone has exit criteria, and a milestone is done only when all its criteria pass on the
reference hardware.

Specs: [architecture/](architecture/ARCHITECTURE.md) · Decisions: [DECISIONS.md](DECISIONS.md)

---

## Phase 0: Specification ✅ (this document set)

- [x] Product blueprint ([KIVO_Project_Plan.md](KIVO_Project_Plan.md))
- [x] Research: UI/lifecycle, voice pipeline, architecture/platform ([research/](research/))
- [x] Architecture, interfaces, state and event model ([ARCHITECTURE.md](architecture/ARCHITECTURE.md))
- [x] Security model ([SECURITY.md](architecture/SECURITY.md))
- [x] Benchmark requirements ([BENCHMARKS.md](architecture/BENCHMARKS.md))
- [x] Owner inputs answered (see "Owner inputs" below)

## M0: Foundations and measurement

**Goal:** a skeleton that builds, and the numbers needed to lock the engine defaults.

- **Workspace and CI:**
  - the Cargo workspace + pnpm UI per ARCHITECTURE §7;
  - CI (fmt, clippy, tests, `cargo deny`) on Windows, plus Linux/macOS checks for the portable
    crates.
- **Core scaffolding:**
  - `kivo-core` event and state types, and the session state machine with tests;
  - `kivo-ipc` transport (named pipe + token + JSON-RPC) and TS type generation;
  - `kivo-store` config (TOML + migrations), SQLite with migrations, and logging with
    redaction.
- **Runtime and app shells:**
  - a `kivo-runtime` that starts, holds the single-instance mutex, shows the tray, and serves IPC;
  - a `kivo-app` Tauri shell that connects, shows the runtime state, and can be restarted
    independently.
- **Spikes, measured with `kivo-bench`:**
  - **overlay:** a transparent, non-focusable, always-on-top pill and card. Measure the white
    flash, show latency, and idle/animating GPU power.
  - **audio:** WASAPI/cpal capture and playback, resampling, the AEC3 loop, Silero VAD, and idle
    CPU.
  - **wake:** sherpa-onnx KWS "Hey Kivo" versus a first openWakeWord-trained model, measuring
    false accepts per hour and false rejects.
  - **STT:** Moonshine v2 / Parakeet v3 / Whisper-turbo via transcribe-rs vs direct ort, on the
    low and mid tiers.
  - **TTS:** Kokoro / Supertonic / system voices, measuring first audio and RTF.
- **License checks:** the `uiautomation` crate, Smart Turn v3, and `interprocess` DACL support.

**Exit:**

- CI is green.
- The runtime and UI restart independently.
- The benchmark report is committed to `docs/benchmarks/`.
- The engine defaults are updated in DECISIONS.md with measured numbers.

## M1: Vertical slice, "push-to-talk → action → speech"

- Ctrl+Space push-to-talk, then streaming STT (the chosen default), with the transcript shown in
  the overlay.
- The fast path: open/close/focus app, volume/mute, media keys, screenshot, lock (grammar + app
  index).
- System TTS or Kokoro reply, with earcons.
- The overlay pill and card with all M1 states, plus Esc/stop cancellation (≤ 100 ms to silence).
- The lifecycle (UX §1): tray, close-to-tray + toast, relaunch focus, and an autostart option.
- The Activity log, the audit log, and the basic permission engine (Safe/Low allow, Medium/High
  confirm).

**Exit:**

- "Mute", "open Chrome" and "take a screenshot" work offline in ≤ 500 ms after the end of speech on
  the mid tier.
- Idle budgets are met.

## M2: Wake word, enrollment, conversation audio

- The "Hey Kivo" trained model plus two-stage verification, and the custom wake words flow (add,
  edit, test, record, sensitivity).
- AEC in production, barge-in, the command spotter ("stop" while speaking), and endpointing with
  Smart Turn.
- Voice enrollment + the speaker profile (prefer owner / owner only / off), with DPAPI storage and
  deletion.
- Onboarding steps 1–7.

**Exit:**

- Wake false accepts ≤ 0.5/h and false rejects ≤ 5% on the corpus.
- Barge-in works with speakers (no headset) on the reference laptop.

## M3: Brains

- `BrainProvider` with the Anthropic, OpenAI, Gemini and OpenRouter adapters, plus the
  OpenAI-compatible adapter for Ollama/LM Studio.
- An **ACP client** with all three CLI agents (Claude Code via `claude-agent-acp`, Gemini CLI via
  `--acp`, Codex via `codex-acp`); permission requests are routed to the KIVO confirmation UI.
- Profiles, deterministic routing with a reason, failover within the privacy class, and health
  checks.
- Streaming LLM → phrase chunker → streaming TTS; the text normalizer.
- The Brains screen: add/test/remove, profiles, keys stored in Credential Manager.

**Exit:**

- p50 end of speech → first audio ≤ 1.2 s on a cloud brain.
- Cancel works mid-stream.
- Killing the provider does not crash the runtime.

## M4: Tools and computer control

- The ToolSpec registry, the router with the capability ladder, and the app capability registry
  (core apps).
- UIA tools + events, files (Recycle Bin), clipboard, shell (Job Objects + risk parser), OCR, and
  screenshot.
- The browser extension + native messaging host (Chrome/Edge); CDP on a KIVO-managed profile.
- Taint tracking + destination binding; grants UI; the emergency stop (all triggers).
- `testenv/` + security test suite v1.

**Exit:**

- The UIA journey on the dummy app passes in CI.
- The injection test pages cannot trigger an outbound action.

## M5: Agents and background tasks

- Planner (`propose_plan`) → task graph, parallel steps, validation before claiming success.
- Coding tasks delegated to an ACP agent with progress in the card and the Tasks screen.
- Watchers (downloads, folders, processes, builds, reminders) with no LLM while waiting.

**Exit:** the plan §137 and §140 example journeys pass end to end.

## M6: MCP

- `rmcp` client, the MCP manager screen, per-server permissions, change detection, dynamic tool
  exposure.
- The KIVO MCP server for CLI agents.

**Exit:** the hostile-MCP fixture tests pass.

## M7: Control Center and memory (MVP complete)

- The full MVP screen set, onboarding 8–12, the memory store (explicit, FTS), and privacy modes +
  data classes.
- Local brain path verified (the Private/Offline profiles).

**Exit:** MVP checklist from plan §156 all green → **internal alpha**.

## M8: Beta hardening

- The companion (optional pointing via UIA bounds), the wake glow (if the power spike was OK), and
  the custom earcon motif.
- More STT/TTS choices, CLI discovery via the ACP registry, and the dependency manager (winget).
- Hybrid memory retrieval (sqlite-vec), performance profiles (Battery/Gaming), and GPU policy.
- The Performance dashboard and diagnostics bundle.

## M9: Release

- NSIS + MSI signed builds, the updater with channels and rollback, and THIRD_PARTY_NOTICES.
- A signed public beta, gathering SmartScreen reputation.
- The quality gates (plan §159) and the architecture invariants checklist all green.
- **Then:** the MSIX/package-identity spike, then the macOS port (platform crates), then Linux.

---

## Feature placement (added 2026-09-21)

| Feature | Spec | Milestone |
|---|---|---|
| Design direction, tokens, logo | [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) | **D0**, in parallel with M0; must finish before M1's UI work |
| Capabilities center (toggles, presets, indicators) | [CAPABILITIES.md](architecture/CAPABILITIES.md) | Core model in M1; full page in M4 |
| Usage and cost tracking + user-set limits and warnings | [BRAINS.md §9](architecture/BRAINS.md) | M3 |
| Personas (Calm / Friendly / Witty / Custom) | [BRAINS.md §10](architecture/BRAINS.md) | M3 |
| Screen awareness (on request, local OCR first) | [CAPABILITIES.md §3](architecture/CAPABILITIES.md) | M4 |
| Proactive speech rules, text-input hotkey | [UX.md §7–8](architecture/UX.md) | M5 (proactive), M1 (text input) |
| Routines and custom commands | [ROUTINES.md](architecture/ROUTINES.md) | Engine + builder in M5; schedule/event triggers and voice-created routines in M8 |
| Companion styles: Orb, Character | [UX.md §6](architecture/UX.md) | M8 |
| Realtime voice conversation mode | [BRAINS.md §8](architecture/BRAINS.md) | M8 |
| Computer use (opt-in, watch mode, caps) | [CAPABILITIES.md §4](architecture/CAPABILITIES.md) | M8 |
| Integrations: local paths (media, CLIs, UIA, extension) | [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) | M4 |
| Integrations: OAuth connectors (Google, Microsoft, Spotify, GitHub) | same | M8+ |
| WASM plugins | same §3 | Post-M9 (interfaces drafted in M4) |
| Permission modes (Ask / Accept edits / Plan / Auto / Bypass) | [SECURITY.md §1.1](architecture/SECURITY.md) | Engine in M1 (Ask/Auto); Accept edits + Plan in M4; Bypass in M5 |
| Undo, follow-up listening, target-app icon | [UX.md §8.1](architecture/UX.md) | M2 (follow-up), M4 (undo, icon) |
| "What can I say?" + Ctrl+K palette | same | M5 (needs the app registry), M7 (palette) |
| Computer-use controller UI and options | [CAPABILITIES.md §4.1](architecture/CAPABILITIES.md) | M8 |
| People profiles | [UX.md §8.2](architecture/UX.md) | Post-M9 (`profile_id` scoping from M0) |
| Voice confirmations and question/approved/cancelled cues | [CONVERSATION.md §7](architecture/CONVERSATION.md) | M2 (grammar and listen window); Hello hand-off in M4 |
| Sound sets | [VOICE.md §6](architecture/VOICE.md) | M2 (Soft set); other sets in M8 |
| Threads, context budgets, compaction, prompt caching | [CONVERSATION.md §1–2](architecture/CONVERSATION.md) | M3 |
| Instructions + workspaces | same §4 | M5 |
| Memory v2 (graph, Markdown mirror, capture modes, MCP sharing) | same §6 | M7 (explicit + Suggest), M8 (workspace notes, graph view) |
| Driving other AIs (ACP sessions, terminal agents, desktop AI apps, prompt drafts) | same §5 | M3 (ACP), M5 (terminal + draft), M8 (desktop apps) |
| CI (`ci.yml`, PR title check) + release-please | [RELEASE.md](architecture/RELEASE.md) | **M0** |
| Release matrix (NSIS/MSI/DMG/AppImage/deb/rpm) + nightly | same | Draft in M1 (Windows), full matrix by M9 |
| Context page, preview, session management UI | [CONVERSATION.md §0, §8](architecture/CONVERSATION.md) | M3 (budgets), M7 (UI) |
| Skills (Agent Skills standard) | same §9 | M6 (with MCP) |
| Model manager (download/delete) | [DISTRIBUTION.md §4](architecture/DISTRIBUTION.md) | M1 |
| KIVO Remote (phone) | same §4 | Post-M9 track |

## Owner inputs (answered 2026-09-21)

All the pre-M0 questions are answered ([DECISIONS.md → Owner answers](DECISIONS.md)):

- open source, with the license type still to choose (no GPL in the core until then);
- **all four** cloud providers and **all three** CLI agents in M3;
- design for all languages, shipping English → Hindi + Punjabi → European → CJK → RTL;
- signing decided later;
- a low-end VM for benchmarks;
- Ctrl+Alt+Shift+Esc as the emergency stop.

**Still open (not blocking):** the final license (needed before the first public release) and the
signing route (needed before the public beta).

## Language milestones (run in parallel with M3+)

| Step | Scope | Exit |
|---|---|---|
| L1 | English (ships with M1/M2) | Budgets met on the English test sets |
| L2 | **Hindi + Punjabi**, including Hindi–English code-mixing | Research note + owner-recorded test sets; STT WER and wake-word metrics meet targets; fast-path grammar in Hindi |
| L3 | Major European languages | Per-language test sets; native-speaker review |
| L4 | Japanese, Chinese, Korean | CJK engines, IME-safe hotkeys, native-speaker review |
| L5 | Arabic and other RTL | RTL UI pass, engines, native-speaker review |
