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

## Platform

Windows 11 first and most polished; Windows 10 supported with fallbacks; macOS and
Linux planned. Keep every OS-specific call behind a platform trait in the runtime —
never call Win32 (or any OS API) directly from core logic.

## Status

Phase 0 (specification). No source code yet. Update this section as phases land.

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

## Working rules for Claude

- **Do not spawn or use subagents** (Agent/Task tool, workflows, or skills that
  delegate to subagents) in this project. Do research, exploration, and writing
  directly in the main session.

## Conventions

- Windows is the primary platform; line endings are normalized via `.gitattributes`.
- Keep this file current as the build, test, and lint commands come into existence.
