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

- **Transport:** `interprocess` local sockets, which are named pipes on Windows (`\\.\pipe\kivo-<user-sid>`).
  - The pipe gets a security descriptor that grants only the current user's SID, plus
    `PIPE_REJECT_REMOTE_CLIENTS`.
  - The runtime writes a random 256-bit **session token** to
    `%LOCALAPPDATA%\KIVO\run\session.token` (user-only ACL). Clients must present it in
    `hello`.
- **Framing:** length-prefixed frames. The payload is **JSON-RPC 2.0**, encoded with serde.
  - Messages come in three kinds: `request`/`response`, `notification` (events), and a separate
    high-rate **`levels`** notification carrying audio levels for the overlay at 30–60 Hz, only
    while the overlay is animating.
- **Versioning:** `hello { protocol_version, client, capabilities }`. The runtime rejects
  incompatible major versions with a clear error (for example after a partial update).
- **Schema source of truth:** Rust types in `kivo-ipc`. TypeScript types are generated with
  **`specta`** (or `ts-rs`) into `apps/kivo-app/ui/src/ipc/generated.ts`, and CI fails if the
  generated types are stale.
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
  - `System(WindowChanged | FullscreenChanged | FocusModeChanged | PowerChanged | DeviceChanged | NetworkChanged)`
  - `Provider(HealthChanged | RateLimited | AuthFailed)`
  - `Ui(OverlayShown | OverlayHidden | UserConfirmed | UserCancelled)`
- **Metadata:** every event carries `ts` (monotonic plus wall clock), `turn_id` / `task_id`
  where relevant, and `trace_id`.
- **Persistence:** a subscriber writes the **Activity** subset to SQLite (plan §83). A separate
  subscriber writes the **Audit** subset (see [SECURITY.md](SECURITY.md)).

### 4.2 State machines

**Assistant session state** (it drives the overlay, companion, sounds and tray icon, per plan §75):

```text
Idle ─wake/hotkey─► Listening ─endpoint─► Thinking ─┬─► Acting ─┬─► Speaking ─► (FollowUp ─timeout─► Idle)
  ▲                    │                            │           │        │
  │                    └─cancel/"stop"──────────────┴───────────┴────────┴──► Interrupted ─► Idle | Listening
  └───────────────── Paused (mic off) ◄── tray/voice ──►  AwaitingConfirmation ◄─ policy
                                                           Error ─► Idle (after message)
```

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
│  ├─ kivo-app/               # Tauri app (src-tauri/ + ui/ React)
│  └─ kivo-bench/             # bin: benchmark harness
├─ extensions/browser/        # Chromium/Firefox extension (native messaging)
├─ testenv/                   # safe computer-control test apps + pages (plan §116)
├─ assets/sounds/, assets/icons/
└─ docs/
```

**Toolchain:**

- Rust stable (MSRV pinned in `rust-toolchain.toml`), `cargo fmt`, `clippy -D warnings`, and
  `cargo deny` (licenses and advisories; GPL is disallowed in the default graph).
- Node LTS with pnpm, TypeScript strict, ESLint and Prettier. Vitest for the UI, and Playwright
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
