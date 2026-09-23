# KIVO system architecture

Status: **Draft v1 (Phase 0)**, 2026-09-21. This is the engineering specification that sits under
the [master plan](../KIVO_Project_Plan.md).
It defines processes, boundaries, interfaces, the state and event models, storage, and repo layout.
Companion specs:

- [VOICE.md](VOICE.md): audio, wake word, STT, TTS
- [BRAINS.md](BRAINS.md): AI providers and routing
- [TOOLS_AND_CONTROL.md](TOOLS_AND_CONTROL.md): tools and computer control
- [SECURITY.md](SECURITY.md): permissions, secrets, prompt injection, audit
- [UX.md](UX.md): lifecycle, overlay, Control Center, settings
- [CONVERSATION.md](CONVERSATION.md), [MEMORY.md](MEMORY.md): threads, context, memory
- [CAPABILITIES.md](CAPABILITIES.md), [ROUTINES.md](ROUTINES.md), [DISCOVERY.md](DISCOVERY.md), [INTEGRATIONS_AND_PLUGINS.md](INTEGRATIONS_AND_PLUGINS.md)
- [RELEASE.md](RELEASE.md), [BENCHMARKS.md](BENCHMARKS.md), [../design/DESIGN_SYSTEM.md](../design/DESIGN_SYSTEM.md)
- [DISTRIBUTION.md](DISTRIBUTION.md): packaging, updates, models, signing
- [../ROADMAP.md](../ROADMAP.md): milestones

Decisions are recorded in [../DECISIONS.md](../DECISIONS.md).

---

## 1. Process model

```text
                     ┌────────────────────────────── user session ──────────────────────────────┐
                     │                                                                           │
 autostart / launch  │   ┌────────────────────────────┐   named pipe (JSON-RPC,   ┌────────────┐  │
 ───────────────────►│   │ kivo-runtime  (Rust, tokio) │◄── per-user DACL) ──────►│ kivo-app   │  │
                     │   │ - supervisor & lifecycle    │                          │ (Tauri 2)  │  │
                     │   │ - tray icon                 │                          │ - overlay  │  │
                     │   │ - audio capture/playback,   │                          │   window   │  │
                     │   │   AEC, VAD, wake word       │                          │ - Control  │  │
                     │   │ - intent & brain routing    │                          │   Center   │  │
                     │   │ - tools, permissions, tasks │                          │ - onboard. │  │
                     │   │ - event bus, state, store   │                          └────────────┘  │
                     │   └──────┬──────────────┬──────┘                                          │
                     │          │ IPC          │ stdio (ACP / JSON-RPC)                           │
                     │   ┌──────▼──────┐  ┌────▼──────────────┐  ┌──────────────────────────┐    │
                     │   │ kivo-infer  │  │ CLI agents        │  │ MCP servers (stdio/HTTP) │    │
                     │   │ worker(s):  │  │ (claude-agent-acp,│  │ + tool child processes   │    │
                     │   │ STT/TTS/LLM │  │  gemini --acp, …) │  │ (shell, in Job Objects)  │    │
                     │   │ /embeddings │  └───────────────────┘  └──────────────────────────┘    │
                     │   └─────────────┘                                                         │
                     └───────────────────────────────────────────────────────────────────────────┘
```

| Process | Role | Lifetime | Crash policy |
|---|---|---|---|
| **kivo-runtime** | The product core, and the only authority for state. Owns the mic, speaker, tray, event bus, routing, tools, permissions, tasks, store and secrets access | Always running while KIVO is "on" | Restarted by the next launch or autostart; crash dumps plus a report on the next start |
| **kivo-app** | The Tauri UI shell: the overlay window (preloaded, hidden) and the Control Center window (created on demand). **No business logic and no secrets** | Launched by the runtime; can restart without stopping KIVO | The runtime relaunches it with backoff |
| **kivo-infer** | Model inference worker(s): STT, TTS, local embeddings, and optionally an embedded local LLM. Same binary, run as a subcommand | Started on demand and kept warm per residency policy (plan §93) | The runtime restarts it; the current turn fails gracefully ("I lost my voice engine, retrying") |
| CLI agents | External brains over ACP / Codex App Server | Per session or task | Isolated; errors are normalized by the adapter |
| Tool children | Shell commands and helper processes | Per tool call | Run inside a Windows **Job Object** (kill-on-close, memory and CPU limits) |

**Why the tray lives in the runtime:** the tray is the "KIVO is on" indicator and the path to Quit
and Pause listening. It must survive a UI crash, so it is built with the `tray-icon` crate on a
runtime thread that has a Win32 message loop.

**Why audio, VAD and wake word run in the runtime but STT and TTS do not:**

- The always-on path is small, stable and latency-critical, so it stays in-process.
- Larger ONNX and GPU models are the likeliest source of crashes and memory growth, so they are
  isolated (plan §8.2 and §8.3).
- Audio moves to kivo-infer as 16 kHz mono PCM frames over the IPC channel. Shared memory is an
  optimization for later.

**The runtime is not a Windows service.** It needs the interactive desktop, UIA and foreground
context, so it is a normal per-user process.

### Launch and single instance

- `KIVO.exe` is the Tauri app, the user-facing binary and Start-menu entry.
  - On start, it tries to connect to the runtime pipe.
  - If the runtime is absent, it spawns `kivo-runtime.exe` (bundled as a sidecar) detached, then
    connects.
- The runtime holds a named mutex, `Local\KIVO.Runtime.<user-sid>`, so a second runtime exits
  immediately.
- The UI uses `tauri-plugin-single-instance`. A second `KIVO.exe` forwards "show main window" and
  exits.
- **Autostart** registers `kivo-runtime.exe --autostart`. The runtime starts, then launches
  `KIVO.exe --background`, which preloads the overlay without showing a window.
- **Low-memory mode (setting):** the runtime does not launch kivo-app until the overlay is first
  needed. This trades the first-show latency for about 100+ MB less idle memory; M0 measures the
  real figure.
- **Quit** (from the tray or the Control Center) stops the runtime, which closes the UI and
  releases the mic. **Closing a window never quits** (see [UX.md](UX.md)).

## 2. Platform abstraction

All OS-specific code sits behind traits in `kivo-platform`. Core crates never call Win32, AppKit or
D-Bus.

| Trait | Windows implementation (v1) | macOS (later) | Linux (later) |
|---|---|---|---|
| `AudioIo` | WASAPI (windows-rs) with a cpal fallback | CoreAudio via cpal | PipeWire via cpal |
| `EchoCancel` | OS AEC (`IAcousticEchoCancellationControl`, Win11 22621+) or WebRTC AEC3 | WebRTC AEC3 | WebRTC AEC3 |
| `Hotkeys` | RegisterHotKey plus an optional low-level hook | CGEventTap / Carbon | XDG GlobalShortcuts portal |
| `Tray` | tray-icon | tray-icon | tray-icon (AppIndicator) |
| `Apps` | Start-menu index, AUMID activation, ShellExecute | NSWorkspace | .desktop files |
| `Windows` | EnumWindows, UIA, DWM | AX API | AT-SPI / compositor-specific |
| `UiAutomation` | UIA (`uiautomation` crate) | AXUIElement | AT-SPI |
| `Input` | SendInput | CGEvent | libei / portal |
| `Screen` | Windows.Graphics.Capture | ScreenCaptureKit | PipeWire portal |
| `Ocr` | Windows.Media.Ocr | Vision framework | Tesseract (optional) |
| `Secrets` | Credential Manager via keyring, plus DPAPI for blobs | Keychain | Secret Service |
| `SystemInfo` | Power, battery, GPU, NPU, fullscreen/Focus detection | IOKit | sysfs / UPower |
| `Notifications` | Toasts (tauri-plugin-notification) | UNUserNotificationCenter | libnotify |

Windows 10 differences are handled inside the Windows implementation through capability checks at
startup (the `Capabilities` struct). They are never scattered through core code.

## 3. IPC

- **Transport:** tokio named pipes on Windows (`\\.\pipe\kivo-<user-sid>`); Unix-domain sockets
  (mode 600, in the private run folder) on other platforms.
  - The pipe gets a protected security descriptor that grants only the current user's SID, plus
    `PIPE_REJECT_REMOTE_CLIENTS`, and is created "first instance only", so a process squatting
    the name makes the runtime fail loudly instead of connecting to it.
  - The runtime writes a random 256-bit **session token** to
    `%LOCALAPPDATA%\KIVO\run\session.token` (user-only ACL). Clients must present it in
    `hello`.
- **Framing:** length-prefixed frames. The payload is **JSON-RPC 2.0**, encoded with serde.
  - Messages come in three kinds: `request`/`response`, `notification` (events), and a separate
    high-rate **`levels`** notification carrying audio levels for the overlay at 30–60 Hz, only
    while the overlay is animating.
- **Versioning:** `hello { protocol_version, client, token }` must be the first message, within 3 s.
  The runtime rejects a wrong token and incompatible major versions with a clear error (for
  example after a partial update). Frames over 1 MiB close the connection.
- **Schema source of truth:** Rust types in `kivo-ipc` and `kivo-core`. TypeScript types are
  generated with **`ts-rs`** into `apps/kivo-app/src/ipc/generated.ts`
  (`KIVO_WRITE_TS=1 cargo test -p kivo-ipc --features ts --test ts_bindings`), and CI fails if
  the generated types are stale.
- **Reconnect:** the UI reconnects with backoff and re-subscribes. On reconnect the runtime sends a
  full `StateSnapshot`, then deltas.
- **The same protocol runs between the runtime and kivo-infer**, with added methods for streaming
  audio frames and results.

## 4. Core model

### 4.1 Event bus

- **Transport:** a typed `enum Event` in `kivo-core`, sent over `tokio::sync::broadcast` inside
  the runtime.
- **Event groups** (this extends plan §66):
  - `Voice(WakeDetected | SpeechStarted | PartialTranscript | FinalTranscript | SpeechEnded | TtsStarted | TtsStopped | BargeIn)`
  - `Turn(Started | IntentDetected | BrainSelected | Completed | Cancelled | Failed)`
  - `Tool(Requested | PermissionDecided | Started | Progress | Completed | Failed)`
  - `Task(Created | StepChanged | Waiting | Completed | Cancelled | Failed)`
  - `System(WindowChanged | FullscreenChanged | FocusModeChanged | PowerChanged | DeviceChanged | NetworkChanged | FileChanged | BrowserChanged | ModelChanged | ModelResidency | ShuttingDown)`
  - `Provider(HealthChanged | RateLimited | AuthFailed)`
  - `Ui(OverlayShown | OverlayHidden | UserConfirmed | UserCancelled)`
- **Metadata:** every event carries `ts` (monotonic plus wall clock), `turn_id` / `task_id`
  where relevant, and `trace_id`.
- **Persistence:** the Activity recorder writes the **Activity** subset to SQLite (plan §83) and
  the **Audit** subset (see [SECURITY.md](SECURITY.md)) as the turn engine produces them. It is
  called directly rather than subscribing to the bus, because Activity needs details the bus
  events don't carry (the permission decision, the step's arguments, the reply text).

### 4.2 State machines

**Assistant session state** (it drives the overlay, companion, sounds and tray icon, per plan §75):

```text
Idle ─wake/hotkey─► Listening ─endpoint─► Thinking ─┬─► Acting ─┬─► Speaking ─► (FollowUp ─timeout─► Idle)
  ▲                    │                            │           │        │
  │                    └─cancel/"stop"──────────────┴───────────┴────────┴──► Interrupted ─► Idle | Listening
  └───────────────── Paused (mic off) ◄── tray/voice ──►  AwaitingConfirmation ◄─ policy
                                                           Error ─► Idle (after message)
```

- A brain's streamed answer can start speaking before it calls a tool, so Speaking also moves to
  Acting and to AwaitingConfirmation (BRAIN-28; DECISIONS "Speaking can move to Acting").
- The runtime owns the machine; UIs render the `SessionState` they receive.
- An invalid transition is a bug, and it is logged at `error`.

**Turn:** one user utterance or text input, from start to response.

- It has a root **`CancellationToken`** (`tokio_util`). Child tokens pass to the STT stream, the
  brain request, each tool call, and the TTS stream.
- "Stop", Esc, the overlay X, the emergency stop and barge-in all cancel the turn token.
  **Cancellation must reach every layer within 100 ms**; an M1 test enforces this.

**Task:** long-running work (plan §41 and §67). It holds a task graph of steps with dependencies and
has its own token. Tasks outlive turns.

- Task state is persisted, so the runtime can report interrupted tasks after a crash. Tasks do not
  auto-resume side effects.

### 4.3 Latency instrumentation

- The `tracing` spans follow plan §97: `T0 wake` … `T10 completion`, all carrying `turn_id`.
- A metrics subscriber stores per-turn timings in SQLite (`turn_metrics`). They feed the
  Performance page and the benchmark harness.

## 5. Storage

| Data | Location | Format |
|---|---|---|
| Config | `%APPDATA%\KIVO\config\kivo.toml` (+ `profiles/*.toml`) | TOML, `schema_version`, validated on load, migrated forward, previous version backed up |
| Database | `%LOCALAPPDATA%\KIVO\data\kivo.db` | SQLite (rusqlite, bundled, WAL), migrations embedded (refinery or rusqlite_migration) |
| Secrets (API keys, tokens) | Windows Credential Manager (`keyring`) | Referenced from config by handle, never the value |
| Voice enrollment / voiceprints | `%LOCALAPPDATA%\KIVO\data\voice\` | DPAPI-encrypted blobs |
| Models | `%LOCALAPPDATA%\KIVO\models\<id>\` | Manifest-verified (sha256) |
| Logs | `%LOCALAPPDATA%\KIVO\logs\` | `tracing` rolling JSON logs, redacted, 7-day default retention |
| Crash dumps | `%LOCALAPPDATA%\KIVO\crashes\` | minidumps; never uploaded without consent |

**Initial tables:**

- `activity`, `audit` (hash-chained), `turns`, `turn_metrics`
- `tasks`, `task_steps`
- `conversations`, `messages` (retention setting)
- `memories` (+ FTS5; `sqlite-vec` later)
- `providers` (metadata only), `wake_words`, `benchmarks`, `permissions_grants`

**Config sections** (plan §107): `general, voice, overlay, sounds, brains, profiles, tools,
permissions, privacy, memory, companion, performance, integrations, automation`.

## 6. Observability

- **Logging:** `tracing` plus `tracing-subscriber`. A **redaction layer** masks keys, tokens and
  anything tagged `sensitive`. Transcript text is logged only when the "debug transcripts" setting
  is on.
- **Diagnostics bundle** (plan §109): versions, OS, hardware, capabilities, provider health, recent
  errors and metrics. It is generated locally, shown to the user for review, and has no secrets
  or content.

## 7. Repository layout (Cargo workspace + pnpm)

```text
K.I.V.O/
├─ Cargo.toml                 # workspace
├─ crates/
│  ├─ kivo-core/              # types, events, state machines, config schema — no OS deps
│  ├─ kivo-ipc/               # protocol types + transport + TS generation
│  ├─ kivo-platform/          # platform traits + Capabilities
│  ├─ kivo-platform-windows/  # Windows implementations
│  ├─ kivo-audio/             # capture/playback, resample, AEC, VAD, ring buffer
│  ├─ kivo-voice/             # wake word, speaker verify, STT/TTS provider traits + adapters
│  ├─ kivo-intent/            # fast-path grammar + intent classification
│  ├─ kivo-brain/             # BrainProvider trait, API/ACP/local adapters, router
│  ├─ kivo-tools/             # tool registry, schemas, built-in tools
│  ├─ kivo-security/          # permission engine, policy, taint, audit
│  ├─ kivo-mcp/               # MCP client + KIVO MCP server (rmcp)
│  ├─ kivo-store/             # SQLite, migrations, config IO, secrets, models manager
│  └─ kivo-testkit/           # fakes (fake brain, fake audio, fake platform)
├─ apps/
│  ├─ kivo-runtime/           # bin: the runtime
│  ├─ kivo-infer/             # bin: inference worker
│  ├─ kivo-app/               # Tauri app: src-tauri/ (Rust shell) + src/ (React UI)
│  └─ kivo-bench/             # bin: benchmark harness
├─ extensions/browser/        # Chromium/Firefox extension (native messaging)
├─ testenv/                   # safe computer-control test apps + pages (plan §116)
├─ assets/sounds/, assets/icons/
└─ docs/
```

**Toolchain:**

- Rust stable (MSRV pinned in `rust-toolchain.toml`), `cargo fmt`, `clippy -D warnings`, and
  `cargo deny` (licenses and advisories; GPL is disallowed in the default graph).
- Node LTS with pnpm, TypeScript strict, **Oxlint** (type-aware; ESLint can't run on TypeScript 7)
  and Prettier. Vitest for the UI, and Playwright
  for UI tests against the Tauri dev build where practical.
- **CI:** GitHub Actions on `windows-latest`, with Linux and macOS build checks for the
  platform-independent crates from M0, so the portability promise is enforced.

## 8. Architecture invariants → enforcement

| Invariant (plan §160) | Enforced by |
|---|---|
| Provider-independent runtime | Brains are reached only through the `BrainProvider` trait; `kivo-core` has no provider deps (a CI check on the dependency graph) |
| UI is not the core | kivo-app has no access to store, secrets or tools; its only API is IPC |
| No heavy AI for deterministic actions | Fast path in `kivo-intent` runs before brain routing; metric `fast_path_ratio` |
| Permissions not bypassable | Every tool call goes through `kivo-security::authorize()`. The tool registry cannot execute without a `Decision` token (type-level) |
| Native before vision | The tool router orders by capability ladder; vision tools are `Fallback` tier |
| MCP optional | Built-in tools are native Rust; MCP is an adapter |
| Voice interruptible | Barge-in and "stop" have priority; cancellation-latency test |
| Tasks cancellable | Every async op takes a `CancellationToken` (lint/review rule) |
| Provider failure isolated | Adapters return normalized errors; CLI/infer run out of process |
| Secrets isolated | The `Secret<T>` type is non-Display and non-Serialize; secrets are resolved only inside tool executors |
| Cloud can be disabled | Privacy mode is checked in the router and in network egress for tools |
| Useful offline | M1 exit criteria include offline fast path + local STT/TTS |

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Processes and lifecycle (§1)**

- [x] **ARCH-01** · M0 · `kivo-runtime` binary: starts, holds the `Local\KIVO.Runtime.<user-sid>` mutex (a second runtime exits), serves IPC (§1) → done: `apps/kivo-runtime` (`main.rs` startup/shutdown order, `core.rs` state + bus + shutdown, `rpc.rs`, args `--autostart`/`--from-app`/`--no-app`); mutex `Local\KIVO.Runtime.<sid>` via `kivo-platform-windows::instance`; fresh session token per start, removed on exit · verified: 16 unit tests; live run on Windows 11 (2026-09-21): a second runtime exits in 33 ms, settings/database/token created, logs written
- [x] **ARCH-02** · M0 · The runtime owns the tray (`tray-icon` on a runtime thread with a Win32 message loop) and it survives a UI crash (§1) → done: `apps/kivo-runtime/src/tray.rs` on `kivo-platform-windows::WindowsTray` (own thread + Win32 message loop); menu, icon and tooltip follow the session state; choices go to the core · verified: tray unit tests; live run on Windows 11 (2026-09-21): the tray stays while the app is killed and relaunched
- [x] **ARCH-03** · M0 · The runtime launches `kivo-app` and relaunches it with backoff if it crashes (§1) → done: `apps/kivo-runtime/src/app.rs` (launches `KIVO.exe`/`kivo-app.exe` beside it; follows the app's IPC connection; 3 s grace, relaunch in the background, backoff 1 s doubling to 30 s after quick crashes, gives up after 5 until the Control Center is requested) · verified: 6 supervisor tests with a fake app; live run on Windows 11 (2026-09-21): killed the app, relaunched with `--background` in ~1 s
- [x] **ARCH-04** · M0 · `KIVO.exe` (Tauri) connects to the runtime pipe; if absent it spawns `kivo-runtime.exe` (sidecar, `externalBin`) detached, then connects; it shows the runtime state and can restart without stopping KIVO (§1) → done: `apps/kivo-app/src-tauri/src/runtime.rs` (connects; if absent starts `kivo-runtime --from-app` hidden from the app's folder (debug builds add `--no-app`, so stopping `pnpm dev` stays stopped), then `connect_with_backoff`; `Link` status/version/state pushed to the UI), `src/ipc/runtime.tsx`, sidebar `RuntimeStatus`, `pages/Home.tsx` · verified: live run on Windows 11 (2026-09-21): launching the app started the runtime and showed "Connected · Ready"; killing the runtime showed "Reconnecting…" with Start KIVO; app restarts leave the runtime running · note: bundling the runtime as a Tauri `externalBin` sidecar is part of the installer (DIST-01)
- [x] **ARCH-05** · M0 · `tauri-plugin-single-instance`: a second `KIVO.exe` forwards "show main window" and exits (§1) → done: `tauri-plugin-single-instance` registered first in `src-tauri/src/lib.rs`; a second launch shows, unminimizes and focuses the window (and `--page`), and starts the runtime if it isn't connected · verified: live run on Windows 11 (2026-09-21): a second `kivo-app.exe` exited, one app remained, the runtime came back
- [x] **ARCH-06** · M1 · Autostart registers `kivo-runtime.exe --autostart`; the runtime then launches `KIVO.exe --background`, which preloads the overlay without showing a window (§1) → done: `Lifecycle::apply_autostart` keeps the HKCU Run entry `"…\kivo-runtime.exe" --autostart` in step with the setting (off by default); an autostarted runtime launches the app with `--background`, which creates the overlay hidden and opens no window · verified: `autostart_follows_the_setting`, `the_startup_entry_is_written_read_and_removed` (real registry, test-only name), `Launch::Background` args test (2026-09-23)
- [x] **ARCH-07** · M1 · Quit (tray or Control Center) stops the runtime, closes the UI and releases the mic; closing a window never quits (§1) → done: Quit from the tray, Ctrl+K and the first-close notification's Quit button sends `runtime.quit`; `ShuttingDown` reaches the app before connections close; the app exits; the runtime shuts down in order and removes its token; the mic is open only while listening; closing the window hides it while connected · verified: `an_event_published_just_before_shutdown_is_still_delivered`, `the_first_close_notice_shows_once_and_its_buttons_work`, live quit over the pipe (2026-09-23)
- [ ] **ARCH-08** · M8 · Low-memory mode: the runtime does not launch `kivo-app` until the overlay is first needed; M0 measures the memory difference (§1)
- [x] **ARCH-09** · M1 · `kivo-infer` runs as a supervised worker (same binary, subcommand), started on demand, restarted on crash; the current turn fails gracefully with a spoken message (§1) → done: `infer::supervise` starts `kivo-infer serve` when an engine is wanted, restarts it with backoff after a crash, and reports `Lost`; the engine fails the turn with a message that is shown and spoken by Windows' voice in-process (`fallback_voice`), since the worker's voices are gone · verified: `a_crashed_speech_worker_fails_the_turn_aloud_and_comes_back` kills the real worker mid-turn and sees the failure spoken and a new worker start (2026-09-23)
- [x] **ARCH-10** · M1 · Crash dumps are written locally to `%LOCALAPPDATA%\KIVO\crashes\` and reported on the next start; never uploaded without consent (§1, §5) → done: `kivo-platform-windows::crash` installs a panic hook and an unhandled-exception minidump writer (runtime, worker) into `%LOCALAPPDATA%\KIVO\crashes\`; `Lifecycle::report_crashes` reports new ones once on the next start (log, Activity, a local notification); nothing is uploaded · verified: `crashes_are_reported_once_on_the_next_start`, kivo-store crash tests (2026-09-23)

**Platform abstraction (§2)**

- [x] **ARCH-11** · M0 · `kivo-platform` defines every trait in the §2 table (`AudioIo`, `EchoCancel`, `Hotkeys`, `Tray`, `Apps`, `Windows`, `UiAutomation`, `Input`, `Screen`, `Ocr`, `Secrets`, `SystemInfo`, `Notifications`); core crates depend only on the traits (§2) → done: `crates/kivo-platform` (AudioIo + FrameSink/FrameSource, EchoCancel, Hotkeys, Tray, Apps, Windows, UiAutomation, Input, Screen, Ocr, Secrets, SystemInfo, Notifications; object-safe; user-safe PlatformError) · verified: 7 unit tests, builds on Windows, Ubuntu and macOS in CI
- [x] **ARCH-12** · M0 · A `Capabilities` struct is filled at startup (Windows build, OS AEC, Mica, NPU, …); Windows 10 differences are handled only inside `kivo-platform-windows` (§2) → done: `crates/kivo-platform-windows/src/capabilities.rs` (RtlGetVersion build, DisplayVersion, OS AEC from 22621, Mica from 22000, package identity, NPU via DXCore GENERIC_ML + NPU attribute GUIDs from the Windows SDK header) · verified: on this PC reports "Windows 11 25H2 (build 26200)", AEC and Mica true, not packaged; NPU=false here (a positive NPU result still needs a Copilot+ PC to confirm)
- [x] **ARCH-13** · M0 · `kivo-testkit` provides fake platform, fake audio and fake brain implementations for tests (§7) → done: `crates/kivo-testkit` provides fakes for hotkeys (conflicts), tray, notifications, apps, windows, secrets, system info, a scripted audio device, and the fake brain (`ScriptedBrain`, `MockServer` for adapter contract tests, re-exported from `kivo-brain::testing`) · verified: testkit tests stable over 25 runs; the brain end-to-end tests run on `ScriptedBrain` (2026-09-23)

**IPC (§3)**

- [x] **ARCH-14** · M0 · Named-pipe transport (`interprocess`), `\.\pipe\kivo-<user-sid>`, security descriptor granting only the current user's SID, `PIPE_REJECT_REMOTE_CLIENTS` (§3) → done: `crates/kivo-ipc/src/transport/windows.rs` (tokio named pipe `\.\pipe\kivo-<sid>`, protected user-only DACL, reject remote clients, first-instance creation) and `transport/unix.rs` (mode-600 socket) · verified: tests read the pipe DACL back as `D:P(A;;FA;;;<user sid>)` and prove a second first-instance (squatting) is refused
- [x] **ARCH-15** · M0 · Random 256-bit session token in `%LOCALAPPDATA%\KIVO\run\session.token` (user-only ACL); clients must present it in `hello` (§3) → done: `crates/kivo-ipc/src/token.rs` (256-bit random, written to a file restricted to the user before the secret is written, constant-time compare) · verified: tests for format, uniqueness, exact matching, file round trip and the file DACL
- [x] **ARCH-16** · M0 · Length-prefixed frames carrying JSON-RPC 2.0: request/response, notifications (events), and a message size limit (§3, SECURITY §9) → done: `frame.rs` (u32 LE length prefix, 1 MiB limit) + `protocol.rs` (strict JSON-RPC 2.0: request/response/notification, "2.0" enforced, unknown fields rejected) · verified: protocol unit tests and `oversized_frames_close_the_connection`
- [x] **ARCH-17** · M0 · `hello { protocol_version, client, capabilities }`; incompatible major versions are rejected with a clear error (§3) → done: `hello { protocolVersion, client, token }` must come first within 3 s; wrong token → -32001, other major → -32002 with a clear message; welcome returns the snapshot · verified: `another_protocol_major_is_refused_with_a_clear_message`, `a_wrong_token_is_refused_and_others_still_connect`, `the_first_message_must_be_hello`, `silent_clients_are_dropped_after_the_hello_timeout`
- [x] **ARCH-18** · M0 · Rust types in `kivo-ipc` are the source of truth; TypeScript types are generated (`specta` or `ts-rs`) into `apps/kivo-app/src/ipc/generated.ts`, and CI fails when they are stale (§3) → done: ts-rs derives behind a `ts` feature; `crates/kivo-ipc/tests/ts_bindings.rs` writes `apps/kivo-app/src/ipc/generated.ts`; CI step fails when stale · verified: UI typechecks the file; a hand edit makes the check fail
- [x] **ARCH-19** · M0 · The UI reconnects with backoff and re-subscribes; on reconnect the runtime sends a full `StateSnapshot`, then deltas (§3) → done: the app keeps reconnecting with `connect_with_backoff` and re-reads the token; welcome snapshot then `snapshot` notifications on every change · verified: `state_changes_are_pushed_to_every_client`, `clients_reconnect_after_the_runtime_restarts_with_a_new_token`; live run on Windows 11 (2026-09-21): the app reconnected to a restarted runtime
- [x] **ARCH-20** · M1 · A separate high-rate `levels` notification (30–60 Hz) carries audio levels, only while the overlay is animating (§3) → done: `crates/kivo-ipc` `levels` notification (`Server::with_levels`), fed by `apps/kivo-runtime/src/mic.rs` at 30 Hz only while listening (peak RMS → −60…−10 dBFS scale); the app forwards it to the overlay only; the waveform shows dots in silence · verified: `microphone_levels_stream_while_they_change`, mic unit tests, live on this PC
- [x] **ARCH-21** · M1 · The same protocol runs between the runtime and `kivo-infer`, with methods for streaming audio frames and results (§3) → done: the runtime and `kivo-infer` speak the same framed JSON-RPC (`kivo_ipc::Peer`) over stdin/stdout, with `hello`, `model.*`, `stt.start/audio/partial/finish/cancel`, `tts.speak/audio/done/cancel` and `shutdown` (`kivo_ipc::infer`) · verified: the spoken end-to-end test streams audio frames and results through the real worker (2026-09-23)

**Core model (§4)**

- [x] **ARCH-22** · M0 · `enum Event` in `kivo-core` with every group in §4.1 (Voice, Turn, Tool, Task, System incl. `FileChanged`/`BrowserChanged` from plan §66, Provider, Ui), each carrying `ts` (monotonic + wall), `turn_id`/`task_id` where relevant, and `trace_id`; delivered over `tokio::sync::broadcast` (§4.1) → done: `crates/kivo-core/src/event.rs` (all §4.1 groups + FileChanged/BrowserChanged), `bus.rs` (tokio broadcast, lag reported) · verified: 6 unit tests incl. JSON shape and round-trip of every group
- [x] **ARCH-23** · M1 · The Activity recorder persists the Activity subset to SQLite (§4.1) → done: the Activity recorder (`activity::Recorder`) writes transcripts, tool calls, replies, stops and crashes to the `activity` table as they happen (DECISIONS "Activity recorder") · verified: the spoken end-to-end test reads the whole story back from Activity; recorder unit tests (2026-09-23)
- [x] **ARCH-24** · M0 · The session state machine (Idle, Listening, Thinking, Acting, Speaking, FollowUp, Interrupted, Paused, AwaitingConfirmation, Error) with every transition in the §4.2 diagram, unit-tested; an invalid transition logs at `error` (§4.2) → done: `crates/kivo-core/src/session.rs` · verified: table test of all 170 state/input pairs, invalid transitions leave state unchanged and log at error
- [x] **ARCH-25** · M0 · A Turn has a root `CancellationToken`; child tokens go to STT, the brain request, each tool call and TTS (§4.2) → done: `crates/kivo-core/src/turn.rs` (root token, child per stage) · verified: 4 tests (turn cancels stages, stage cancel is local, late stages start cancelled, waiting stage wakes)
- [x] **ARCH-26** · M1 · Stop, Esc, the overlay X, the emergency stop and barge-in cancel the turn token, and cancellation reaches every layer within 100 ms, enforced by a test (§4.2) → done: Stop, Esc, the Island's X and the emergency stop all go through `Engine::cancel`, which cancels the turn token, recognition, the tool, the voice (also a reply still playing after its turn ended) and the listener; barge-in (M2) takes the same path · verified: `cancelling_stops_every_layer_within_100_ms` (listening: < 1 ms for Esc, Stop and the emergency stop; speaking: 33 ms) and `timeouts_and_cancellation_end_the_wait_promptly` for the tool layer (2026-09-23)
- [ ] **ARCH-27** · M5 · A Task holds a graph of steps with dependencies and its own token, outlives turns, is persisted, and after a crash is reported as interrupted without auto-resuming side effects (§4.2)
- [x] **ARCH-28** · M1 · `tracing` spans T0 wake … T10 completion carry `turn_id`; a metrics subscriber stores per-turn timings in `turn_metrics` (§4.3) → done: each turn records its spans (t0 … t10, keyed by `turn_id`) and `turn_metrics` stores them when it finishes · verified: the spoken end-to-end test reads `t4EndOfSpeech`, `t5FinalTranscript`, `t8ToolDone`, `t10Complete` back (end of speech → action 37 ms) (2026-09-23)

**Storage (§5)**

- [x] **ARCH-29** · M0 · Config: `%APPDATA%\KIVO\config\kivo.toml` (+ `profiles/*.toml`) with `schema_version`; validated on load, migrated forward, previous version backed up; all §5 config sections exist (§5) → done: schema `crates/kivo-core/src/config.rs` (all §5 sections, UX §5 defaults, `schema-version`, validation, unknown keys rejected), IO `crates/kivo-store/src/config.rs` (atomic save, forward migrations with a `.vN.bak` backup, broken files kept as `.invalid-<ts>`, newer-version files left untouched, Bypass reset to Auto on load), paths `crates/kivo-store/src/paths.rs` · verified: 10 tests incl. synthetic two-step migration, partial files, out-of-range values and typos · note: `profiles/*.toml` arrive with brain profiles (M3)
- [x] **ARCH-30** · M0 · Database: `%LOCALAPPDATA%\KIVO\data\kivo.db`, rusqlite (bundled), WAL, embedded migrations (§5) → done: `crates/kivo-store/src/db.rs` (rusqlite bundled, WAL, synchronous=NORMAL, foreign keys, busy timeout, rusqlite_migration with embedded SQL, owner profile created on first open) · verified: tests for WAL + FK pragmas, stable owner id across opens, single-owner constraint
- [x] **ARCH-31** · M0 · Every user-owned record carries `profile_id` from the first migration, so people profiles need no later migration (§5, UX §8.2) → done: `profiles` table from migration 1; every other table must reference it via `profile_id` · verified: test `the_shipped_schema_is_profile_scoped` checks the real migrated schema in CI, and `the_scoping_check_catches_a_table_without_profile_id` proves the check works
- [x] **ARCH-32** · M0 · Logs: `tracing` rolling JSON in `%LOCALAPPDATA%\KIVO\logs\`, 7-day retention, a redaction layer for keys, tokens and `sensitive` fields; transcripts logged only with the "debug transcripts" setting (§5, §6) → done: `crates/kivo-store/src/logging.rs` (daily rolling JSON `kivo.*.log`, 7 files kept, non-blocking writer, redaction of sensitive field names, known key/token formats, name=value secrets, and transcripts unless debug transcripts is on) · verified: 5 tests incl. an end-to-end file write that checks the secret and transcript never reach disk and the line stays valid JSON

**Repository and toolchain (§7)**

- [x] **ARCH-33** · M0 · Cargo workspace + pnpm workspace with every crate and app in the §7 layout → done: `Cargo.toml`, `crates/*`, `apps/*`, `pnpm-workspace.yaml` · verified: `cargo check --workspace`, `pnpm typecheck` (2026-09-21)
- [x] **ARCH-34** · M0 · Rust stable with MSRV pinned in `rust-toolchain.toml`; `cargo fmt`; `clippy -D warnings` → done: `rust-toolchain.toml` pins 1.97.0 (MSRV stays `rust-version` 1.90), `rustfmt.toml`, CI runs `cargo fmt --check` and clippy with `RUSTFLAGS=-D warnings` · verified: CI green on Windows, Ubuntu, macOS (2026-09-21)
- [x] **ARCH-35** · M0 · `cargo deny` configured: licenses (GPL/AGPL denied in the default graph), advisories, bans (§7) → done: `deny.toml` (permissive allow-list, no GPL/AGPL, advisories, crates.io only, unmaintained checked for direct deps) · verified: `cargo deny check` clean locally with 0 warnings and in CI
- [x] **ARCH-36** · M0 · UI toolchain: Node LTS + pnpm, TypeScript strict, Oxlint (type-aware; ESLint cannot run on TypeScript 7), Prettier, Vitest (§7) → done: pnpm 11.27.1, TypeScript strict, Oxlint type-aware (`.oxlintrc.json`), Prettier (`.prettierrc.json`), Vitest on jsdom (21 tests) · verified: all run clean in CI
- [ ] **ARCH-37** · M7 · Playwright UI tests against the Tauri dev build where practical (§7)

**Invariant enforcement (§8)**

- [x] **ARCH-38** · M3 · A CI check fails if `kivo-core` gains a provider dependency (§8) → done: `crates/kivo-core/tests/no_provider_dependencies.rs` reads kivo-core's manifest and fails on any dependency outside its allowed list or any target-specific one; CI's `cargo test --workspace` runs it · verified: the test passes in the workspace run (2026-09-23)
- [x] **ARCH-39** · M0 · `kivo-app` has no access to the store, secrets or tools; its only API is IPC (reviewed in every milestone) (§8) → done: the app depends only on `kivo-core` (types), `kivo-ipc` and `kivo-platform` (paths) — no store, secrets or tools crates; its webview gets three commands (`ui_ready`, `runtime_request`, `runtime_start`) · verified: `apps/kivo-app/src-tauri/Cargo.toml` review; re-check every milestone
- [ ] **ARCH-40** · M8 · Diagnostics bundle: versions, OS, hardware, capabilities, provider health, recent errors and metrics; generated locally, shown to the user for review, no secrets or content (§6, plan §109)
