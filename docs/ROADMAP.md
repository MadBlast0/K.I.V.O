# KIVO roadmap

Status: v3, 2026-09-21. The build plan for KIVO, from the first line of code to release. It refines
the plan's §155 phases into **vertical milestones** (plan §154: *user speaks → KIVO understands →
performs one useful action → speaks back*), so every milestone ends with something that works end
to end.

## How to use this file

1. **Pick the current milestone:** the first one in the progress table that isn't ✅.
2. **Read** every document in its *Read first* list **in full**, plus [DECISIONS.md](DECISIONS.md)
   and the tracking rules in [README.md](README.md).
3. **Build in the listed order.** Each step names the checklist items it completes. Backend, IPC
   and UI are built together, so each feature works end to end before the next starts.
4. **Verify** each item (tests, `pnpm typecheck`, `cargo clippy --workspace --all-targets`, and the
   checks under *How to verify*), then **mark it in its spec**: `[x]` with a
   `→ done: <paths> · verified: <how>` note, or `[~]` with what's missing.
5. **Run `pnpm docs:sync`.** It copies every mark into this file and updates the progress table.
   It refuses to run if a box was ticked here but not in the spec.
6. **Tick the exit criteria** below the milestone by hand when they pass.
7. A milestone is **complete** when every item is ✅ or ⏭ and every exit criterion is ticked. Then
   update *Status* in the root `CLAUDE.md` and move on.

**Prompt to use:** *"Build M1 per docs/ROADMAP.md."* Items tagged with a later milestone are not
built early unless the owner asks, but code is shaped so they fit (for example `profile_id` on every
table from M0).

**Legend:** ⬜ `[ ]` not started · 🟡 `[~]` partial · ✅ `[x]` done and verified · ⏭ `[-]` dropped
(reason in DECISIONS.md). Item IDs never change; see [README.md](README.md) for the prefixes.

## Progress

<!-- progress -->
| Milestone | State | Items | ✅ Done | 🟡 Partial | ⏭ Dropped | ⬜ Not started | Complete | Exit criteria |
|---|---|---|---|---|---|---|---|---|
| [D0](#d0) | 🟡 In progress | 13 | 13 | 0 | 0 | 0 | 100% | 1/2 |
| [M0](#m0) | 🟡 In progress | 51 | 40 | 10 | 1 | 0 | 80% | 1/4 |
| [M1](#m1) | 🟡 In progress | 86 | 79 | 7 | 0 | 0 | 92% | 2/4 |
| [M2](#m2) | 🟡 In progress | 32 | 31 | 0 | 1 | 0 | 100% | 1/3 |
| [M3](#m3) | 🟡 In progress | 61 | 57 | 4 | 0 | 0 | 93% | 3/4 |
| [M4](#m4) | 🟡 In progress | 45 | 43 | 2 | 0 | 0 | 96% | 2/3 |
| [M5](#m5) | ✅ Complete | 39 | 39 | 0 | 0 | 0 | 100% | 2/2 |
| [M6](#m6) | ✅ Complete | 16 | 16 | 0 | 0 | 0 | 100% | 2/2 |
| [M7](#m7) | 🟡 In progress | 33 | 32 | 1 | 0 | 0 | 97% | 2/3 |
| [M8](#m8) | 🟡 In progress | 45 | 38 | 6 | 1 | 0 | 87% | 0/2 |
| [M9](#m9) | ⬜ Not started | 18 | 0 | 0 | 0 | 18 | 0% | 0/2 |
| [L1](#l1) | ⬜ Not started | 2 | 0 | 0 | 0 | 2 | 0% | — |
| [L2](#l2) | ⬜ Not started | 1 | 0 | 0 | 0 | 1 | 0% | — |
| [Post](#post) | ⬜ Not started | 16 | 0 | 0 | 0 | 16 | 0% | — |
| **Total** | | **458** | **388** | **30** | **3** | **37** | **85%** | |
<!-- /progress -->

## Milestone map

```text
Phase 0 (specs) ✅
   │
   ├── D0 Design foundation ─────────────────────────┐ (parallel with M0; needed before M1 UI)
   │                                                  │
   └── M0 Foundations & measurement ─► M1 Push-to-talk slice ─► M2 Wake word & conversation audio
                                                                     │
          M3 Brains ◄────────────────────────────────────────────────┘
            │
            ├─► M4 Tools & computer control ─► M5 Agents, tasks & routines ─► M6 MCP, connectors & skills
            │                                                                        │
            └──────────────────────────► M7 Control Center & memory (MVP, internal alpha) ◄┘
                                                   │
                                        M8 Beta hardening ─► M9 Release ─► Post-M9 (MSIX, macOS, Linux, plugins, Remote)

Language track: L1 English (with M1–M2) → L2 Hindi + Punjabi (from M3) → L3 European → L4 CJK → L5 RTL
```

---

<a id="phase-0"></a>

## Phase 0: Specification ✅

- [x] Product blueprint ([KIVO_Project_Plan.md](KIVO_Project_Plan.md)), with amendments (§167) and the coverage map (§168)
- [x] Research ([research/](research/)), decisions ([DECISIONS.md](DECISIONS.md)) and owner inputs
- [x] Engineering specs ([architecture/](architecture/ARCHITECTURE.md)) and design system ([DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md), [mockup](design/mockups/kivo-app.html))
- [x] Build checklists with IDs in every spec, the tracking rules ([README.md](README.md)) and `pnpm docs:sync`

---

<a id="d0"></a>

## D0: Design foundation

**Goal:** the design tokens, component library, Island component and app shell that every KIVO
screen is built from, matching the approved mockup.

**Depends on:** nothing. Runs alongside M0 and must be complete before M1's UI work.

**Read first:** [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md), [mockups/kivo-app.html](design/mockups/kivo-app.html), [UX.md](architecture/UX.md).

**Build order:**

1. Tokens and theme: colors, type, shape, motion, 7 accents, Light/Dark/System (DS-01, DS-02).
2. Semantic icons (DS-04).
3. Base components: buttons, controls, fields, lists, status, feedback (DS-05).
4. Overlays, menus with submenus, toasts, palette, pickers (DS-06, DS-07, DS-08).
5. The Island with all states, spring morph and waveform (DS-09).
6. System-surface previews (DS-10).
7. App shell and custom title bar (DS-11), motion pass (DS-12), gallery page (DS-13), logo (DS-14).

**Still open:** the Island's 120 ms content cross-fade (DS-09), the remaining system-surface
previews (DS-10), and verifying each element's motion values against §5 (DS-12).

**Deliverable:** `pnpm dev` (or `preview.bat`) opens KIVO on the Components gallery, showing every
component in light and dark with every accent, and the Island cycling through all 22 states.

**How to verify:** `pnpm typecheck` and `pnpm --filter kivo-app build` with zero warnings; the
gallery in the browser pane with no console errors; the Tauri window (drag, maximize, close).

**Exit criteria:**

- [x] **D0-X1** · Every D0 item is ✅; the gallery shows every component in light and dark with no console errors.
- [ ] **D0-X2** · The Island's states and motion match the mockup side by side (owner review).

### D0 items

<!-- items:D0 -->
**13 items** · ✅ 13 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) (13)

- [x] **DS-01** · Tokens for type, shape, surfaces, text, status, accents (7 presets × light/dark), Island and motion in `styles/tokens.css` (§2) → done: `apps/kivo-app/src/styles/tokens.css` · verified: gallery in light and dark with accent switching (2026-09-21) <!-- synced:x -->
- [x] **DS-02** · Theme provider: Light (default) / Dark / System (follows Windows live), accent and text size applied as `data-*` attributes and persisted (§2) → done: `src/lib/theme.tsx` · verified: switched in the gallery, `pnpm typecheck` <!-- synced:x -->
- [x] **DS-04** · Semantic icon set with the one-meaning rule (§4) → done: `src/icons/index.tsx` · verified: all names render on the gallery Icons section <!-- synced:x -->
- [x] **DS-05** · Buttons, controls, fields, lists, status and feedback components (§3) → done: `src/components/ui/{Button,Controls,Fields,List,Status,Feedback}.tsx` · verified: gallery, no console errors, `tsc` and `vite build` clean <!-- synced:x -->
- [x] **DS-06** · Overlays: tabs with sliding indicator, tooltip, dialog, sheet, popover (§3) → done: `src/components/ui/Overlays.tsx` · verified: opened each in the gallery <!-- synced:x -->
- [x] **DS-07** · Dropdown and context menus with nested submenus, checks, shortcuts, labels and danger items (§3) → done: `src/components/ui/Menu.tsx` · verified: two submenu levels by hover in the gallery <!-- synced:x -->
- [x] **DS-08** · Toasts with Undo, command palette, accent picker, shortcut recorder (§3) → done: `src/components/ui/{Toast,CommandPalette,Pickers}.tsx` · verified: toast + Undo, Ctrl+K filtering in the gallery <!-- synced:x -->
- [x] **DS-09** · Island component with all states (22 + 4 notices), spring morph, 120 ms content cross-fade and audible-only waveform (§3, §5) → done: `src/components/island/{Island,presets}.tsx` · verified: opacity sampled in the browser (content holds at 0, fades in from ~120 ms after mount over ~180 ms), height equals content after the morph, one row after exit, no console errors across a state cycle (2026-09-21); geometry measured across all 26 states after the owner review: row items centred to 0.0 px, leading and round trailing items 18 px from their ends, body on the label column, waveform 44 × 18 <!-- synced:x -->
- [x] **DS-10** · System-surface previews: tray menu with the permission-mode submenu, tray tooltip, jump list, notifications (first close, task finished with reply, budget, update, microphone blocked), Windows Hello, File Explorer menu; Island notices as Island states (§3) → done: `src/components/system/SystemSurfaces.tsx`, notices in `island/presets.tsx`, shown in the gallery · verified: side by side with the mockup's System surfaces section in the browser pane <!-- synced:x -->
- [x] **DS-11** · App shell: sidebar with 13 grouped items, page header, custom title bar with caption buttons and drag regions, single scroll area under the title bar (§1, §3) → done: `src/components/layout/{Shell,TitleBar}.tsx`, `src-tauri/tauri.conf.json` · verified: running Tauri app on Windows 11 (drag, maximize/restore, close; rounded corners kept) <!-- synced:x -->
- [x] **DS-12** · Motion per §5 with reduced-motion support (§5) → done: page stagger 30 ms per block capped at 12 (`styles/base.css`), dialogs scale .96 spring, palette drop, sheet 350 ms + content stagger, menus/toasts scale from anchor, submenus slide 6 px after 120 ms, button press .97, Island spring + cross-fade; in-app Motion setting (`lib/theme.tsx`, `data-motion` + `MotionConfig`) alongside Windows "Animation effects" · verified: computed delays and transforms in the browser, Island instant with Reduced (opacity 1 and final width after two frames) · note: onboarding step motion ships with the onboarding screens (UX-33) <!-- synced:x -->
- [x] **DS-13** · Components gallery page showing every component in both themes (§3) → done: `src/pages/Gallery.tsx` · verified: `pnpm dev` and the browser pane <!-- synced:x -->
- [x] **DS-14** · App logo as SVG and PNG, Windows icon set generated from it (§1) → done: `assets/icons/kivo.svg`, `kivo.png` (1024), `kivo-256.png`, `apps/kivo-app/src-tauri/icons/` · verified: shown in the window, taskbar and favicon <!-- synced:x -->
<!-- /items -->

---

<a id="m0"></a>

## M0: Foundations and measurement

**Goal:** a skeleton that builds, lints and tests in CI; the runtime and the app running as two
processes that talk over a secured named pipe; and the measured numbers needed to lock the engine
defaults.

**Depends on:** Phase 0.

**Read first:** [ARCHITECTURE.md](architecture/ARCHITECTURE.md), [SECURITY.md](architecture/SECURITY.md), [BENCHMARKS.md](architecture/BENCHMARKS.md), [RELEASE.md](architecture/RELEASE.md), [VOICE.md](architecture/VOICE.md), [DISTRIBUTION.md](architecture/DISTRIBUTION.md), [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) §4, [UX.md](architecture/UX.md) §2, plan §115.

**Build order:**

1. **Toolchain and CI.** The Cargo + pnpm workspace (ARCH-33, done); `cargo deny` with GPL denied (ARCH-35, DIST-17); Oxlint, Prettier and
   Vitest for the UI (ARCH-36); `ci.yml` on Windows plus Linux/macOS checks for the portable
   crates (REL-03, ARCH-34); PR title check and release-please (REL-01, REL-02); CodeQL and
   Dependabot (REL-12, SEC-31); required checks on `main` (REL-04); unit-test layout (PLAN-13).
2. **`kivo-core`.** The `Event` enum with metadata (ARCH-22), the session state machine with tests
   (ARCH-24), turn cancellation tokens (ARCH-25), `Secret<T>` (SEC-18).
3. **`kivo-platform` + `kivo-testkit`.** Every platform trait and the `Capabilities` struct
   (ARCH-11, ARCH-12); fakes for tests (ARCH-13).
4. **`kivo-store`.** Versioned TOML config with migrations and backup (ARCH-29, DIST-10); SQLite
   in WAL mode with embedded migrations (ARCH-30); `profile_id` on every user table (ARCH-31);
   redacted rolling logs (ARCH-32).
5. **`kivo-ipc`.** Named pipe with user-only DACL (ARCH-14), session token (ARCH-15), framed
   JSON-RPC with a size limit (ARCH-16), `hello` versioning (ARCH-17), hardening review (SEC-29),
   generated TypeScript types with a CI staleness check (ARCH-18), reconnect with snapshot + deltas
   (ARCH-19), clients treated as untrusted (INT-11).
6. **Runtime and app shells.** `kivo-runtime` with the single-instance mutex and IPC server
   (ARCH-01), the tray on its own thread (ARCH-02), launching and supervising the app (ARCH-03);
   `KIVO.exe` spawning/connecting to the runtime and showing its state (ARCH-04), single instance
   (ARCH-05), no store/secrets/tools in the app (ARCH-39), Tauri hardening (SEC-28).
7. **Spikes and benchmarks.** The `kivo-bench` harness and machine records (BENCH-01, BENCH-10);
   the overlay window spike (UX-05, BENCH-07); audio capture/playback and idle cost (VOICE-01,
   BENCH-05, BENCH-06); wake-word comparison (BENCH-04); STT comparison (BENCH-02); TTS comparison
   (BENCH-03); CI smoke benchmarks (BENCH-13); the report and updated defaults (BENCH-11).
8. **License checks:** the `uiautomation` crate, Smart Turn v3 and `interprocess` DACL support,
   recorded in DECISIONS.md.

**Deliverable:** launching `KIVO.exe` starts the runtime; the tray icon appears; the app shows
"Connected · Idle" with live state. Killing the app makes the runtime relaunch it; killing the
runtime makes the app show "Reconnecting" and recover when it's back. `docs/benchmarks/` holds the
first report.

**How to verify:** CI green on a PR; `cargo test --workspace` (state machine, config migration,
IPC round trip and rejection of a wrong token or remote client); manual kill/restart of each
process; the benchmark report reviewed by the owner.

**Exit criteria:**

- [ ] **M0-X1** · CI is green on every PR (REL-03).
- [x] **M0-X2** · The runtime and the UI restart independently, and the UI reconnects and shows the runtime state.
- [ ] **M0-X3** · The benchmark report is committed to `docs/benchmarks/`.
- [ ] **M0-X4** · The engine defaults in DECISIONS.md are updated with measured numbers, and the three license checks are recorded.

### M0 items

<!-- items:M0 -->
**51 items** · ✅ 40 done · 🟡 10 partial · ⏭ 1 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (26)

- [x] **ARCH-01** · `kivo-runtime` binary: starts, holds the `Local\KIVO.Runtime.<user-sid>` mutex (a second runtime exits), serves IPC (§1) → done: `apps/kivo-runtime` (`main.rs` startup/shutdown order, `core.rs` state + bus + shutdown, `rpc.rs`, args `--autostart`/`--from-app`/`--no-app`); mutex `Local\KIVO.Runtime.<sid>` via `kivo-platform-windows::instance`; fresh session token per start, removed on exit · verified: 16 unit tests; live run on Windows 11 (2026-09-21): a second runtime exits in 33 ms, settings/database/token created, logs written <!-- synced:x -->
- [x] **ARCH-02** · The runtime owns the tray (`tray-icon` on a runtime thread with a Win32 message loop) and it survives a UI crash (§1) → done: `apps/kivo-runtime/src/tray.rs` on `kivo-platform-windows::WindowsTray` (own thread + Win32 message loop); menu, icon and tooltip follow the session state; choices go to the core · verified: tray unit tests; live run on Windows 11 (2026-09-21): the tray stays while the app is killed and relaunched <!-- synced:x -->
- [x] **ARCH-03** · The runtime launches `kivo-app` and relaunches it with backoff if it crashes (§1) → done: `apps/kivo-runtime/src/app.rs` (launches `KIVO.exe`/`kivo-app.exe` beside it; follows the app's IPC connection; 3 s grace, relaunch in the background, backoff 1 s doubling to 30 s after quick crashes, gives up after 5 until the Control Center is requested) · verified: 6 supervisor tests with a fake app; live run on Windows 11 (2026-09-21): killed the app, relaunched with `--background` in ~1 s <!-- synced:x -->
- [x] **ARCH-04** · `KIVO.exe` (Tauri) connects to the runtime pipe; if absent it spawns `kivo-runtime.exe` (sidecar, `externalBin`) detached, then connects; it shows the runtime state and can restart without stopping KIVO (§1) → done: `apps/kivo-app/src-tauri/src/runtime.rs` (connects; if absent starts `kivo-runtime --from-app` hidden from the app's folder (debug builds add `--no-app`, so stopping `pnpm dev` stays stopped), then `connect_with_backoff`; `Link` status/version/state pushed to the UI), `src/ipc/runtime.tsx`, sidebar `RuntimeStatus`, `pages/Home.tsx` · verified: live run on Windows 11 (2026-09-21): launching the app started the runtime and showed "Connected · Ready"; killing the runtime showed "Reconnecting…" with Start KIVO; app restarts leave the runtime running · note: bundling the runtime as a Tauri `externalBin` sidecar is part of the installer (DIST-01) <!-- synced:x -->
- [x] **ARCH-05** · `tauri-plugin-single-instance`: a second `KIVO.exe` forwards "show main window" and exits (§1) → done: `tauri-plugin-single-instance` registered first in `src-tauri/src/lib.rs`; a second launch shows, unminimizes and focuses the window (and `--page`), and starts the runtime if it isn't connected · verified: live run on Windows 11 (2026-09-21): a second `kivo-app.exe` exited, one app remained, the runtime came back <!-- synced:x -->
- [x] **ARCH-11** · `kivo-platform` defines every trait in the §2 table (`AudioIo`, `EchoCancel`, `Hotkeys`, `Tray`, `Apps`, `Windows`, `UiAutomation`, `Input`, `Screen`, `Ocr`, `Secrets`, `SystemInfo`, `Notifications`); core crates depend only on the traits (§2) → done: `crates/kivo-platform` (AudioIo + FrameSink/FrameSource, EchoCancel, Hotkeys, Tray, Apps, Windows, UiAutomation, Input, Screen, Ocr, Secrets, SystemInfo, Notifications; object-safe; user-safe PlatformError) · verified: 7 unit tests, builds on Windows, Ubuntu and macOS in CI <!-- synced:x -->
- [x] **ARCH-12** · A `Capabilities` struct is filled at startup (Windows build, OS AEC, Mica, NPU, …); Windows 10 differences are handled only inside `kivo-platform-windows` (§2) → done: `crates/kivo-platform-windows/src/capabilities.rs` (RtlGetVersion build, DisplayVersion, OS AEC from 22621, Mica from 22000, package identity, NPU via DXCore GENERIC_ML + NPU attribute GUIDs from the Windows SDK header) · verified: on this PC reports "Windows 11 25H2 (build 26200)", AEC and Mica true, not packaged; NPU=false here (a positive NPU result still needs a Copilot+ PC to confirm) <!-- synced:x -->
- [x] **ARCH-13** · `kivo-testkit` provides fake platform, fake audio and fake brain implementations for tests (§7) → done: `crates/kivo-testkit` provides fakes for hotkeys (conflicts), tray, notifications, apps, windows, secrets, system info, a scripted audio device, and the fake brain (`ScriptedBrain`, `MockServer` for adapter contract tests, re-exported from `kivo-brain::testing`) · verified: testkit tests stable over 25 runs; the brain end-to-end tests run on `ScriptedBrain` (2026-09-23) <!-- synced:x -->
- [x] **ARCH-14** · Named-pipe transport (`interprocess`), `\.\pipe\kivo-<user-sid>`, security descriptor granting only the current user's SID, `PIPE_REJECT_REMOTE_CLIENTS` (§3) → done: `crates/kivo-ipc/src/transport/windows.rs` (tokio named pipe `\.\pipe\kivo-<sid>`, protected user-only DACL, reject remote clients, first-instance creation) and `transport/unix.rs` (mode-600 socket) · verified: tests read the pipe DACL back as `D:P(A;;FA;;;<user sid>)` and prove a second first-instance (squatting) is refused <!-- synced:x -->
- [x] **ARCH-15** · Random 256-bit session token in `%LOCALAPPDATA%\KIVO\run\session.token` (user-only ACL); clients must present it in `hello` (§3) → done: `crates/kivo-ipc/src/token.rs` (256-bit random, written to a file restricted to the user before the secret is written, constant-time compare) · verified: tests for format, uniqueness, exact matching, file round trip and the file DACL <!-- synced:x -->
- [x] **ARCH-16** · Length-prefixed frames carrying JSON-RPC 2.0: request/response, notifications (events), and a message size limit (§3, SECURITY §9) → done: `frame.rs` (u32 LE length prefix, 1 MiB limit) + `protocol.rs` (strict JSON-RPC 2.0: request/response/notification, "2.0" enforced, unknown fields rejected) · verified: protocol unit tests and `oversized_frames_close_the_connection` <!-- synced:x -->
- [x] **ARCH-17** · `hello { protocol_version, client, capabilities }`; incompatible major versions are rejected with a clear error (§3) → done: `hello { protocolVersion, client, token }` must come first within 3 s; wrong token → -32001, other major → -32002 with a clear message; welcome returns the snapshot · verified: `another_protocol_major_is_refused_with_a_clear_message`, `a_wrong_token_is_refused_and_others_still_connect`, `the_first_message_must_be_hello`, `silent_clients_are_dropped_after_the_hello_timeout` <!-- synced:x -->
- [x] **ARCH-18** · Rust types in `kivo-ipc` are the source of truth; TypeScript types are generated (`specta` or `ts-rs`) into `apps/kivo-app/src/ipc/generated.ts`, and CI fails when they are stale (§3) → done: ts-rs derives behind a `ts` feature; `crates/kivo-ipc/tests/ts_bindings.rs` writes `apps/kivo-app/src/ipc/generated.ts`; CI step fails when stale · verified: UI typechecks the file; a hand edit makes the check fail <!-- synced:x -->
- [x] **ARCH-19** · The UI reconnects with backoff and re-subscribes; on reconnect the runtime sends a full `StateSnapshot`, then deltas (§3) → done: the app keeps reconnecting with `connect_with_backoff` and re-reads the token; welcome snapshot then `snapshot` notifications on every change · verified: `state_changes_are_pushed_to_every_client`, `clients_reconnect_after_the_runtime_restarts_with_a_new_token`; live run on Windows 11 (2026-09-21): the app reconnected to a restarted runtime <!-- synced:x -->
- [x] **ARCH-22** · `enum Event` in `kivo-core` with every group in §4.1 (Voice, Turn, Tool, Task, System incl. `FileChanged`/`BrowserChanged` from plan §66, Provider, Ui), each carrying `ts` (monotonic + wall), `turn_id`/`task_id` where relevant, and `trace_id`; delivered over `tokio::sync::broadcast` (§4.1) → done: `crates/kivo-core/src/event.rs` (all §4.1 groups + FileChanged/BrowserChanged), `bus.rs` (tokio broadcast, lag reported) · verified: 6 unit tests incl. JSON shape and round-trip of every group <!-- synced:x -->
- [x] **ARCH-24** · The session state machine (Idle, Listening, Thinking, Acting, Speaking, FollowUp, Interrupted, Paused, AwaitingConfirmation, Error) with every transition in the §4.2 diagram, unit-tested; an invalid transition logs at `error` (§4.2) → done: `crates/kivo-core/src/session.rs` · verified: table test of all 170 state/input pairs, invalid transitions leave state unchanged and log at error <!-- synced:x -->
- [x] **ARCH-25** · A Turn has a root `CancellationToken`; child tokens go to STT, the brain request, each tool call and TTS (§4.2) → done: `crates/kivo-core/src/turn.rs` (root token, child per stage) · verified: 4 tests (turn cancels stages, stage cancel is local, late stages start cancelled, waiting stage wakes) <!-- synced:x -->
- [x] **ARCH-29** · Config: `%APPDATA%\KIVO\config\kivo.toml` (+ `profiles/*.toml`) with `schema_version`; validated on load, migrated forward, previous version backed up; all §5 config sections exist (§5) → done: schema `crates/kivo-core/src/config.rs` (all §5 sections, UX §5 defaults, `schema-version`, validation, unknown keys rejected), IO `crates/kivo-store/src/config.rs` (atomic save, forward migrations with a `.vN.bak` backup, broken files kept as `.invalid-<ts>`, newer-version files left untouched, Bypass reset to Auto on load), paths `crates/kivo-store/src/paths.rs` · verified: 10 tests incl. synthetic two-step migration, partial files, out-of-range values and typos · note: `profiles/*.toml` arrive with brain profiles (M3) <!-- synced:x -->
- [x] **ARCH-30** · Database: `%LOCALAPPDATA%\KIVO\data\kivo.db`, rusqlite (bundled), WAL, embedded migrations (§5) → done: `crates/kivo-store/src/db.rs` (rusqlite bundled, WAL, synchronous=NORMAL, foreign keys, busy timeout, rusqlite_migration with embedded SQL, owner profile created on first open) · verified: tests for WAL + FK pragmas, stable owner id across opens, single-owner constraint <!-- synced:x -->
- [x] **ARCH-31** · Every user-owned record carries `profile_id` from the first migration, so people profiles need no later migration (§5, UX §8.2) → done: `profiles` table from migration 1; every other table must reference it via `profile_id` · verified: test `the_shipped_schema_is_profile_scoped` checks the real migrated schema in CI, and `the_scoping_check_catches_a_table_without_profile_id` proves the check works <!-- synced:x -->
- [x] **ARCH-32** · Logs: `tracing` rolling JSON in `%LOCALAPPDATA%\KIVO\logs\`, 7-day retention, a redaction layer for keys, tokens and `sensitive` fields; transcripts logged only with the "debug transcripts" setting (§5, §6) → done: `crates/kivo-store/src/logging.rs` (daily rolling JSON `kivo.*.log`, 7 files kept, non-blocking writer, redaction of sensitive field names, known key/token formats, name=value secrets, and transcripts unless debug transcripts is on) · verified: 5 tests incl. an end-to-end file write that checks the secret and transcript never reach disk and the line stays valid JSON <!-- synced:x -->
- [x] **ARCH-33** · Cargo workspace + pnpm workspace with every crate and app in the §7 layout → done: `Cargo.toml`, `crates/*`, `apps/*`, `pnpm-workspace.yaml` · verified: `cargo check --workspace`, `pnpm typecheck` (2026-09-21) <!-- synced:x -->
- [x] **ARCH-34** · Rust stable with MSRV pinned in `rust-toolchain.toml`; `cargo fmt`; `clippy -D warnings` → done: `rust-toolchain.toml` pins 1.97.0 (MSRV stays `rust-version` 1.90), `rustfmt.toml`, CI runs `cargo fmt --check` and clippy with `RUSTFLAGS=-D warnings` · verified: CI green on Windows, Ubuntu, macOS (2026-09-21) <!-- synced:x -->
- [x] **ARCH-35** · `cargo deny` configured: licenses (GPL/AGPL denied in the default graph), advisories, bans (§7) → done: `deny.toml` (permissive allow-list, no GPL/AGPL, advisories, crates.io only, unmaintained checked for direct deps) · verified: `cargo deny check` clean locally with 0 warnings and in CI <!-- synced:x -->
- [x] **ARCH-36** · UI toolchain: Node LTS + pnpm, TypeScript strict, Oxlint (type-aware; ESLint cannot run on TypeScript 7), Prettier, Vitest (§7) → done: pnpm 11.27.1, TypeScript strict, Oxlint type-aware (`.oxlintrc.json`), Prettier (`.prettierrc.json`), Vitest on jsdom (21 tests) · verified: all run clean in CI <!-- synced:x -->
- [x] **ARCH-39** · `kivo-app` has no access to the store, secrets or tools; its only API is IPC (reviewed in every milestone) (§8) → done: the app depends only on `kivo-core` (types), `kivo-ipc` and `kivo-platform` (paths) — no store, secrets or tools crates; its webview gets three commands (`ui_ready`, `runtime_request`, `runtime_start`) · verified: `apps/kivo-app/src-tauri/Cargo.toml` review; re-check every milestone <!-- synced:x -->

#### [BENCHMARKS.md](architecture/BENCHMARKS.md) (10)

- [x] **BENCH-01** · `kivo-bench` CLI with the harness: machine fingerprint (CPU, GPU, RAM, OS build, power source), ≥ 5 runs with p50/p95, warmup excluded from cold metrics, JSON into the `benchmarks` table and `bench-results/<date>-<machine>.json`, Markdown summary into `docs/benchmarks/` (§1) → done: `apps/kivo-bench` (`machine.rs` fingerprint from `kivo-platform-windows::WindowsSystemInfo` + capabilities; `harness.rs` warmup discarded then N runs; `stats.rs` nearest-rank p50/p95/min/max/mean; `report.rs` merges each day's `bench-results/<date>-<machine>.json`, rewrites `docs/benchmarks/<date>-<machine>.md`; rows in the new `benchmarks` table, migration 2); saved runs require ≥ 5 runs · verified: 8 unit tests; `kivo-bench ipc --runs 10 --warmup 2` on this PC wrote all three outputs (ping round trip p50 28.7 µs) <!-- synced:x -->
- [~] **BENCH-02** · `stt` suite: cold load, first partial, final latency, RTF, WER (overall + accented), peak RAM/VRAM, CPU% on LibriSpeech subset, accented English and 50 KIVO commands (§1) → partial: `apps/kivo-bench/src/speech/stt.rs` (Moonshine v2, Parakeet TDT v3, Whisper turbo via sherpa-onnx; cold load, end-of-speech → final, RTF, WER with normalization, peak memory on LibriSpeech test-clean) + `wer.rs` (tested) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; accented-English WER needs a licensed set (L2-ARCTIC / Common Voice); first-partial time needs a streaming engine <!-- synced:~ -->
- [~] **BENCH-03** · `tts` suite: first audio (short/medium/long), RTF, RAM, cancel-to-silence (§1) → partial: `apps/kivo-bench/src/speech/tts.rs` (Kokoro-82M int8: first audio for short/medium/long text, RTF, cancel → silence, memory) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>` <!-- synced:~ -->
- [~] **BENCH-04** · `wake` suite: false accepts/hour on ≥ 10 h negative audio, false rejects % on the recorded "Hey Kivo" set, CPU% (§1) → partial: `apps/kivo-bench/src/speech/wake.rs` ("Hey Kivo" with sherpa KWS; synthetic positives from every Kokoro voice × 3 speeds through a room, clean and over speech; LibriSpeech negatives; false rejects, false accepts/hour, CPU; tokenizer tested) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; ≥ 10 h mixed negatives (5.4 h of read speech available) <!-- synced:~ -->
- [~] **BENCH-05** · `vad` / `aec` suite: detection latency, false triggers with TTS playing, echo leakage into STT (§1) → partial: `apps/kivo-bench/src/speech/vad_aec.rs` (Silero VAD v6 + WebRTC AEC3 via sonora on a simulated room: onset/end latency, false triggers and STT leakage from echo with and without AEC, ERLE, barge-in onset) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; the OS AEC on real devices (VOICE-30) <!-- synced:~ -->
- [~] **BENCH-06** · `idle` suite: CPU%, RAM, wakeups/s and package power over 30 min with listening on (§1) → partial: `apps/kivo-bench/src/suites/idle.rs`; 30-min run saved (release build, 2026-09-21): runtime 0.00% CPU, 6.8 MB, 0 wakeups/s; app + WebView2 0.00% CPU, 230 MB (over the 120 MB UI budget, BENCH-12) · missing: "listening on" (always-on wake listening arrives in M2) <!-- synced:~ -->
- [x] **BENCH-07** · `overlay` suite: hotkey → done: `apps/kivo-bench/src/suites/overlay.rs` + `win.rs` (synthetic push-to-talk; screen sampling over a grey backdrop for first frame, white flash and exit; window style and focus checks; GPU 3D load and RAPL package power from Windows performance counters instead of PresentMon, DECISIONS "Benchmark counters") · verified: saved release-build run (2026-09-21): 155 ms p50 to visible, no flash, +1.1 W package power while animating (was +12 W before the redraw fixes) <!-- synced:x -->
- [~] **BENCH-10** · Reference machines recorded: the owner's PC (mid/high) and the 4-core / 8 GB / no-GPU VM (low, labelled approximate) (§2, DECISIONS) → partial: the owner's PC is recorded in every report (Ryzen 7 6800H, 16 threads, 15 GB, RTX 3060 6 GB + Radeon iGPU, Windows 11 25H2); `--tier low` emulates the 4-core tier on it, labelled approximate (DECISIONS "Wake-word data") · missing: a low-tier run (not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`) <!-- synced:~ -->
- [~] **BENCH-11** · The first benchmark report is committed to `docs/benchmarks/` and the engine defaults in DECISIONS.md are updated with measured numbers (§1) → partial: first report committed: `docs/benchmarks/2026-09-21-amd-ryzen-7-6800h.md` (ipc, audio, overlay, idle) + `bench-results/` · missing: the engine results and updated engine defaults in DECISIONS (needs BENCH-02–05; not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`) <!-- synced:~ -->
- [~] **BENCH-13** · CI smoke subset on every PR: intent-router latency, IPC round trip, state-machine and cancellation tests (§4) → partial: `ci.yml` runs `kivo-bench ipc` after the tests (state-machine and cancellation tests are unit tests in the same job) · missing: the intent-router latency (the router is M1); not yet run in CI (runs on the next version tag) <!-- synced:~ -->

#### [DISTRIBUTION.md](architecture/DISTRIBUTION.md) (2)

- [x] **DIST-10** · Config and database migrations run forward only, with a backup first (§2, ARCH-29) → done: config migrations back up to `kivo.toml.vN.bak` first; database upgrades take a `VACUUM INTO` copy (`kivo.db.vN.bak`) first; both forward-only (a newer database is refused, a newer config is left untouched) · verified: tests `older_files_are_backed_up_then_migrated_in_order`, `a_backup_is_a_complete_readable_copy`, `a_database_from_a_newer_kivo_is_not_downgraded` <!-- synced:x -->
- [x] **DIST-17** · `cargo deny` denies GPL/AGPL in the default graph; GPL components only as separately downloaded add-ons after legal review (§6, ARCH-35) → done: `deny.toml` denies GPL/AGPL by allow-list · verified: `cargo deny check licenses` in CI <!-- synced:x -->

#### [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) (1)

- [x] **INT-11** · Nothing assumes the UI is local-only: IPC is versioned and clients are untrusted (§4) → done: every IPC client is untrusted: token-authenticated, version-checked, schema-validated and size-limited; nothing assumes the UI is local-only beyond the transport · verified: kivo-ipc tests <!-- synced:x -->

#### [RELEASE.md](architecture/RELEASE.md) (5)

- [x] **REL-01** · Conventional Commits for every commit, kept by convention (§1) → done: rule in the root `CLAUDE.md`; CI enforcement removed with the tag-only workflows (DECISIONS: "GitHub Actions only on version tags") · verified: every commit since `a1fbee5` follows it (`git log --format=%s`) <!-- synced:x -->
- [x] **REL-02** · `pnpm release:version X.Y.Z` sets the version in `Cargo.toml` (workspace), `tauri.conf.json` and both `package.json` files and refreshes `Cargo.lock`; the owner commits and pushes the tag `vX.Y.Z` (§1) → done: `scripts/set-version.mjs`; release-please removed (DECISIONS: "GitHub Actions only on version tags"); versions reset to 0.0.0 · verified: ran it with a test version and back; `git diff` showed exactly the four version lines and the workspace entries in Cargo.lock; a bad version is refused <!-- synced:x -->
- [~] **REL-03** · `ci.yml` on every `v*` tag (and by hand): Rust fmt, clippy `-D warnings`, tests and `cargo deny` on windows-latest, plus ubuntu-latest and macos-latest for the portable crates; UI install, typecheck, lint, Vitest; generated IPC types up to date; smoke benchmarks (§2, §3) → partial: `ci.yml` (tags `v*` + manual): Rust fmt/clippy -D warnings/tests on Windows, portable crates on Ubuntu + macOS, cargo-deny, generated-IPC-types check, IPC smoke benchmark, UI typecheck/lint/format/test/build/audit, ROADMAP sync · verified: the jobs were green on pushes before the tag-only switch · missing: a run on a version tag since the switch <!-- synced:~ -->
- [-] **REL-04** · Required status checks added to the `main` ruleset once CI exists; Actions pinned by commit SHA (§4) → dropped: CI runs only on version tags, so there are no checks on `main` to require (DECISIONS: "GitHub Actions only on version tags"); every action stays pinned by commit SHA <!-- synced:- -->
- [x] **REL-12** · `codeql.yml` on version tags, and Dependabot alerts (§2) → done: `codeql.yml` (JS/TS, Rust, Actions) on `v*` tags and by hand; Dependabot alerts stay on in the repository; update PRs removed (DECISIONS: "GitHub Actions only on version tags") · verified: CodeQL run succeeded earlier; `dependabot.yml` removed <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (4)

- [x] **SEC-18** · `Secret<String>` newtype: no `Display`, `Debug` prints `***`, no `Serialize`, zeroized on drop (§5) → done: `crates/kivo-core/src/secret.rs` · verified: tests for Debug `***`, expose, zeroize; no Display/Serialize impls <!-- synced:x -->
- [x] **SEC-28** · Tauri: strict CSP, no remote content, `withGlobalTauri: false`, no `shell` plugin, minimal `capabilities/*.json` per window (the overlay window gets almost nothing) (§9) → done: strict CSP, `withGlobalTauri` off, no shell plugin; the app manifest (`build.rs`) exposes only `ui_ready`, `runtime_request`, `runtime_start`, `overlay_fit`; `capabilities/default.json` (main: core + window controls + three app commands) and `capabilities/overlay.json` (overlay: listen/unlisten, `ui_ready`, `overlay_fit` only) · verified: both windows work with exactly these grants (release build, 2026-09-21) <!-- synced:x -->
- [x] **SEC-29** · IPC hardening: user-only DACL, reject remote clients, session token, schema validation, message size limit (§9, ARCH-14–16) → done: user-only DACL, PIPE_REJECT_REMOTE_CLIENTS, first-instance pipes, session token, strict schema parsing (unknown fields and malformed messages close the connection), 1 MiB frame limit · verified: 20 IPC tests over the real transport, stable over 20 runs <!-- synced:x -->
- [x] **SEC-31** · Dependencies: `cargo deny`, `pnpm audit`, lockfiles committed, Dependabot alerts (§9) → done: `cargo deny` and `pnpm audit --audit-level high` in CI (on version tags), `Cargo.lock` + `pnpm-lock.yaml` committed and used with --locked/--frozen-lockfile, Dependabot alerts · verified: CI green; `cargo deny check` and `pnpm audit` also run locally <!-- synced:x -->

#### [UX.md](architecture/UX.md) (1)

- [x] **UX-05** · Overlay spike: a transparent, borderless, non-focusable (`WS_EX_NOACTIVATE`), always-on-top, click-through window with no taskbar entry; white-flash and show latency measured (§2, BENCH-07) → done: `apps/kivo-app/src-tauri/src/overlay.rs` (preloaded hidden; transparent, borderless, `focusable(false)` → WS_EX_NOACTIVATE, click-through, topmost re-asserted on show, no taskbar entry; WebView2 set invisible when hidden) + `src/overlay/` page · verified: `kivo-bench overlay` on the release build (2026-09-21): hotkey → Island visible p50 155 ms / p95 155 ms, no white flash in 6 runs over a grey backdrop, never activates, click-through, topmost, never took focus; zero CPU in every process while hidden <!-- synced:x -->

#### [VOICE.md](architecture/VOICE.md) (1)

- [x] **VOICE-01** · Capture through `AudioIo` (WASAPI via windows-rs, cpal fallback) and playback, measured for idle CPU in the audio spike (§1) → done: `crates/kivo-platform-windows/src/audio.rs` (`WindowsAudio`: device lists with friendly names and defaults; WASAPI shared mode, event-driven, 32-bit float at the device rate with AUTOCONVERTPCM, stream thread under MMCSS "Audio", stops on drop) · verified: tests capture 24 000 frames in 500 ms at 48 kHz from the Realtek mic array and play to the default output; `kivo-bench audio`: holding the mic open costs 0.05% of the machine, 10 ms packets (p99 11.2 ms), 100% of frames delivered, playback starts in 39 ms · note: no cpal fallback: on Windows cpal is itself WASAPI, and AUTOCONVERTPCM accepts float on every device (DECISIONS) <!-- synced:x -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (1)

- [~] **PLAN-13** · Test layers in place: unit tests (routing, permissions, state, config, adapters, task graph) from M0; integration tests as each subsystem lands; end-to-end tests for voice → partial: unit tests across kivo-core, kivo-ipc (incl. end-to-end over the real pipe), kivo-store, kivo-platform-windows (real hotkeys, audio, tray), kivo-runtime, kivo-bench and the UI (Vitest) · missing: integration and end-to-end voice tests as the M1 pipeline lands <!-- synced:~ -->
<!-- /items -->

---

<a id="m1"></a>

## M1: Vertical slice, "push-to-talk → action → speech"

**Goal:** hold Ctrl+Space, say "mute", "open Chrome" or "take a screenshot", and KIVO does it
natively, offline, and says so, with the Island, tray, Activity, audit log and basic permissions
all real.

**Depends on:** M0 (IPC, store, platform traits, measured engine defaults) and D0 (components).

**Read first:** [ARCHITECTURE.md](architecture/ARCHITECTURE.md), [VOICE.md](architecture/VOICE.md), [BRAINS.md](architecture/BRAINS.md), [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md), [SECURITY.md](architecture/SECURITY.md), [CAPABILITIES.md](architecture/CAPABILITIES.md), [UX.md](architecture/UX.md), [DISCOVERY.md](architecture/DISCOVERY.md), [DISTRIBUTION.md](architecture/DISTRIBUTION.md), [RELEASE.md](architecture/RELEASE.md), [BENCHMARKS.md](architecture/BENCHMARKS.md), [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md), plan §25–26, §95, §126–128, §138 and §146.

**Build order:**

1. **Startup and shutdown skeleton.** Startup order (PLAN-14), shutdown order (PLAN-15), crash
   dumps (ARCH-10), hardware detection (PLAN-01).
2. **Audio in.** Resampling and frames (VOICE-02), the capture thread and ring buffer (VOICE-03),
   energy gate + Silero VAD (VOICE-04), the push-to-talk hotkey (VOICE-41).
3. **Inference worker.** `kivo-infer` supervision (ARCH-09) and its protocol (ARCH-21); the voice
   provider traits (VOICE-06); language packs and settings (VOICE-36, VOICE-37); the privacy check
   for cloud engines (VOICE-07); streaming STT (VOICE-08); streaming TTS (VOICE-09); residency and
   prewarm (VOICE-34, PLAN-02).
4. **Understanding.** The grammar fast path (BRAIN-01), permission routing for destructive commands
   (BRAIN-02), router metrics (BRAIN-05).
5. **Doing.** `ToolSpec` and the `Decision`-gated executor (TOOL-01, TOOL-02), readable errors
   (TOOL-03, PLAN-18); apps (TOOL-06), windows (TOOL-07), volume/mute (TOOL-09), media (TOOL-11),
   lock/sleep/restart/shutdown (TOOL-12), screenshot (TOOL-14), toast (TOOL-18), open URL/search
   (TOOL-23).
6. **Safety.** `authorize()` (SEC-06) and the default policy (SEC-07); Ask and Auto modes
   (SEC-01), switching modes (SEC-04), hard limits (SEC-05); the confirmation card (SEC-10); the
   hash-chained audit log (SEC-22); the emergency stop hotkey/tray/button (SEC-25); the capability
   model, "turn it on?" answers and toggle auditing (CAP-01, CAP-02, CAP-03).
7. **Speaking back.** Earcons with gating (VOICE-24, VOICE-25); fast visual acknowledgement
   (PLAN-11).
8. **State to the UI.** Activity persistence (ARCH-23), T0–T10 spans (ARCH-28), cancellation in
   ≤ 100 ms (ARCH-26), audio levels while animating (ARCH-20), push-only live state (DISC-13), no
   work while the Control Center is closed (DISC-19).
9. **The Island window.** Window and placement (UX-06), states (UX-07), card (UX-09), collapse rules
   (UX-10), fullscreen/Focus behaviour (UX-11), keyboard (UX-12), type-to-KIVO (UX-41).
10. **The Control Center pages for M1.** Home (UX-19), Activity (UX-20); i18n for every string
    (UX-49), logical CSS properties (UX-50), ARIA/keyboard pass (UX-52).
11. **Lifecycle.** First launch / relaunch (UX-01), close-to-tray with the one-time toast (UX-02),
    tray menu (UX-03), tray icon states (UX-04), tray tooltip (UX-56), actionable notifications
    (UX-57), autostart (ARCH-06), Quit (ARCH-07).
12. **Packaging.** NSIS per-user installer with sidecars (DIST-01), bundle contents (DIST-02),
    shortcuts/uninstaller/upgrades (DIST-05), model manager and licenses (DIST-12, DIST-13), no
    telemetry (DIST-15), `release.yml` for Windows x64 and the install smoke test (REL-05, REL-06).
13. **Measure.** End-to-end benchmarks (BENCH-08), idle and startup budgets (BENCH-12, VOICE-40),
    the resource-rules review (PLAN-16), the "Kivo, mute" acceptance journey (PLAN-19).

**Deliverable:** an installer. After install: KIVO starts at sign-in if chosen, sits in the tray,
and Ctrl+Space → "open Chrome" opens Chrome and says "Done" through the chosen voice, with the
Island showing listening → thinking → acting → speaking. "Shut down" asks first. The emergency stop
silences everything. Activity and the audit log show every step.

**How to verify:** the M1 exit criteria measured with `kivo-bench e2e` on the mid tier with the
network off; a cancellation-latency test in CI; unit tests for the grammar, the policy table and the
audit chain; a manual run through every Island state; a clean-VM install/uninstall.

**Exit criteria:**

- [x] **M1-X1** · "Mute", "open Chrome" and "take a screenshot" work offline in ≤ 500 ms after the end of speech on the mid tier.
- [x] **M1-X2** · Cancel → silence ≤ 100 ms, enforced by a test (ARCH-26).
- [ ] **M1-X3** · Idle budgets are met (VOICE-40, BENCH-12).
- [ ] **M1-X4** · The installer installs, upgrades and uninstalls cleanly on a fresh Windows 11 VM and a Windows 10 VM.

### M1 items

<!-- items:M1 -->
**86 items** · ✅ 79 done · 🟡 7 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (9)

- [x] **ARCH-06** · Autostart registers `kivo-runtime.exe --autostart`; the runtime then launches `KIVO.exe --background`, which preloads the overlay without showing a window (§1) → done: `Lifecycle::apply_autostart` keeps the HKCU Run entry `"…\kivo-runtime.exe" --autostart` in step with the setting (off by default); an autostarted runtime launches the app with `--background`, which creates the overlay hidden and opens no window · verified: `autostart_follows_the_setting`, `the_startup_entry_is_written_read_and_removed` (real registry, test-only name), `Launch::Background` args test (2026-09-23) <!-- synced:x -->
- [x] **ARCH-07** · Quit (tray or Control Center) stops the runtime, closes the UI and releases the mic; closing a window never quits (§1) → done: Quit from the tray, Ctrl+K and the first-close notification's Quit button sends `runtime.quit`; `ShuttingDown` reaches the app before connections close; the app exits; the runtime shuts down in order and removes its token; the mic is open only while listening; closing the window hides it while connected · verified: `an_event_published_just_before_shutdown_is_still_delivered`, `the_first_close_notice_shows_once_and_its_buttons_work`, live quit over the pipe (2026-09-23) <!-- synced:x -->
- [x] **ARCH-09** · `kivo-infer` runs as a supervised worker (same binary, subcommand), started on demand, restarted on crash; the current turn fails gracefully with a spoken message (§1) → done: `infer::supervise` starts `kivo-infer serve` when an engine is wanted, restarts it with backoff after a crash, and reports `Lost`; the engine fails the turn with a message that is shown and spoken by Windows' voice in-process (`fallback_voice`), since the worker's voices are gone · verified: `a_crashed_speech_worker_fails_the_turn_aloud_and_comes_back` kills the real worker mid-turn and sees the failure spoken and a new worker start (2026-09-23) <!-- synced:x -->
- [x] **ARCH-10** · Crash dumps are written locally to `%LOCALAPPDATA%\KIVO\crashes\` and reported on the next start; never uploaded without consent (§1, §5) → done: `kivo-platform-windows::crash` installs a panic hook and an unhandled-exception minidump writer (runtime, worker) into `%LOCALAPPDATA%\KIVO\crashes\`; `Lifecycle::report_crashes` reports new ones once on the next start (log, Activity, a local notification); nothing is uploaded · verified: `crashes_are_reported_once_on_the_next_start`, kivo-store crash tests (2026-09-23) <!-- synced:x -->
- [x] **ARCH-20** · A separate high-rate `levels` notification (30–60 Hz) carries audio levels, only while the overlay is animating (§3) → done: `crates/kivo-ipc` `levels` notification (`Server::with_levels`), fed by `apps/kivo-runtime/src/mic.rs` at 30 Hz only while listening (peak RMS → −60…−10 dBFS scale); the app forwards it to the overlay only; the waveform shows dots in silence · verified: `microphone_levels_stream_while_they_change`, mic unit tests, live on this PC <!-- synced:x -->
- [x] **ARCH-21** · The same protocol runs between the runtime and `kivo-infer`, with methods for streaming audio frames and results (§3) → done: the runtime and `kivo-infer` speak the same framed JSON-RPC (`kivo_ipc::Peer`) over stdin/stdout, with `hello`, `model.*`, `stt.start/audio/partial/finish/cancel`, `tts.speak/audio/done/cancel` and `shutdown` (`kivo_ipc::infer`) · verified: the spoken end-to-end test streams audio frames and results through the real worker (2026-09-23) <!-- synced:x -->
- [x] **ARCH-23** · The Activity recorder persists the Activity subset to SQLite (§4.1) → done: the Activity recorder (`activity::Recorder`) writes transcripts, tool calls, replies, stops and crashes to the `activity` table as they happen (DECISIONS "Activity recorder") · verified: the spoken end-to-end test reads the whole story back from Activity; recorder unit tests (2026-09-23) <!-- synced:x -->
- [x] **ARCH-26** · Stop, Esc, the overlay X, the emergency stop and barge-in cancel the turn token, and cancellation reaches every layer within 100 ms, enforced by a test (§4.2) → done: Stop, Esc, the Island's X and the emergency stop all go through `Engine::cancel`, which cancels the turn token, recognition, the tool, the voice (also a reply still playing after its turn ended) and the listener; barge-in (M2) takes the same path · verified: `cancelling_stops_every_layer_within_100_ms` (listening: < 1 ms for Esc, Stop and the emergency stop; speaking: 33 ms) and `timeouts_and_cancellation_end_the_wait_promptly` for the tool layer (2026-09-23) <!-- synced:x -->
- [x] **ARCH-28** · `tracing` spans T0 wake … T10 completion carry `turn_id`; a metrics subscriber stores per-turn timings in `turn_metrics` (§4.3) → done: each turn records its spans (t0 … t10, keyed by `turn_id`) and `turn_metrics` stores them when it finishes · verified: the spoken end-to-end test reads `t4EndOfSpeech`, `t5FinalTranscript`, `t8ToolDone`, `t10Complete` back (end of speech → action 37 ms) (2026-09-23) <!-- synced:x -->

#### [BENCHMARKS.md](architecture/BENCHMARKS.md) (2)

- [x] **BENCH-08** · `e2e` suite: T0–T10 spans for the three plan §102 journeys with scripted audio via a virtual mic (§1) → done: `kivo-bench e2e` drives `kivo-e2e` (the runtime's scripted journeys: Windows' voice into a scripted microphone, the real turn engine, speech worker and models, fake apps and system controls) through the three plan §102 journeys and reports T spans from the end of speech; medium and complex end as "unhandled" until the brain router (M3) and agents (M5) exist · verified: 5 runs on the owner's PC (2026-09-23): simple end of speech → action done 37 ms p50 / 38 ms p95; medium → intent 95 ms; complex → intent 172 ms; `spans_become_samples_from_the_end_of_speech` <!-- synced:x -->
- [~] **BENCH-12** · Budgets met: runtime idle RAM ≤ 150 MB, UI idle RAM ≤ 120 MB (overlay preloaded, CC closed), runtime cold start ≤ 1.5 s, Control Center open (warm) ≤ 300 ms (§3) → partial: hidden webviews drop to WebView2's low memory target and the hidden Control Center is suspended (`src-tauri/src/memory.rs`); RAM is the private working set (Task Manager's "Memory"; the `idle` suite now reports it beside committed memory). Measured on the owner's PC, debug build, started as at sign-in (CC closed, overlay preloaded): UI 48 MB (≤ 120), runtime 31 MB (≤ 150), runtime cold start to IPC ready 192–253 ms (≤ 1.5 s); the suspended Control Center reopens live (2026-09-23) · missing: Control Center warm-open timing (≤ 300 ms) and the release-build numbers on the reference tiers (deferred with the benchmarks) <!-- synced:~ -->

#### [BRAINS.md](architecture/BRAINS.md) (3)

- [x] **BRAIN-01** · Grammar stage: slot-based commands (`open {app}`, volume, `mute`, media, screenshot, window commands, `lock`) resolved against live app and window indexes, with phrasing loaded from per-language data files (§2) → done: `kivo-intent` grammar from `grammar/en/commands.toml` (per-language data: fillers, patterns, slots `{app}`, `{window}`, `{number}`, `{url}`, `{text}`), normalized transcripts, slots resolved against the live app index (Start menu + AppsFolder, fuzzy with aliases) and window index; covers open/close app, window commands, volume, mute, mic, media, screenshot, lock, sleep, restart, shutdown, open URL and web search · verified: grammar/index/normalize unit tests (`the_m1_journeys_match`, `misheard_names_still_match_but_nonsense_does_not` …) and both end-to-end tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-02** · Destructive commands (shutdown, restart) still go through the permission engine (§2) → done: shutdown and restart are High-risk tools; the grammar only produces a call, which goes through `authorize` like any other and always asks (except in Bypass) · verified: `destructive_commands_are_recognized_like_any_other`, policy tests (High must confirm) (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-05** · Metrics `fast_path_ratio` and per-stage p95 latency (§2) → done: `IntentRouter::metrics` keeps the fast-path share and the grammar stage's p95 over the last requests; the engine logs them per turn and exposes `Engine::router_metrics` · verified: `the_fast_path_ratio_and_grammar_latency_are_tracked`, `p95_uses_the_nearest_rank`, and the typed end-to-end test asserts 1 of 1 routed without AI (2026-09-23) <!-- synced:x -->

#### [CAPABILITIES.md](architecture/CAPABILITIES.md) (3)

- [x] **CAP-01** · Capability model in the runtime: every §1 capability with its default; a disabled capability's tools are **not registered** for any brain, routine, agent, MCP exposure or remote client (§1, §2) → done: `kivo_core::Capability` (all 27 §1 capabilities with their defaults) in the settings; the tool registry only hands out tools whose capability is on, so a disabled one's tools don't exist for any caller (brains, routines, agents, MCP and remote clients all go through the registry) · verified: `tools_of_a_disabled_capability_are_not_registered`, capability defaults test (2026-09-23) <!-- synced:x -->
- [x] **CAP-02** · A request that needs a disabled capability gets a plain answer ("Screen awareness is off. Turn it on?") with a one-tap link; toggles never turn on automatically (§2) → done: a request needing a disabled capability gets "<Capability> is off. Turn it on?" on the Island with a Turn on button (the overlay may only switch on that one capability); nothing turns on by itself · verified: `the_island_may_only_turn_on_the_capability_the_request_needed`, the Island turn tests, policy `CapabilityOff` test (2026-09-23) <!-- synced:x -->
- [x] **CAP-03** · Every toggle change is written to the audit log (§2) → done: `capabilities.set` writes an audit row (`capabilities.set`, "<Capability> = on/off") in the hash chain · verified: `capabilities_are_listed_toggled_and_audited` (2026-09-23) <!-- synced:x -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (2)

- [x] **DISC-13** · Live state (Island, Tasks, Activity, agent progress, usage) is pushed over IPC; nothing polls it (§3) → done: state, the live turn, mic and voice levels and Activity changes are pushed over IPC (`runtime://link`, `runtime://level`, `runtime://event`); the UI has no timers polling the runtime · verified: no `setInterval` in the UI, Activity refreshes on the pushed event (2026-09-23); tasks, agents and usage join when they exist (M3–M7) <!-- synced:x -->
- [x] **DISC-19** · Zero background work while the Control Center is closed beyond what the runtime needs (§3) → done: with nothing to do the runtime sleeps: the detection thread blocks until a command arrives (it used to wake every 30 ms), the speech worker isn't running, and the speaker closes when quiet · verified: live runtime with the app running, 0 ms of CPU over 20 s idle, 67 MB working set (2026-09-23) <!-- synced:x -->

#### [DISTRIBUTION.md](architecture/DISTRIBUTION.md) (6)

- [~] **DIST-01** · NSIS per-user installer (no admin, `%LOCALAPPDATA%\Programs\KIVO`) built from the Tauri bundle, with `kivo-runtime.exe` and `kivo-infer.exe` as `externalBin` sidecars (§1) → partial: `pnpm build` = `scripts/sidecars.mjs` (release `kivo-runtime`/`kivo-infer` → `src-tauri/binaries/<name>-<triple>.exe`) + `tauri build --config src-tauri/tauri.bundle.conf.json` (NSIS `installMode: currentUser` → `%LOCALAPPDATA%\Programs\KIVO`, `externalBin` sidecars, `mainBinaryName: KIVO`) · verified: the app compiles with the bundle config merged (tauri-build resolves the sidecars and resources), the runtime and app find each other and the worker beside them (2026-09-23) · missing: the first real installer build, which runs on the first `v*` tag (`release.yml`; DECISIONS "Installer and release at M1") <!-- synced:~ -->
- [~] **DIST-02** · Bundle contents: WebView2 bootstrapper (Windows 10), sounds, icons, default intent grammar; no models of any kind: every model is a download the user chooses (§1, DECISIONS "No models in the installer") → partial: the bundle carries the three executables and icons; the WebView2 bootstrapper is downloaded silently when missing; sounds are synthesized by the runtime and the grammar is compiled in; no models at all: Silero VAD is now a model-manager download installed with any speech-recognition model (`requires`), and the runtime downloads nothing unasked (DECISIONS "No models in the installer") · verified: `speech_recognition_brings_the_voice_activity_model`, bundle config compiles (2026-09-23) · missing: contents checked in a built installer (the smoke test asserts no model files; first `v*` tag) <!-- synced:~ -->
- [~] **DIST-05** · Installer registers the startup option, Start-menu shortcut and uninstaller; supports in-place upgrades (§1, plan §110) → partial: Tauri's NSIS template adds the Start-menu shortcut, uninstaller and in-place upgrades; `src-tauri/windows/hooks.nsh` stops the runtime tree before files are replaced or removed, registers startup with `/STARTUP` and removes it on a real uninstall (upgrades keep it); the runtime adopts the installer's entry on its first start (`Lifecycle::adopt_installer_startup`) · verified: `the_installers_startup_choice_is_kept_on_the_first_start` (2026-09-23) · missing: `scripts/smoke-install.ps1` run against a built installer (first `v*` tag) <!-- synced:~ -->
- [x] **DIST-12** · Model manager (`kivo-store::models`): per-model manifest, resumable HTTP-range downloads, sha256 verification, atomic install into `%LOCALAPPDATA%\KIVO\models`, sources Hugging Face or KIVO's release mirror (§4) → done: `kivo_store::models`: per-model manifests (Moonshine, Kokoro), resumable HTTP-range downloads, sha256 checks, a retry for a damaged file, files in subfolders, atomic install into `%LOCALAPPDATA%\KIVO\models`; sources pinned Hugging Face revisions and GitHub commits · verified: model-store tests (install, resume, damage, cancel, subfolders), the real Moonshine download (2026-09-23) <!-- synced:x -->
- [x] **DIST-13** · Licenses and attribution (CC-BY, OpenRAIL) shown before download; Voice → done: the Voice page lists every model with its licence, size and whether it is on this PC; Download first shows the licence, attribution and source; Remove confirms and says what changes; progress and residency update by push · verified: `Voice.test.tsx` (licence before download, list), accessibility audit of Voice (2026-09-23) <!-- synced:x -->
- [x] **DIST-15** · No telemetry by default; crash dumps stay local; the diagnostics bundle (generated on request and reviewed before sharing) is ARCH-40 (§5) → done: KIVO sends nothing anywhere by default: the only network use is a model download the user chooses and web pages they open; crash dumps and reports stay in the crashes folder · verified: a search of every network client in the tree (only the model downloader), crash tests (2026-09-23) <!-- synced:x -->

#### [RELEASE.md](architecture/RELEASE.md) (2)

- [~] **REL-05** · `release.yml` on `v*` tags, Windows x64 first: sidecars built and copied to `src-tauri/binaries/<name>-<target-triple>`, `tauri-action` → partial: `.github/workflows/release.yml` (tags `v*` only, actions pinned by SHA): sidecars for `x86_64-pc-windows-msvc`, `tauri-action` build, smoke test, `SHA256SUMS.txt`, then `gh release create --draft` · verified: config compiles; the workflow has not run yet (first tag) · missing: a first tagged run <!-- synced:~ -->
- [~] **REL-06** · Install/launch smoke test on a clean Windows runner (silent install, start the runtime, IPC health, uninstall) and checksums before publishing a draft (§2) → partial: `scripts/smoke-install.ps1` installs silently with `/STARTUP`, checks files, shortcut, uninstall entry and startup entry, starts the runtime and checks `kivo-runtime --health` (IPC hello + ping, `kivo_ipc::health`), checks the startup choice was adopted, upgrades in place while it runs, uninstalls and checks nothing is left; `release.yml` runs it before checksums and the draft · verified: `health_reports_a_running_runtime_and_gives_up_on_a_missing_one`; `--health` against a live runtime (exit 0) and none (exit 1) (2026-09-23) · missing: a run on a clean runner (first tag) <!-- synced:~ -->

#### [SECURITY.md](architecture/SECURITY.md) (8)

- [x] **SEC-01** · Permission modes Ask every time and Auto (default) in the engine (§1.1) → done: `PermissionMode` Ask every time and Auto (default), plus Accept edits and Plan first, decided in `kivo_security::authorize` (§1.1 table; DECISIONS "Auto means Medium runs") · verified: `each_mode_asks_for_what_it_promises`, `the_default_policy_table` (2026-09-23) <!-- synced:x -->
- [x] **SEC-04** · Switching mode only through UI or hotkey (Ctrl+Shift+M), from the Island, tray, chat and Home; never through a tool call or voice alone; the Island shows the mode while acting (hidden for Auto) (§1.1) → done: the mode lives in the runtime and kivo.toml; the user switches it from Home, the tray submenu, the command palette and Ctrl+Shift+M (with an Island notice); the Island shows the mode chip while KIVO acts (hidden in Auto) and a click opens Home to switch it (the overlay itself can't switch); no tool or voice path can change it; Bypass needs its confirmation step (SEC-03) · verified: core, RPC, tray and hotkey tests, `shows the permission mode, except in Auto (SEC-04)` (2026-09-23); the Chat page's mode picker comes with Chat (UX-21, M3) <!-- synced:x -->
- [x] **SEC-05** · Hard limits enforced in every mode: emergency stop, disabled capabilities, blocked apps, no typing into password fields, destination binding, per-task cost caps (§1.1) → done: checked first in `authorize`, in every mode including Bypass: the emergency stop, disabled capabilities, blocked apps (by id or name) and destination binding (an address from untrusted content is refused); no M1 tool types into fields or spends money, so the password-field check landed with typing in M4 (`Tool::hard_limit`, checked before anyone is asked; verified by `password_fields_are_refused_in_every_mode` and `password_fields_are_refused_before_asking`) and cost caps with paid work (tasks, cloud brains) · verified: `hard_limits_hold_in_every_mode_including_bypass` (2026-09-23) <!-- synced:x -->
- [x] **SEC-06** · `authorize(ToolCall, Context) -> Decision { Allow | Confirm(ConfirmSpec) | Deny(reason) }` with the §2 `Context`, in `kivo-security` (§2) → done: `kivo_security::authorize(&ToolSpec, &ToolCall, &Context) -> Decision { Allow(Permit) | Confirm(ConfirmSpec) | Deny(Denial) }` with the §2 context (mode, session kind, taint, capabilities, grants, hard limits) · verified: the policy tests, and every call in the engine goes through it (end-to-end tests) (2026-09-23) <!-- synced:x -->
- [x] **SEC-07** · Default policy table (risk × clean/tainted/guest) implemented and unit-tested; High always confirms except in Bypass (§2) → done: the risk × clean/tainted/guest table, with High always confirming except in Bypass and grants never lifting High · verified: `the_default_policy_table`, `grants_lift_medium_asks_but_never_high`, `a_spoken_yes_is_not_enough_for_high_risk` (2026-09-23) <!-- synced:x -->
- [x] **SEC-10** · Confirmation card: exact action in plain words, target, why, provenance; Allow once / Always for… / Deny; never auto-dismisses (§2) → done: the Island's confirmation card shows the exact action, the target when the action doesn't name it, why and who asked; Allow once / Always allow for <target> / Deny; it stays until answered (a waiting turn never collapses) and answers go through `confirmed` · verified: `asks with the action, why and who asked, and never goes away by itself (SEC-10)`, policy confirmation tests (2026-09-23) <!-- synced:x -->
- [x] **SEC-22** · Append-only `audit` table with the §7 fields and `hash = sha256(prev_hash || row)` (§7) → done: the append-only `audit` table (migration 3, triggers refuse UPDATE/DELETE) with the §7 fields and `hash = sha256(prev_hash || row)`; `verify_audit` finds the first broken row · verified: `audit_rows_chain_and_the_chain_verifies`, `the_audit_log_refuses_edits_and_detects_tampering`, the end-to-end test verifies the chain (2026-09-23) <!-- synced:x -->
- [x] **SEC-25** · Emergency stop from the Ctrl+Alt+Shift+Esc hotkey (low-level hook fallback), tray "Stop everything" and the overlay/Control Center Stop button (§8) → done: the emergency stop (`Engine::stop_everything`: the turn, the voice, and tasks once they exist) from Ctrl+Alt+Shift+Esc (with the keyboard-hook fallback when another app owns it), the tray's Stop everything, Home's Stop everything button and the command palette; the Island's Stop cancels the current request · verified: core, tray and hotkey tests, `cancelling_stops_every_layer_within_100_ms` (emergency stop < 1 ms), the RPC stop test (2026-09-23) <!-- synced:x -->

#### [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) (11)

- [x] **TOOL-01** · `ToolSpec` and `ToolResult` exactly as in §1 (namespaced id, JSON Schema params/result, risk, side effects, data egress, timeout, cancellable, capability tier, platforms), plus `undo` handler or `irreversible: true` (§1, UX §8.1) → done: `kivo_core::tool::ToolSpec` (namespaced id, JSON Schema params/result, risk, side effects, data egress, timeout, cancellable, capability tier, platforms, reversibility, capability) and `ToolResult`; every Undoable tool has an undo handler (`Tool::undo`, run through `registry::undo` with its own permit) that restores the recorded previous state, and restart/shutdown are Irreversible · verified: `every_undoable_tool_has_an_undo_and_no_other_does`, `undoable_tools_take_back_what_they_did`, `an_undo_is_a_permitted_call_of_its_own`, `every_tool_is_fully_declared` (2026-09-23); the Undo experience is UX-43 (M4) <!-- synced:x -->
- [x] **TOOL-02** · The executor API requires the `Decision` from `authorize()`; nothing executes without one (type-level) (§1, SECURITY §2) → done: `registry::execute` and `registry::undo` take a `Permit`, which only `kivo_security::authorize`/`confirmed` can create and which must name the exact call; there is no other way to run a tool · verified: `a_permit_runs_only_its_own_call`, `an_undo_is_a_permitted_call_of_its_own` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-03** · Errors carry a user-readable message and a machine code; raw HRESULTs are logged, never spoken (§1) → done: `ToolError { code, message, detail }`: the message is plain words from the text catalog (spoken and shown), the OS detail (HRESULTs) goes to the log only; the engine adds what to do next (plan §146) · verified: `failures_are_worded_for_people`, `platform_error` tests (2026-09-23) <!-- synced:x -->
- [x] **TOOL-06** · Apps: launch, close, focus, restart, list installed (Start-menu `.lnk` + `shell:AppsFolder`, UWP via AUMID, fuzzy match with aliases) (§3) → done: `apps.launch`, `apps.close`, `apps.restart` (close, wait for it to go, open again) and the installed-app index from the Start menu `.lnk`s and `shell:AppsFolder` (UWP by AUMID), fuzzy-matched with aliases; focusing an app's window is `windows.focus` · verified: `the_start_menu_lists_real_apps_without_uninstallers`, `restarting_closes_then_opens_the_app`, index tests, the spoken end-to-end test (2026-09-23) <!-- synced:x -->
- [x] **TOOL-07** · Windows: list, focus (foreground-lock rules), minimize, maximize, close; DWM cloaking check (§3) → done: `WindowsWindows`: list (visible, uncloaked via DWMWA_CLOAKED, front to back), focus (foreground-lock handled with AttachThreadInput), minimize, maximize, restore, close; the runtime is per-monitor DPI aware so positions are physical pixels · verified: `windows_are_listed_front_to_back_with_their_app`, window tool tests (2026-09-23) <!-- synced:x -->
- [x] **TOOL-09** · Audio: volume get/set, mute, mic mute (§3) → done: `audio.volume_set/up/down`, `audio.mute/unmute`, `audio.mic_mute/unmute` through `WindowsControl` (Core Audio endpoint volume), each undoable · verified: `the_speaker_and_microphone_levels_are_readable`, tool tests (tests never change the real volume) (2026-09-23) <!-- synced:x -->
- [x] **TOOL-11** · Media: play/pause/next/previous and now-playing via `GlobalSystemMediaTransportControlsSessionManager` (§3) → done: `media.play_pause/next/previous/now_playing` through `GlobalSystemMediaTransportControlsSessionManager` · verified: tool tests with the fake, live "what's playing" (2026-09-22) <!-- synced:x -->
- [x] **TOOL-12** · System: lock and sleep; restart and shutdown as High risk with confirmation (§3) → done: `system.lock` and `system.sleep` (Low risk), `system.restart` and `system.shutdown` (High risk, Irreversible, always confirmed, a spoken yes isn't enough) · verified: tool tests (power actions are recorded by the fake, never performed), policy tests (2026-09-23) <!-- synced:x -->
- [x] **TOOL-14** · Screen: screenshot of screen, window or region (Windows.Graphics.Capture) (§3) → done: `screen.screenshot` (the monitor in front, or a region) and `screen.screenshot_window` via Windows.Graphics.Capture, one frame on request, saved as PNG to Pictures\Screenshots · verified: `captures_one_frame_of_the_active_monitor`, `a_region_is_cropped_from_its_monitor`, `one_window_is_captured_at_its_size`, `screenshots_of_the_screen_a_window_or_a_region` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-18** · Notifications: show a Windows toast (§3) → done: `notifications.show` through `WindowsNotifications` (Windows toasts, AUMID KIVO.Desktop), gated by the Notifications capability · verified: `toast_xml_escapes_text_and_wires_buttons`, `notifications_respect_their_capability` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-23** · Open URL and web search via the default browser (§5) → done: `browser.open_url` (http/https only) and `browser.search` open the default browser; the address is a destination the hard limits check · verified: `only_web_addresses_are_opened`, tool and grammar tests (2026-09-23) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (18)

- [x] **UX-01** · A manual launch starts the runtime and shows the Control Center; a relaunch shows, unminimizes and focuses the main window (§1) → done: a manual launch starts the runtime and shows the Control Center; a relaunch shows, unminimizes and focuses it; `--page` opens a page · verified: live runs on Windows 11 (2026-09-21, 2026-09-23), single-instance and launch-argument tests <!-- synced:x -->
- [x] **UX-02** · Close (X, Alt+F4, taskbar) hides the window and KIVO keeps running ("On close, keep KIVO running", on); the first close shows a one-time toast with Settings and Quit (§1) → done: X, Alt+F4 and the taskbar hide the window while KIVO runs (the app reports `ui.windowClosed`); with "On close, keep KIVO running" off, closing quits; the first close shows a one-time notification with Settings and Quit KIVO · verified: `the_first_close_notice_shows_once_and_its_buttons_work`, `closing_quits_when_keep_running_is_off`, live (2026-09-23) <!-- synced:x -->
- [x] **UX-03** · Tray: left-click opens the Control Center; right-click menu Open KIVO · Pause/Resume listening · Hide overlay for 1 hour · Stop everything · Settings · Quit KIVO (§1) → done: `apps/kivo-runtime/src/tray.rs`: left-click opens the Control Center; menu Open KIVO · Pause/Resume listening (enabled by state) · Permission mode (submenu, current mode checked) · Hide Island for 1 hour / Show the Island · Stop everything · Settings · Quit KIVO · verified: tray unit tests incl. every choice reaching the core; live on this PC <!-- synced:x -->
- [x] **UX-04** · Tray icon states: normal, listening, paused (slashed mic), error (badge), updating (§1) → done: `kivo-platform-windows/src/tray.rs`: normal, listening (blue ring), paused (greyed + slash), error (red badge), updating (amber badge), driven by the session · verified: `every_state_looks_different`, `listening_rings_and_paused_is_slashed` <!-- synced:x -->
- [x] **UX-56** · Tray tooltip: "KIVO · <state>" plus the permission mode and running-task count (the count comes from UX-24) (mockup → done: tooltip "KIVO · <state>" and "<mode> mode · <n> tasks running" from the text catalog, updated live with the state and permission mode; n is 0 until tasks exist (UX-24) · verified: `the_icon_and_tooltip_follow_the_state` (2026-09-23) <!-- synced:x -->
- [x] **UX-57** · Actionable Windows notifications with buttons routed back to the runtime: first close (Settings / Quit KIVO), microphone blocked (Open Windows settings / Type instead); later sources reuse the same template (§1, §7) → done: `WindowsNotifications` shows actionable toasts (AUMID KIVO.Desktop) whose buttons come back to the runtime (`Lifecycle::answered`): first close (Settings / Quit KIVO), microphone blocked (Open Windows settings / Type instead), a crash last time (Open folder) · verified: lifecycle tests for each notice and button, toast XML tests (tests never pop real toasts) (2026-09-23) <!-- synced:x -->
- [x] **UX-06** · The Island window: top center of the monitor with the foreground window, ~8 px from the top, never takes focus, draws nothing when hidden; hidden windows set WebView2 `IsVisible=false` (§2, §3) → done: the Island window appears top center, 8 px down, on the monitor of the window in front when the request began (the runtime's turn `anchor`), else the one under the pointer; never takes focus unless asked; fits its height to the Island (up to 50% of the monitor); hidden windows set WebView2 IsVisible=false · verified: the typed end-to-end test checks the anchor for a window on a second monitor; live (2026-09-23) <!-- synced:x -->
- [x] **UX-07** · Island states driven by the runtime's `SessionState`: listening (live transcript), thinking, acting (target-app icon, steps), speaking, awaiting confirmation, error, paused (§2) → done: `components/island/turn.tsx` shows listening (live transcript and waveform), thinking, working (target-app icon, steps, mode chip), speaking (answer), awaiting confirmation (the card), error, paused and the finished answer, all from the runtime's state and turn · verified: Island, turn and overlay tests, the end-to-end tests, live (2026-09-23) <!-- synced:x -->
- [x] **UX-09** · Card: ≤ 520 px wide and ≤ 50% of screen height, scrolls, focusable only while typing; header brain/profile chip (routing reason on hover, M3); body with the editable transcript (click to fix and resend), answer and step rows; footer text field, mic toggle, Stop and "Open in Control Center" (§2) → done: the card is at most 520 px wide and 50% of the screen high and scrolls; it takes focus only while typing; a final transcript is a button that opens it in the text field to fix and resend; answer and step rows; the footer has a text field, the mic, Stop and Open in Control Center; the brain chip comes with brains (M3) · verified: `lets the user fix what KIVO heard, type or talk again from the card (UX-09)`, overlay tests (2026-09-23) <!-- synced:x -->
- [x] **UX-10** · End: collapses 4 s after the answer, not while hovered, typing or confirming; confirmations never auto-dismiss (§2) → done: the finished answer collapses 4 s after it (runtime), except while the pointer is over the card (the overlay holds it and keeps its window) or while typing; a confirmation never collapses (its turn waits) · verified: `keeps a finished card while the pointer is over it (UX-10)`, SEC-10 test (2026-09-23) <!-- synced:x -->
- [x] **UX-11** · Fullscreen app / Focus mode: suppressed or tiny pill (setting), sounds only (§2) → done: over a fullscreen app or in Focus (`SystemInfo::attention`) the Island hides or shows a tiny pill (setting `overlay.in-fullscreen`) and KIVO answers with sounds only, never speech · verified: `over_a_fullscreen_app_kivo_stays_quiet` (2026-09-23) <!-- synced:x -->
- [x] **UX-12** · Keyboard: Esc cancels, Ctrl+Enter sends, Tab moves between confirmation buttons (§2) → done: Esc cancels (a hotkey registered only while KIVO is busy); Enter or Ctrl+Enter sends from the text field; with the Island's buttons focused (Ctrl+Shift+Space) Tab and the arrows move between them · verified: overlay keyboard tests, hotkey tests (2026-09-23) <!-- synced:x -->
- [x] **UX-19** · Home: status orb and state ("Listening for Hey Kivo" once the wake word exists, VOICE-13), Talk / Pause listening / mode picker, Recent list (Running joins with tasks, UX-24) (mockup → done: `pages/Home.tsx`: status orb, state title and detail, Talk, Pause/Resume listening, Stop everything while busy, the permission-mode picker, Recent (from Activity, refreshed on pushed events), the speech-model and hotkey notes with the rebind prompt, Start KIVO when disconnected · verified: accessibility audit of Home, live (2026-09-23) <!-- synced:x -->
- [x] **UX-20** · Activity: timeline of turns, tool calls and results from the Activity table (mockup → done: `pages/Activity.tsx`: the timeline of turns (transcript, tool calls with status, replies, stops, crashes) from the `activity` table and the audit view, refreshed on pushed events · verified: `lib/activity` tests, accessibility audit of Activity, live (2026-09-22) <!-- synced:x -->
- [x] **UX-41** · Ctrl+Shift+Space opens the card in text mode with focus in the field; behaves like a spoken request; no speech reply unless enabled (§8) → done: Ctrl+Shift+Space opens the Island's text field with focus (or, when the Island shows buttons, focuses them); the text goes through the same path as speech (`session.say`); no spoken reply unless "Speak typed replies" is on · verified: `a_typed_command_is_treated_like_a_spoken_one`, overlay tests (2026-09-23) <!-- synced:x -->
- [x] **UX-49** · Every UI string goes through i18n (`i18next`, ICU messages), English source locale; no hard-coded strings (§9) → done: the Control Center, the Island and the gallery read every string from `src/i18n/locales/en.json` (i18next + ICU, `withNodes` for sentences with elements); the runtime's own text (tray, notifications, spoken replies, action titles, refusals) comes from `crates/kivo-core/locales/en.json` through `kivo_core::text` · verified: `pnpm test` (presets have translated labels), `cargo test -p kivo-core --test text_keys` (every key the code uses exists), tray and tool tests assert no raw keys, sweep of `.tsx` for literal text (2026-09-22) <!-- synced:x -->
- [x] **UX-50** · CSS logical properties so RTL works by switching `dir`; dates and numbers via `Intl` (§9) → done: every inline direction in the stylesheets and inline styles is logical (`margin-inline-*`, `inset-inline-*`, `text-align: start`); the switch thumb and the sheet mirror under `:dir(rtl)`; only centring and Base UI’s physical popup sides stay physical. Times and dates use `Intl.DateTimeFormat`, numbers, percentages, units and money use ICU number skeletons (`Intl.NumberFormat`) · verified: `dir="rtl"` in the running UI mirrors the sidebar (border and position flip), typecheck/lint/tests (2026-09-22) <!-- synced:x -->
- [x] **UX-52** · ARIA roles, full keyboard navigation and visible focus rings in the overlay and Control Center (§10) → done: Base UI components (ARIA and keyboard) with a focus ring everywhere (white on the Island); the Island is a polite live region, its dots are decorative and its spinner/check are labelled images; Ctrl+Shift+Space puts keyboard focus on the Island’s buttons (arrows/Tab, Enter, Esc) · verified: axe-core audits in Vitest of Home, Activity, Chat, Settings, the component gallery and the Island with a confirmation (`src/a11y.test.tsx`, `src/overlay/Overlay.test.tsx`: no violations; contrast is UX-54), keyboard-mode tests (2026-09-22) <!-- synced:x -->

#### [VOICE.md](architecture/VOICE.md) (14)

- [x] **VOICE-02** · Resample to 16 kHz mono (`rubato`); 10 ms internal frames batched to 80 ms for models (§1) → done: `kivo-audio::RateConverter` (rubato FFT) turns any device format into 16 kHz mono in 10 ms steps; the detection thread works on those frames and sends the speech worker 80 ms batches · verified: resample tests (`a_48k_stream_becomes_16k_in_10ms_steps` …), the spoken end-to-end test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-03** · Capture on an MMCSS "Audio" thread writing a lock-free ring buffer of ≥ 3 s; detection on one worker thread with EcoQoS while idle (§1) → done: WASAPI capture on an MMCSS "Audio" thread writes a lock-free ring (`capture_ring`, 3 s); detection runs on one worker thread, in EcoQoS (efficiency mode) while it waits and at full speed while listening · verified: capture ring tests, `silence_after_speech_ends_the_utterance` checks the EcoQoS switches, 0 ms idle CPU live (2026-09-23) <!-- synced:x -->
- [x] **VOICE-04** · Energy gate → done: `EnergyGate` in front of Silero VAD v6 on `ort` (bundled model) · verified: gate tests, the Silero model test, the spoken end-to-end test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-06** · Traits `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `SttEngine` (partial/stable/final events), `TtsEngine` (streaming) and `TurnDetector`; every engine declares `EngineInfo` (id, kind, license, languages, streaming, accel, resource estimate) (§2) → done: `kivo-voice::traits`: `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `TurnDetector`, `SttEngine`/`SttStream` (partial, stable and final events) and streaming `TtsEngine`; every engine declares `EngineInfo` (id, kind, licence, languages, streaming, accel, resource estimate) · verified: engine and language tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-07** · Cloud engines pass the privacy check before any audio leaves the device (§2, SECURITY §6) → done: `kivo_security::privacy::speech_egress`: a cloud speech engine is used only when the privacy mode (Cloud or Custom) and the Cloud AI capability allow it, checked where the runtime picks engines, so no audio or text reaches one otherwise; a refused cloud voice falls back to the Windows voices · verified: `local_engines_always_pass_and_cloud_ones_follow_the_mode`, `the_privacy_mode_never_blocks_local_speech_engines`, `only_cloud_engines_send_data_off_the_device` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-08** · Default streaming STT engine (the M0 winner among Moonshine v2, Parakeet TDT v3 and Whisper-turbo) running in `kivo-infer`, with partial transcripts shown live (§3) → done: Moonshine Base (MIT) streaming in `kivo-infer` on `ort`, partial transcripts shown live on the Island; it is the provisional default until the deferred M0 comparison runs (DECISIONS "Speech engines on ort", "Benchmarks deferred") · verified: `transcribes_the_sample_recording_when_the_model_is_installed`, the spoken end-to-end test (end of speech → action 40 ms), live (2026-09-23) <!-- synced:x -->
- [x] **VOICE-09** · TTS: system voices (WinRT SpeechSynthesizer / SAPI 5) and Kokoro-82M (EN via misaki, no espeak), streaming from `kivo-infer` (§3) → done: TTS engines in `kivo-infer`: the Windows voices (WinRT, default) and Kokoro-82M (Apache-2.0, quantized ONNX on `ort`) with KIVO's own English phonemizer (misaki's dictionaries and rules ported to Rust, NRL rules for unknown words, no espeak; DECISIONS "Kokoro's phonemizer"); both stream sentence by sentence; Kokoro downloads through the model manager (model, five voices, dictionaries, sha256-checked) when chosen, and the Windows voices speak meanwhile · verified: phonemizer, number and rule tests, `speaks_a_sentence_when_the_model_is_installed`, `replies_can_be_spoken_by_kokoro` through the real worker (2026-09-23) <!-- synced:x -->
- [x] **VOICE-41** · Push-to-talk: hold Ctrl+Space to talk (`Hotkeys` trait: RegisterHotKey with a low-level-hook fallback), optional toggle mode, auto-end on silence; registration conflicts (e.g. IME switching on CJK layouts) detected with a rebind prompt (DECISIONS "Activation", UX §5) → done: hold Ctrl+Space (setting `voice.push-to-talk`) to talk through `WindowsHotkeys` (RegisterHotKey on its own thread; release by a 15 ms key check only while held); when another app owns the keys a low-level keyboard hook catches them first (`Binding::Shared`) and Home says so with a rebind prompt that applies new keys at once; toggle mode; utterances end on silence (VAD) · verified: hotkey tests incl. the hook's key logic and a shared combination, `shared_push_to_talk_keys_work_and_are_reported`, `toggle_mode_starts_on_one_press_and_ends_on_the_next`, `silence_after_speech_ends_the_utterance`, live (2026-09-23) <!-- synced:x -->
- [x] **VOICE-24** · Earcons `listen_start`, `listen_stop`, `error`, `done`, `thinking` (off; after 1 s) and `hangup`, each < 300 ms, pre-decoded into the playback mixer; wake → done: KIVO's own generated cues (DECISIONS "Earcons generated, not Kenney"): listen_start, listen_stop, error, done, thinking (off by default, after 1 s of thinking) and hangup, each under 300 ms, rendered once at start and played through the mixer · verified: `every_cue_is_short_audible_and_starts_and_ends_quietly`, the spoken end-to-end test measures press → listening cue at 6 ms (2026-09-23); a wake word plays the same listening cue as its turn starts (M2, `hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word`) <!-- synced:x -->
- [x] **VOICE-25** · Capture continues during earcons; voice detection is gated for the earcon plus 50 ms while recognition keeps every sample (DECISIONS "Earcon gating"); AEC removes them from the recognized audio with VOICE-30 (§6) → done: the microphone keeps capturing during cues; voice detection ignores it for the cue plus 50 ms (`Speaker::muting_microphone`) while recognition gets every sample · verified: the spoken end-to-end test (the first word survives the listening cue), speaker tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-34** · Residency: capture, VAD, wake and spotter always resident while listening; STT and TTS warm for 10 min after use (setting); STT prewarm on the wake-stage-1 hit and TTS prewarm on turn start (§8) → done: capture and VAD run only while listening; STT and TTS load when a request starts (`Infer::warm`, the prewarm on turn start), stay warm for the setting's minutes (10 by default) after use and then unload, and with nothing loaded the worker exits · verified: `models_load_on_use_and_unload_after_the_warm_time`, live (2026-09-22); a wake word prewarms the models as its turn starts (M2) <!-- synced:x -->
- [x] **VOICE-36** · `LanguagePack { code, stt_engines, tts_voices, kws_model?, grammar, vocabulary, status }` with English first; the router picks the engine for the active language (§9) → done: `LanguagePack { code, name, stt_engines, tts_voices, kws_model, grammar, vocabulary, status }` with English (Alpha) first and Hindi/Punjabi Planned; the runtime picks the STT engine from the active language's pack · verified: `english_ships_first_and_the_rest_are_planned`, `the_router_prefers_the_packs_order_then_anything_that_fits` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-37** · Primary + secondary language settings; automatic language ID only where the engine provides it and it measures reliable (§9) → done: settings `general.language` (primary) and `general.languages` (secondary), validated; no shipped engine offers language ID, so none is used (detected languages would count only when the user speaks them) · verified: config tests, `detected_languages_count_only_when_the_user_speaks_them` (2026-09-23); the language picker is UX-51 <!-- synced:x -->
- [~] **VOICE-40** · Every §10 budget is measured by `kivo-bench` on the reference tiers and met, or the miss is logged in DECISIONS.md (§10) → partial: every §10 budget has its `kivo-bench` suite, and the end-to-end tests measure the M1 path on this PC (press → cue 6 ms, end of speech → action 40 ms, cancel < 35 ms, 0 ms idle CPU, 67 MB idle runtime) · missing: the reference-tier runs, deferred by the owner (DECISIONS "Benchmarks deferred") <!-- synced:~ -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (8)

- [x] **PLAN-01** · Hardware detection through `SystemInfo`: CPU, RAM, GPU, VRAM, NPU where supported, battery state and current load; feeds the voice recommendation (§25, §90) → done: `SystemInfo::snapshot` (CPU name and threads, RAM total and free, GPUs with VRAM, battery, CPU load) and `detect_capabilities` (NPU through DXCore where present) feed `kivo_voice::recommend`, which sets the speech engines' threads · verified: `machines_fall_into_the_reference_tiers`, `battery_and_load_hold_the_models_back`, live hardware advice (2026-09-22) <!-- synced:x -->
- [x] **PLAN-02** · Model residency states Unloaded → done: the worker reports each engine Unloaded → Warming → Warm → Active → Idle → Unloading (STT and TTS); the runtime keeps them per engine, publishes `ModelResidency` events and puts the state on each model in `models.list`; the Voice page shows it · verified: `residency_changes_are_kept_and_published`, `models_load_on_use_and_unload_after_the_warm_time` (2026-09-23) <!-- synced:x -->
- [x] **PLAN-11** · Fast acknowledgement: long operations show immediate visual feedback in the Island, with no spoken filler (§36, §95) → done: the Island changes the moment KIVO moves on (listening with the live transcript, thinking, working with its steps) and the listening cue plays at once; no spoken filler (the optional thinking cue is a sound, off by default) · verified: press → cue 6 ms and the Island states in the end-to-end tests, turn tests (2026-09-23) <!-- synced:x -->
- [x] **PLAN-14** · Startup sequence: runtime → done: `kivo-runtime` starts in the §126 order: single instance, settings, logging, the event bus and state, IPC, audio and voice detection, then OS registrations (tray, hotkeys, autostart, notifications); speech models load only when a request needs them · verified: startup reading of `main.rs` against §126, cold start and idle measured live (2026-09-23) <!-- synced:x -->
- [x] **PLAN-15** · Shutdown sequence: stop new tasks, notify and cancel active work, stop audio and providers, persist state, close the database, exit workers, then the runtime (§127) → done: on quit the UIs are told (`ShuttingDown`), the current turn is cancelled, the voice pipeline and the speech worker stop, the IPC server, tray, hotkeys and app supervisor end, the store closes and the session token is removed (§127) · verified: `an_event_published_just_before_shutdown_is_still_delivered`, live quits from the tray (2026-09-23) <!-- synced:x -->
- [x] **PLAN-16** · The ten resource rules are reviewed at each milestone exit, with any exception logged in DECISIONS.md (§128) → done: the ten rules reviewed at M1 exit, with the polling found fixed and the three exceptions logged (DECISIONS "Resource rules at M1 exit") · verified: the review and the idle measurement (2026-09-23); repeated at each milestone exit <!-- synced:x -->
- [x] **PLAN-18** · Error UX: plain-language cause plus Retry / Open settings actions, never raw codes (§146) → done: failures read as a plain cause plus what to do ("… Try saying the name a different way", "… You may need to allow it in Windows"), with Retry and Open in Control Center on the Island, Open Windows settings / Type instead for a blocked mic; raw codes only in the log · verified: `failures_are_worded_for_people`, turn and lifecycle tests (2026-09-23) <!-- synced:x -->
- [x] **PLAN-19** · Acceptance journey §138 "Kivo, mute": no external AI request, brief confirmation → done: "Kivo, mute" said aloud is transcribed, routed by the grammar (no AI), mutes the speakers and answers "Muted." · verified: `saying_mute_mutes_with_no_ai_and_a_brief_answer`, `a_typed_command_is_treated_like_a_spoken_one` (2026-09-23) <!-- synced:x -->
<!-- /items -->

---

<a id="m2"></a>

## M2: Wake word, enrollment, conversation audio

**Goal:** "Hey Kivo" works hands-free, with echo cancellation good enough to interrupt KIVO while
it speaks, owner-voice recognition, and decisions answered by voice.

**Depends on:** M1 (audio pipeline, Island, permissions).

**Read first:** [VOICE.md](architecture/VOICE.md), [CONVERSATION.md](architecture/CONVERSATION.md), [UX.md](architecture/UX.md), [SECURITY.md](architecture/SECURITY.md), [BENCHMARKS.md](architecture/BENCHMARKS.md).

**Build order:**

1. **Wake word.** The trained "Hey Kivo" model (VOICE-13), two-stage detection (VOICE-14), the wake
   word model and table (VOICE-15), pre-roll (VOICE-05).
2. **Custom wake words.** The add/test/record/tune flow (VOICE-16) and limits/collisions (VOICE-18).
3. **Conversation audio.** Production AEC (VOICE-30), barge-in (VOICE-31), raising the wake
   threshold while speaking (VOICE-32), Smart Turn endpointing (VOICE-33), the command spotter
   (VOICE-19) and the voice emergency stop (SEC-26).
4. **Your voice.** Enrollment (VOICE-20), the speaker profile (VOICE-21), speaker modes (VOICE-22).
5. **Sounds.** Conversational cues (VOICE-26) and the Soft sound set with Settings → Sounds
   (VOICE-27).
6. **Voice decisions.** The question cue and listen window (CONV-26), the local grammar in English
   and Hindi (CONV-27), the voice-approval rules (CONV-28).
7. **Choosing speech engines (VOICE §11).** The engine registry (VOICE-42), curated profiles
   (VOICE-43), the recommendation function (VOICE-44), safe switching (VOICE-45), evaluating
   Supertonic 3 and Moonshine Streaming (VOICE-46), fallbacks with a notice (VOICE-47), language
   compatibility (VOICE-48).
8. **Island and onboarding.** Guest, follow-up and waiting states (UX-08), follow-up listening
   (UX-45), onboarding steps 1–5 (UX-33) with the speech choice steps (UX-60), screen-reader
   announcements (UX-53).

**Deliverable:** from across the room, "Hey Kivo, what's the time… actually, open Spotify" works;
talking over KIVO stops it; "Kivo stop" works mid-answer; a confirmation can be answered "yes" by
the owner's voice, and a guest's "yes" is refused.

**How to verify:** `kivo-bench wake` on the positive and negative corpora; a barge-in test with
laptop speakers; enrollment deletion leaves no files behind; onboarding 1–5 walked through.

**Exit criteria:**

- [ ] **M2-X1** · Wake false accepts ≤ 0.5/h and false rejects ≤ 5% on the corpus.
- [ ] **M2-X2** · Barge-in works with speakers (no headset) on the reference laptop.
- [x] **M2-X3** · Voice approvals are ignored while KIVO's own TTS is playing, and guests cannot approve (tested).

### M2 items

<!-- items:M2 -->
**32 items** · ✅ 31 done · 🟡 0 partial · ⏭ 1 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [CONVERSATION.md](architecture/CONVERSATION.md) (3)

- [x] **CONV-26** · Decisions play the `question` earcon, listen without the wake word for 10 s (extended while the user talks), show a mic ring and voice hints matching the buttons (§7, DESIGN_SYSTEM voice-hint rule) → done: a decision plays `question`, listens 10 s (extended while the user talks), and the Island shows the mic ring and the buttons' words ("allow", "always allow", "deny", "wait") · verified: `decisions_are_answered_by_voice_but_high_risk_needs_a_click`, Island hint tests (2026-09-23) <!-- synced:x -->
- [x] **CONV-27** · Local confirmation grammar (EN + HI): approve, approve with scope, deny, defer ("Waiting for you", no timeout), edit (to the brain), explain ("why?") (§7) → done: `kivo_intent::answers` (EN + HI): approve, approve with scope, deny, defer ("Waiting for you", no timeout), explain ("why?"), and edit, which drops the waiting action and hands the corrected request to a brain · verified: answers tests, `changing_a_decision_by_voice_goes_to_a_brain` (2026-09-23) <!-- synced:x -->
- [x] **CONV-28** · Voice approval rules: Medium needs the owner's voice (or signed-in device with recognition off); guests can't approve; speech heard during KIVO's own TTS is ignored (§7) → done: Medium needs the owner's voice (or recognition off), guests can't approve, High needs a click; speech heard during KIVO's own TTS is ignored for answers · verified: `decisions_are_answered_by_voice_but_high_risk_needs_a_click`, voice-ID tests (2026-09-23) <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (1)

- [x] **SEC-26** · Voice trigger via the command spotter (§8, VOICE-19) → done: the command spotter's stop words cancel a busy turn without the wake word (VOICE-19), and "stop" as a whole request ends the turn · verified: `saying_stop_while_kivo_talks_stops_it` (2026-09-23) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (5)

- [x] **UX-08** · Island states guest, follow-up (ring countdown) and waiting-for-you (§2, §8.1, CONVERSATION §7) → done: `components/island/turn.tsx`: a Guest chip on guest turns, the follow-up ring counting down (CSS), "Waiting for you" after "wait", with the buttons kept · verified: Island M2 state tests (2026-09-23) <!-- synced:x -->
- [x] **UX-60** · Onboarding speech steps (part of UX-33): "Recommended for your PC" with its reason and [Use recommended] / [See other options]; STT profile cards with accuracy, speed, resources, languages, Local/Cloud and [Try sample]; TTS profile cards with [▶ Preview] of one sentence per voice; a confirm step showing Understanding / Speaking / AI / resource profile (VOICE §11) → done: onboarding's speech step: "Recommended for your PC" with its reason and Use recommended, other options on request; STT cards with accuracy, speed, size, languages, privacy and Try sample; voices with Preview; the Your setup step (understanding, speaking, AI, resources) · verified: `Onboarding.test.tsx`, Voice page tests (2026-09-23) <!-- synced:x -->
- [x] **UX-33** · The first launch opens the Control Center on onboarding (moved from UX-01). Steps 1–5: Welcome (black, Island demo), Microphone check, How you call KIVO, Hearing & speaking (engine + voice, background download), Your voice (optional enrollment with consent); steps slide 18 px in the direction of travel and the progress dots stretch (DESIGN_SYSTEM §5) (§4) → done: `pages/Onboarding.tsx` opens while `general.onboarded` is false: Welcome (black, the Island), microphone check, how you call KIVO (keys, "Hey Kivo"), hearing and speaking (background downloads), your setup, your voice (optional, consent first); steps slide 18 px, the phase dots stretch; Finish or Skip marks setup done · verified: `Onboarding.test.tsx` (the walk, skip, consent, accessibility audit) (2026-09-23) <!-- synced:x -->
- [x] **UX-45** · Follow-up without the wake word for N s (Off / 5 / 8 default / 15), ring countdown, VAD-gated (§8.1) → done: Off / 5 / 8 / 15 s on the Voice page; the Island's ring counts down; VAD-gated, and the spotter still runs · verified: `hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word`, Island ring test (2026-09-23) <!-- synced:x -->
- [x] **UX-53** · State changes and final transcripts announced through UIA notifications (§10) → done: the overlay raises UI Automation notifications (`announce` in the app: `UiaRaiseNotificationEvent`) for state changes, the final transcript, answers and errors (`components/island/announce.ts`) · verified: announcement tests, `cargo check -p kivo-app` (2026-09-23) <!-- synced:x -->

#### [VOICE.md](architecture/VOICE.md) (23)

- [x] **VOICE-05** · Pre-roll: STT starts from the buffer 300 ms before the wake word ends, and the wake phrase is stripped by alignment ("Hey Kivo, open Chrome" in one breath works) (§1) → done: hands-free requests start from the buffer 150 ms before the wake phrase begins (more than the 300 ms before it ends), and `kivo_intent::strip_wake_phrase` removes the phrase by words, sound skeleton, a clipped tail or joined-sound alignment · verified: `hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word` ("Hey Kivo, mute." in one breath), wake-strip tests (2026-09-23) <!-- synced:x -->
- [-] **VOICE-13** · Built-in "Hey Kivo" model trained with the openWakeWord pipeline (KIVO-owned); it can be disabled but not deleted (§3, §4) → dropped: KIVO trains no models (owner); "Hey Kivo" runs on the open-vocabulary keyword spotter with its pronunciation variants, built in (it can be turned off, not deleted), see DECISIONS.md "No training (owner)", 2026-09-23 <!-- synced:- -->
- [x] **VOICE-14** · Two-stage detection: the keyword spotter per word (its pronunciation variants and per-word thresholds), then the speaker check on the whole request (§1, §3; no trained verifier, DECISIONS "No training") → done: stage 1 is `kivo_voice::kws` (KIVO's `ort` port of sherpa-onnx's streaming Zipformer keyword decoder) with each word's variants and threshold; stage 2 is the CAM++ speaker check on the whole request (`voiceid.rs`) · verified: keyword spotter tests against sherpa's detections, `hey_kivo_wakes_kivo…`, voice-ID tests with a fake verifier (2026-09-23) <!-- synced:x -->
- [x] **VOICE-15** · `WakeWord` data model as in §4, stored in the `wake_words` table (§4) → done: `kivo_store::wake::WakeWord` in the `wake_words` table (migration 4), "Hey Kivo" built in · verified: store wake tests (the built-in word can't be deleted) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-16** · Custom wake words via sherpa-onnx KWS: validate (Good / Fair / Risky) → done: `wake.check` (Good/Fair/Risky with reasons, `kivo_voice::wakeword::assess`), `wake.hear` (KIVO says it), `wake.try`, `wake.sample` (encrypted samples), `wake.tune`, `wake.falseAlarms` (minutes of Windows-voice speech through the spotter); the Voice page's Add and Edit dialogs (`components/voice/WakeWords.tsx`) · verified: wakeword tests, `tuned_sensitivity` tests, the Voice page test (turning a word on downloads the listener first) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-18** · Up to 5 enabled wake words; collisions are checked against the other words and the fast-path vocabulary (§4) → done: at most 5 enabled (`MAX_ENABLED`, store and UI); new words are checked against the other words and the fast-path command phrases · verified: wakeword collision tests, store limit test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-19** · Command spotter (sherpa KWS) runs while KIVO speaks or acts, on the AEC-cleaned signal; "Kivo stop", "stop" and "cancel" cancel the turn without the wake word (§4) → done: the stop words ("Kivo stop", "stop", "cancel") run in the spotter only while KIVO is busy, on the echo-cancelled signal; a request that is only a stop word (`kivo_intent::is_stop_request`, EN + HI) ends the turn quietly, since talking over KIVO usually starts a barge-in first · verified: `saying_stop_while_kivo_talks_stops_it` (KIVO silent ~2 s into a 12 s reply), stop-request tests, `stop_words_are_stricter_than_the_default_wake_word` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-20** · Enrollment: consent, then 8 prompts; clips DPAPI-encrypted in `%LOCALAPPDATA%\KIVO\data\voice\`; one-click deletion (§5, SECURITY §5) → done: consent, then 8 prompts recorded through the listener; clips DPAPI-encrypted in the voice folder; Delete removes clips and profile (`voiceid.rs`, `voiceId.*`, `components/voice/Enrollment.tsx` in onboarding and on the Voice page) · verified: voice-ID tests, onboarding test (no recording before consent) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-21** · `SpeakerProfile` (≤ 40 embeddings, CAM++) that grows from high-confidence turns and is rebuilt from clips when the model changes (§5) → done: up to 40 CAM++ embeddings (the joined enrollment plus clips of 2 s or more), grown from confident owner matches, rebuilt from the clips when the model changes · verified: `confident_long_matches_grow_the_profile_up_to_forty`, rebuild test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-22** · Speaker modes Off / Prefer owner (default after enrollment; unknown voices get a guest session) / Owner only (§5) → done: Off / Prefer owner (set when enrollment finishes; others get a guest turn with the Island's Guest chip) / Owner only (other voices dropped); the choice on the Voice page · verified: voice-ID tests, engine guest tests, Island guest test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-26** · Conversational cues `question`, `approved` and `cancelled` (§6, CONVERSATION §7) → done: `question`, `approved` and `cancelled` cues in every set, played by the decision flow · verified: `every_cue_is_short_audible_and_starts_and_ends_quietly`, `decisions_are_answered_by_voice_but_high_risk_needs_a_click` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-27** · Sound set "Soft" (default) covering every cue plus a notification sound; Settings → done: Soft is the default set, with a notification cue; Settings → Sounds (`pages/Settings.tsx`): master switch, set picker with Preview, volume relative to Windows, each cue on or off with its own preview · verified: `Settings.test.tsx`, sound-set tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-30** · AEC in production: WebRTC AEC3 (`sonora`) with KIVO's output mix as the reference, stepping aside when there is no echo path (headphones); the OS AEC is not used (§1, §3; DECISIONS "Echo cancellation") → done: AEC3 (`sonora`) on the output mix while KIVO's voice plays (+500 ms); `EchoPath` compares played and heard loudness and steps the canceller aside with headphones (it erased 99% of the user's first half-second there); reset when the output device changes; OS AEC not used (DECISIONS "Echo cancellation") · verified: echo tests (38.5 dB echo reduction; headphones keep 100% of the first half-second; a speaker's echo is found and cancelled), `talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request` (headphones), `talking_over_kivo_works_with_speakers_echoing_its_voice` (a simulated room) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-31** · Barge-in: stricter VAD during TTS; duck TTS −12 dB within 50 ms; after ≥ 300 ms of speech or one STT word, cancel the turn (30 ms fade, brain and tools cancelled) and start listening from the buffer; otherwise restore the volume (§7) → done: VAD at 0.75 while KIVO speaks, once the canceller has settled (removing ≥ 10 dB, or no echo path) so KIVO's own voice never interrupts it; the voice ducks −12 dB at the first strong frame; 300 ms of speech cancels the turn, stops the voice with a fade and starts listening from the buffer; otherwise the volume comes back · verified: `talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request` (headphones: barge-in ~1 s after KIVO starts, the whole request heard and acted on), `talking_over_kivo_works_with_speakers_echoing_its_voice` (a room echoing KIVO at −10 dB, 40 ms: no self-interruption, the request acted on) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-32** · The wake threshold rises while KIVO plays audio (§7) → done: while KIVO plays audio a wake-word hit needs a score of 0.6 (`PLAYING_MIN_SCORE`) · verified: keyword tests, the hands-free end-to-end tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-33** · Endpointing: Silero pause 250 ms + Smart Turn v3 (§3) → done: Silero pause 250 ms, then Smart Turn v3.2 decides (re-checked at 700 ms, end forced at 1.6 s) · verified: Smart Turn feature tests against the reference, `silence_after_speech_ends_the_utterance`, the spoken end-to-end tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-42** · Speech engine registry: every engine's profile tags, languages, streaming, local/cloud/hybrid, devices, size, licence, voices and KIVO benchmark results; UI and onboarding read only the registry (§11) → done: `kivo_voice::registry` (profiles, privacy, commercial use, languages, devices, size, voices, KIVO's measurements from the `stt`/`tts` bench runs via `Database::latest_benchmark`); `voice.engines` serves it and onboarding and the Voice page read only it · verified: `every_engine_kivo_ships_is_in_the_registry`, `measurements_come_only_from_kivos_benchmarks`, `benchmark_results_are_stored_as_valid_json` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-43** · Curated profiles: 2–4 STT (Recommended, Lightweight, High accuracy, Multilingual) and 2–4 TTS (Recommended/Natural, Lightweight, Multilingual, Expressive), each mapped to a registry engine; unmeasured values read "Not benchmarked by KIVO" (§11) → done: STT Recommended (Moonshine Base) / Lightweight (Tiny) / High accuracy ("Not available yet") / Multilingual (the language's Moonshine); TTS Natural (Kokoro) / Lightweight (Windows voices) / Multilingual (Supertonic 3) / Expressive ("Not available yet"); unmeasured engines read "Not benchmarked by KIVO" · verified: `profiles_map_to_engines_and_say_when_there_is_none`, Voice page test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-44** · Recommendation function: hardware + OS + languages + privacy/offline + priority + installed models + benchmarks → done: `kivo_voice::recommend::recommend` weighs the tier, battery and load, language, privacy mode, priority, installed engines and measurements (slower-than-real-time engines passed over); recommended and fallback STT/TTS with a reason; CPU engines, keeping the GPU free · verified: recommend tests, onboarding test (2026-09-23) <!-- synced:x -->
- [x] **VOICE-45** · Safe engine switching: licence → done: `switch.rs`: check (language, privacy) → download (licence shown first) → load in a second worker → test (a Windows voice says a sentence the recognizer must hear, or the voice must speak audibly) → save; the old engine works until then; the listening model can't be removed without a replacement unless confirmed · verified: `a_new_speech_engine_is_tested_before_it_is_used`, switch tests, Voice page test (licence before download) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-46** · Evaluate Supertonic 3 (licence, languages, size, CPU latency, streaming) and Moonshine Streaming sizes against KIVO's budgets; add the ones that pass to the registry and log the result in DECISIONS (§11) → done: Supertonic 3 added (RTF 0.25, 0.9 s load, EN and HI), Moonshine Tiny and per-language models added, Moonshine Streaming not added (safetensors only); logged in DECISIONS "Speech engines evaluated (VOICE-46)" · verified: `speaks_several_languages_when_the_model_is_here`, `hears_speech_in_its_own_language_when_the_models_are_here` (Base/Tiny EN, Base ES, Tiny JA exact) (2026-09-23) <!-- synced:x -->
- [x] **VOICE-47** · Fallback policy: a failed primary STT/TTS switches to the configured fallback for the session with a visible notice; saved settings are unchanged (§11) → done: an engine that fails to load is swapped for another installed recognizer or the Windows voices for the session (`infer.rs` `load_engines`), with a notice (`SpeechFallback` event → toast; Home says when nothing could hear); settings unchanged · verified: `a_voice_that_fails_to_load_falls_back_to_the_windows_voices_with_a_notice` (2026-09-23) <!-- synced:x -->
- [x] **VOICE-48** · Language compatibility: selected languages are checked against each engine; incompatible choices are explained with compatible alternatives, never switched silently (§11, §9) → done: `registry::language_problem` names the engines that do fit; a switch to an engine without the language is refused with them, and cards say "Not available for <language> yet" · verified: `incompatible_languages_are_explained_with_alternatives`, `engines_are_checked_against_the_language_and_the_slot`, the switch end-to-end test (2026-09-23) <!-- synced:x -->
<!-- /items -->

---

<a id="m3"></a>

## M3: Brains

**Goal:** anything that isn't a fast command goes to an AI: all four cloud providers, local
servers and the three CLI agents, with sign-in that needs no API key, routing with a reason, cost
tracking and streamed spoken answers.

**Depends on:** M1 (turn flow, TTS), M2 (conversation audio).

**Read first:** [BRAINS.md](architecture/BRAINS.md), [CONVERSATION.md](architecture/CONVERSATION.md), [MEMORY.md](architecture/MEMORY.md), [DISCOVERY.md](architecture/DISCOVERY.md), [SECURITY.md](architecture/SECURITY.md), [UX.md](architecture/UX.md), [VOICE.md](architecture/VOICE.md), plan §72, §94, §145.

**Build order:**

1. **Contract.** `BrainProvider` and events (BRAIN-08), hidden reasoning (BRAIN-09), the CI check
   that `kivo-core` stays provider-free (ARCH-38).
2. **Secrets.** Credential Manager handles (SEC-17), secrets never leaving the runtime (SEC-19).
3. **Adapters.** Anthropic, OpenAI, Gemini, OpenRouter (BRAIN-10); OpenAI-compatible local
   (BRAIN-12); ACP client and `AgentSession` (BRAIN-13); Claude Code, Gemini CLI and Codex over ACP
   (BRAIN-14); agent permission requests into KIVO's engine (BRAIN-15); no-key sign-in and
   write-only keys (BRAIN-17, BRAIN-18).
4. **Discovery.** Detector framework, cache and rules (DISC-01, DISC-02, DISC-03); CLI and local
   server detectors (DISC-04, DISC-05); Refresh (DISC-12); health and re-detection timing (DISC-14,
   DISC-15).
5. **Routing.** Profiles (BRAIN-20), deterministic routing with a reason (BRAIN-21), privacy-safe
   failover (BRAIN-22), health checks (BRAIN-23), network resilience (PLAN-05), prewarming
   (PLAN-10), explainability (PLAN-17).
6. **Context.** Always-on block with deltas (BRAIN-24, MEM-11), provenance (BRAIN-26), tool
   exposure (BRAIN-27), context layers (CONV-30), budgets (CONV-04), assembly order (CONV-05),
   compaction (CONV-06), prompt caching (CONV-07).
7. **Conversations.** Sessions and threads (CONV-01), agent session ids (CONV-02), conversation
   tables (MEM-01), preferences (MEM-03), "Tell Claude …" follow-ups (CONV-12).
8. **Speaking answers.** LLM → phrase chunker → TTS (BRAIN-28), voice style and text normalizer
   (BRAIN-29), personas (BRAIN-38), vocabulary for transcript repair (VOICE-23).
9. **Smarter routing.** Semantic stage (BRAIN-03), hybrid requests (BRAIN-04), misroute report
   (BRAIN-06).
10. **Money.** Usage metering (BRAIN-34), cost estimates (BRAIN-35), optional limits (BRAIN-36).
11. **Screens.** Chat (UX-21), Brains with Context tab (UX-22), Voice page (UX-23) with its speech
   sections, voice cards, advanced mode and per-engine benchmark (UX-61, UX-62, VOICE-49, BENCH-15),
   onboarding step
    6 (UX-34), free options labelled (CONV-08).
12. **Measure.** The `brain` benchmark suite (BENCH-09).

**Deliverable:** "Hey Kivo, explain this error" is answered by the default brain, spoken as it
streams; "use Claude for this" switches; Claude Code in a project asks for a permission and the
Island shows it; the card shows "Coding · Claude — because this looked like a coding task" and a
cost estimate; Gemini CLI signed in with a free Google account works without any key.

**How to verify:** adapter contract tests against recorded responses; a kill-the-provider test
(the runtime survives); `kivo-bench brain` and `e2e` for first audio; discovery finds a locally
installed CLI and Ollama; secrets absent from logs and IPC (tested).

**Exit criteria:**

- [ ] **M3-X1** · p50 end of speech → first audio ≤ 1.2 s on a cloud brain.
- [x] **M3-X2** · Cancel works mid-stream for API brains and ACP agents.
- [x] **M3-X3** · Killing a provider (including a CLI agent process) does not crash the runtime.
- [x] **M3-X4** · A brain can be connected with no API key (CLI sign-in, OpenRouter OAuth or local).

### M3 items

<!-- items:M3 -->
**61 items** · ✅ 57 done · 🟡 4 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (1)

- [x] **ARCH-38** · A CI check fails if `kivo-core` gains a provider dependency (§8) → done: `crates/kivo-core/tests/no_provider_dependencies.rs` reads kivo-core's manifest and fails on any dependency outside its allowed list or any target-specific one; CI's `cargo test --workspace` runs it · verified: the test passes in the workspace run (2026-09-23) <!-- synced:x -->

#### [BENCHMARKS.md](architecture/BENCHMARKS.md) (2)

- [~] **BENCH-09** · `brain` suite: TTFT, tokens/s, tool-call latency, structured-output validity, cancellation latency, error rate (§1) → partial: the brain path records its spans (t6Intent, t7FirstToken, t9FirstAudio, t10Complete) in the turn metrics · missing: the `brain` suite itself, deferred by the owner with the other benchmark runs (DECISIONS "Benchmarks deferred") <!-- synced:~ -->
- [~] **BENCH-15** · "Benchmark this engine" in the Control Center: STT first-partial and final latency, RTF, WER, CPU/RAM/VRAM, noise; TTS first audio, RTF, CPU/RAM/VRAM, long text, interruption; stored locally and shown against KIVO's thresholds (VOICE §11) → partial: KIVO's own measurements already steer the choice (the enrollment WER per recogniser, VOICE-23) · missing: the per-engine "Benchmark this engine" run, deferred by the owner with the other benchmark runs <!-- synced:~ -->

#### [BRAINS.md](architecture/BRAINS.md) (25)

- [x] **BRAIN-03** · Semantic stage: a local embedding model + kNN over command exemplars, accepted only above a high threshold and with resolvable slots (§2) → done: `kivo-intent/src/semantic.rs` (kNN over `grammar/en/exemplars.toml`, threshold 0.72 with margin and consensus, slots resolved by the grammar) backed by all-MiniLM-L6-v2 run by `kivo-voice/src/embed.rs` (own WordPiece, `ort`), pinned in the model catalog and loaded when the user has it (DECISIONS "Semantic stage") · verified: tokenizer matches BERT's reference ids; real-model test: 8 unseen paraphrases resolve, 8 other requests (incl. two-action and ambiguous ones) don't; router tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-04** · Hybrid requests go to the brain, with the fast-path tools exposed as tools (§2) → done: requests joining two actions skip the semantic stage and go to the brain, which gets the fast-path tools as tools (≤ 20, BRAIN-27) · verified: `the_brains_tool_calls_go_through_the_permission_engine` (mute + launch + close through tools), semantic tests reject "open chrome and search for cats" (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-06** · A "That's not what I meant" misroute report in the card (§2) → done: "That's not what I meant" on the Island's answer and on each Chat reply records the turn, its route and note in `misroutes` and the router's metrics (`chat.misroute`) · verified: `the_control_centers_brain_requests`, `Chat.test.tsx`, Island tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-08** · `BrainProvider` trait (`info`, `health`, `models`, `chat` with cancellation), `BrainEvent` stream and `NormalizedError`, exactly as in §3 → done: `kivo-brain/src/provider.rs` + `types.rs`: `BrainProvider` (`info`, `health`, `models`, `chat` with a `CancellationToken`), `BrainEvent` stream, `NormalizedError` · verified: 10 adapter contract tests (text, tool calls, usage, errors, cancel < 100 ms) (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-09** · Reasoning content is never shown or spoken; it is kept in the turn log only if the user enables that (§3) → done: reasoning deltas never reach the card or the voice; with `brains.keep-reasoning` on they are kept in the turn record's `reasoning` column only · verified: adapter tests (reasoning separate from text), engine code path (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-10** · API adapters for Anthropic, OpenAI, Gemini and OpenRouter (native adapters where prompt caching, realtime or reasoning controls need them; `genai` for breadth) (§4) → done: native Anthropic (prompt caching) and Gemini (schema trimming) adapters, and one OpenAI-compatible adapter for OpenAI and OpenRouter (plus Groq, Mistral, DeepSeek, xAI, custom) on a shared streaming `reqwest` layer (DECISIONS "Own adapters on `reqwest`") · verified: `contract_tests.rs` against recorded SSE for each (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-12** · Local adapter: OpenAI-compatible (Ollama, LM Studio, llama.cpp server, any localhost URL) (§4) → done: the OpenAI-compatible adapter for local servers (Ollama, LM Studio, llama.cpp, any localhost URL), no key needed, free and private · verified: `local_servers_need_no_key_and_health_follows_the_model_list`, contract tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-13** · `AgentSession` trait (prompt, cancel, streamed `AgentEvent`, `set_mode`) and an ACP client (JSON-RPC over stdio) (§3, §4) → done: `kivo-brain/src/acp.rs`: ACP client (newline JSON-RPC over stdio), `AcpSession` (prompt with streamed `AgentEvent`, cancel via `session/cancel`, `set_mode`, resume with `session/load`) · verified: 8 ACP tests incl. a dying agent process (2026-09-23) <!-- synced:x -->
- [~] **BRAIN-14** · CLI agents over ACP: Claude Code (`claude-agent-acp`), Gemini CLI (`--acp`) and Codex (`codex-acp`) (§4) → partial: KIVO runs CLI agents over ACP with the catalog's entry points (Claude Code `claude-agent-acp`, Gemini CLI `--acp`, Codex `codex-acp`, OpenCode `acp`); live handshakes with the installed Gemini CLI and OpenCode pass (`KIVO_TEST_REAL_AGENTS=1`), and the whole agent turn is tested with a scripted ACP agent · missing: a live run with Claude Code and Codex, whose ACP adapters aren't installed on this PC (owner: `npm install -g @zed-industries/claude-agent-acp @zed-industries/codex-acp`) <!-- synced:~ -->
- [x] **BRAIN-15** · ACP `session/request_permission` is routed into KIVO's permission engine and confirmation UI; the agent's file and terminal operations show as activity (§4) → done: an agent's `session/request_permission` becomes a KIVO action (`agent.read` … `agent.execute`, DECISIONS "Agents' requests") decided by `authorize()`, shown on the card and answerable by click or voice; its plan, tool work and file changes show as steps · verified: `claude_code_asks_for_permission_on_the_card_and_follow_ups_reuse_its_session` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-17** · No-key sign-in first: CLI login flows launched from KIVO, OpenRouter OAuth PKCE ("Connect with OpenRouter"), local auto-detect; direct API keys only under "Advanced: use your own API key" (§4) → done: Brains and onboarding lead with no-key options: the CLI's own login in a terminal (`brains.signIn`), "Connect with OpenRouter" (OAuth PKCE, key straight into Credential Manager), local servers found on this PC; API keys under "Use your own API key" · verified: `openrouter_oauth_ends_with_a_stored_key_and_no_copying`, `a_cli_sign_in_opens_the_clis_own_login`, Brains and onboarding tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-18** · API keys are write-only from the UI, stored in Credential Manager and tested by the runtime (§4, SECURITY §5) → done: keys go UI → runtime once (`brains.setKey`), are tested against the provider first, stored in Credential Manager as `secret://kivo/<provider>/api-key`, and never returned (views say only `hasKey`) · verified: `the_control_centers_brain_requests` (refused key not stored, key absent from list and config), `testing_a_key_does_not_save_it`, `Brains.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-20** · `Profile` model and the built-in profiles Default, Fast, Smart, Coding, Private, Offline and Cheap; users can create more (§5) → done: `Profile` with the seven built-ins; users can change a built-in (reset restores it) or add their own (Brains → Profiles, `brains.saveProfile`) · verified: `user_profiles_override_built_ins_and_add_their_own`, routing tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-21** · Deterministic routing in the §5 rule order; each decision records a one-line reason that the card shows (§5) → done: `routing.rs` in the §5 order with a one-line reason ("Coding · Claude Code — because this looked like a coding task") in the card chip, Activity and the turn record · verified: 12 routing tests, `an_open_question_is_answered_by_the_default_brain_with_a_reason` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-22** · Failover on RateLimited / ProviderDown / Network only to a fallback in the same privacy class; otherwise ask the user (§5) → done: on RateLimited/ProviderDown/Network with nothing said yet, the next fallback of the same privacy class answers and the chip says so; otherwise a plain message · verified: `a_busy_brain_fails_over_within_its_privacy_class` (local brain untouched), routing failover tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-23** · Health checks per provider, refreshed as in DISCOVERY §3 (§3) → done: health per provider from its model list, cached, refreshed as DISC-14 describes, re-checked after errors · verified: `brains::tests` (8), `a_failing_provider_is_reported_plainly_and_kivo_carries_on` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-24** · Always-on context block ≤ 300 tokens, updated with deltas (§6, MEMORY §4) → done: the live block (time, language, permission mode, active app, window title as untrusted) within 300 tokens in every request, with "since the last message" deltas; agents get the block once, then deltas (MEM-11) · verified: `an_open_question…` checks the live context in the request; context tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-26** · Every context item carries `Provenance { source, trust }` (§6) → done: `ContextItem { text, provenance: { source, trust } }`; untrusted items are fenced `<untrusted>` (window titles, attachments) · verified: context tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-27** · Tool exposure by task class and capability tags, at most 20 tools per request (§6) → done: `select_tools` by task class, namespace hints and profile scope, at most 20, wire names `apps__launch` · verified: context tests, requests in the e2e tests carry ≤ 20 tools (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-28** · Streaming LLM → phrase chunker → streaming TTS, so speech starts before the full answer (§1, §11) → done: streamed deltas → `speech::Chunker` (first phrase at a clause break) → phrases queued to the speech worker while the answer still streams · verified: `a_spoken_answer_starts_before_the_brain_finishes`, chunker tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-29** · Voice style: short, speakable answers; the card shows rich text while TTS gets a speakable version; code blocks are replaced with "I've put the details on screen"; a text normalizer for numbers, URLs and units (§11) → done: voice turns ask for short speakable answers; the card shows the rich text; speech drops markdown, says "I've put the details on screen" for code, and normalizes numbers, money, units and addresses · verified: speech tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-34** · Usage metering into a `usage` table for every brain, STT, TTS, realtime and computer-use call, with turn, task and routine ids (§9) → done: every brain and agent request writes a `usage` row with turn/task/routine ids (`Brains::meter`), pushed as `usageRecorded` · verified: `usage_is_metered_…`, e2e usage checks · note: STT/TTS/realtime/computer-use metering joins with cloud speech (M8), realtime (M8) and computer use (M7), which are local-only or not built yet <!-- synced:x -->
- [x] **BRAIN-35** · Cost = usage × a bundled price table (LiteLLM-derived, weekly refresh that can be turned off, per-model overrides, local = $0), always labelled as an estimate (§9) → done: cost = usage × the bundled LiteLLM-derived table (703 models, MIT), weekly refresh (can be turned off), per-model overrides, local = $0, labelled estimates; CSV export · verified: cost tests, `usage_is_metered_with_a_cost_estimate_and_limits_add_up`, `the_control_centers_brain_requests` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-36** · Limits with scope, period, amount, warning thresholds, at-limit action and per-task caps; the default is "track only" (§9) → done: limits by scope, period (local reset day), amount, warnings and at-limit action (ask on the card, cheaper profile, local only, block) plus task caps; default track only · verified: `a_spending_limit_can_block_cloud_requests`, limit tests (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-38** · Personas Calm (default), Friendly, Witty and Custom; guardrails keep confirmations, errors and status reports neutral; set per user profile and overridable per brain profile (§10) → done: Calm (default), Friendly, Witty and Custom with guardrails after the style; chosen on the Voice page, overridable per profile · verified: persona tests, Voice page test (2026-09-23) <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (9)

- [x] **CONV-01** · Voice sessions end after 2 min of silence (setting); threads join voice sessions within 30 min on the same topic; "Kivo, new topic" starts a new thread; typed chats choose a thread (§1) → done: voice sessions end after `brains.voice-session-minutes` (2) of silence; a new session joins a thread from the last 30 min on the same topic; "Kivo, new topic" starts one; Chat picks its thread · verified: `follow_ups_continue_the_thread_and_new_topic_starts_another` (2026-09-23) <!-- synced:x -->
- [x] **CONV-02** · Agent sessions: the CLI agent's session id is stored so KIVO can resume it (§1) → done: the agent's session id is stored per agent and workspace and resumed with `session/load` when the agent restarts · verified: the Claude Code e2e test checks the stored id; `a_stored_session_is_resumed_when_the_agent_can` (2026-09-23) <!-- synced:x -->
- [x] **CONV-04** · Per-request budget by model class (small local, large local, cloud chat 32k / voice 12k, CLI handoff, realtime), user-adjustable per profile (§2) → done: `budget()` per model class (small/large local, cloud 32k chat / 12k voice, agent handoff), overridable per profile (max context) · verified: budget tests (2026-09-23) <!-- synced:x -->
- [x] **CONV-05** · Assembly order 1–8 with guaranteed space for the first items and at least the last 4 turns verbatim (§2) → done: `assemble()` in the 1–8 order with guaranteed first layers and at least the last four turns verbatim · verified: context tests, compaction e2e (older turn dropped, last four kept) (2026-09-23) <!-- synced:x -->
- [~] **CONV-06** · Compaction into a stored running summary by the cheapest suitable model; full history kept in SQLite and recallable by semantic search (§2) → partial: older turns are summarized into the thread's stored running summary by the cheapest suitable brain, automatically and by "Compact now"; the full history stays in SQLite, searchable (FTS5) and recalled into requests · missing: recall by semantic (embedding) search; recall uses full-text search today <!-- synced:~ -->
- [x] **CONV-07** · Prompt caching: the stable prefix (layers 1, 2, 3, 6) marked cacheable for Anthropic, OpenAI and Gemini (§2, §8) → done: the stable prefix (system, instructions, workspace, skills) is marked cacheable; Anthropic gets `cache_control`, OpenAI and Gemini cache the leading blocks themselves · verified: contract tests, e2e request check (2026-09-23) <!-- synced:x -->
- [x] **CONV-08** · Free and low-cost options labelled "Free" in onboarding and Brains (local models, Gemini CLI, Codex with ChatGPT, OpenRouter free models) (§3) → done: free options are labelled in the catalog and shown as "Free" in Brains and onboarding (local models, Gemini CLI, Codex with ChatGPT, OpenRouter free models) · verified: Brains and onboarding tests (2026-09-23) <!-- synced:x -->
- [x] **CONV-12** · KIVO-run ACP sessions: "Tell Claude …" follow-ups into the same session, progress in the Island, Tasks and Chat (§5.1) → done: "Tell Claude Code …" goes into the same live ACP session; progress shows in the Island and Chat · verified: the Claude Code e2e test (second prompt in the same session) (2026-09-23) · note: the Tasks view joins with Tasks (M5) <!-- synced:x -->
- [x] **CONV-30** · Context layers 1–8 with the default sizes, lazy-loading tools and skills (~1.5k tokens at start), CLI handoff = request + ≤ 300 tokens of memory (§8) → done: layers 1–8 with the default sizes; tools are chosen per request, the agent handoff is the request plus ≤ 300 tokens of saved context; Brains → Context shows each layer · verified: context tests, `Brains.test.tsx` · note: skills load with M6 (2026-09-23) <!-- synced:x -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (8)

- [x] **DISC-01** · `Detector { id, scope, run() → Vec<Found> }` trait; discovery runs in the runtime on a low-priority EcoQoS task, never in the UI (§3) → done: `discovery.rs`: `Detector { id, scope, run() }`, run by the runtime in the background, never the UI · verified: discovery tests (2026-09-23) <!-- synced:x -->
- [x] **DISC-02** · Results cached in a `discovery` table with `checked_at`, shown instantly and then updated (§3) → done: results in the `discovery` table with `checked_at`; the page shows the cache at once, then updates · verified: discovery tests (2026-09-23) <!-- synced:x -->
- [x] **DISC-03** · Detection suggests, the user enables: nothing is switched on automatically, nothing is sent anywhere, credential contents are never read (§1, §3) → done: found items only suggest (Use / Sign in); nothing is connected or sent; sign-in files are checked for existence, never read · verified: the CLI detector test (a credentials file that isn't JSON is never read), onboarding test (nothing connects unasked) (2026-09-23) <!-- synced:x -->
- [x] **DISC-04** · CLI agent detector: PATH and known install dirs, `--version`, sign-in state where exposed, cross-checked with the ACP registry → "Found on this PC · signed in / needs sign-in" + Use (§1.1) → done: PATH (as Windows has it now) plus npm, WinGet, pnpm, Scoop and Bun folders; `--version`; signed-in files; ACP adapter presence from KIVO's catalog (DECISIONS "CLI agents found") · verified: `cli_agents_are_found_with_version_and_sign_in_state_without_reading_credentials` (2026-09-23) <!-- synced:x -->
- [x] **DISC-05** · Local model server detector: ports 11434, 1234, 8080 with model lists → "Running on this PC · N models" + Use (§1.1) → done: probes 11434, 1234, 8080 `/v1/models` with a short timeout, listing models · verified: `local_servers_are_found_with_their_models` (2026-09-23) <!-- synced:x -->
- [x] **DISC-12** · Every discovery section shows "Checked … · Refresh"; Refresh re-runs only that section's detectors; new items get a New label until viewed (§2) → done: "Checked … · Refresh" per section; Refresh re-runs that section only; New until viewed · verified: discovery and Brains tests (2026-09-23) <!-- synced:x -->
- [x] **DISC-14** · Brain health: on page open if older than 5 min, every 5 min while visible, immediately after errors, lazily before use (§3) → done: page open (older than 5 min), every 5 min while visible, at once after errors, lazily before use (background) (DECISIONS "Health checked around use") · verified: brains tests, `a_failing_provider…` (2026-09-23) <!-- synced:x -->
- [x] **DISC-15** · CLI/local-server re-detection: app start (after 30 s, low priority), `WM_SETTINGCHANGE` PATH changes, page open if older than 10 min (§3) → done: 30 s after start at low priority; `WM_SETTINGCHANGE` "Environment" re-reads PATH from the registry and re-detects CLIs; page open if older than 10 min · verified: `the_current_path_comes_from_the_registry`, `a_watch_starts_and_stops`, discovery tests (2026-09-23) <!-- synced:x -->

#### [MEMORY.md](architecture/MEMORY.md) (3)

- [x] **MEM-01** · `conversations` and `messages` tables with the retention setting (30 days default, "never keep" option) (§1) → done: `conversations` and `messages` (FTS5) with the retention setting; retention 0 keeps nothing on disk (the session is kept in memory only) · verified: store tests, e2e thread checks (2026-09-23) <!-- synced:x -->
- [x] **MEM-03** · `preferences` table for explicit settings and stated preferences ("call me Sam", "use metric") (§1) → done: `preferences` table, edited in Brains → Context ("About me") and sent in every request · verified: `the_control_centers_brain_requests` ("Call me Sam" in the next request) (2026-09-23) <!-- synced:x -->
- [x] **MEM-11** · `ContextSnapshot` per session with `ContextDelta`s ("active app changed: VS Code, main.rs"); rebuilt for a new brain session or after failover (§4) → done: a live snapshot per thread and per agent session with "what changed" deltas; rebuilt whole for a new session or brain · verified: context tests, agent e2e (2026-09-23) <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (2)

- [x] **SEC-17** · Secrets in Credential Manager (`keyring`) referenced as `secret://kivo/<provider>/<name>`; loaded only inside the adapter or tool executor (§5) → done: `WindowsSecrets` on Credential Manager (`kivo/<provider>/<name>`), referenced as `secret://kivo/…` in config, read only inside the brain adapter · verified: `keys_round_trip_through_credential_manager`, brains tests (2026-09-23) <!-- synced:x -->
- [x] **SEC-19** · Secrets never reach prompts, logs, activity, diagnostics or IPC to the UI; the UI can only set (write-only) and ask the runtime to test (§5) → done: keys never enter prompts, logs, Activity or IPC; the UI can only set and ask for a test · verified: brains view test (no key material), `the_control_centers_brain_requests` (not in list or config), `Brains.test.tsx` (write-only field) (2026-09-23) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (6)

- [x] **UX-21** · Chat: threads list, conversation with brain switcher and permission-mode picker (SEC-04), attachments, tool activity, cancel, context meter, Compact now (mockup → Chat; CONVERSATION §0–1) → done: `pages/Chat.tsx`: threads (pinned, recent, voice, search), conversation with the brain chip and reason, context meter, cost, live steps and inline decisions, composer with brain switcher, permission-mode picker, attachments, mic, Stop; menu: rename, pin, Compact now, delete · verified: `Chat.test.tsx` incl. accessibility audit (2026-09-23) <!-- synced:x -->
- [x] **UX-22** · Brains page (Brains / Context tabs): providers with found-on-this-PC, add/test/remove, profiles, free options labelled (mockup → Brains) → done: `pages/Brains.tsx` (Brains / Context tabs): connected with health, found on this PC with Refresh and New, not installed with the command, add (OpenRouter, key, custom), test, sign in, remove, profiles, free labels · verified: `Brains.test.tsx` incl. accessibility audits (2026-09-23) <!-- synced:x -->
- [x] **UX-61** · Voice page speech sections: Speech Recognition (engine, status, streaming, language, live transcript, microphone, [Change]) and Text-to-Speech (engine, voice, [Preview], language, speed, expressiveness), plus Manage models (Download, Pause, Cancel, Remove, Set as default; Not installed / Downloading / Installing / Ready / Update available / Error) and [Run benchmark] (VOICE §11, DIST-13) → done: Voice page: understanding (engine, status, streaming, language, live transcript, microphone, Change) and speaking (engine, voice, Preview, language, speed, speaker; expressiveness noted as not adjustable, DECISIONS "Speech speed and expressiveness"); models with Download, Pause, Resume, Cancel, Remove, Use this, Update and every state · verified: `Voice.test.tsx`, store pause/resume/update test, `a_faster_speaking_rate_gives_shorter_audio` (2026-09-23) · note: [Run benchmark] is BENCH-15, deferred <!-- synced:x -->
- [x] **UX-62** · Voice cards: name, style, language, characteristics and Preview, separate from the engine choice (VOICE §11) → done: voice cards (name, style, language, Preview) apart from the engine choice (`VoiceList`) · verified: `Voice.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **UX-23** · Voice page: mic, speaker, wake words (add/edit), STT/TTS engine and voice, personality, models on this PC (download/delete) (mockup → Voice; plan §86) → done: the Voice page has mic and speaker, wake words, STT/TTS engine and voice, personality, your words and the models on this PC · verified: `Voice.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **UX-34** · Step 6: Connect a brain (optional; sign-in; free options marked) (§4) → done: onboarding step 6 finds brains on this PC, offers OpenRouter's sign-in, marks free options; optional, nothing connects unasked · verified: `Onboarding.test.tsx` incl. accessibility audit (2026-09-23) <!-- synced:x -->

#### [VOICE.md](architecture/VOICE.md) (2)

- [x] **VOICE-23** · STT personalization: the enrollment WER picks the recommended engine; `user_vocabulary` feeds hotwords or the Whisper prompt, and the brain receives it for transcript repair (§5) → done: after enrollment every installed recogniser is scored on the owner's recordings and a clearly better one is recommended; `user_vocabulary` (Voice → Your words) goes to STT start for engines with hotwords/prompts and to the brain for repair (DECISIONS "Your voice picks the recogniser") · verified: `every_installed_recognizer_is_scored_on_the_owners_voice`, recommendation and WER tests (2026-09-23) <!-- synced:x -->
- [x] **VOICE-49** · Advanced mode: exact engine, model, device, model path, chunk/latency settings, resource limits, fallback and cache (§11) → done: Voice → Advanced shows the exact engines, models, devices, model paths, the listening timings, the thread limit (settable), the fallback (switchable) and how long models stay loaded (settable) · verified: `Voice.test.tsx` (2026-09-23) <!-- synced:x -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (3)

- [x] **PLAN-05** · Network resilience: offline, timeout, provider failure, rate limit, auth failure and degraded connection each produce a useful fallback and a plain message (§72) → done: offline (cloud unreachable → Offline profile), timeout, provider down, rate limit, auth and quota each give a plain message and, where possible, a same-privacy fallback · verified: `a_failing_provider…`, `a_busy_brain_fails_over…`, `failures_are_said_plainly` (2026-09-23) <!-- synced:x -->
- [x] **PLAN-10** · Predictive prewarming after wake: prepare STT, the likely tool subsystem and optionally the likely provider connection, never with side effects (§94) → done: on a spoken request KIVO warms speech, refreshes a stale app index and opens the likely brain's connection (a keyless GET, at most once a minute) · verified: `prewarming_opens_the_connection_without_a_key_or_a_request` (2026-09-23) <!-- synced:x -->
- [x] **PLAN-17** · Explainability: the card and Activity show the selected provider, tool, permission decision and result, never hidden chain-of-thought (§145) → done: the card's chip (profile, brain, reason on hover, cost), Activity rows for the route, tools, decisions and results; reasoning never shown · verified: e2e tests (chip, Activity), Island tests (2026-09-23) <!-- synced:x -->
<!-- /items -->

---

<a id="m4"></a>

## M4: Tools and computer control

**Goal:** KIVO operates apps semantically (UIA, the browser extension, files, clipboard, shell),
with the full permission system, prompt-injection defense and the Capabilities page.

**Depends on:** M3 (brains call tools), M1 (tool framework, permissions).

**Read first:** [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md), [SECURITY.md](architecture/SECURITY.md), [CAPABILITIES.md](architecture/CAPABILITIES.md), [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md), [UX.md](architecture/UX.md), [CONVERSATION.md](architecture/CONVERSATION.md), [BRAINS.md](architecture/BRAINS.md).

**Build order:**

1. **Safe test ground first.** `testenv/` dummy app, pages and files (TOOL-38); CI runs UIA against
   it (TOOL-39).
2. **Router.** Capability ladder (TOOL-04), app registry (TOOL-05), plugin-shaped interfaces
   (INT-09).
3. **More native tools.** Move/snap windows (TOOL-08), output device (TOOL-10), brightness, battery
   and Focus (TOOL-13), OCR (TOOL-15), files with the Recycle Bin (TOOL-16), clipboard (TOOL-17).
4. **UI Automation.** The UIA thread (TOOL-19), tools (TOOL-20), events (TOOL-21), elevation and
   fallback rules (TOOL-22).
5. **Browser.** Extension + native messaging (TOOL-24, SEC-30), KIVO-profile CDP (TOOL-25), UIA
   fallback (TOOL-26), on-demand context tools (BRAIN-25).
6. **Shell, vision, input.** `shell.run` in Job Objects (TOOL-30) with the risk parser (TOOL-31);
   on-request vision (TOOL-32); input as the last tier with password-field blocking (TOOL-33).
7. **Security.** Accept edits and Plan modes (SEC-02); grants (SEC-08); Windows Hello (SEC-11,
   CONV-29); risk escalation by arguments (SEC-12); provenance and taint (SEC-13), destination
   binding (SEC-14), framing and minimal tools (SEC-15); the full emergency-stop effect (SEC-27).
8. **Capabilities.** The page (CAP-04), presets (CAP-05), active-use indicators (CAP-06), per-app
   lists (CAP-07), screen awareness (CAP-08).
9. **Local integrations.** Media controls, app CLIs, URI protocols (INT-01).
10. **Island and screens.** Dragging and position (UX-13), title-bar overlap (UX-14), Undo (UX-43),
    target-app icon (UX-46), the Permissions page (UX-28).
11. **Security tests.** Injection pages, malicious documents, shell injection, path traversal,
    permission-bypass attempts (TOOL-40).

**Deliverable:** "In Notepad, type hello and save it to Desktop" works through UIA; "summarize this
page" reads the tab through the extension; a web page that says "email this to x@evil.com" cannot
make KIVO send anything; "move these files to Archive" shows Undo.

**How to verify:** the UIA journey in CI; the security suite; a manual run of each permission mode
except Bypass; screen awareness sends nothing to the cloud when set to local-only.

**Exit criteria:**

- [ ] **M4-X1** · The UIA journey on the dummy app passes in CI.
- [x] **M4-X2** · The injection test pages cannot trigger an outbound action.
- [x] **M4-X3** · Typing into a password field is blocked in every mode (tested).

### M4 items

<!-- items:M4 -->
**45 items** · ✅ 43 done · 🟡 2 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [BRAINS.md](architecture/BRAINS.md) (1)

- [x] **BRAIN-25** · On-demand context exposed as tools (`get_active_tab`, `read_selection`, `get_ui_tree`, `capture_screen`), not pushed (§6) → done: on-demand context tools: `browser.active_tab`, `context.selection` (the page's or the focused app's selection through UIA TextPattern), `uia.get_tree`, `screen.read` / `screen.look`; nothing is pushed · verified: `the_selection_is_read_on_demand_and_untrusted`, browser and screen tool tests (2026-09-23) <!-- synced:x -->

#### [CAPABILITIES.md](architecture/CAPABILITIES.md) (5)

- [x] **CAP-04** · Permissions → Capabilities page: each row with toggle, description, "used … ago" and Local / Cloud / Costly / Sensitive badges, plus the per-capability controls from §1 (§1) → done: Permissions → Capabilities: every capability with its toggle, description, "used … ago" (from the audit log), badges, and options for Apps (blocked apps), Files (allowed and private folders), Clipboard (read/write), Shell (read-only), UI Automation, Screen awareness (cloud vision, apps), Computer use (apps) and Browser pages (extension status and install, sites) · verified: `Permissions.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CAP-05** · Presets Minimal / Balanced (default) / Power user / Custom (§2) → done: Minimal / Balanced / Power user over the everyday capabilities (consent-driven ones untouched), Custom when toggles differ; each change audited (`capabilities.preset`) · verified: `presets_set_their_toggles_and_are_recognized`, `Permissions.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CAP-06** · Active-use indicators in the tray and the Island: screen (eye), input control (hand), shell (terminal) (§2) → done: while screen awareness, input control or the shell is in use, the tray badge and tooltip ("Using the screen, shell commands") and the Island's eye / hand / terminal icons show it (`StateSnapshot.in_use`) · verified: tray tests (`icon_shown`, tooltip), `the_emergency_stop_reaches_commands` (shell indicator on, then cleared), Island indicator test (2026-09-23) · note: computer use's banner and screen frame are CAP-12 (M8) <!-- synced:x -->
- [x] **CAP-07** · Per-app allow and block lists for UIA, screen awareness and computer use; the default block list covers password managers, banking apps and Windows Security (§2) → done: per-capability allow and block lists for UI Automation, screen awareness and computer use (`tools.*-apps`), matched by program, name or `*pattern*`; the default block list covers password managers, `*bank*` apps and Windows Security · verified: `app_scopes_block_password_managers_banking_and_windows_security_by_default`, `Permissions.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CAP-08** · Screen awareness on request: capture once → UIA excerpt + local OCR first → vision brain only if needed and allowed, with "Sent a screenshot of … to …"; screenshots held in memory only (§3) → done: `screen.read` captures once, reads the UIA excerpt and local OCR, keeps nothing; `screen.look` sends one screenshot only to a vision brain allowed by the cloud-vision option and privacy mode, and the card and Activity say "Sent a screenshot of … to …" · verified: `reading_uses_the_tree_and_local_ocr_and_keeps_nothing`, `screenshots_go_only_to_a_brain_allowed_to_see` (2026-09-23) <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (1)

- [x] **CONV-29** · High risk: voice "approve" triggers Windows Hello; a click also works; voice alone never suffices (§7) → done: a spoken "approve" on a High-risk card starts Windows Hello, which must confirm; a click also works; voice alone never approves (without Hello set up, KIVO asks for a click) · verified: `windows_hello_confirms_high_risk`, `a_spoken_yes_is_not_enough_for_high_risk` (2026-09-23) <!-- synced:x -->

#### [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) (2)

- [x] **INT-01** · Local integration paths that need no account: Windows media controls for any player, app CLIs (`gh`, `code`, `git`, `wt`), UIA on desktop apps, the browser extension on web apps, URI protocols such as `spotify:` (§1, §2) → done: local paths with no account: Windows media controls (M1), app CLIs through `apps.cli` (VS Code `code`, Windows Terminal `wt`, `git`, GitHub CLI `gh`), UIA on desktop apps, the browser extension on web apps, and URI protocols through `apps.open_uri` (`spotify:`, `ms-settings:`, `calculator:`) · verified: `app_clis_run_declared_verbs_with_quoted_arguments`, `only_declared_uri_schemes_open`, the UIA and browser tests (2026-09-23) <!-- synced:x -->
- [x] **INT-09** · ToolSpec and WIT interfaces drafted so built-in tools already fit the plugin shape (§3) → done: `crates/kivo-tools/wit/kivo-plugin.wit` (types, host imports one per grant, `tools` export) with `tool-spec` mirroring `ToolSpec` (DECISIONS "Plugin interface drafted") · verified: `the_wit_tool_spec_is_the_tool_spec`, `the_wit_enums_are_the_rust_enums`, `every_tool_fits_the_plugin_shape` (2026-09-23) <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (9)

- [x] **SEC-02** · Permission modes Accept edits and Plan first (Plan: read-only, plan shown, approval grants exactly the planned steps) (§1.1) → done: Accept edits (edits and apps run, commands and settings ask) and Plan first: read-only, a round's changes become one plan card, approving grants exactly those calls (`approve_plan`), a High step needs a click or Hello (DECISIONS "Plan first: one card per round") · verified: `each_mode_asks_for_what_it_promises`, `a_plan_is_approved_as_exactly_its_steps`, `plan_first_approves_exactly_the_planned_steps` (e2e), Island plan test (2026-09-23) <!-- synced:x -->
- [x] **SEC-08** · Grants scoped by tool, argument pattern and duration (once / session / 24 h / always) in `permissions_grants`; the user can view and revoke them (§2) → done: grants by tool, target, `*` argument pattern and duration (this session, 24 h, always; migration 7), chosen on the card's "Always for…"; Permissions → Mode lists them with how long they last and Revoke · verified: `grants_match_argument_patterns`, `grants_round_trip_and_arguments_are_summarized_short` (durations, session end), `grants_and_the_audit_log_are_readable_and_revocable`, Permissions and Island tests (2026-09-23) <!-- synced:x -->
- [x] **SEC-11** · Windows Hello confirmation for High risk (§2, CONVERSATION §7) → done: `WindowsHello` (`UserConsentVerifier` via `IUserConsentVerifierInterop`); High-risk cards offer "Confirm with Windows Hello" when it is set up (`ConfirmSpec.hello`), and only a verified Hello approves · verified: `windows_hello_confirms_high_risk` (e2e with the fake verifier: a failed Hello approves nothing, audit says `hello`), `availability_is_answered_without_a_prompt` (real API, no prompt), Island Hello test (2026-09-23) <!-- synced:x -->
- [x] **SEC-12** · Risk escalation by arguments: protected paths and bulk (> 20 files) become High; egress of personal/sensitive data becomes High (blocked in Strict Private); untrusted-origin destinations become Deny unless the user restates them (§3) → done: each tool assesses its call (`Tool::assess`): protected paths and > 20 files High, the shell by its parser, CLI verbs by the registry; the engine makes personal/sensitive data leaving the device High and refuses it in Strict Private; untrusted destinations are refused unless the user restates them · verified: `protected_paths_and_bulk_raise_the_risk_to_high`, `arguments_raise_the_risk_and_private_data_stays_home_in_strict_private`, `destinations_are_bound_to_the_users_own_words` (2026-09-23) <!-- synced:x -->
- [x] **SEC-13** · Provenance on every value (User / System / Untrusted) and turn taint when untrusted content enters the context (§4) → done: tool outputs from someone else's content carry a source (`Output::untrusted`, provenance Untrusted); the turn records where untrusted content entered and is Tainted from then on, which the card shows ("Requested after reading …") · verified: `window_content_is_fenced_and_taints_the_turn`, `destinations_from_content_are_refused` (2026-09-23) <!-- synced:x -->
- [x] **SEC-14** · Destination binding: outbound actions only to destinations present in `User`-provenance text for the task (§4) → done: destinations are bound to the user's words for the task (`bind`): after taint an unnamed destination is Untrusted and refused; data never goes to one the user didn't name (DECISIONS "Destination binding, precisely"; SECURITY §4 updated) · verified: `destinations_are_bound_to_the_users_own_words`, `destinations_from_content_are_refused` (2026-09-23) <!-- synced:x -->
- [x] **SEC-15** · Untrusted content wrapped in delimited, labelled blocks; tainted turns get the minimal tool set (§4) → done: untrusted tool output reaches the brain inside `<untrusted source=…>` blocks; a tainted turn is offered at most 8 tools and none that send, delete, spend, run commands or power off, and a tool that wasn't offered is refused · verified: `window_content_is_fenced_and_taints_the_turn` (2026-09-23) <!-- synced:x -->
- [x] **SEC-27** · Full effect: cancel all turns and tasks, stop TTS, kill tool Job Objects, `session/cancel` to ACP agents, stop input injection, pause background tasks, audit entry (§8) → done: the emergency stop pauses running tasks, which resume only when the user says so · verified: `timeouts_pauses_and_checks` (2026-09-23) <!-- synced:x -->
- [x] **SEC-30** · Native messaging host manifest allows only KIVO's extension ID; payload limits (§9) → done: the host manifest names only KIVO's extension (`chrome-extension://ohaefaipbiknncgdjdpkjgaiocpnlloc/`), the host re-checks the caller, needs the session token on the user-only pipe, and every frame is capped at 1 MiB (the extension caps its replies too) · verified: `the_manifest_allows_only_one_origin`, `registration_round_trips_under_a_test_key`, `a_connection_without_the_token_is_refused`, `flags_are_parsed_and_unknown_ones_refused` (2026-09-23) <!-- synced:x -->

#### [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) (22)

- [x] **TOOL-04** · Tool router with the capability ladder (Native/OS API → App CLI → UIA → Browser DOM → A11y → Vision → Input) and per-app overrides (§2) → done: `control.act` (`crates/kivo-tools/src/router.rs`) walks the ladder Native → URI → App CLI → UIA → Browser DOM → Vision → Input with the app's registry preferences first, skips tiers whose capability is off, stops at a password field and reports which tier did it and what was tried · verified: `uia_is_used_first_for_a_desktop_app_and_says_so`, `custom_drawn_apps_fall_to_vision_and_input_only_when_allowed`, `password_fields_stop_the_ladder`, `the_ladder_puts_an_apps_preferences_first` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-05** · App Capability Registry as data files (`apps/*.toml`: exe/AUMID match, launch method, CLI verbs, UIA hints, quirks) for the core apps; users and plugins can add entries (§2) → done: `crates/kivo-tools/apps/*.toml` (13 core apps: VS Code, Windows Terminal, git, GitHub CLI, Spotify, Chrome, Edge, Brave, Firefox, Explorer, Notepad, Settings, Calculator) compiled in, plus the user's `%APPDATA%\KIVO\apps` files, which override core ones; exe/AUMID match, launch, CLI verbs with risk, URI verbs, UIA hints, preferred tiers, quirks (DECISIONS "App registry as data") · verified: `the_core_registry_loads_and_matches_programs_and_names`, `user_entries_are_added_and_override_core_ones_and_bad_files_are_reported` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-08** · Windows: move to monitor, snap (§3) → done: `windows.move_to_monitor` (monitors numbered left to right, window kept whole and centred) and `windows.snap` (halves and quarters of the work area), both undoable; `WindowsDisplays` (EnumDisplayMonitors, GetWindowPlacement, SetWindowPos without activating) · verified: `snapping_and_moving_use_the_work_area_and_undo`, `snapping_splits_the_work_area`, `there_is_one_primary_monitor_numbered_from_one` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-10** · Audio: output device switch via documented APIs (§3) → done: `audio.outputs` lists the devices; `audio.set_output` moves KIVO's own voice there and opens Windows' output settings, since Windows has no documented default-device API (spec §3 updated, DECISIONS "Output device switch") · verified: `output_switch_moves_kivos_voice_and_opens_the_picker`, `lists_devices_with_one_default_per_direction` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-13** · System: brightness where supported, battery, Focus/DND state (§3) → done: `system.brightness` / `system.set_brightness` (built-in panels through WMI `WmiMonitorBrightness`, undoable; external monitors say they can't be changed), `system.battery` (GetSystemPowerStatus), `system.focus_state` (Windows' notification state) · verified: `brightness_battery_and_focus` (tool), `brightness_is_a_percentage_or_unsupported`, `battery_is_consistent_when_present`, `focus_state_reads` on this PC; the WMI write is not run against the real display (owner rule: tests never change real settings) (2026-09-23) <!-- synced:x -->
- [x] **TOOL-15** · OCR via Windows.Media.Ocr (offline) (§3) → done: `WindowsOcr` on `Windows.Media.Ocr` (the given language, else the user's), line boxes from word boxes; `screen.ocr` and screen reading use it · verified: `recognizes_text_drawn_into_an_image` (GDI-rendered text read back), `a_malformed_image_is_refused` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-16** · Files: search (Windows Search `SystemIndex` + fallback walk), open, reveal, create, rename, move, copy; delete always to the Recycle Bin; path-traversal and protected-path guard (§3) → done: `files.search` (Windows index via `search-ms:`, else a bounded walk), `read`, `open`, `reveal`, `create`, `rename`, `move`, `copy` (undoable) and `delete` (Recycle Bin via `IFileOperation`, confirmed up front); the path guard, allowed and private folders, protected paths and bulk → High (DECISIONS "Files: scoped, recycled") · verified: `create_rename_move_copy_and_undo_on_the_synthetic_tree`, `delete_goes_to_the_recycle_bin`, `the_guard_refuses_traversal_devices_networks_and_streams`, `files_stay_inside_the_allowed_folders_and_out_of_private_ones`, `recycling_a_temp_file_moves_it_out_of_its_folder` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-17** · Clipboard read/write; reads are tagged `Untrusted` (§3) → done: `clipboard.read` (untrusted, `the clipboard` source; taints the turn) and `clipboard.write` (undoable) on `WindowsClipboard` (Unicode text), each switchable in Capabilities → Clipboard · verified: `clipboard_reads_are_untrusted_and_writes_undo`, `destinations_from_content_are_refused` (a clipboard read taints the turn) (2026-09-23) <!-- synced:x -->
- [x] **TOOL-19** · A dedicated UIA thread (COM MTA) using the `uiautomation` crate (license verified in M0) with `windows-rs` for gaps (§4) → done: `WindowsUiAutomation` (`crates/kivo-platform-windows/src/uia.rs`): one COM-MTA thread owning the `uiautomation` client, a 12 s answer deadline and IUIAutomation2 provider timeouts; `windows-rs` for elevation and window checks (DECISIONS "UI Automation on its own thread") · verified: `uia_journey_on_the_dummy_app` against `testenv/app` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-20** · UIA tools `uia.find`, `uia.get_tree` (depth-limited, pruned), `uia.invoke`, `uia.set_value`, `uia.toggle`, `uia.select`, `uia.expand`, `uia.scroll_into_view`, `uia.get_bounds` (§4) → done: `uia.find` (name fuzzy, role, AutomationId), `uia.get_tree` (depth 6, 150 nodes, unnamed containers lifted), `invoke`, `set_value` (undoable, never passwords), `toggle` (undoable), `select`, `expand` (undoable), `scroll_into_view`, `get_bounds`, plus `context.selection` · verified: `uia_journey_on_the_dummy_app` (find, tree, set value, invoke, toggle, select, expand/collapse, bounds, Export writes its file), tool tests in `uia_tools.rs` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-21** · UIA events (FocusChanged, scoped StructureChanged, WindowOpened/Closed, PropertyChanged) subscribed on demand into the event bus; no polling (§4) → done: focus, window opened/closed, and window-scoped structure and property events, subscribed only on request (`uia.wait`) and removed after; each also goes onto the event bus (`SystemEvent::Automation`) · verified: `uia_journey_on_the_dummy_app` (the Export window's WindowOpened), `structure_and_property_events_are_scoped_to_one_window`, `waiting_is_event_driven_and_unsubscribes` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-22** · Elevated windows (UIPI) are reported clearly; KIVO never auto-elevates; custom-drawn apps fall back to Vision/Input (§4) → done: a window whose process is elevated above KIVO is refused up front with "<app> is running as administrator …" (`PlatformError::Elevated`); KIVO never elevates; an empty tree is reported as custom-drawn so the ladder falls to Vision/Input · verified: `this_process_is_not_elevated_in_tests`, `trees_are_compact_and_custom_drawn_windows_are_flagged`, `custom_drawn_apps_fall_to_vision_and_input_only_when_allowed`; a live elevated window isn't tested (tests never run as administrator) (2026-09-23) <!-- synced:x -->
- [x] **TOOL-24** · KIVO Chromium MV3 extension (Chrome, Edge, Brave) + native messaging host (`kivo-runtime --native-messaging`): tabs, URL/title, selection, readable DOM excerpt (~4k tokens), click/type on elements (§5) → done: MV3 extension `extensions/browser` (tabs, active tab, readable excerpt ≈ 4k tokens, selection, click and type, never password fields) and the native messaging host in `kivo-runtime` relaying to the user-only bridge pipe with the session token; `browser.*` tools (DECISIONS "The browser extension and its host") · verified: `extension.test.ts` (7, on testenv pages), `the_extension_answers_through_the_bridge`, `the_host_relays_between_the_browser_and_the_runtime`, `a_connection_without_the_token_is_refused`, browser tool tests (2026-09-23) <!-- synced:x -->
- [x] **TOOL-25** · CDP automation (`chromiumoxide`) only in a KIVO-managed profile (§5) → done: `ManagedChrome` (`chromiumoxide`) runs Chrome/Edge/Brave already installed in a KIVO profile folder, respecting certificate errors, never downloading a browser; `browser.auto.*` tools under Autonomous browsing · verified: `kivos_browser_reads_clicks_and_types_in_its_own_profile` (headless, throwaway profile, local page), `the_managed_browser_opens_only_web_addresses` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-26** · UIA on the browser window as the fallback when the extension is absent; all page content is `Untrusted` (§5) → done: without the extension, the active tab's address and title come from the browser window through UIA and the page text from its UIA tree; all page content is untrusted · verified: `without_the_extension_the_active_tab_comes_from_uia`, `with_the_extension_pages_are_read_as_untrusted_excerpts` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-30** · `shell.run { command, shell: pwsh|cmd, cwd, timeout }` inside a Job Object (kill-on-close, memory/CPU limits), output captured, cancellable, logged (§7, ARCHITECTURE §1) → done: `shell.run { command, shell: pwsh|cmd, cwd, timeoutMs, env }` via `WindowsCommands`: started suspended, joined to its own Job Object (kill on close, memory cap, CPU cap), output captured (64 KiB), cancellable, killed with its tree; logged through the audit (DECISIONS "Commands in Job Objects") · verified: `cmd_runs_in_its_folder_with_per_call_env`, `powershell_reports_exit_codes_and_stderr`, `timeouts_and_cancellation_kill_the_whole_tree`, `kill_all_ends_running_commands`, `output_is_capped`, `cancellation_stops_a_running_command` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-31** · Shell risk parser: read-only commands Low; writes, deletes, network, registry, `Invoke-Expression`, encoded commands or elevation Medium/High; unparseable High; never elevated; secrets only as per-call env vars (§7) → done: the parser (`shell::assess`): read-only Low; writes, installs, network Medium; deletes with recursion or wildcards, registry, services, disks, users, `Invoke-Expression`, encoded commands, download-and-run and elevation High; unreadable High; a protected working folder High; read-only mode by default; secrets only as per-call environment variables, scrubbed from output · verified: `read_only_commands_are_low`, `writes_installs_and_network_are_medium`, `dangerous_commands_are_high`, `quoting_hides_nothing`, `the_call_risk_comes_from_the_command`, `a_run_goes_through_the_job_runner_with_secrets_as_env_only`, `read_only_mode_refuses_anything_that_changes_something` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-32** · Vision on request only: capture region/window → local OCR or a vision brain within the privacy class; no continuous capture (§8) → done: `screen.read` (one capture, UIA excerpt + local OCR, dropped after), `screen.ocr`, and `screen.look` (one downscaled PNG to a vision brain only when it sees and the settings allow; `Part::Image` in the Anthropic, OpenAI and Gemini adapters); never continuous · verified: `reading_uses_the_tree_and_local_ocr_and_keeps_nothing`, `looking_needs_cloud_vision_allowed_and_sends_a_small_png`, `screenshots_reach_vision_models_in_each_providers_shape`, `screenshots_go_only_to_a_brain_allowed_to_see` (2026-09-23) <!-- synced:x -->
- [~] **TOOL-33** · Input (SendInput) last tier: re-validate foreground window and bounds before clicking; block typing into password fields (UIA `IsPassword`); at least Medium unless user-initiated (§8) → partial: `input.click/type/press/scroll` under Computer use: the window must still be in front and unmoved, clicks land inside it, typing and non-navigation keys never go to a password field, Medium unless the user asked directly; `WindowsInput` builds `SendInput` events with an abort flag for the emergency stop · verified: `clicks_land_inside_the_window_in_front_or_not_at_all`, `a_window_that_is_no_longer_in_front_is_refused`, `never_types_into_a_password_field`, `input_is_medium_unless_the_user_asked_directly`, the event-list tests · missing: a live `SendInput` run, which only a Windows Sandbox or VM session may do (owner rule: no synthetic input on the live desktop) <!-- synced:~ -->
- [x] **TOOL-38** · `testenv/` dummy Win32/WinUI app with known AutomationIds (buttons, fields, lists, dialogs, fake Export), local HTML pages (forms, injection pages, fake login), synthetic file tree, sandboxed shell dir (§10) → done: `testenv/`: the dummy Win32 app (`kivo-test-app`, AutomationIds 101–110 and 201–203, never takes focus, Export writes only into `--out`), `pages/` (form, fake login, injection), `documents/malicious.md`, the synthetic `files/` tree and the `shell/` folder, with the testkit launcher · verified: `uia_journey_on_the_dummy_app`, file tests on a copy of the tree, `extension.test.ts` on the pages (2026-09-23) <!-- synced:x -->
- [~] **TOOL-39** · CI runs the UIA journey against the dummy app on `windows-latest`; destructive tests run only in Windows Sandbox or a VM (§10) → partial: CI's Windows job (on version tags) names the UIA journey against the dummy app as its own step (`cargo test -p kivo-platform-windows --test uia_testenv`), next to the workspace tests that build the app; nothing in the suite is destructive, so no Windows Sandbox step yet · verified: the step's command locally (3 pass) · missing: its first run on `windows-latest`, which happens on the first `v*` tag (owner) <!-- synced:~ -->
- [x] **TOOL-40** · Security test suite v1: injection pages, malicious documents, shell injection strings, path traversal, permission-bypass attempts (§10, SECURITY) → done: the security suite v1 in two halves. Tools (`crates/kivo-tools/src/security_suite.rs`): 31 smuggled-change shell strings (chaining, newlines, pipes into `iex`, script blocks, subexpressions in double quotes, en-dash flags, `git --output`, cmd `&`) never pass as read-only and the read-only shell runs none; destructive and obfuscated ones are High; a brain's extra arguments can't lower a risk; traversal, device, share, stream, drive-relative and variable paths are refused by every file tool; renames can't leave the folder; disguised schemes never open. End to end (`apps/kivo-runtime/tests/security_suite.rs`, the `testenv/` fixtures): the malicious document and the injection page reach the brain fenced (a forged `</UNTRUSTED >` can't close the fence — `ContextItem::render` now defuses any spelling), the tainted round offers no shell, delete or sending and refuses them without asking; switched-off, unknown and unoffered tools are refused; arguments claiming approval still get the strong question, and forged answers approve nothing. The suite found and fixed parser gaps (script blocks, `$()` in double quotes, `@()`, `iex(`, `InvokeScript`, unicode dashes, `git --output`) · verified: the 8 tools tests and 4 end-to-end tests, `untrusted_text_cant_leave_its_fence`; CI runs both as a named step (2026-09-23) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (5)

- [x] **UX-13** · Draggable, position remembered per monitor; setting Top center (default) / Bottom center / Remember drag (§2) → done: the Island is dragged by its card; the spot is remembered per monitor (`overlay.spots`, `island.moved`) and Settings → Island offers Top center (default) / Bottom center / Where I dragged it · verified: `place_top_bottom_and_remembered_spots` (overlay placement), Settings Island test (2026-09-23) <!-- synced:x -->
- [x] **UX-14** · Title-bar overlap: shifts down by the title-bar height while only listening (§2) → done: the foreground window's title bar or tab strip (DWM caption-button height, else system metrics) is sent with the turn; while only listening, an Island over it moves just below · verified: `listening_moves_below_a_title_bar_only_while_listening` (2026-09-23) <!-- synced:x -->
- [x] **UX-28** · Permissions page tabs Mode · Capabilities · Privacy (mockup → Permissions) → done: `pages/Permissions.tsx` with Mode (modes, hard limits, comparison, grants with Revoke), Capabilities (CAP-04/05/07) and Privacy (where requests go, what stays, conversation keeping, transcripts, private folders) · verified: `Permissions.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **UX-43** · Undo in the Island with an ~8 s ring, "Kivo, undo that" and the Control Center toast; irreversible actions never offer Undo (§8.1) → done: undoable changes offer Undo in the Island with an 8 s ring and as a Control Center toast; "Kivo, undo that" (`session.undo` grammar command) works for 10 minutes; irreversible tools never offer it · verified: Island Undo test, `every_undoable_tool_has_an_undo_and_no_other_does`, `undo_takes_back_the_last_change` (2026-09-23) <!-- synced:x -->
- [x] **UX-46** · Target-app icon as the Island's leading icon while acting (§8.1) → done: while acting, the Island's leading icon is the target app's own icon (`IShellItemImageFactory` → PNG data URL, cached), else its initials · verified: `notepad_has_an_icon`, Island icon test (2026-09-23) <!-- synced:x -->
<!-- /items -->

---

<a id="m5"></a>

## M5: Agents, background tasks and routines

**Goal:** multi-step work with plans and validation, coding tasks run by CLI agents, watchers that
cost nothing while waiting, routines and custom commands, and driving terminal agents by voice.

**Depends on:** M3 (brains, ACP), M4 (tools, grants).

**Read first:** [BRAINS.md](architecture/BRAINS.md), [ROUTINES.md](architecture/ROUTINES.md), [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md), [CONVERSATION.md](architecture/CONVERSATION.md), [UX.md](architecture/UX.md), [SECURITY.md](architecture/SECURITY.md), [MEMORY.md](architecture/MEMORY.md), [DISCOVERY.md](architecture/DISCOVERY.md), plan §137, §140.

**Build order:**

1. **Tasks.** The persisted task graph (ARCH-27), task retention (MEM-02), task grants (SEC-09),
   cancellation from timeouts, failures and shutdown (PLAN-03).
2. **Watchers.** Download, folder, process/build, window and time watchers (TOOL-28) with owners,
   history and no LLM while waiting (TOOL-29); event-task classification (BRAIN-07).
3. **Agent path.** Planner (BRAIN-30), validation before "done" (BRAIN-31), coding delegation
   (BRAIN-32).
4. **Routines.** Data model (ROUT-01), execution (ROUT-02), routine grants (ROUT-03), error
   policies (ROUT-04), phrase and hotkey triggers (ROUT-05), custom commands (ROUT-06), collision
   checks (ROUT-07), AI steps (ROUT-08), the builder (ROUT-09), starter routines (ROUT-10).
5. **Workspaces and instructions.** Instructions (CONV-09), workspace detection (CONV-10), project
   agent files (CONV-11).
6. **Driving other AIs.** Open in terminal (CONV-13), visible terminal agents (CONV-14), the Draft
   card (CONV-15), desktop AI app detection (DISC-06).
7. **Bypass mode** (SEC-03).
8. **Screens and conveniences.** Live activities (UX-15), Tasks (UX-24), Agents (UX-25), Routines
   (UX-26), proactive speech rules (UX-40), notification replies and the jump list (UX-58), the
   selection shortcut (UX-42), "What can I say?" (UX-44), voice-only operation (UX-55).
9. **Acceptance.** The §137 coding journey (PLAN-20) and §140 build watcher (PLAN-22).

**Deliverable:** "Hey Kivo, fix the failing tests in K.I.V.O" runs Claude Code, shows steps in the
Island and Tasks, re-runs the tests and only then says it's fixed; "tell me when the build
finishes" waits with no AI calls; "Kivo, work mode" runs the routine.

**How to verify:** the two acceptance journeys end to end; a watcher's CPU and token use while
waiting (zero tokens); routine grants cannot be exceeded (tested).

**Exit criteria:**

- [x] **M5-X1** · The plan §137 and §140 journeys pass end to end (PLAN-20, PLAN-22). → `a_coding_request_is_fixed_checked_and_reported` (scripted ACP agent) and `the_build_watcher_waits_without_ai_and_tells_you` pass (2026-09-23)
- [x] **M5-X2** · A watcher uses no brain calls while waiting (measured). → the brain request count stays the same while watchers wait: `the_build_watcher_waits_without_ai_and_tells_you`, `a_brain_turns_an_unclear_request_into_a_watcher` (2026-09-23)

### M5 items

<!-- items:M5 -->
**39 items** · ✅ 39 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (1)

- [x] **ARCH-27** · A Task holds a graph of steps with dependencies and its own token, outlives turns, is persisted, and after a crash is reported as interrupted without auto-resuming side effects (§4.2) → done: tasks are step graphs with dependencies and their own token under KIVO's, stored in `tasks`/`task_steps` (`kivo-store/src/tasks.rs`, `kivo-runtime/src/tasks.rs`); after a crash running tasks are Interrupted and never resumed, waiting watchers are armed again · verified: `a_crash_leaves_interrupted_tasks_and_rearmed_watchers`, `a_restart_interrupts_running_tasks_but_keeps_watchers` (2026-09-23) <!-- synced:x -->

#### [BRAINS.md](architecture/BRAINS.md) (4)

- [x] **BRAIN-07** · Event tasks ("tell me when…", "watch…", "remind me…") classified by grammar first and by the brain when ambiguous, then turned into Tasks with watchers (§2) → done: grammar commands `tasks.remind` / `tasks.watch` (build, download, window, process) first; brains get the same tools (`tasks` hints) when the grammar can't tell; both become tasks with watchers · verified: `reminders_and_missing_builds`, `the_build_watcher_waits_without_ai_and_tells_you`, `a_brain_turns_an_unclear_request_into_a_watcher` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-30** · Planner: multi-step requests produce `propose_plan`, which becomes a Task graph; independent steps run concurrently (§7) → done: `tasks.propose_plan` (always asks; risk of its highest step; steps shown on the card first); approving grants exactly those calls and runs the plan as a task, independent steps together · verified: `a_multi_step_request_becomes_an_approved_plan`, `tasks_keep_to_their_grants_and_run_independent_steps_together` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-31** · Validation: tasks declare `success_criteria`, and "done" is reported only after a verification step passes (§7) → done: tasks carry success checks (command, file, window) that must pass before Done; coding work re-runs the project's tests and is called fixed only when they pass (`checks.rs`, `engine/agent.rs`) · verified: `timeouts_pauses_and_checks`, `a_coding_request_is_fixed_checked_and_reported`, `a_fix_that_still_fails_is_never_called_done` (2026-09-23) <!-- synced:x -->
- [x] **BRAIN-32** · Coding tasks are delegated to the Coding profile's ACP agent, with progress, relayed permission prompts and a summary (§7) → done: coding requests go to the Coding profile's ACP agent in the workspace, with its plan and edits as steps, its permission requests on the card, one retry with the failing output, a summary, and a Coding task in Tasks · verified: `a_coding_request_is_fixed_checked_and_reported`, `claude_code_asks_for_permission_on_the_card_and_follow_ups_reuse_its_session` (2026-09-23) <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (6)

- [x] **CONV-09** · Global instructions ("About me") and per-workspace instructions in SQLite, mirrored as Markdown (`instructions\global.md`, `workspaces\<name>\instructions.md`), watched and re-imported on edit (§4) → done: global ("About me") and per-workspace instructions in SQLite, mirrored to `instructions\global.md` and `workspaces\<name>\instructions.md`, watched with `notify` and read back when edited; edited on Agents → Workspaces · verified: `instructions_are_mirrored_and_edits_come_back`, `Agents.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CONV-10** · Workspaces detected (VS Code folder, terminal cwd, git repo acted in), with a one-time "Remember … as a workspace?" (§4) → done: folders KIVO works in (a command's or CLI's folder, an agent's, the folder open in VS Code) are offered once: "Remember … as a workspace?" in the Island, answered by click or voice · verified: `a_new_project_is_offered_once_and_remembered`, `vs_codes_open_folder_is_found`, `the_islands_buttons_work_by_voice`, Island offer test (2026-09-23) <!-- synced:x -->
- [x] **CONV-11** · Project `CLAUDE.md` / `AGENTS.md` / `GEMINI.md` read when working in that folder, never written unless asked; "Export as AGENTS.md to project" on request (§4) → done: a workspace's CLAUDE.md / AGENTS.md / GEMINI.md are read into the context when working there, never written; "Export as AGENTS.md" writes a KIVO section and keeps the rest · verified: `exporting_agents_md_keeps_the_rest_of_the_file`, `Agents.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CONV-13** · "Open in terminal": hand the session to a visible terminal where the agent supports resuming (§5.1) → done: "Open in terminal" on the Agents page starts the agent's resume command for a KIVO-run session in Windows Terminal (`agents.openInTerminal`) · verified: `the_agents_page_requests`, `Agents.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **CONV-14** · Visible terminal agents: launch Windows Terminal with cwd + agent command (mode flags from the app registry; bypass/yolo launch is High risk), track the session, paste prompts via UIA/clipboard, wait for "send", read replies via TextPattern (§5.2) → done: `agents.open_terminal` launches Windows Terminal in a folder with the agent and its mode flags from the app registry (checked against `--help`; bypass is High), tracks the session; `agents.send_prompt` pastes by clipboard into its window; `agents.read_reply` reads the window's text as untrusted; "Start" on the Agents page · verified: `opening_an_agent_checks_its_flags_and_bypass_is_high`, `prompts_go_into_the_sessions_window_and_replies_are_read_back`, `terminal_agents_start_with_a_yes_and_prompts_are_drafts_first` (2026-09-23) <!-- synced:x -->
- [x] **CONV-15** · Prompt Draft card: target, text, Send · Edit · Cancel, voice edits live ("add …", "remove the last sentence", "read it back") (§5.4) → done: the Draft card (target, text, Send · Edit · Cancel) for every prompt to another AI; voice edits ("add …", "remove the last sentence", "read it back", "send") without AI; the Island's Edit field · verified: `terminal_agents_start_with_a_yes_and_prompts_are_drafts_first`, `draft_edits_are_read_from_speech`, Island draft test (2026-09-23) <!-- synced:x -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (1)

- [x] **DISC-06** · Desktop AI apps detector (Claude Desktop, ChatGPT, Copilot) from the installed-apps index (§1.1) → done: Claude Desktop, ChatGPT and Copilot from the installed-apps index via the app registry's `desktop_ai` entries; listed on the Agents page · verified: `the_agents_page_requests`, `Agents.test.tsx` (2026-09-23) <!-- synced:x -->

#### [MEMORY.md](architecture/MEMORY.md) (1)

- [x] **MEM-02** · `tasks` and `task_steps` kept until deleted; completed tasks summarized after 90 days (§1) → done: tasks and steps are kept until deleted; finished tasks older than 90 days keep a summary instead of their steps (checked at start) · verified: `old_finished_tasks_keep_a_summary_only` (2026-09-23) <!-- synced:x -->

#### [ROUTINES.md](architecture/ROUTINES.md) (10)

- [x] **ROUT-01** · `Routine`, `Trigger` and `Step` data model (§5) in a versioned `routines` table with a JSON body → done: `Routine`, `Trigger`, `RoutineStep` (`kivo-core/src/routine.rs`) stored as a versioned JSON body in `routines` · verified: `routine.rs` unit tests, `custom_commands_and_routines_run_from_their_phrases` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-02** · A routine compiles to a Task graph (sequential, optional parallel groups); every step goes through `authorize()`; cancellation like any task (§3) → done: a routine compiles to a task graph (in order, parallel groups together); every step goes through `authorize()` with the routine's grants; cancellable like any task · verified: `custom_commands_and_routines_run_from_their_phrases`, `routines_run_only_what_was_granted` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-03** · Routine grants: the full permission list is shown on save and granted scoped to the routine and its exact arguments (§3) → done: saving shows the full permission list (Routines → Save) and grants each tool with its exact arguments; anything else asks, and a task can't exceed them · verified: `routines_run_only_what_was_granted`, `Routines.test.tsx` builder test (2026-09-23) <!-- synced:x -->
- [x] **ROUT-04** · Error policy per step: stop (default), continue, retry(n), ask (§3) → done: stop (default), continue, retry(n), ask per step, set in the builder · verified: `error_policies_retry_continue_and_ask` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-05** · Phrase triggers (with variants and `{variables}`) resolved in the intent router's grammar stage; hotkey triggers; manual run (§1, §2) → done: phrases with variants and `{variables}` matched before the grammar; hotkeys registered and re-registered on change; Run on the page and the jump list · verified: `custom_commands_and_routines_run_from_their_phrases`, `starters_are_added_once_and_hotkeys_run_routines`, `routine_hotkeys_replace_the_previous_ones` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-06** · Custom commands: a phrase that runs one fast-path action without AI ("Kivo, cinema") (§1) → done: one phrase + one tool runs as a direct action without AI · verified: `custom_commands_and_routines_run_from_their_phrases` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-07** · Phrase collision check against the fast-path grammar, other routines and wake words, using the wake-word confusability checker (§5) → done: phrases checked against the grammar, other routines and wake words (exact clash blocks, sound-alike warns, the wake-word confusability list), hotkeys against KIVO's and other routines' · verified: `clashing_phrases_and_hotkeys_are_caught`, `Routines.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-08** · AI steps (`brain.ask`, `brain.decide`) with a chosen profile, counted toward cost limits; routines containing AI are badged (§3) → done: `ask` / `decide` steps call a brain with a chosen profile, metered toward cost limits; routines with AI are badged · verified: `ai_steps_ask_a_brain_and_count_toward_its_cost`, `Routines.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-09** · Builder UI: trigger picker → done: `pages/Routines.tsx`: triggers (phrases, hotkey) → steps reordered by dragging or Move up / down → tool picker with forms generated from each tool's JSON Schema (`lib/schemaForm.ts`), say, wait, ask AI → error policy → save with the grant list → test run · verified: `Routines.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **ROUT-10** · Built-in starter routines, all disabled: Work mode, Break, Meeting, Goodnight, Focus for {minutes} (§6) → done: Work mode, Break, Meeting, Goodnight, Focus for {minutes}, all disabled, added once; without Focus / notification / default-mic APIs (DECISIONS "Starter routines without Focus", ROUTINES §6 updated) · verified: `starters_are_added_once_and_hotkeys_run_routines` (2026-09-23) <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (2)

- [x] **SEC-03** · Bypass permissions: explicit opt-in dialog (optional Windows Hello), auto-expiry (15 min / 1 h default / until off), red BYPASS chip in the Island and tray, every action audited, unavailable to guests and remote clients (§1.1) → done: Permissions → Bypass: a dialog with 15 min / 1 h / until off and optional Windows Hello; expires on its own; never stored; red tray ring and BYPASS chip; audited; owners only · verified: `bypass_is_switched_on_only_through_its_dialog`, `bypass_expires_on_its_own`, `the_icon_and_tooltip_follow_the_state`, `hard_limits_hold_in_every_mode_including_bypass` (2026-09-23) <!-- synced:x -->
- [x] **SEC-09** · Task grants: a background task cannot exceed the permissions granted at creation (§2) → done: a task runs only its creation grants (exact arguments) in every mode, `NotGranted` otherwise · verified: `tasks_keep_to_their_grants_and_run_independent_steps_together`, `routines_run_only_what_was_granted`, policy tests (2026-09-23) <!-- synced:x -->

#### [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) (2)

- [x] **TOOL-28** · Event-driven watchers: download finished, folder changed (`notify`), process exited / build finished, window/app state (UIA/WinEvent), time/reminders (persisted scheduler; missed runs reported on start) (§6) → done: `watchers.rs`: download finished and folder changed (`notify`), process exit / build finished (kernel waits), window opened/closed (UIA events), time (stored, missed ones reported on start) · verified: `watchers.rs` tests, `a_process_is_listed_and_its_exit_is_waited_for`, `the_build_watcher_waits_without_ai_and_tells_you` (2026-09-23) <!-- synced:x -->
- [x] **TOOL-29** · Every task has an owner, permissions granted at creation, history, status and cancellation; watchers use no LLM while waiting (§6) → done: every task has an owner, its grants, step history, status and cancel; watchers make no brain calls while waiting · verified: `the_build_watcher_waits_without_ai_and_tells_you` (zero brain requests), `Tasks.test.tsx` (2026-09-23) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (9)

- [x] **UX-58** · Notification reply field ("Task finished … Want me to commit?" → done: task toasts carry "Reply to KIVO" and Send; a reply is a request to KIVO; the taskbar jump list (Routines, New conversation, Pause listening, Stop everything) via `ICustomDestinationList` and `--action` · verified: `a_reply_field_feeds_the_send_button`, `task_toasts_offer_a_reply`, `a_toast_reply_is_a_request`, jump list action test; real toasts are never popped by tests (2026-09-23) <!-- synced:x -->
- [x] **UX-15** · Live activities (media, timer, download, agent progress), each source configurable, off in fullscreen (§2) → done: the collapsed Island shows timers (countdown), downloads, agent progress and, after a media request, the track; each source has a switch in Settings; hidden over fullscreen apps, games and presentations · verified: `a_media_command_shows_the_track_for_a_moment`, Island activity test, `activities_hide_over_fullscreen_apps_games_and_presentations` (2026-09-23) <!-- synced:x -->
- [x] **UX-24** · Tasks (also feeding the tray tooltip's running-task count, UX-56, and Home's Running list, UX-19): running and past tasks with status, current step, elapsed time, tool activity, cancel, result (mockup → done: `pages/Tasks.tsx`: running and finished tasks with status, current step, elapsed time, each step, a task's question, pause, resume, stop, stop all, result; the tray tooltip counts running tasks; Home lists them · verified: `Tasks.test.tsx`, `the_icon_and_tooltip_follow_the_state` (2026-09-23) <!-- synced:x -->
- [x] **UX-25** · Agents page: CLI agents, desktop AI apps, sessions (mockup → done: `pages/Agents.tsx`: sessions (Open in terminal), CLI agents with Start (folder and mode), desktop AI apps, safety notes, and the Workspaces tab · verified: `Agents.test.tsx`, `the_agents_page_requests` (2026-09-23) <!-- synced:x -->
- [x] **UX-26** · Routines page and builder (mockup → done: `pages/Routines.tsx`: the list (triggers, AI badge, switch, Run, Edit), templates from the starters, and the builder (ROUT-09) · verified: `Routines.test.tsx` (2026-09-23) <!-- synced:x -->
- [x] **UX-40** · Proactive rules: speech/toast/queue by situation (active, call, fullscreen, away, quiet hours, urgent), grouped to ≤ 1 spoken interruption per 10 min, "what did I miss?", per-source speak/toast/silent (§7) → done: `notifier.rs`: speak / toast / queue by situation (active, call, fullscreen, away, quiet hours, urgent), at most one spoken interruption per 10 minutes, "what did I miss?", per-source speak / toast / silent in Settings → Notifications · verified: notifier `decide` tests, `in_a_call_news_waits_for_what_did_i_miss` (2026-09-23) <!-- synced:x -->
- [x] **UX-42** · Selection shortcut: with text selected, Explain / Rewrite / Translate (clipboard or UIA TextPattern) (§8) → done: Ctrl+Shift+Space reads the selection first (UIA TextPattern, clipboard fallback) and the Island offers Explain · Rewrite · Translate; the text goes to the brain as untrusted content · verified: `the_selection_shortcut_reads_the_selection_first` (2026-09-23) <!-- synced:x -->
- [x] **UX-44** · "What can I say?" / "help" / F1: 3–5 examples for the foreground app from the App Capability Registry (§8.1) → done: "What can I say?" / "help" / F1 in the Island: examples for the app in front from the app registry, then general ones · verified: `the_islands_buttons_work_by_voice`, Island help test (2026-09-23) <!-- synced:x -->
- [x] **UX-55** · Voice-only use: every overlay action can be triggered by voice (§10) → done: every Island button has words: allow / deny / wait (decisions), send / edit (drafts), yes / no (offers), undo, stop, "try again", "that's not what I meant", "turn it on", "open my tasks" · verified: `the_islands_buttons_work_by_voice`, decision and draft voice tests (2026-09-23) <!-- synced:x -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (3)

- [x] **PLAN-03** · Cancellation also propagates from task timeouts, provider failures and application shutdown (§43) → done: a task's timeout, a provider failure and shutdown cancel through the task's token (and KIVO's) · verified: `timeouts_pauses_and_checks`, `a_failing_provider_is_reported_plainly_and_kivo_carries_on` (2026-09-23) <!-- synced:x -->
- [x] **PLAN-20** · Acceptance journey §137 (coding project: open, find failing test, fix, validate, summarize, speak) → done: "fix the failing test" runs the Coding agent in the project, shows its steps, re-runs the tests itself, has the agent retry once, summarizes and says it's fixed only when they pass · verified: `a_coding_request_is_fixed_checked_and_reported` (with a scripted ACP agent; a live Claude Code run needs its adapter, M3-open) (2026-09-23) <!-- synced:x -->
- [x] **PLAN-22** · Acceptance journey §140 (build watcher: no LLM while waiting, notify on completion) → done: "tell me when the build finishes" finds the build, waits on the process with no brain call, and tells the user · verified: `the_build_watcher_waits_without_ai_and_tells_you` (2026-09-23) <!-- synced:x -->
<!-- /items -->

---

<a id="m6"></a>

## M6: MCP, connectors and skills

**Goal:** third-party tools through MCP with per-server permissions, the KIVO MCP server for
agents (including memory), connectors that need no user keys, and Agent Skills.

**Depends on:** M4 (permissions, tools), M5 (agents).

**Read first:** [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md), [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md), [DISCOVERY.md](architecture/DISCOVERY.md), [CONVERSATION.md](architecture/CONVERSATION.md), [SECURITY.md](architecture/SECURITY.md), [UX.md](architecture/UX.md).

**Build order:**

1. **MCP client.** `rmcp` transports (TOOL-34), tool mapping and risk (TOOL-35), untrusted
   descriptions and change detection (TOOL-36).
2. **KIVO MCP server.** Selected tools for agents (TOOL-37), passed to ACP sessions (BRAIN-16),
   memory tools (CONV-24).
3. **Imports and detection.** Connector detection (DISC-08), MCP config import (DISC-09), skills
   detection (DISC-10), file watchers and `tools/list_changed` (DISC-16).
4. **Connectors.** The connector shape (INT-02), the Connectors page (INT-03), remote MCP with
   OAuth/DCR/CIMD and custom URLs (INT-04).
5. **Skills** (CONV-32).
6. **Screen.** The Extensions page with its four tabs (UX-27).
7. **Security.** The hostile MCP server fixture tests (TOOL-41).

**Deliverable:** MCP servers configured in Claude Desktop appear under "Found on this PC" and import
with one click; Claude Code launched by KIVO can search the user's KIVO memory; the GitHub remote
MCP connects through a browser sign-in with no key.

**How to verify:** the hostile-MCP fixtures; a description change is flagged; imported secrets
land in Credential Manager, not in config.

**Exit criteria:**

- [x] **M6-X1** · The hostile-MCP fixture tests pass (TOOL-41). → `a_hostile_server_is_contained` (poisoned description, a tool named `shell.run`, flood, slow call, injection, password bait, bad schema) and the rug pull in `a_description_changed_after_approval_is_switched_off` pass (2026-09-23)
- [x] **M6-X2** · The original MCP config files of other apps are never modified by an import (tested). → `an_imported_server_is_reviewed_then_used_by_a_brain` compares the Claude Desktop config byte for byte before and after the import (2026-09-23)

### M6 items

<!-- items:M6 -->
**16 items** · ✅ 16 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [BRAINS.md](architecture/BRAINS.md) (1)

- [x] **BRAIN-16** · The KIVO MCP server is passed in each ACP session's MCP server list (§4) → done: `Agents::set_kivo_server` adds `kivo-runtime --mcp-server --agent <id>` to each ACP `session/new` mcpServers list for agents the user allowed (`apps/kivo-runtime/src/agents.rs`) · verified: `m6_extensions::kivos_mcp_server_shares_memory_only_with_allowed_agents` drives the server over MCP; `acp_tests::a_new_session_carries_kivos_mcp_server` checks the entry reaches the agent `session/new` <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (2)

- [x] **CONV-24** · Memory MCP tools `memory.search`, `memory.read`/`memory.get`, `memory.write_note`/`memory.add` (permission-gated), `memory.tags`; each agent allowed first; sensitive memories never shared with cloud agents unless allowed (§6) → done: `memory.search`, `memory.get`, `memory.add` (asks, Medium) and `memory.tags` (`apps/kivo-runtime/src/memory_tools.rs`) over preferences, notes and conversations (the M7 vault replaces the store behind them); shared per agent from Agents → Memory; sensitive items (personal, About me, conversations) filtered unless allowed · verified: `m6_extensions::kivos_mcp_server_shares_memory_only_with_allowed_agents` (search, add asks, sensitive filtered, unshared agent sees nothing) and memory_tools unit tests <!-- synced:x -->
- [x] **CONV-32** · Agent Skills (`SKILL.md` folders) from `%APPDATA%\KIVO\skills\`, import from folder/zip; only name + description in context, body loaded on use; scripts run through the shell tool under the permission engine; external skills reviewed before enabling (§9) → done: `apps/kivo-runtime/src/skills.rs`: SKILL.md folders in `%APPDATA%\KIVO\skills\`, import from folder or zip (zip-slip checked), only name + description in the prompt, `skills.load` loads the body, scripts run through `shell.run`; external skills off until reviewed · verified: `m6_extensions::skills_are_found_reviewed_and_offered_to_brains`, skills unit tests, `Extensions.test.tsx` <!-- synced:x -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (4)

- [x] **DISC-08** · Connectors detector: signed-in CLIs (`gh auth status`), installed apps with built-in connectors, browsers and whether the KIVO extension is installed (§1.2) → done: `apps/kivo-runtime/src/connectors.rs` detects `gh auth status`, installed apps with connectors (VS Code, Spotify) and the KIVO browser extension, shown under Ready to use · verified: `m6_extensions::connectors_sign_in_and_local_ones_are_found` with a fake `gh`, `Extensions.test.tsx` <!-- synced:x -->
- [x] **DISC-09** · MCP config import from Claude Desktop (incl. the Store path), Claude Code, Cursor, VS Code, Codex and Gemini CLI: copied into KIVO's config, originals never modified, env-var secrets moved to Credential Manager (§1.2) → done: `crates/kivo-mcp/src/imports.rs` reads Claude Desktop (incl. the Store path), Claude Code (incl. projects), Cursor, VS Code (JSONC), Codex (TOML) and Gemini CLI; copied into KIVO, secrets to Credential Manager · verified: imports unit tests and `m6_extensions::an_imported_server_is_reviewed_then_used_by_a_brain`, which checks the original file is byte-for-byte unchanged (M6-X2) <!-- synced:x -->
- [x] **DISC-10** · Skills detector for `~/.claude/skills/`, project `.claude/skills/` and KIVO's `skills\`; referenced in place unless the user chooses Copy; review before enabling (§1.2) → done: skills detector for `~/.claude/skills/`, project `.claude/skills/` and KIVO's folder, referenced in place, Copy on import; review before enabling · verified: `m6_extensions::skills_are_found_reviewed_and_offered_to_brains`, `Extensions.test.tsx` <!-- synced:x -->
- [x] **DISC-16** · File watchers on other apps' MCP configs and skills folders; MCP `tools/list_changed` handling with description-hash comparison (§3) → done: `notify` watchers on the folders of other apps' MCP config files (so a setup made later is seen) and the skills folders (`Mcp::watch`, `Skills::watch`) raise `DiscoveryChanged`; `tools/list_changed` re-lists and compares description hashes · verified: `m6_extensions::a_description_changed_after_approval_is_switched_off` (rug pull on the hostile fixture) and `m6_extensions::other_apps_setups_and_skills_are_watched` <!-- synced:x -->

#### [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) (3)

- [x] **INT-02** · `Connector` shape: namespaced ToolSpecs, auth flow, health checks and a scope list shown to the user; covered by capability toggles and the permission engine (§1) → done: connectors (`crates/kivo-mcp/src/connectors.rs` + `connectors.toml`) run as MCP servers whose tools are namespaced ToolSpecs; OAuth sign-in, a health state per server and the data-access line shown; switched-off local connectors are denied in `control.act` · verified: `m6_extensions::connectors_sign_in_and_local_ones_are_found`, connectors unit tests <!-- synced:x -->
- [x] **INT-03** · Extensions → Connectors page: directory (logo, description, tools, data access, badges), Connect → system-browser sign-in, connected account, per-tool toggles and risk, Disconnect, last used (§0) → done: Extensions → Connectors (`pages/Extensions.tsx`): directory with description, access and badges, Connect → system-browser sign-in, connected with tools count and last used (audit log), per-tool toggles and risk in the review dialog, Disconnect · verified: `Extensions.test.tsx`, `m6_extensions::connectors_sign_in_and_local_ones_are_found` <!-- synced:x -->
- [x] **INT-04** · Remote MCP connectors with MCP authorization (OAuth 2.1 + PKCE, DCR or a KIVO Client ID Metadata Document) and "Custom connector": paste a remote MCP URL (§0) → done: remote MCP connectors with OAuth 2.1 + PKCE and dynamic client registration on a loopback redirect, tokens in Credential Manager (`crates/kivo-mcp/src/auth.rs`); Add custom takes a remote MCP URL · verified: `kivo-mcp/tests/signin.rs` against the local OAuth fixture (`testenv/mcp --http`) and `m6_extensions::connectors_sign_in_and_local_ones_are_found`; not yet run against a live publisher (needs the owner's accounts) <!-- synced:x -->

#### [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) (5)

- [x] **TOOL-34** · `rmcp` client with stdio and Streamable HTTP transports (§9) → done: `crates/kivo-mcp/src/client.rs` on `rmcp` 3.4 with stdio and Streamable HTTP transports · verified: `kivo-mcp/tests/servers.rs` (stdio fixture) and `tests/signin.rs` (HTTP) <!-- synced:x -->
- [x] **TOOL-35** · MCP tools become `mcp.<server>.<tool>` ToolSpecs, default Medium risk (user can lower per tool), `data_egress` for remote servers (§9) → done: `mcp.<server>.<tool>` ToolSpecs (`crates/kivo-mcp/src/tool.rs`), default Medium, lowerable per tool, `data_egress` for remote servers · verified: kivo-mcp unit tests, `m6_extensions::an_imported_server_is_reviewed_then_used_by_a_brain` <!-- synced:x -->
- [x] **TOOL-36** · Tool descriptions treated as untrusted: shown on install, description hashes compared and changes flagged (§9) → done: descriptions shown for review and labelled as the server's; approval by SHA-256 of name, description and schema (`hash.rs`); a changed tool is switched off until reviewed again · verified: `m6_extensions::a_description_changed_after_approval_is_switched_off`, hash unit tests, `Extensions.test.tsx` <!-- synced:x -->
- [x] **TOOL-37** · KIVO MCP server exposing selected KIVO tools to CLI agents, all calls through the permission engine (§9) → done: `kivo-runtime --mcp-server --agent <id>` (`mcp_bridge.rs`, `crates/kivo-mcp/src/server.rs`) relays to `Engine::shared_call` with initiator Mcp through the permission engine · verified: `m6_extensions::kivos_mcp_server_shares_memory_only_with_allowed_agents`, args test <!-- synced:x -->
- [x] **TOOL-41** · Hostile MCP server fixture tests (§10) → done: `testenv/mcp --hostile`: poisoned description, a tool named `shell.run`, a 500k flood, a 10-minute call, injected instructions, a failure asking for a password, a bad schema and a rug pull · verified: `kivo-mcp/tests/servers.rs::a_hostile_server_is_contained` and `m6_extensions::a_description_changed_after_approval_is_switched_off` (M6-X1) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (1)

- [x] **UX-27** · Extensions page tabs Connectors · MCP servers · Plugins · Skills, each: one-line description + actions, grouped lists with counts, found-on-this-PC sections with Refresh (mockup → Extensions; DISCOVERY) → done: `pages/Extensions.tsx` with Connectors · MCP servers · Plugins · Skills tabs, descriptions, grouped lists with counts, found-in-other-apps and Refresh, review dialogs, add server and custom connector · verified: `Extensions.test.tsx` (6 tests) <!-- synced:x -->
<!-- /items -->

---

<a id="m7"></a>

## M7: Control Center and memory (MVP complete)

**Goal:** every Control Center screen matching the mockup with real data, the memory vault,
privacy modes, and the rest of onboarding. This is the **internal alpha**.

**Depends on:** M1–M6.

**Read first:** [UX.md](architecture/UX.md), [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md), [mockups/kivo-app.html](design/mockups/kivo-app.html), [CONVERSATION.md](architecture/CONVERSATION.md), [MEMORY.md](architecture/MEMORY.md), [SECURITY.md](architecture/SECURITY.md), [BRAINS.md](architecture/BRAINS.md), plan §71, §139, §156.

**Build order:**

1. **Memory.** Tables and FTS (MEM-04), remember/proposals (MEM-05), privacy and guest rules
   (MEM-07), retrieval (MEM-08), "Why did you say that?" (MEM-10); the graph (CONV-17), the
   Markdown vault (CONV-18), capture modes (CONV-19), suggestions (CONV-20).
2. **Privacy.** Data classes (SEC-20), privacy modes (SEC-21), the audit view (SEC-24).
3. **Screens.** Navigation complete (UX-18); Memory (UX-29, MEM-06); Usage (UX-30, BRAIN-37);
   Settings with every tab and default (UX-31, UX-37); Chat power features (CONV-03); Settings →
   Context (CONV-31); the command palette with real commands (UX-47); Mica (UX-32); every screen
   matching the mockup (DS-15).
4. **Onboarding.** Steps 7–11 (UX-35) and recommendations (UX-36).
5. **Polish and access.** Custom accent and transparency (DS-03), the accessibility pass (DS-16,
   UX-54), Playwright UI tests (ARCH-37).
6. **Acceptance.** Offline mode end to end (PLAN-04), the private-document journey (PLAN-21), the
   MVP definition (PLAN-27).

**Deliverable:** a new user installs KIVO, finishes onboarding without technical knowledge, and
uses every MVP feature from the plan's §156 list.

**How to verify:** Playwright runs through every screen; the offline journey with the network off;
a first-run test with someone who hasn't seen KIVO.

**Exit criteria:**

- [x] **M7-X1** · The plan §156 MVP definition is fully met (PLAN-27) → **internal alpha**. → every §156 item is built and tested (PLAN-27); the alpha tag is the owner's (2026-09-24)
- [x] **M7-X2** · The local brain path is verified (Private/Offline profiles, PLAN-21). → `m7_acceptance` (confidential document on the local brain, cloud blocked; offline routing to the local brain) (2026-09-24)
- [ ] **M7-X3** · Every Control Center screen matches the mockup in light and dark (owner review).

### M7 items

<!-- items:M7 -->
**33 items** · ✅ 32 done · 🟡 1 partial · ⏭ 0 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (1)

- [x] **ARCH-37** · Playwright UI tests against the Tauri dev build where practical (§7) → done: Playwright (`@playwright/test`, no browser download) drives the dev app's own WebView2 over CDP: `pnpm dev:e2e` starts `pnpm dev` with a debugging port on 127.0.0.1, then `pnpm --filter kivo-app e2e` runs `apps/kivo-app/e2e/app.e2e.ts` (every page and Settings tab, the Ctrl+K palette opening a setting, no console errors; read-only, so it never changes the owner's settings) · verified: 4 passed against the running dev build (2026-09-24) <!-- synced:x -->

#### [BRAINS.md](architecture/BRAINS.md) (1)

- [x] **BRAIN-37** · Usage page: charts by day, provider, feature and routine; top expensive tasks; CSV export; a live "≈ $0.12" in the card when enabled (§9) → done: Usage page with today/period totals, charts by day (AI and speech), by provider, feature and routine, the most expensive tasks and turns, CSV export saved to Downloads (`usage.export` with `save`), and the card's live cost (`brains.show-cost`) · verified: `Usage.test.tsx` (3), `brain_turns::the_control_centers_brain_requests` usage and export asserts (2026-09-24) <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (6)

- [x] **CONV-03** · Chat power features: rename, pin, continue, branch, export, delete, context meter, Compact now (§0) → done: the thread menu has rename, pin, Continue by voice (`chat.continue`: the next voice request joins the thread), Branch and "Branch from here" on an answer (`chat.branch`, the summary carried when every message it covers is), Export as Markdown or JSON to Downloads (`chat.export`), Compact now and delete; the context meter shows between turns from `brains.context` · verified: `Chat.test.tsx` power features, `brain_turns::follow_ups_continue_the_thread_and_new_topic_starts_another` (continue), `the_control_centers_brain_requests` (branch, export, continue), `kivo-store` `a_branch_copies_up_to_a_message_and_keeps_the_summary` (2026-09-24) <!-- synced:x -->
- [x] **CONV-17** · Knowledge graph tables `entities`, `relations`, `observations` with `valid_from` / `valid_to` (§6) → done: migration 10 adds `entities`, `relations` and `observations` (with `valid_from` / `valid_to`), filled from each note's people, `[[links]]` and facts when the vault is indexed (`kivo-memory` graph, `kivo_store::memory`); a superseded fact gets its `valid_to` · verified: `kivo-store` memory tests, `kivo-memory` graph tests, `m7_memory::the_memory_page` backlinks (2026-09-24) <!-- synced:x -->
- [x] **CONV-18** · Obsidian-compatible Markdown vault in `%APPDATA%\KIVO\memory\` (front-matter, wikilinks, `people/`, `workspaces/`, `topics/`, `.kivo/` index); SQLite is the index rebuilt from the files, and user edits win (§6) → done: `crates/kivo-memory`: Markdown notes with front-matter (unknown keys kept), wikilinks and tags in `%APPDATA%\KIVO\memory\` (`people/`, `workspaces/`, `topics/`, `.kivo/` for temp files); SQLite is only the index, rebuilt from the files by hash, and a note edited outside KIVO wins (watched) · verified: 19 `kivo-memory` tests, `m7_memory` (3), runtime `memory` tests (2026-09-24) <!-- synced:x -->
- [x] **CONV-19** · Capture modes Only when I ask / Suggest (on) / Workspace notes (on), each switchable; memory only from conversations and tasks KIVO took part in (§6) → done: `memory.capture` Only when I ask / Suggest (default) and `memory.workspace-notes` (on), each a switch on the Memory page; memories come only from turns and tasks KIVO ran (`remember`, compaction, task end), never from other apps' content · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, `compaction_suggests_and_the_island_asks`, runtime `memory` tests (2026-09-24) <!-- synced:x -->
- [x] **CONV-20** · Suggest: memory candidates at compaction or after tasks, accepted, edited or dismissed by the user (§2, §6) → done: compaction's `REMEMBER:` lines and finished tasks become suggestions (`memory_suggestions`), offered on the Island ("Want me to remember …?") and on the Memory page to accept, edit or dismiss · verified: `m7_memory::compaction_suggests_and_the_island_asks`, `Memory.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **CONV-31** · Settings → Context: per-layer toggles and edit links, per-profile budget, auto-compaction threshold, "Start each conversation fresh", estimated cost per session, "Preview what the AI sees" with secrets redacted (§8) → done: Brains → Context (`components/brains/ContextSettings.tsx`): each layer's size with a switch and an Edit link (About me, workspace, memories, skills; `[context]` in `kivo.toml`), the live fields sent, auto-compaction and its threshold (60–100 %), "Start each conversation fresh", the budget, an estimated cost of a new conversation, and "Preview what the AI sees" (the next request built without side effects, secrets redacted) · verified: `brain_turns::the_control_centers_brain_requests` (preview, toggles, live fields), `follow_ups…` (fresh start), `engine::brain` `compaction_follows_the_context_settings`, `Brains.test.tsx` Context (2026-09-24) <!-- synced:x -->

#### [MEMORY.md](architecture/MEMORY.md) (6)

- [x] **MEM-04** · `memories` + `memories_fts` (FTS5) with `text, scope, sensitivity, source, created_at, last_used_at, use_count` (§1, §2) → done: migration 10 `memories` + `memories_fts` (FTS5, triggers) with text, scope, sensitivity, source, created_at, last_used_at, use_count, indexed from the vault · verified: `kivo-store` memory tests (search by bm25, use counts) (2026-09-24) <!-- synced:x -->
- [x] **MEM-05** · "Remember …" and the card's "Remember this" button create memories; inline proposals ("Want me to remember …?") (§2) → done: "Remember …" by voice or typed (grammar `memory.add`, exact words kept), the Island's "Remember this" on an answer (`memory.rememberTurn`) and inline proposals ("Want me to remember …?") · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, `turn.test.tsx` Remember this (2026-09-24) <!-- synced:x -->
- [x] **MEM-06** · Memory page: list, search, edit, delete, "forget everything", export to JSON (§2) → done: Memory page: the vault by tags and folders, search, edit (the Markdown), delete, "Forget everything" (confirmed), export to JSON or Markdown in Downloads · verified: `Memory.test.tsx` (4), `m7_memory::the_memory_page` (2026-09-24) <!-- synced:x -->
- [x] **MEM-07** · `sensitive` and above never leave the device unless the privacy mode and the per-memory flag both allow it; guest sessions read and write no memory (§2) → done: a memory marked sensitive (or found so by the detectors) is recalled for a cloud brain only when both the privacy mode and `memory.sensitive-to-cloud` allow it; credentials are never stored; guests get no memory tool at all (policy) and no recall · verified: runtime `memory` tests (recall for cloud vs local), `kivo-security` `guests_get_no_memory` (2026-09-24) <!-- synced:x -->
- [x] **MEM-08** · Retrieval: FTS5 + scope filter, top memories injected as `System` provenance marked "user-provided memory", ≤ 400 tokens per request (§3) → done: `Memory::recall`: FTS5 with scope (app, project, workspace) filtering, ≤ 150 tokens per item and 400 per request, injected as `System` provenance "user-provided memory" (MEMORY_TOKENS = 400) · verified: runtime `memory` recall tests, `m7_memory::remembering_by_voice_then_answers_use_it` (the request carries it) (2026-09-24) <!-- synced:x -->
- [x] **MEM-10** · "Why did you say that?" in Activity shows which memories were used (§3) → done: Activity's "Why?" on an answer lists the memories that turn used (`turn_memories`, `memory.why`), including ones since deleted · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, Activity popover (2026-09-24) <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (3)

- [x] **SEC-20** · Data classes `public … highly_sensitive`, classified by source, local detectors (keys, card numbers, ID patterns) and user labels (§6) → done: data classes public … highly_sensitive from the local detectors (keys, cards with Luhn, IDs, contacts), by source (a folder under Privacy → "Keep on this PC" is sensitive; password fields are never read) and user labels (Privacy → Private words: a word gives text its class); a sensitive request routes to a local brain, and a private tool result is never shown to a cloud brain (egress check, logged in Activity) · verified: `classify` tests (`labels_and_private_folders_classify_by_source`), `brain_turns::private_folders_and_labels_stay_on_this_pc`, `Permissions.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **SEC-21** · Privacy modes Cloud / Local / Strict Private / Custom enforced in the router and egress checks (cloud STT/TTS included); privacy overrides failover (§6) → done: Cloud / Local / Strict Private / Custom enforced in the router (`privacy::cloud_brains`, `cloud_limit`), cloud STT/TTS egress (`speech_egress`, Custom has its own switch), screenshots (`cloud_vision`) and the tool-result egress check; Custom shows four switches (cloud brains, personal details to cloud, cloud speech, cloud screen); privacy wins over failover (same-privacy only, BRAIN-22) · verified: `privacy` tests (`custom_turns_each_kind_of_cloud_on_by_itself`), `brain_turns` privacy and failover tests, `Permissions.test.tsx` Custom (2026-09-24) <!-- synced:x -->
- [x] **SEC-24** · Activity → Audit view in the Control Center (§7) → done: Activity → Audit lists every permission decision (tool, arguments, risk, allow/deny, who confirmed, error) from the audit log · verified: `Activity.test.tsx` (2026-09-24) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (10)

- [x] **UX-18** · Navigation: 13 items in the §3 groups with in-page tabs, plus Settings (§3) → done: the sidebar's 13 items in the §3 groups plus Settings, each page with its in-page tabs (Brains/Context, Extensions, Permissions, Settings), all opening from the palette too · verified: the e2e walk of every page (`e2e/app.e2e.ts`), page tests (2026-09-24) <!-- synced:x -->
- [x] **UX-29** · Memory page: vault by tags and folders, suggestions, instructions, Open folder / Open in Obsidian (mockup → Memory; CONVERSATION §6) → done: Memory page: vault by tags and folders, note preview with wikilinks and backlinks, suggestions, instructions (About me, workspaces), Open folder / Open in Obsidian (`obsidian://`) · verified: `Memory.test.tsx` (4), `m7_memory::the_memory_page` (2026-09-24) <!-- synced:x -->
- [x] **UX-30** · Usage page (BRAIN-37) (mockup → Usage) → done: the Usage page (BRAIN-37) · verified: `Usage.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **UX-31** · Settings tabs General · Appearance · Island · Sounds · Notifications · Accessibility · Shortcuts · Performance · Diagnostics · About (with every model's and library's licence and attribution, DIST-13), with the §5 defaults (mockup → Settings) → done: Settings with ten tabs (General, Appearance, Island, Sounds, Notifications, Accessibility, Shortcuts, Performance, Diagnostics, About with every model's and library's licence from `pnpm licenses:gen`), each with the §5 defaults; export/import/reset of the settings file · verified: `Settings.test.tsx` (14), `m7_settings` (3), the e2e tab walk (2026-09-24) <!-- synced:x -->
- [x] **UX-32** · Mica on the Control Center (Windows 11), solid on Windows 10 (§3) → done: `window_effects` applies Mica when transparency is on and Windows is build 22000 or later; Windows 10 stays solid; the page backgrounds let it through (`data-mica`) · verified: `effects::mica_only_on_windows_11_and_only_when_wanted`, the dev app on Windows 11 (2026-09-24) <!-- synced:x -->
- [x] **UX-35** · Steps 7–11: Connect your apps and tools, Permission mode, Look & feel, Startup, Try it → Finish opens the Control Center; recommended choices preselected, optional steps skippable (§4) → done: setup steps 7–11: Apps & tools (ready now, one sign-in, the browser extension, MCP and skills for after setup), Permission mode (Auto recommended, no Bypass), Look & feel (theme, accent, Island position, chimes on a live Island), Startup (with Windows, keep running, what suits this PC) and Try it (a request played on the Island); Finish opens the Control Center or the page asked for; optional steps skippable · verified: `Onboarding.test.tsx` (all eleven steps) (2026-09-24) <!-- synced:x -->
- [x] **UX-36** · Recommendation engine: defaults chosen from hardware, installed software, GPU, RAM, network and providers; the user can override (§4, plan §130) → done: `setup.recommend` (`runtime/setup.rs`): from RAM, the GPU, battery, the network (`SystemInfo::network`: offline, metered), CLI agents and local servers found, and brains connected, it preselects the brain, the permission mode, the performance profile and the privacy mode, each with its reason; everything stays changeable on the same screen · verified: `setup` tests (3), `system::tests::tells_whether_it_is_online`, `Onboarding.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **UX-37** · Every §5 control exists with its listed default, persisted in `kivo.toml` through the runtime (§5) → done: every §5 control is in the UI with its default and saved to `kivo.toml` through the runtime: General (with low-memory mode, which unloads speech models after each request), Voice page, Island tab, Sounds (per cue), Brains, Permissions (hotkeys in Shortcuts, with press-once and stop-on-pause), Privacy (with data classes), Memory, Performance, Accessibility; the overlay's wake glow is M8 (DS-12) · verified: `Settings.test.tsx`, `models::low_memory_mode_unloads_speech_models_after_use`, `m7_settings` (2026-09-24) <!-- synced:x -->
- [x] **UX-47** · Ctrl+K command palette: search every setting, run routines, trigger actions, ask KIVO; black Island material, top center (§8.1) → done: Ctrl+K palette on the Island's black material: pages, every setting (`pages/settings/index.ts`), routines to run, actions (remember, new conversation, add wake word, switch mode, tidy memory, export settings, diagnostics) and "Ask KIVO" for anything else · verified: `CommandPalette.test.tsx` (6), the e2e palette test (2026-09-24) <!-- synced:x -->
- [x] **UX-54** · Contrast ≥ 4.5:1, never color alone, Windows high-contrast respected; warn if overlay and sounds are both off (§5, §10) → done: text contrast ≥ 4.5:1 in both themes and every accent (DS-16), state always with text or an icon, Windows contrast themes followed (forced-colors styles; the Accessibility tab shows it), and a warning when the Island is hidden and every sound is off · verified: `contrast.test.ts`, `Settings.test.tsx` (the silent warning) (2026-09-24) <!-- synced:x -->

#### [DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) (3)

- [x] **DS-03** · Custom accent color (the 8th swatch in Appearance) and the transparency-effects setting (§2, DECISIONS "Appearance settings") → done: Appearance has the seven presets plus a custom colour (the 8th swatch opens a colour picker; its text-safe variant is computed, `lib/contrast.ts`) and the transparency-effects switch (Mica on Windows 11) · verified: `Settings.test.tsx` Appearance, `Pickers.test.tsx`, `contrast.test.ts` custom accents (2026-09-24) <!-- synced:x -->
- [~] **DS-15** · Every Control Center screen matches the mockup, rebuilt from these components (per-page items are UX-19 to UX-31) (§1) → partial: every Control Center page and Settings tab is built from the design-system components and spot-checked in the dev app against the mockup (Brains → Context, Privacy, Appearance, Chat screenshots over CDP; fixes: tab overflow fade, theme-tile radio, empty-list lines) · missing: the full side-by-side review with the owner, light and dark (2026-09-24) <!-- synced:~ -->
- [x] **DS-16** · Accessibility pass: contrast ≥ 4.5:1 in both themes and all accents, high-contrast mode, focus order (§6) → done: `--text-3` darkened and text-safe tokens added (`--acc-text`, `--green/orange/red-text`, `--on-acc`) so all text is ≥ 4.5:1 on every surface in both themes and all accents (controls ≥ 3:1; light Amber moved to `#BF7C00`); Windows contrast themes get system colours, visible edges and Highlight outlines (`@media (forced-colors: active)`); focus order follows the DOM, checked by axe · verified: `src/lib/contrast.test.ts` (6, reads `tokens.css`), the axe audits in the page tests (2026-09-24) <!-- synced:x -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (3)

- [x] **PLAN-04** · Offline mode verified end to end: wake, STT, TTS, native commands, files, UIA, local tools, local brain and local automation all work with the network off (§71) → done: with every cloud brain failing on the network KIVO knows it's offline and routes to the local brain ("because this PC is offline"); wake, STT, TTS, native commands, files, a local brain and a routine all run; UIA runs with no network by design (its own COM thread) · verified: `m7_acceptance::everything_local_works_with_the_network_off` (the real keyword spotter and Moonshine when installed), `uia_testenv` (2026-09-24) <!-- synced:x -->
- [x] **PLAN-21** · Acceptance journey §139 (confidential document summarized with a local model, cloud blocked) → done: "Summarize this confidential document locally" is seen as a privacy requirement (`privacy::asks_to_stay_local`, the language's word list), the cloud is blocked for it, the local brain reads the file on this PC and answers · verified: `m7_acceptance::a_confidential_document_is_summarized_on_this_pc`, `kivo-security` `a_request_can_ask_to_stay_on_this_pc` (2026-09-24) <!-- synced:x -->
- [x] **PLAN-27** · MVP definition (§156) fully met → internal alpha → done: every §156 item exists and is tested: Windows runtime, wake word, push-to-talk, Moonshine STT, Kokoro/Windows TTS, cloud brains, a local brain, CLI agents over ACP, native commands, app launch, files, browser launch, UI Automation, permissions, Control Center, logging, cancellation, the Island companion; internal alpha is the owner's first `v*` tag · verified: the M1–M7 journeys (`spoken_command`, `brain_turns`, `control_turns`, `m5_journeys`, `m6_extensions`, `m7_*`) (2026-09-24) <!-- synced:x -->
<!-- /items -->

---

<a id="m8"></a>

## M8: Beta hardening

**Goal:** everything the beta definition requires: more engines and providers, computer use,
companion styles, full routines, OAuth connectors, performance profiles and diagnostics.

**Depends on:** M7.

**Read first:** [VOICE.md](architecture/VOICE.md), [BRAINS.md](architecture/BRAINS.md), [CAPABILITIES.md](architecture/CAPABILITIES.md), [ROUTINES.md](architecture/ROUTINES.md), [CONVERSATION.md](architecture/CONVERSATION.md), [MEMORY.md](architecture/MEMORY.md), [DISCOVERY.md](architecture/DISCOVERY.md), [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md), [DISTRIBUTION.md](architecture/DISTRIBUTION.md), [UX.md](architecture/UX.md), [ARCHITECTURE.md](architecture/ARCHITECTURE.md), [SECURITY.md](architecture/SECURITY.md), plan §89–95, §114, §141–142, §157.

**Build order:**

1. **Voice.** More STT and TTS tiers (VOICE-10, VOICE-11), Enhance training (VOICE-17), sound sets
   and the custom motif (VOICE-28, VOICE-29), GPU unloading (VOICE-35).
2. **Brains.** More API providers (BRAIN-11), realtime conversation mode (BRAIN-33).
3. **Computer use.** Provider trait and rules (CAP-09), loop and limits (CAP-10), watch mode
   (CAP-11), experience (CAP-12), options (CAP-13).
4. **Routines.** Schedule and event triggers (ROUT-11), unattended safety (ROUT-12), creating by
   voice/history (ROUT-13), import/export (ROUT-14).
5. **Memory and AI apps.** Workspace notes (CONV-21), tidy job (CONV-22), hybrid search (CONV-23,
   MEM-09), graph view (CONV-25), skills from routines/agents (CONV-33), desktop AI apps (CONV-16).
6. **Connectors and installs.** KIVO OAuth apps (INT-05), GitHub (INT-06), token rules (INT-07),
   in-app CLI installs (DISC-07), the dependency manager (DIST-14), signed catalogs (DISC-17).
7. **Companion.** Styles (UX-38), pointing (UX-39), edge glow (UX-16), overlay style setting
   (UX-17), the §141 journey (PLAN-23).
8. **Performance and resilience.** Low-memory mode (ARCH-08), product modes (PLAN-06), the
   performance dashboard (PLAN-07, DISC-18), performance profiles (PLAN-08), GPU policy (PLAN-09),
   crash recovery (PLAN-12), diagnostics bundle and audit-chain check (ARCH-40, SEC-23).
9. **Beta definition** (PLAN-28).

**Deliverable:** a feature-complete beta build used daily by the owner.

**How to verify:** the full benchmark suite on all tiers; the end-to-end tests for provider
fallback, offline and privacy modes (plan §115); a week of daily use without a critical crash.

**Exit criteria:**

- [ ] **M8-X1** · The plan §157 beta definition is fully met (PLAN-28).
- [ ] **M8-X2** · No known critical crash after a week of daily use.

### M8 items

<!-- items:M8 -->
**45 items** · ✅ 38 done · 🟡 6 partial · ⏭ 1 dropped · ⬜ 0 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [ARCHITECTURE.md](architecture/ARCHITECTURE.md) (2)

- [x] **ARCH-08** · Low-memory mode: the runtime does not launch `kivo-app` until the overlay is first needed; M0 measures the memory difference (§1) → done: with low-memory mode on, the supervisor starts no app at sign-in or after a crash; it launches it when the Island first has something to show (listening or busy, a live activity, an offer, a selection) or shows it when the Control Center is asked for (`app::needs_overlay`, `wait_for_need`); speech models also unload after each request. Measured on the owner's PC (dev build, Control Center open): the app and its WebView2 processes hold 357 MB private (573 MB working set), which is what isn't started; the release-build figure comes with the deferred bench runs · verified: `app::low_memory_mode_starts_the_app_only_when_the_island_is_needed`, `the_island_is_needed_while_kivo_listens_or_something_waits` (2026-09-24) <!-- synced:x -->
- [x] **ARCH-40** · Diagnostics bundle: versions, OS, hardware, capabilities, provider health, recent errors and metrics; generated locally, shown to the user for review, no secrets or content (§6, plan §109) → done: Settings → Diagnostics → Create report builds the bundle (`diagnostics.rs`): KIVO version, OS version and build, hardware, settings summary, every capability, brains (kind, privacy, health, whether a key is saved), the checks, performance and latency medians, and recent warnings and errors from the logs with only level, source and message kept, repeats counted, secrets redacted twice and the user folder replaced by %USERPROFILE%; the whole report is shown in a dialog and Save writes exactly what was shown to Downloads · verified: `diagnostics::tests` (3), `m7_settings::performance_and_diagnostics_report_real_state` (no key or user folder in the bundle, saved = shown), `Settings.test.tsx` report test (2026-09-24) <!-- synced:x -->

#### [BRAINS.md](architecture/BRAINS.md) (2)

- [x] **BRAIN-11** · More API adapters: Groq, Mistral, DeepSeek, xAI and custom OpenAI-compatible endpoints (§4) → done: Groq, Mistral, DeepSeek and xAI from the catalog through the OpenAI-compatible adapter with their quirks (Mistral refuses `stream_options` with a 422, so KIVO leaves it out and reads usage from the last chunk; `reasoning_content` is never text; `max_tokens`), and custom endpoints (name + address, key tested before it's stored, a cloud brain for privacy) · verified: `contract_tests::groq_mistral_deepseek_and_xai_speak_the_compatible_dialect` (a mock that answers like Mistral's 422), `brain_turns::the_control_centers_brain_requests` (custom endpoint); no live call to these services (they need the owner's keys) (2026-09-24) <!-- synced:x -->
- [x] **BRAIN-33** · `RealtimeProvider` (OpenAI realtime, Gemini Live): the fast path runs first; tool calls go through `authorize()`; audio is held while a confirmation is pending; session resumption hides provider limits; closes after 15 s of silence, "that's all" or the budget (§8) → done: `RealtimeProvider` with OpenAI Realtime (`gpt-realtime`, PCM16 24 kHz, server VAD) and Gemini Live (native audio, session resumption and context compression) over WebSocket; a session opens only after the fast path ("let's talk about…", a profile set to Realtime, or the card's "Talk live"), with the persona's instructions and the offered tools; its own echo-cancelled microphone; tool calls through `authorize()` with the model held while a decision waits, a declined action declining only that action; connections renewed by themselves (Gemini's handle, else the conversation carried over); closes after 15 s of silence, "that's all" or the 30-minute cap; transcripts on the card and in the thread; metered as `realtime`; Capabilities → Realtime voice options (service, silence) · verified: adapter tests against local WebSocket servers (setup, audio both ways, tools, usage, refused key, Gemini goAway → resume with the handle), `a_live_conversation_runs_tools_through_the_permission_engine_and_closes_on_request`, Island and Permissions UI tests · not yet run against the live services (no keys on this PC) <!-- synced:x -->

#### [CAPABILITIES.md](architecture/CAPABILITIES.md) (5)

- [x] **CAP-09** · Computer use: `ComputerUseProvider` trait (Anthropic, OpenAI, Gemini); used only when no semantic method exists, the capability is on and the app is allowed; estimated cost shown before starting (§4) → done: `ComputerUseProvider` (Anthropic computer use, OpenAI computer-use-preview, Gemini computer use); `computer.use` is the last rung — offered only when asked for in so many words, with the capability on, cloud vision allowed and the app allowed — and its High-risk card shows the most it may cost · verified: provider contract tests on mock servers, `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (the card shows $0.25 for 25 steps) <!-- synced:x -->
- [x] **CAP-10** · Computer-use loop screenshot → proposed action → `authorize()` → execute → verify, with max 25 steps, max $0.50, 2-minute timeout (all configurable) (§4) → done: the loop — screenshot of the app's window → the model's one action → KIVO's input tool in screen pixels → the permission engine under the task's grant (that window only) → run → next screenshot; stops at 25 steps, $0.50 or 2 minutes (all settable), on Stop, or when the user takes the mouse (pause-on-mouse) · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (steps on the fake input, the step limit), engine unit tests (coordinates, estimate, watch) <!-- synced:x -->
- [x] **CAP-11** · Watch mode (default for the first 10 tasks): overlay marker at the target, approval by click, Enter, or voice for Low risk (§4) → done: watch mode for the first 10 tasks (or always / never): each step waits for Allow or Skip on the Island, by click, Enter or voice, with the target highlighted; a skipped step doesn't run and the task goes on · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (Allow, Skip, Allow) <!-- synced:x -->
- [x] **CAP-12** · Computer-use experience: Island controller live activity (step, cost, Pause, Stop), pulsing target highlight, black callout with Allow / Skip in watch mode, optional KIVO cursor, persistent "KIVO is controlling your screen — Stop (Ctrl+Alt+Shift+Esc)" banner and screen frame Off / Subtle / Full (§2, §4.1) → done: the Island's controller live activity (step, cost, Pause, Stop), the pulsing target highlight, the black watch card with Allow / Skip, the optional KIVO cursor and the "KIVO is controlling" banner with the chosen frame (glow window) · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (controller and highlight shown, gone after), Island tests, glow page test <!-- synced:x -->
- [x] **CAP-13** · Computer-use options page (Visibility, Control incl. "Pause when I use the mouse" and speed, Limits, Apps) (§4.1) → done: Capabilities → Computer use → Options: Visibility (frame, KIVO cursor, highlight, Island controller), Control (approve each step, pause when I use the mouse, speed), Limits (steps, cost, time) and Apps (allow and block lists) · verified: `sets computer use's visibility, control and limits (CAP-13)` <!-- synced:x -->

#### [CONVERSATION.md](architecture/CONVERSATION.md) (6)

- [~] **CONV-16** · Desktop AI apps (Claude Desktop, ChatGPT, Copilot) through versioned app-registry UIA entries, with the "press Enter when ready" fallback (§5.3) → partial: `ai_apps.prompt` (kivo-tools `ai_apps.rs`) prompts Claude Desktop, ChatGPT and Copilot through versioned `[ai]` hints in their app-registry entries (composer, send and reply controls, the app version they were written for): it fills the composer by UI Automation, sends when asked and reads the reply, and when the app has changed and the controls aren't found it puts the prompt on the clipboard and says "press Enter when ready" · verified: `prompts_an_ai_app_and_reads_its_reply`, `falls_back_when_the_app_changed`, `the_real_apps_have_versioned_hints` on the fake UIA shaped like the test app · missing: checking the Claude, ChatGPT and Copilot hints against the installed apps (they ship marked `verified = false`; until they match, KIVO uses the clipboard fallback) <!-- synced:~ -->
- [x] **CONV-21** · Workspace notes (automatic) with detail level Brief / Standard / Detailed (§6) → done: agent sessions KIVO runs write a workspace log entry by themselves (`engine/agent.rs` → `memory.workspace_log`) at the chosen detail, Brief ≤ 3 bullets, Standard ≤ 10, Detailed all, with an overview note per workspace; secrets are never kept · verified: `workspace_notes_and_suggestions`, `a_new_workspace_is_asked_about_first` <!-- synced:x -->
- [x] **CONV-22** · Tidy job: merge duplicates, condense logs older than 14 days, mark superseded facts, 50 KB per-workspace cap, never keep secrets, ask before new workspace/person notes (§6) → done: the tidy job merges duplicates (`tidy::same_fact`: the specifics — names, numbers, paths — must match, then MiniLM cosine ≥ 0.9, word overlap when no model), condenses logs older than 14 days, marks superseded facts, keeps each workspace under 50 KB, never keeps secrets, and asks before a new workspace's or person's notes (`memory.ask-new-notes`, on by default; "never for this one" is remembered) · verified: kivo-memory tidy tests with measured MiniLM cosines (paraphrase 0.93 merged; "jasmine tea" vs "coffee" 0.74 and 10 am vs 11 am kept apart by their specifics), runtime tidy tests, `a_new_workspace_is_asked_about_first` <!-- synced:x -->
- [x] **CONV-23** · Hybrid search FTS5 + `sqlite-vec` (§6, MEMORY §3) → done: hybrid search — FTS5 and sqlite-vec nearest neighbours fused by reciprocal rank (`memory.find`, `kivo_memory::query::fuse`), used by `memory.search` and recall; vector hits beyond cosine distance 0.65 are dropped · verified: `notes_are_recalled_by_meaning_with_the_local_model` (real MiniLM: a note found by meaning with no shared words), store vector tests <!-- synced:x -->
- [x] **CONV-25** · Memory page graph view (tags, backlinks) (§6) → done: Memory page Notes / Graph toggle; `MemoryGraph` draws the notes with their tag and backlink edges (runtime `links()` resolves `[[links]]`), laid out by `forceLayout.ts`, click opens the note · verified: forceLayout tests, Memory page tests, runtime links test <!-- synced:x -->
- [x] **CONV-33** · Skills created from a routine, or shared by agents KIVO launches (with permission) (§9) → done: "Make a skill" on a routine (`routines.toSkill` writes a SKILL.md with a from-routine marker) and skills that agents KIVO launches keep in `.claude`, `.codex`, `.gemini` or `.agents` folders, shared only with permission · verified: `routines_become_skills_and_agents_share_theirs_with_permission` <!-- synced:x -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (3)

- [x] **DISC-07** · In-app CLI install: explain + exact command from the catalog, dependency check (Node via winget with consent), visible progress sheet, verify, start sign-in, test prompt (§1.1) → done: Agents → "Not installed" → Install: the sheet explains, shows the exact commands from the catalog (a newer signed catalog's when there is one), checks dependencies (Node via winget, asked first), runs with visible progress, verifies with `--version`, then offers the CLI's own sign-in and a test · verified: `installing_a_cli_agent_explains_asks_runs_and_verifies`, installer unit tests, Agents UI test <!-- synced:x -->
- [~] **DISC-17** · Signed catalogs (connector directory, plugin index, model catalog, CLI install catalog) fetched daily with an offline cache (§3) → partial: signed catalogs (connectors, plugins, models, CLI installs): fetched daily, ed25519 signatures checked against `catalogs/keys.txt`, an offline cache, the bundled copy until a newer signed one arrives; `pnpm catalog:sign` signs; off in Strictly private and with "Check for catalog updates" · verified: catalogs unit tests (a tampered or unsigned catalog is refused), Permissions test · missing: the owner's signing key in `keys.txt` and a published catalog location (until then only the bundled catalogs are used) <!-- synced:~ -->
- [x] **DISC-18** · Performance metrics sampled every 2 s only while the Performance page is open (§3) → done: Settings → Performance asks the runtime every 2 s only while that tab is open; nothing is sampled otherwise · verified: `Settings.test.tsx`, the tab's effect cleanup (2026-09-24) <!-- synced:x -->

#### [DISTRIBUTION.md](architecture/DISTRIBUTION.md) (1)

- [x] **DIST-14** · Dependency manager (Node.js, Git, Ollama, …): detect → explain → consent → install via winget or vendor link → verify, configure, test; version and health checked; nothing installed silently (§4, plan §20) → done: the dependency manager (`installer.rs`) for Node.js, Git and Ollama: detect → explain → consent per command → winget (or the vendor's page when winget can't) → verify the version → configure PATH; versions and health shown in Agents → "Tools KIVO uses"; nothing installs without the user's OK · verified: installer unit tests, `installing_a_cli_agent_explains_asks_runs_and_verifies` <!-- synced:x -->

#### [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) (3)

- [~] **INT-05** · KIVO-owned OAuth apps (public PKCE clients, loopback redirect per RFC 8252, never an embedded webview): Google Calendar + `drive.file` first, then Microsoft Graph (§0, §2) → partial: KIVO's own OAuth as a public PKCE client (S256), loopback redirect with a CSRF state per RFC 8252, the system browser (never an embedded webview); Google Calendar (upcoming, create) and Drive `drive.file` (save), then Microsoft Graph calendar · verified: PKCE against RFC 7636 appendix B, sign-in and token refresh against a local mock authorization server, `calendar_and_drive_ask_for_their_own_scope_when_first_used` · missing: registering KIVO's client ids with Google and Microsoft (owner), built in with `KIVO_GOOGLE_CLIENT_ID` / `KIVO_MICROSOFT_CLIENT_ID`; until then the connectors say they aren't set up <!-- synced:~ -->
- [x] **INT-06** · GitHub via the user's `gh` auth (REST/GraphQL) (§2) → done: GitHub tools through the user's own `gh` sign-in — pull requests, issues, notifications, who am I (GraphQL / REST), create an issue (the body goes through a JSON file, never the command line); results are untrusted content · verified: `reads_go_through_gh_and_are_untrusted`, `user_text_never_reaches_the_command_line`, `a_missing_gh_or_sign_in_says_what_to_do` <!-- synced:x -->
- [x] **INT-07** · Tokens in Credential Manager, narrowest scopes requested incrementally, consumer session cookies never extracted (§2) → done: native connectors' tokens live in Credential Manager (never in settings or model context); each tool asks only for its own scope the first time it is used, added to what was granted; no browser cookies are read · verified: `calendar_and_drive_ask_for_their_own_scope_when_first_used`, token refresh test <!-- synced:x -->

#### [MEMORY.md](architecture/MEMORY.md) (1)

- [x] **MEM-09** · Embeddings (MiniLM-class via `fastembed`/`ort`, 384-dim, shared with the intent router) in `sqlite-vec`; model id stored per vector, background re-embed on change (§3) → done: MiniLM-L6 (384-d, the intent router's model, shared) embeds each note into sqlite-vec 0.1.9 (`memory_vectors`, cosine), the model id kept per vector (`memory_vector_models`, profile-scoped); a changed note loses its vector by trigger and is embedded again in the background · verified: `notes_are_recalled_by_meaning_with_the_local_model`, store vector tests, the profile-scoping check <!-- synced:x -->

#### [ROUTINES.md](architecture/ROUTINES.md) (4)

- [x] **ROUT-11** · Schedule (cron-like, time zone) and event triggers (app launched, USB device, Wi-Fi network, time of day, idle/return, battery low, routine-from-routine) (§1) → done: schedule triggers (cron with a time zone, `kivo_core::cron` on jiff, DST-safe) and event triggers — app launched, USB device, Wi-Fi network (NetworkListManager, SetupAPI), time of day, idle / return, battery low, after another routine — sampled every 5 s by `triggers.rs`; the builder's "Other triggers" dialog adds them · verified: cron and triggers unit tests (DST), `devices.rs` against this PC's networks and USB devices, `events_start_routines_unattended_and_high_risk_steps_ask_first`, Routines UI tests <!-- synced:x -->
- [x] **ROUT-12** · Unattended triggers never run High-risk steps without an on-screen confirmation (§3) → done: a routine started by a trigger runs unattended with every High-risk step set to confirm on screen first (`run_unattended`) · verified: `events_start_routines_unattended_and_high_risk_steps_ask_first` <!-- synced:x -->
- [x] **ROUT-13** · Create by voice or chat (brain drafts, builder reviews, save after confirm) and "Save what you just did as a routine" (§4) → done: "make a routine that…" by voice or chat — the brain drafts it with `routines.draft`, the builder opens with the draft to review, nothing is saved until the user saves; "save what you just did as a routine" drafts one from the last request's tool calls · verified: `routines_are_drafted_by_voice_and_chat_and_move_as_files`, Routines UI test <!-- synced:x -->
- [x] **ROUT-14** · Import/export `.kivo-routine.json`, treated as untrusted, permissions shown before import (§4) → done: Export / Import `.kivo-routine.json` (`routines.export`, `routines.import`): an imported file is untrusted — checked, unknown tools refused, and its steps and permissions shown in the builder before it is saved · verified: `routines_are_drafted_by_voice_and_chat_and_move_as_files`, Routines UI test <!-- synced:x -->

#### [SECURITY.md](architecture/SECURITY.md) (1)

- [x] **SEC-23** · Chain verification in diagnostics (§7) → done: Diagnostics re-computes the audit hash chain (`Database::verify_audit`) and shows the row count, or the first broken row with a link to Activity · verified: `records::the_audit_log_refuses_edits_and_detects_tampering`, `m7_settings` (a row edited behind KIVO's back turns the check red with Fix → Activity) (2026-09-24) <!-- synced:x -->

#### [UX.md](architecture/UX.md) (4)

- [~] **UX-16** · Optional edge glow (2–4 px, ~400 ms on wake, off by default), only if the M0 power measurement is acceptable (§2) → partial: the wake glow — a click-through window (`glow.rs`, `glow.html`) draws a 3 px edge glow for ~400 ms on wake on the Island's monitor, off by default (Settings → Island "Wake glow") · verified: `wakes`/`should_glow` tests, glow page test · missing: the M0 power measurement the spec makes it conditional on (bench runs deferred by the owner) <!-- synced:~ -->
- [x] **UX-17** · Overlay style setting: Pill + card / Pill only / Card only / Off (§5) → done: Settings → Island "What it shows": Pill + card / Pill only / Card only / Off, applied to every state by `withStyle()` · verified: Island tests, Settings test <!-- synced:x -->
- [~] **UX-38** · Companion style setting Pill (default) / Orb (WebGL or Rive) / Character (Rive) / Hidden; all render the same `SessionState`, zero frames at idle (§6) → partial: Pill (default), Orb (a WebGL shader, `OrbGl.tsx`, drawing at 30 fps only while KIVO is active and not at all at idle, with a static fallback) and Hidden, all from the same `SessionState` · verified: OrbGl test, Island tests · missing: Character (needs a Rive character asset; none is bundled) <!-- synced:~ -->
- [x] **UX-39** · Pointing: the companion moves to UIA bounds on the target monitor without covering the target (§6, plan §141) → done: `uia.point_at` finds the control by name or text on the target app's UI Automation tree; the Island moves beside its bounds on that monitor without covering it (`overlay::beside`), shows an arrow toward it, and KIVO says where it is · verified: `kivo_points_at_a_control_and_says_where_it_is`, the UIA journey's real Export bounds on the dummy app, overlay placement test, Island tests <!-- synced:x -->

#### [VOICE.md](architecture/VOICE.md) (6)

- [x] **VOICE-10** · More STT tiers: Parakeet (Balanced, DirectML/CUDA), Whisper large-v3-turbo (Accurate), Deepgram Flux, AssemblyAI and OpenAI (Cloud) (§3) → done: Parakeet TDT 0.6B v3 (NeMo features, greedy TDT; DirectML with CPU fallback) as Multilingual, Whisper large-v3-turbo as High accuracy, and cloud recognizers Deepgram Flux, AssemblyAI streaming (WebSockets) and OpenAI transcribe (HTTP), keys in Credential Manager and tested before they're saved, never in model context · verified: `parakeet`/`whisper` tests pin real transcripts with the real models (and a GPU run), `cloud` tests (7) against mock servers speaking each service's protocol; no live cloud call (needs the owner's keys) (2026-09-24) <!-- synced:x -->
- [x] **VOICE-11** · More TTS tiers: Supertonic-2 (Instant), Chatterbox (Expressive), Cartesia, ElevenLabs, Azure, OpenAI and Deepgram (Cloud) (§3) → done: Chatterbox Turbo (the publisher's ONNX export) as Expressive, with a Windows voice as its reference; cloud voices Cartesia Sonic, ElevenLabs Flash, Azure Neural (region), OpenAI and Deepgram Aura; the Instant tier is Supertonic 3 (M2), which replaced Supertonic-2 (DECISIONS "M8 build") · verified: `chatterbox` round trip (Parakeet hears "the weather is lovely today"), `cloud` tests against mock servers; no live cloud call (needs the owner's keys) (2026-09-24) <!-- synced:x -->
- [-] **VOICE-17** · Optional "Enhance" background training job for a custom word (§4) → dropped: KIVO trains no models (DECISIONS "No training (owner)") (2026-09-24) <!-- synced:- -->
- [x] **VOICE-28** · Sound sets Glass, Pulse, Wood, Minimal and Custom (user `.wav`/`.ogg` per cue); a separately chosen notification sound (§6) → done: sound sets Soft, Glass, Pulse, Wood, Minimal and Custom: the user imports a `.wav` (8/16/24/32-bit, float) or `.ogg` per cue (≤ 2 MB, decoded and checked, copied into KIVO's data folder), "Use the set's" per cue, and a separately chosen notification sound · verified: `sounds` tests (WAV/OGG decode, fixture `chime.ogg`), `speaker` custom-cue test, `Settings.test.tsx` VOICE-28 (2026-09-24) <!-- synced:x -->
- [x] **VOICE-29** · A custom KIVO earcon motif replaces the placeholders before beta (§6) → done: KIVO's own motif (rounded two-note tones, each under 300 ms) is synthesized at startup for every cue, replacing the Kenney placeholders (DECISIONS "Earcons generated, not Kenney") · verified: `speaker` cue tests, heard on the owner's PC (2026-09-24) <!-- synced:x -->
- [x] **VOICE-35** · GPU models unload on the Gaming profile or when the GPU is busy (§8) → done: the recognizer leaves the GPU on the Gaming profile, with a fullscreen app in front or when the card is over 60 % busy, and comes back under 30 %, checked every 10 s while idle · verified: `models::the_recognizer_leaves_the_gpu_for_games_and_busy_gpus`, `gpu::tests` (2026-09-24) <!-- synced:x -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (7)

- [x] **PLAN-06** · Product modes Normal, Private, Offline, Battery, Performance, Gaming, Presentation, each mapped onto privacy mode, performance profile and Island behaviour (§142) → done: product modes in the config and `modes.rs`: Private → prefer this PC, Offline → strictly private, Battery/Performance/Gaming → the profile (Gaming also hides the Island over fullscreen), Presentation → large Island with words and hints, no toasts, nothing said unasked; Normal restores exactly the user's settings (kept in the database while a mode is on, so a restart in between is fine, and modes never stack); Performance prewarms the models. Switched from Settings → General, Ctrl+K ("Switch to … mode") or by voice ("presentation mode", "back to normal") through the grammar, no brain · verified: `modes::tests` (2), `m8_modes` (2, one by voice), `Settings.test.tsx` mode picker (2026-09-24) <!-- synced:x -->
- [x] **PLAN-07** · Performance dashboard: CPU, RAM, GPU, VRAM, NPU, model residency, and STT / brain / tool / TTS / total latency (§89) → done: Settings → Performance shows KIVO's CPU and memory, the GPU's load, KIVO's dedicated GPU memory (PDH "GPU Process Memory"), the card's VRAM, the PC's RAM and what's free, the NPU, each model's residency, and median times per request kind and per stage (speech to text, brain to first word, tool, voice to first sound, whole request) from the T0–T10 spans · verified: `system_rpc::each_stage_has_its_own_median`, `medians_come_from_each_kind_of_request`, `reads_a_processs_gpu_memory` (real PDH), `m7_settings`, `Settings.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **PLAN-08** · Performance profiles Low Resource, Balanced, Performance, Battery, Gaming, Custom, chosen automatically with manual override (§90) → done: `profiles.rs` resolves Low Resource, Balanced, Performance, Battery, Gaming and Custom into threads, residency and GPU use; Auto picks Gaming for a fullscreen app, Battery on battery, Low Resource on a small PC (< 8 GB or ≤ 4 threads), else Balanced, rechecked every 10 s while idle; the user can pick one outright and the page says what Auto is using · verified: `profiles::tests` (2), `models` recheck test, `Settings.test.tsx` (2026-09-24) <!-- synced:x -->
- [x] **PLAN-09** · GPU policy: consider GPU load, VRAM, active apps, battery, gaming and model needs; never compete with GPU-heavy work (§91) → done: `gpu.rs` puts the recognizer on the GPU (DirectML) only when the engine can use it, the profile allows it, no fullscreen app is in front, the card has ≥ 3 GB VRAM and its load is under 60 %; it leaves above 60 % and comes back under 30 % (hysteresis); a crash on the GPU moves it to the CPU for the session (PLAN-12) · verified: `gpu::tests`, `models::the_recognizer_leaves_the_gpu_for_games_and_busy_gpus`, Parakeet on the RTX 3060 (229 ms vs 398 ms on CPU) (2026-09-24) <!-- synced:x -->
- [x] **PLAN-12** · Crash recovery: a crashed worker is isolated and restarted where safe, task state is preserved, the problem is reported, failures do not cascade (§114) → done: the speech worker is a separate process restarted with backoff; quick crashes are counted and recovered where safe (`infer::recovery`): a crash on the GPU → the processor for the session, 3 quick crashes → the recognizer, then the voice, set aside for their stand-ins, 6 → no more restarts until the speech settings change, with speech shown as failed and a notice; each crash is in Activity with what was done. Tasks are persisted as they run and reported as interrupted after a crash, never resumed on their own; the app is relaunched by its supervisor; an agent that dies is started again on next use; runtime crashes are reported on the next start · verified: `infer::tests::crashes_are_recovered_where_it_is_safe`, `spoken_command::a_crashed_speech_worker_fails_the_turn_aloud_and_comes_back` (the real worker killed mid-turn: the turn fails aloud, Activity has the crash, a new worker comes back), `app::tests`, `tasks::a_restart_interrupts_running_tasks_but_keeps_watchers`, `lifecycle::crashes_are_reported_once_on_the_next_start` (2026-09-24) <!-- synced:x -->
- [x] **PLAN-23** · Acceptance journey §141 (where is the Export button: UIA bounds, companion points, voice explains) → done: "where is the Export button" → UIA bounds → the Island points beside it → "It's at the top right of Test App" (UX-39) · verified: `kivo_points_at_a_control_and_says_where_it_is`, `uia_testenv` Export bounds <!-- synced:x -->
- [~] **PLAN-28** · Beta definition (§157) fully met → partial: of §157, built and verified — multiple cloud providers, provider profiles, CLI discovery, dependency management (DIST-14), several STT/TTS engines, browser automation, MCP, background tasks and their history, privacy modes, the permission system, provider fallback, and the companion (Island, Orb) · missing: the benchmark runs (the `kivo-bench` harness is built; runs deferred by the owner) and the Character companion (UX-38) <!-- synced:~ -->
<!-- /items -->

---

<a id="m9"></a>

## M9: Release

**Goal:** signed installers, safe updates with rollback, the full release pipeline and a public
beta.

**Depends on:** M8, plus the owner decisions on license and signing.

**Read first:** [DISTRIBUTION.md](architecture/DISTRIBUTION.md), [RELEASE.md](architecture/RELEASE.md), [BENCHMARKS.md](architecture/BENCHMARKS.md), [SECURITY.md](architecture/SECURITY.md), plan §158–160.

**Build order:**

1. **Owner decisions:** the final license and the signing route (see *Still open* below).
2. **Installers.** Size check (DIST-03), MSI for IT (DIST-04), signing (DIST-11, REL-09),
   notices and SBOM (DIST-16, REL-10).
3. **Updates.** Channels and signed manifests (DIST-07), the update flow (DIST-08), rollback
   (DIST-09), verification before install (SEC-32), the "What's new" dialog (UX-59).
4. **Pipeline.** The full matrix with mac/Linux kept unpublished (REL-07), nightly (REL-08),
   release secrets and approvals (REL-11).
5. **Gates.** Full benchmarks with the regression gate (BENCH-14), the production definition
   (PLAN-29), quality gates (PLAN-30), the invariants checklist (PLAN-31).

**Deliverable:** a signed public beta on GitHub Releases that updates itself and rolls back on a
failed update.

**How to verify:** an update from the previous version and a forced failed update that rolls
back; installer size; signature checks on every binary.

**Exit criteria:**

- [ ] **M9-X1** · A signed public beta is published and gathering SmartScreen reputation.
- [ ] **M9-X2** · The quality gates (plan §159) and the architecture invariants checklist are all green (PLAN-30, PLAN-31).

**Then:** the MSIX / package-identity spike, the macOS port (platform crates), then Linux.

### M9 items

<!-- items:M9 -->
**18 items** · ✅ 0 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 18 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [BENCHMARKS.md](architecture/BENCHMARKS.md) (1)

- [ ] **BENCH-14** · Full suites before each release on the reference machines; a > 10% regression on a budgeted metric blocks the release (§4) <!-- synced:  -->

#### [DISTRIBUTION.md](architecture/DISTRIBUTION.md) (7)

- [ ] **DIST-03** · Installer size under 40 MB, checked in the release pipeline (§1, BENCHMARKS §3) <!-- synced:  -->
- [ ] **DIST-04** · MSI per-machine for IT, updater-aware (§1) <!-- synced:  -->
- [ ] **DIST-07** · `tauri-plugin-updater` with a minisign-signed `latest.json` per channel (Stable, Beta, Experimental), channel chosen in Settings (§2) <!-- synced:  -->
- [ ] **DIST-08** · Update flow: download + verify → ask (or install at idle if opted in) → runtime finishes/cancels turns and persists state → exits → installer `/UPDATE` → relaunch → health check (§2) <!-- synced:  -->
- [ ] **DIST-09** · Rollback: if the runtime fails to start twice after an update, reinstall the previous version (kept for one version) (§2) <!-- synced:  -->
- [ ] **DIST-11** · Authenticode signing (Azure Artifact Signing or OV certificate) of all three executables, the installers and the native-messaging host; betas published signed (§3) <!-- synced:  -->
- [ ] **DIST-16** · Generated `THIRD_PARTY_NOTICES` (`cargo about` + npm license checker) in the installer and in About (§6) <!-- synced:  -->

#### [RELEASE.md](architecture/RELEASE.md) (5)

- [ ] **REL-07** · Full matrix: Windows ARM64, macOS universal `.dmg` + `.app.tar.gz`, Linux `.AppImage`/`.deb`/`.rpm`; mac/Linux built but kept as 14-day workflow artifacts, not published (§2, §3) <!-- synced:  -->
- [ ] **REL-08** · Experimental channel from `vX.Y.Z-exp.N` tags through `release.yml` (artifacts expire after 14 days) (§2) <!-- synced:  -->
- [ ] **REL-09** · Platform signing steps (Authenticode `signCommand`; later Apple notarization; Linux `SHA256SUMS`/GPG) (§2) <!-- synced:  -->
- [ ] **REL-10** · SBOM (`cargo cyclonedx` + npm) and `THIRD_PARTY_NOTICES` uploaded with each release (§2) <!-- synced:  -->
- [ ] **REL-11** · `release` environment secrets (`TAURI_SIGNING_PRIVATE_KEY` + password, signing credentials), release jobs only on tags from `main`, a `production` approval for Stable (§2, §4) <!-- synced:  -->

#### [SECURITY.md](architecture/SECURITY.md) (1)

- [ ] **SEC-32** · Updates: minisign-signed manifests + Authenticode binaries, both verified before running the installer (§9) <!-- synced:  -->

#### [UX.md](architecture/UX.md) (1)

- [ ] **UX-59** · "What's new in KIVO x.y" dialog shown once after an update, with Release notes / Got it (mockup → System surfaces, DIST-08) <!-- synced:  -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (3)

- [ ] **PLAN-29** · Production definition (§158) fully met <!-- synced:  -->
- [ ] **PLAN-30** · Quality gates (§159) pass for the release <!-- synced:  -->
- [ ] **PLAN-31** · Architecture invariants checklist (§160, ARCHITECTURE §8) all green <!-- synced:  -->
<!-- /items -->

---

<a id="post"></a>

## Post-M9

Planned after the first release: MSIX and Windows AI Speech, plugins, KIVO Remote, people profiles,
the full CaMeL defense and more agents.

### Post items

<!-- items:Post -->
**16 items** · ✅ 0 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 16 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [BRAINS.md](architecture/BRAINS.md) (1)

- [ ] **BRAIN-19** · Codex App Server adapter; GitHub Copilot CLI, OpenCode and Goose over ACP (§4) <!-- synced:  -->

#### [DISCOVERY.md](architecture/DISCOVERY.md) (1)

- [ ] **DISC-11** · Plugins folder watcher: a dropped plugin shows "New plugin found" + Review (§1.2) <!-- synced:  -->

#### [DISTRIBUTION.md](architecture/DISTRIBUTION.md) (1)

- [ ] **DIST-06** · MSIX / sparse package spike for package identity (Windows AI Speech, richer notifications, Store) (§1) <!-- synced:  -->

#### [INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) (6)

- [ ] **INT-08** · Restricted Google scopes (Gmail read, full Drive) once verification/CASA is funded; each other major app gets a research note before implementation (§0, §2) <!-- synced:  -->
- [ ] **INT-15** · File Explorer context menu "Ask KIVO about this" / "Summarize with KIVO" (needs package identity on Windows 11, DIST-06) (mockup → System surfaces) <!-- synced:  -->
- [ ] **INT-10** · WASM plugins (wasmtime Component Model): `kivo-plugin.toml` manifest, capabilities granted by linking only approved imports, consent at install and on capability-adding updates, fuel/time/memory limits, tools through `authorize()` (§3) <!-- synced:  -->
- [ ] **INT-12** · Pairing by single-use QR (Ed25519 key, one-time secret, 2 min expiry), Noise/X25519 handshake, AEAD channel with replay counters (§4) <!-- synced:  -->
- [ ] **INT-13** · Transports LAN (mDNS), WebRTC P2P, self-hostable ciphertext-only relay (§4) <!-- synced:  -->
- [ ] **INT-14** · Remote principal with the Remote Balanced profile (no shell, no computer use, Medium confirmed), High confirmed on the phone with biometrics, paired devices listed and revocable, remote actions audited with device id (§4) <!-- synced:  -->

#### [SECURITY.md](architecture/SECURITY.md) (1)

- [ ] **SEC-16** · Full CaMeL plan-then-execute mode with a quarantined extractor (§4) <!-- synced:  -->

#### [TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) (1)

- [ ] **TOOL-27** · Firefox extension (§5) <!-- synced:  -->

#### [UX.md](architecture/UX.md) (1)

- [ ] **UX-48** · People profiles (voiceprint, memory, preferences, routines, sign-ins, mode, usage per person) (§8.2) <!-- synced:  -->

#### [VOICE.md](architecture/VOICE.md) (1)

- [ ] **VOICE-12** · Windows AI Speech STT (needs package identity, DIST-06) and GPL add-ons (Piper, espeak-ng) as separately downloaded components (§3) <!-- synced:  -->

#### [KIVO_Project_Plan.md](KIVO_Project_Plan.md) (3)

- [ ] **PLAN-24** · Multi-agent coordination with KIVO as the coordinator (§147) <!-- synced:  -->
- [ ] **PLAN-25** · Voice marketplace: voices, languages, styles, pronunciation packs, legally permitted only (§150) <!-- synced:  -->
- [ ] **PLAN-26** · Home/IoT tools under the same permission model (§152) <!-- synced:  -->
<!-- /items -->

---

## Language milestones (in parallel with M3+)

| Step | Scope | Exit |
|---|---|---|
| <a id="l1"></a>L1 | English (ships with M1/M2) | Budgets met on the English test sets |
| <a id="l2"></a>L2 | **Hindi + Punjabi**, including Hindi–English code-mixing | Research note + owner-recorded test sets; STT WER and wake-word metrics meet targets; fast-path grammar in Hindi |
| L3 | Major European languages | Per-language test sets; native-speaker review |
| L4 | Japanese, Chinese, Korean | CJK engines, IME-safe hotkeys, native-speaker review |
| L5 | Arabic and other RTL | RTL UI pass, engines, native-speaker review |

### L1 items

<!-- items:L1 -->
**2 items** · ✅ 0 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 2 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [UX.md](architecture/UX.md) (1)

- [ ] **UX-51** · Language settings show each language's status (Supported / Alpha / Planned) (§9) <!-- synced:  -->

#### [VOICE.md](architecture/VOICE.md) (1)

- [ ] **VOICE-39** · Each language has STT test audio, command utterances and wake positives/negatives before it moves from Alpha to Supported (§9) <!-- synced:  -->
<!-- /items -->

### L2 items

<!-- items:L2 -->
**1 items** · ✅ 0 done · 🟡 0 partial · ⏭ 0 dropped · ⬜ 1 not started

> Generated by `pnpm docs:sync` from the specs. Mark items **in their spec** (with the `→` note), then re-run.

#### [VOICE.md](architecture/VOICE.md) (1)

- [ ] **VOICE-38** · Hindi + Punjabi, including Hindi–English and Punjabi–English code-mixing, benchmarked on owner-recorded test sets (§9) <!-- synced:  -->
<!-- /items -->

## Owner inputs (answered 2026-09-21)

All the pre-M0 questions are answered ([DECISIONS.md → Owner answers](DECISIONS.md)): open source
(license type still to choose; no GPL in the core until then); **all four** cloud providers and
**all three** CLI agents in M3; all languages, shipped English → Hindi + Punjabi → European →
CJK → RTL; signing decided later; a low-end VM for benchmarks; Ctrl+Alt+Shift+Esc as the
emergency stop.

**Still open (not blocking until M9):** the final license (before the first public release) and the
signing route (before the public beta).
