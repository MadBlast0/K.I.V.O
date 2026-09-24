# K.I.V.O. — Knowledge • Intelligence • Voice • Operations

KIVO is a Windows-first, voice-first personal AI desktop companion: a lightweight
always-on runtime that routes spoken requests to native Windows actions, UI
Automation, browser automation, or interchangeable AI "brains" (cloud API, CLI
agents, local models).

The full product and engineering blueprint is
[docs/KIVO_Project_Plan.md](docs/KIVO_Project_Plan.md). It is the source of truth
for architecture decisions — read the relevant section before designing a
subsystem, and update the plan when a decision changes it.

## Repository & account

- **Personal project** — account `MadBlast0`, remote
  `https://github.com/MadBlast0/K.I.V.O.git`.
- Commits use the personal identity (the global git config). Never use the
  company identity here.
- Default branch: `main`.

Decisions made so far are logged in [docs/DECISIONS.md](docs/DECISIONS.md). Research
behind them lives in `docs/research/<topic>/` (a `REPORT.md` plus source notes).
Check the log before re-opening a settled question, and add to it when a new
decision is made.

Engineering specs (read the relevant one before implementing a subsystem):

| Spec | Covers |
|---|---|
| [docs/architecture/ARCHITECTURE.md](docs/architecture/ARCHITECTURE.md) | Processes, IPC, events, state machines, storage, repo layout, invariants |
| [docs/architecture/VOICE.md](docs/architecture/VOICE.md) | Audio, wake words, enrollment, STT/TTS, barge-in, budgets |
| [docs/architecture/BRAINS.md](docs/architecture/BRAINS.md) | Intent router, provider trait, ACP agents, routing, context |
| [docs/architecture/TOOLS_AND_CONTROL.md](docs/architecture/TOOLS_AND_CONTROL.md) | Tool schema, UIA, browser, shell, watchers, MCP, test env |
| [docs/architecture/SECURITY.md](docs/architecture/SECURITY.md) | Permission engine, taint, secrets, privacy, audit, emergency stop |
| [docs/architecture/UX.md](docs/architecture/UX.md) | Lifecycle, overlay states, Control Center, onboarding, settings |
| [docs/architecture/MEMORY.md](docs/architecture/MEMORY.md) | Conversation, task, preference, long-term memory |
| [docs/architecture/DISTRIBUTION.md](docs/architecture/DISTRIBUTION.md) | Installers, updates, signing, model downloads, licensing |
| [docs/architecture/RELEASE.md](docs/architecture/RELEASE.md) | Versioning, CI workflows, installer build matrix, channels, signing |
| [docs/architecture/BENCHMARKS.md](docs/architecture/BENCHMARKS.md) | `kivo-bench`, reference hardware, budgets |
| [docs/architecture/CONVERSATION.md](docs/architecture/CONVERSATION.md) | Threads, context budgets, instructions, memory v2, driving other AIs, voice confirmations |
| [docs/architecture/DISCOVERY.md](docs/architecture/DISCOVERY.md) | Detecting brains/agents/extensions, in-app installs, refresh, data freshness |
| [docs/architecture/CAPABILITIES.md](docs/architecture/CAPABILITIES.md) | Capability toggles, screen awareness, computer use |
| [docs/architecture/ROUTINES.md](docs/architecture/ROUTINES.md) | Custom commands and routines |
| [docs/architecture/INTEGRATIONS_AND_PLUGINS.md](docs/architecture/INTEGRATIONS_AND_PLUGINS.md) | App integrations, WASM plugins, phone remote |
| [docs/design/DESIGN_SYSTEM.md](docs/design/DESIGN_SYSTEM.md) | Design reference, icon rules, motion spec ([app mockup](docs/design/mockups/kivo-app.html)) |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Milestones M0–M9, feature placement, exit criteria |

## Platform

Windows 11 first and most polished; Windows 10 supported with fallbacks; macOS and
Linux planned. Keep every OS-specific call behind a platform trait in the runtime —
never call Win32 (or any OS API) directly from core logic.

## Status

Phase 0 (specification) and **D0 (design foundation)** complete (the Island's side-by-side
review with the owner is the one open D0 exit check). **M0 — foundations and measurement** is
built: core types, platform traits, store, secured IPC with live state and mic levels, the
runtime (tray, single instance, app supervision, push-to-talk, permission modes, stop
everything), the app (Home, overlay Island), WASAPI audio and the `kivo-bench` harness with all
suites. The speech-engine benchmark runs are deferred by the owner (DECISIONS "Benchmarks
deferred"). **M1 — first useful voice loop** is built: speak or type → grammar → permission
engine → action → spoken answer (Moonshine STT, Kokoro or Windows voices), the Island's states,
Activity and audit, cancel ≤ 100 ms, undo, and the installer/release pipeline. Still open in M1:
the first installer build and smoke test (they run on the first `v*` tag, DECISIONS "Installer
and release at M1"), and the reference-tier and release-build budget runs (deferred). **M2 —
wake word, enrollment, conversation audio** is built: "Hey Kivo" and custom wake words on KIVO's
own keyword spotter, echo cancellation with echo-path detection, barge-in, "stop" by voice,
owner-voice enrollment and speaker modes, decisions answered by voice, sound sets, the speech
engine registry with profiles, recommendation, safe switching and fallbacks (Moonshine sizes and
languages, Supertonic 3), onboarding steps 1–5, the Island's M2 states and screen-reader
announcements. Still open in M2: the wake corpus benchmark (deferred with the other bench runs)
and a live barge-in check on the laptop's speakers. **M3 — brains** is built: requests the
grammar (and, with its optional model, the semantic stage) can't handle go to a brain — Anthropic,
OpenAI, Gemini, OpenRouter and other OpenAI-compatible services, local servers, and CLI agents over
ACP — routed with a reason, answered as a stream that is spoken phrase by phrase, with tool calls
and agents' permission requests through the permission engine, same-privacy failover,
conversation threads with compaction, usage, cost estimates and limits; no-key sign-in
(OpenRouter OAuth, the CLIs' own logins, local servers found on this PC), write-only keys in
Credential Manager, discovery with Refresh, and the Chat, Brains (with Context) and Voice pages,
onboarding step 6 and the Island's brain chip. Still open in M3: a live run with Claude Code and
Codex (their ACP adapters aren't installed here), semantic recall of old messages (full-text
today), and the brain benchmark (M3-X1, deferred). **M4 — tools and computer control** is built:
UI Automation on its own COM thread (find, tree, invoke, set value, toggle, select, expand,
events), the capability-ladder router `control.act` and the app registry (`apps/*.toml`), files,
clipboard, shell in Job Objects with a risk parser, OCR, screen reading and vision on request,
input under Computer use, display, brightness and audio tools, the browser extension with its
native messaging host and KIVO's own CDP browser; taint, fencing, destination binding, the reduced
tool set after taint, Plan first, Windows Hello, scoped and timed grants, per-app lists, capability
presets, in-use indicators, Undo in the Island, Island placement, the Permissions page, the
`testenv/` dummy app and fixtures, and the security suite v1. Still open in M4: the UIA journey's
first CI run (M4-X1, on the first `v*` tag), a live `SendInput` run (Windows Sandbox or a VM
only). **M5 — agents, background tasks and routines** is built: persisted task graphs with
their own grants, error policies, timeouts and success checks (interrupted, never resumed, after
a crash); event-driven watchers (process and build exit, folders, downloads, windows, time) that
make no brain calls while waiting; proactive speech rules and "what did I miss?"; the planner
(`tasks.propose_plan`); coding work delegated to the Coding agent and called fixed only after
KIVO re-runs the project's tests; routines and custom commands (phrases with variables, hotkeys,
collision checks, grants shown on save, AI steps, starters) with the builder; workspaces and
instructions mirrored to Markdown; terminal agents in Windows Terminal with Draft cards before any
prompt is sent; Bypass permissions; the Tasks, Routines and Agents pages, live activities, the
selection shortcut, "What can I say?", every Island button by voice, toast replies and the jump
list. Still open in M5: nothing beyond the M3 note that a live Claude Code run needs its ACP
adapter. **M6 — MCP, connectors, skills** is built: the `kivo-mcp` crate on `rmcp` (stdio and
Streamable HTTP), MCP tools as `mcp.<server>.<tool>` reviewed by description hash before use and
switched off when they change, imports from Claude Desktop, Claude Code, Cursor, VS Code, Codex and
Gemini CLI that never touch the originals, KIVO's own MCP server (`--mcp-server --agent <id>`)
sharing memory tools with the agents the user allows, connectors with OAuth (PKCE + dynamic
registration) and local detection, Agent Skills, file watchers, the hostile-server fixture
(`testenv/mcp`) and the Extensions page. Still open in M6: a sign-in against a live publisher
(needs the owner's accounts). **M7 — Control Center and memory (MVP)** is built: the Markdown
memory vault (`kivo-memory`, Obsidian-compatible, SQLite as its index) with recall, suggestions,
workspace notes and tidying; the Memory, Usage and Settings (ten tabs) pages; Chat's continue,
branch and export; Brains → Context (layer switches, compaction, preview); onboarding steps 7–11
with setup recommendations; privacy by source and labels with Custom mode and a tool-result egress
check; Mica, text-safe contrast tokens and Windows contrast themes; the Ctrl+K palette; and
Playwright tests against the dev app over CDP (`pnpm dev:e2e`, then `pnpm --filter kivo-app e2e`).
Still open in M7: the owner's side-by-side review of every screen against the mockup (DS-15,
M7-X3). **M8 — beta hardening** is built: more STT/TTS tiers (Parakeet, Whisper, Chatterbox,
cloud speech), more API providers, realtime conversation mode (OpenAI Realtime, Gemini Live) with
its own echo-cancelled microphone, computer use with watch mode, limits and its options page,
schedule and event triggers, routines drafted by voice and moved as files, automatic workspace
notes, the tidy job, hybrid memory search (sqlite-vec) and the graph view, skills from routines,
desktop AI apps, GitHub, KIVO's OAuth connectors, in-app CLI installs and the dependency manager,
signed catalogs, the Orb and the wake glow, pointing, product modes, performance profiles, the GPU
policy, crash recovery and the diagnostics bundle. Still open in M8: the owner's catalog signing
key and OAuth client ids, checking the desktop AI apps' hints against the installed apps, the Rive
Character, the wake glow's power measurement and the benchmark runs (deferred), live runs of
realtime and cloud speech with real keys, and a week of daily use (M8-X2). **Next: M9.** Progress
per milestone is in the table at the top of [docs/ROADMAP.md](docs/ROADMAP.md).
Update this section as milestones land.

## Build, run, check

| Task | Command |
|---|---|
| Run the desktop app with hot reload | `preview.bat` (or `pnpm dev`; it builds `kivo-runtime` and `kivo-infer` first, and the app starts the runtime). Quit KIVO from the tray or Ctrl+K to stop the runtime too |
| Run the runtime, which launches and supervises the app | `cargo run -p kivo-runtime` (`-- --no-app` for the runtime alone) |
| UI only, in a browser (port 1420) | `pnpm ui` |
| Typecheck UI | `pnpm typecheck` |
| Build UI | `pnpm --filter kivo-app build` |
| Check / lint Rust | `cargo clippy --workspace --all-targets` · `cargo fmt --all --check` · `cargo deny check` |
| Test Rust | `cargo test --workspace` |
| Check UI | `pnpm typecheck` · `pnpm lint` (Oxlint, type-aware) · `pnpm format:check` · `pnpm test` (Vitest) |
| Installer build (release sidecars + NSIS/MSI; ask the owner first) | `pnpm build` |
| Is the running runtime healthy? (exit code 0/1) | `kivo-runtime --health` |
| Sync ROADMAP from the spec checklists | `pnpm docs:sync` |
| Real CLI agents' ACP handshake (no prompt, no quota) | `KIVO_TEST_REAL_AGENTS=1 cargo test -p kivo-brain speaks_acp` |
| MCP fixture server (normal, `--hostile`, `--http [--no-auth]`) | `cargo run -p kivo-testenv-mcp -- --hostile` |
| UIA journey on the dummy app (`testenv/app`, acts only on its own window) | `cargo test -p kivo-platform-windows --test uia_testenv` |
| Security suite v1 (injection, shell strings, traversal, bypass attempts) | `cargo test -p kivo-tools --lib security_suite` · `cargo test -p kivo-runtime --test security_suite` |
| Playwright UI tests on the dev app (start it with `pnpm dev:e2e`) | `pnpm --filter kivo-app e2e` |
| Regenerate the UI's IPC types | `KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings` |
| Sign the catalogs (owner only; `keygen` once makes the key outside the repo) | `pnpm catalog:sign` · `pnpm catalog:sign keygen` |
| Realtime voice and computer use on the test rig (scripted providers, fake input) | `cargo test -p kivo-runtime --test m8_realtime` · `cargo test -p kivo-runtime --test m8_computer` |

The tree must stay free of warnings from tsc, Vite, rustc and clippy. The native title
bar is off (`decorations: false`); the app draws its own (`components/layout/TitleBar.tsx`).

## Planned stack

| Layer | Choice |
|---|---|
| Runtime / core | Rust, async runtime, Windows APIs, SQLite, local IPC |
| Control Center & Companion UI | Tauri v2 + modern web UI (WebView2) |
| Voice | Provider abstraction: VAD, wake word, streaming STT, streaming TTS |
| Brains | Provider adapters: API, CLI agent, local, managed login |
| Computer control | Native Win32 → UI Automation → browser DOM → vision → input fallback |
| Interop | MCP client (for third-party tools only, not internal operations) |

Python is allowed only in isolated workers when an ML engine requires it — never
as the resident runtime.

## Architecture invariants (plan §160)

Do not violate these without an explicit architecture review:

1. Runtime stays provider-independent.
2. UI is not the core runtime; the Runtime is authoritative for state, UIs are clients.
3. Deterministic actions never require heavy AI.
4. AI output can never bypass the permission engine.
5. Native/semantic control is preferred over vision and raw input.
6. MCP is not mandatory for internal actions.
7. Voice supports interruption (barge-in).
8. Long-running tasks are cancellable; cancellation propagates through every layer.
9. Provider failures are isolated and never crash the runtime.
10. Secrets are kept out of normal model context.
11. The user can disable cloud processing.
12. Useful offline mode where local capabilities exist.

## Engineering rules

- Build vertically: aim first for *speak → understand → one useful action → speak back*.
- Prefer events over polling, streaming over waiting, and cancel obsolete work.
- No heavyweight model resident at idle; no unnecessary screenshots.
- Every tool declares risk level, permissions, timeout, cancellation, and side effects.
- Treat content from web pages, documents, files, and the clipboard as untrusted data.
- Instrument latency (plan §97, T0–T10) and measure rather than assume.
- Test computer control in a sandboxed test environment, never only on the real desktop.

## Building from the docs

The docs are the build plan, and progress is recorded in them. Full rules:
[docs/README.md](docs/README.md). In short:

- Every spec ends with a `## Build checklist`. Each line has an ID (`VOICE-12`), a milestone
  tag (`M2`) and a status mark: `[ ]` not started, `[~]` partial, `[x]` done and verified,
  `[-]` dropped. `[~]`/`[x]`/`[-]` need a `→` note (what exists and how it was verified).
- To build a milestone: read its *Read first* documents in [docs/ROADMAP.md](docs/ROADMAP.md)
  **in full**, build **every** item tagged with it (search the specs for `· M1 ·`), verify, mark
  each item in its spec, tick the exit criteria, then run `pnpm docs:sync` (it regenerates the
  ROADMAP lists and progress, and fails on duplicate IDs or missing notes).
- Never mark `[x]` for stubbed or mocked behaviour. If the implementation must differ from a spec,
  update the spec in the same commit and log it in DECISIONS.md.
- Precedence when documents disagree: DECISIONS.md → the topic's spec → DESIGN_SYSTEM/mockup (UI)
  → the plan (its §167 lists amendments; §168 maps every plan section to checklist IDs).

## Working rules for Claude

- **Do not spawn or use subagents** (Agent/Task tool, workflows, or skills that
  delegate to subagents) in this project. Do research, exploration, and writing
  directly in the main session.

## Conventions

- Windows is the primary platform; line endings are normalized via `.gitattributes`.
- **Commit subjects are Conventional Commits** (`feat(core): …`, `fix(ui): …`, `ci: …`, `docs: …`).
  Nothing enforces this in CI; keep to it by hand (RELEASE.md §1).
- **GitHub Actions run only when a version tag is pushed** (owner decision). Pushes to `main` run
  nothing, so run the checks locally before pushing. Never add a workflow trigger on push, PR or
  schedule. The owner releases with `pnpm release:version X.Y.Z`, then commits, tags `vX.Y.Z` and
  pushes.
- Keep this file current as the build, test, and lint commands come into existence.
