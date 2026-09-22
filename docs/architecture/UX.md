# UX spec: lifecycle, overlay, Control Center

Status: Draft v1, 2026-09-21. Research: [voice-ui-and-app-presence/REPORT.md](../research/voice-ui-and-app-presence/REPORT.md).
It implements plan §74–90, §118 and §129–135, and records the decisions in [DECISIONS.md](../DECISIONS.md).

## 1. Lifecycle

| Event | Behavior |
|---|---|
| First launch | The Control Center opens on **onboarding** (§4) |
| Manual launch, not running | The runtime starts, then the Control Center is shown |
| Launch at login (opt-in) | The runtime starts with `--autostart`; the UI preloads hidden; the tray icon appears |
| Relaunch while running | The main window is shown, unminimized and focused |
| Close (X / Alt+F4 / taskbar close) | The window hides, and KIVO keeps running ("On close, keep KIVO running", on by default). The first close shows a one-time toast with **Settings** and **Quit** buttons |
| Tray left-click | Opens the Control Center |
| Tray right-click | **Open KIVO** · Pause/Resume listening · Hide overlay for 1 hour · Stop everything · Settings · Quit KIVO |
| Tray icon states | Normal, listening, paused (slashed mic), error (badge), updating |
| Quit | Only from the tray or the Control Center menu. Stops the runtime and releases the mic |

## 2. Voice overlay

**Chosen concept: "Island"** (owner decision, 2026-09-21; see the Island section of
[kivo-app.html](../design/mockups/kivo-app.html)). Throughout the
specs, "pill" means the Island capsule.

**Surfaces:**

- **Island (pill):**
  - **Placement:** a black capsule at the **top center** of the monitor with the foreground
    window, about 8 px from the top edge. It never takes focus, and it draws nothing when hidden.
  - **Morphing:** it springs between sizes by state (≈ 230×34 listening → wider with the live
    transcript → a tall card for answers and confirmations → a compact **live activity**, such as
    "Lo-fi Focus · Spotify" or a timer, then gone).
  - **Moving it:** draggable, and position is remembered per monitor. A setting offers "Top
    center (default) / Bottom center / Remember drag".
  - **Title-bar overlap:** if the foreground window's title bar or tabs sit under the island, it
    shifts down by the title-bar height while it's only listening. It expands over the content only
    when it has text to show, and it collapses right after.
  - **Contrast:** the capsule is always near-black (`#000`), in light and dark mode, for maximum
    contrast and an identity of its own. The accent orb and waveform carry the state color.
- **Card (expanded island):** up to 520 px wide and 50% of the screen height, with the same rounded
  black material. It scrolls, and focus is enabled only while the user types.
- **Live activities:** after a request, the island can stay collapsed with ongoing status (media,
  timer, download, agent task progress) until it's dismissed or times out. Each source is
  configurable, and they're off in fullscreen.
- **Edge glow (optional, off by default):** a 2–4 px gradient on the active monitor for about
  400 ms on wake.

**States:**

| State | Pill | Card | Sound |
|---|---|---|---|
| Idle / standby | Hidden (optional 10 px dot) | Hidden | — |
| Listening | Accent + live waveform + mic icon | Opens as soon as partial text arrives (gray, live) | listen_start |
| Thinking | Shimmer | "You said…" locked in | listen_stop |
| Acting | Tool icon + one-line step | Step list with statuses | — |
| Speaking | Pulse synced to TTS level | Streaming answer (rich text) | — |
| Awaiting confirmation | Amber | Exact action + Allow once / Always… / Deny; never auto-dismisses | spoken prompt |
| Error | Red + short reason | Reason + Retry / Open settings | error |
| Paused | Slashed mic (only when summoned) | — | — |
| Guest speaker | Neutral outline + "Guest" chip | Limited | — |
| End | Collapses 4 s after the answer (not while hovered, typing or confirming) | — | hangup |
| Fullscreen app / Focus mode | Suppressed, or a tiny pill (setting) | Suppressed | Sounds only |

**Card contents:**

- **Header:** the brain/profile chip ("Coding · Claude"), which shows the routing reason on hover.
- **Body:** the transcript bubble (editable: click to fix and resend), the answer, and tool/step
  rows.
- **Footer:** a text field ("Type to KIVO…"), a mic toggle, the Stop button, and "Open in Control
  Center".

**Keyboard:**

- Esc cancels.
- Ctrl+Enter sends.
- Tab moves between confirmation buttons.
- The overlay is reachable through the Control Center hotkey.

## 3. Control Center (plan §80–89)

Navigation (updated 2026-09-21; 13 items, with tabs inside pages):

| Group | Items |
|---|---|
| — | Home · Chat · Tasks · Activity · Routines |
| Intelligence | Brains (Brains / Context) · Agents · Voice · Extensions (Connectors / MCP servers / Plugins / Skills) |
| Control | Permissions (Mode / Capabilities / Privacy) · Memory · Usage |
| System | Settings (General / Appearance / Island / Sounds / Notifications / Accessibility / Shortcuts / Performance / Diagnostics / About) |

**Design system (plan §134–135):**

- **Stack:** React + TypeScript, Tailwind v4, **Base UI** primitives styled by KIVO's own `k-`
  component CSS (no shadcn; see [DESIGN_SYSTEM.md](../design/DESIGN_SYSTEM.md)), Motion, and Lucide icons.
- **Fonts:** the system font (Segoe UI Variable on Windows; nothing bundled, and SF Pro is never used), with Noto fallbacks for other scripts.
- **Tokens:**
  - an 8 px spacing scale;
  - radii of 10 px (controls), 14 px (cards) and 20 px (pill/sheets);
  - elevation from soft CSS shadows;
  - **Light theme by default**, with Light / Dark / System.
- **Materials:** Mica on the Control Center (Windows 11), solid on Windows 10. The overlay uses a
  CSS translucent surface, with acrylic only on the fixed-size pill if M0 shows it is acceptable.
- **Motion:** 150–250 ms ease-out for state changes, spring motion for card growth, and Windows
  "Animation effects" plus an in-app setting turn motion off (WCAG 2.3.3).
- **Voice visuals:** adapted from ElevenLabs UI (MIT): `live-waveform`, `bar-visualizer`,
  `shimmering-text`, `transcript-viewer`, `conversation`. The LiveKit aura shader is an optional
  later orb.
- **Idle cost rule:** hidden windows also set WebView2 `IsVisible=false`. Animation loops run only
  while listening or speaking, and the idle pill renders zero frames.

## 4. Onboarding (plan §129–133)

Updated 2026-09-21: **11 steps in 5 phases**, one decision per screen, with the recommended
choice preselected. The source of truth is [the mockup](../design/mockups/kivo-app.html) →
Onboarding.

| Phase | Steps |
|---|---|
| Welcome | 1 Welcome (black screen; the Island demonstrates itself) |
| Voice | 2 Microphone check · 3 How you call KIVO (Ctrl+Space, "Hey Kivo") · 4 Hearing & speaking (engine + voice, background download) · 5 Your voice (optional enrollment, with consent) |
| Brain | 6 Connect a brain (optional; sign-in, free options marked) · 7 Connect your apps and tools (optional; connectors, extension, MCP, skills) |
| Control | 8 Permission mode · 9 Look & feel (theme, accent, Island position, chimes) · 10 Startup ("Open KIVO when Windows starts", keep running) |
| Ready | 11 Try it (live Island demo) → **Finish opens the Control Center** |

## 5. Settings (defaults)

> **Updated 2026-09-21:** the Settings screen is organized into tabs (General, Appearance, Island, Sounds, Notifications, Accessibility, Shortcuts, Performance, Diagnostics, About). See [DECISIONS.md](../DECISIONS.md) for the structure and [the mockup](../design/mockups/kivo-app.html) for the exact controls. **Theme default: Light**; the options are Light / Dark / System.

| Group | Controls (default) |
|---|---|
| General | Open KIVO when Windows starts (off) · On close keep running (on) · Tray icon (on) · Language (EN) · Low-memory mode (off) |
| Voice | Wake words list with Add/Edit (Hey Kivo) · Push-to-talk hotkey (Ctrl+Space) · Toggle mode (off) · Auto-end on silence (on) · Mic · Speaker profile mode (Prefer owner, after enrollment) · STT engine · TTS engine + voice · User vocabulary |
| Overlay | Style: Pill + card / Pill only / Card only / Off · Position (top center; bottom center or remember drag) · Monitor (active) · Wake glow (off) · Reduce motion (follow Windows) · In fullscreen & Focus: hide / tiny pill (hide) |
| Sounds | Master (on) · start/stop/error/done (on) · thinking (off) · volume · sound set |
| Brains | Providers, profiles, routing rules, fallbacks |
| Permissions | Mode (Auto) · grants list · per-tool overrides · emergency stop hotkey |
| Privacy | Mode (Cloud) · data classes · conversation retention (30 days) · debug transcripts (off) |
| Memory | On (explicit only) · list/edit/delete · export |
| Performance | Profile (auto: Battery / Balanced / Performance / Gaming) · model residency timers |
| Accessibility | Screen-reader announcements (on) · captions for spoken replies (on) · high contrast (follow Windows) · warn if overlay and sounds are both off |

## 6. Companion styles

The user picks a style in Settings → Companion (switchable at any time). All styles render the
same `SessionState` and audio levels.

| Style | Description | Tech | Default |
|---|---|---|---|
| **Pill** | The overlay pill and card (§2) | CSS + Canvas waveform | **Yes** |
| **Orb** | An abstract reactive orb that floats near the pill. It can glide to point at UI targets (plan §141) | Raw WebGL shader (adapted LiveKit aura) or Rive | No |
| **Character** | A small animated mascot with expressions (idle, listening, thinking, speaking, pointing, error). Draggable, with click-through on transparent pixels | **Rive** state machine; the character is designed in the design phase | No |
| **Hidden** | No visual companion; sounds only | — | No |

- **Card:** Orb and Character still use the card for text. The companion replaces the pill, not
  the card.
- **Idle cost:** zero frames at idle for every style. The Character may play a short idle
  animation every N minutes (off by default).
- **Pointing:** the companion moves to UIA bounds on the target monitor while KIVO explains
  something. It never covers the target.
- **Custom skins:** additional characters and orb themes can be installed as plugins (data-only
  Rive files).

## 7. Proactive speech and notifications

KIVO sometimes needs to speak without being asked: a task finished, a watcher fired, a reminder,
or a budget warning. Rules:

| Situation | Behavior |
|---|---|
| User active, no call, not fullscreen | Earcon + pill shows the message; speaks if the item is marked "tell me" |
| In a call (mic in use by another app), fullscreen, presentation, Focus/DND | **No speech.** Queue it, then show a silent toast or pill badge; read it out later on "Kivo, what did I miss?" |
| User away (idle > 5 min or locked) | Queue; summarize on return (optional) |
| Quiet hours (setting) | Toast only |
| Urgent (a user-marked reminder, or a task needing confirmation to continue) | Toast + earcon even in Focus, but never speech during calls |

- **Grouping:** notifications are grouped, so there is at most one spoken interruption per 10 min
  unless it is urgent.
- **Per-source settings:** each source can be set to speak, toast or silent.

## 8. Text input ("type to KIVO")

- **Ctrl+Shift+Space** (configurable) opens the card in text mode at the pill position, with focus
  in the text field. It behaves the same as a spoken request, with no speech reply unless enabled.
- **Selection shortcut:** with text selected in any app, "Ctrl+Shift+Space → Explain / Rewrite /
  Translate" offers quick actions. It uses the clipboard or UIA TextPattern, and requires the
  Clipboard or UIA capability.

## 8.1 Conversation conveniences (owner decision, 2026-09-21)

| Feature | Behavior |
|---|---|
| **Undo** | After reversible changes (file move, rename, edit, window arrangement, setting change), the Island shows **Undo** with a countdown ring (about 8 s). "Kivo, undo that" works too, and so does the Control Center toast. Each tool declares an `undo` handler, or `irreversible: true`; irreversible actions never show Undo, and they confirm up front in Ask/Accept-edits/Plan/Auto |
| **"What can I say?"** | Saying "Kivo, what can I say?" / "help", or pressing F1 while the Island is open, shows 3–5 example commands **for the foreground app** (from the App Capability Registry), plus general ones |
| **Follow-up without wake word** | After KIVO answers, the Island stays in *Listening for a follow-up* for N s (Off / 5 / **8** / 15). A ring shows the time left. It is VAD-gated, and the command spotter still works |
| **Target-app icon** | When acting, the Island's leading icon is the target app's icon (for example Spotify), so it's always clear where actions go |
| **Command palette (Ctrl+K)** | Search every setting, run routines, trigger actions, or type a question to KIVO. It uses the black Island material and appears top-center |

## 8.2 Multi-person profiles (planned, post-MVP)

- **Profiles:** separate **people profiles** on one PC, each with its own voiceprint, memory,
  preferences, routines, brain sign-ins, permission mode and usage.
- **Selection:** KIVO picks the active person by speaker recognition, or by the Windows account.
  Unknown voices fall back to Guest.
- **MVP:** one owner plus Guest mode. The data model scopes everything by `profile_id` from M0,
  so adding people later needs no migration of meaning.

## 9. Internationalization (plan §119)

- **UI strings:** from day one, every UI string goes through i18n (`i18next` / ICU message
  format). There are no hard-coded strings, and English is the source locale.
  Text the runtime produces itself (tray menu, notifications, spoken replies, action titles,
  refusals) comes from a catalog in `crates/kivo-core/locales/<lang>.json` with the same nesting
  and `{name}` placeholders; English is the fallback for missing keys.
- **Layout:** CSS logical properties (`margin-inline-start`, etc.), so RTL (Arabic) works by
  switching `dir`. The fonts need Devanagari, Gurmukhi, CJK and Arabic coverage (Noto fallbacks
  bundled per language pack).
- **Formatting:** dates, numbers and units use `Intl` in the UI and ICU4X in the runtime (for
  spoken text normalization).
- **Language settings:** a primary language plus secondary languages. Each language shows its
  status (Supported, Alpha or Planned), so users know what is tested.

## 10. Accessibility (plan §118)

- **Screen readers:** the overlay and Control Center expose ARIA roles. State changes and final
  transcripts are announced through UIA notifications.
- **Keyboard:** full keyboard navigation with visible focus rings. The Island never takes focus on its own; when it
  shows buttons, Ctrl+Shift+Space (type to KIVO) moves focus onto them instead of opening the text
  field: Tab or the arrow keys move, Enter presses, Esc gives focus back.
- **Color and contrast:** no information is conveyed by color alone. Contrast is at least 4.5:1,
  and the Windows high-contrast theme is respected.
- **Voice-only use:** every action in the overlay can be triggered by voice.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md). Visuals come from
[the mockup](../design/mockups/kivo-app.html) and the components in `apps/kivo-app/src/components`
([DESIGN_SYSTEM.md](../design/DESIGN_SYSTEM.md)). Screen items are "done" only when they show
real runtime data over IPC, not mockup data.

**Lifecycle (§1)**

- [~] **UX-01** · M1 · First launch opens the Control Center on onboarding; a manual launch starts the runtime and shows the Control Center; a relaunch shows, unminimizes and focuses the main window (§1) → partial: a manual launch starts the runtime and shows the Control Center; a relaunch shows, unminimizes and focuses it; `--page` opens a page · verified: live run on Windows 11 (2026-09-21) · missing: first launch on onboarding (UX-33)
- [~] **UX-02** · M1 · Close (X, Alt+F4, taskbar) hides the window and KIVO keeps running ("On close, keep KIVO running", on); the first close shows a one-time toast with Settings and Quit (§1) → partial: closing hides the window while KIVO runs (and really closes when the runtime is gone, since there is no tray then) · verified: live run on Windows 11 (2026-09-21) · missing: the one-time first-close toast (UX-57) and honoring the "keep running" setting
- [x] **UX-03** · M1 · Tray: left-click opens the Control Center; right-click menu Open KIVO · Pause/Resume listening · Hide overlay for 1 hour · Stop everything · Settings · Quit KIVO (§1) → done: `apps/kivo-runtime/src/tray.rs`: left-click opens the Control Center; menu Open KIVO · Pause/Resume listening (enabled by state) · Permission mode (submenu, current mode checked) · Hide Island for 1 hour / Show the Island · Stop everything · Settings · Quit KIVO · verified: tray unit tests incl. every choice reaching the core; live on this PC
- [x] **UX-04** · M1 · Tray icon states: normal, listening, paused (slashed mic), error (badge), updating (§1) → done: `kivo-platform-windows/src/tray.rs`: normal, listening (blue ring), paused (greyed + slash), error (red badge), updating (amber badge), driven by the session · verified: `every_state_looks_different`, `listening_rings_and_paused_is_slashed`
- [~] **UX-56** · M1 · Tray tooltip: "KIVO · <state>" plus the permission mode and running-task count (mockup → partial: tooltip "KIVO · Ready" + "<mode> mode · 0 tasks running", updated live with the state and permission mode · missing: the real running-task count (tasks arrive in M5)
- [ ] **UX-57** · M1 · Actionable Windows notifications with buttons routed back to the runtime: first close (Settings / Quit KIVO), microphone blocked (Open Windows settings / Type instead); later sources reuse the same template (§1, §7)
- [ ] **UX-58** · M5 · Notification reply field ("Task finished … Want me to commit?" → Reply to KIVO / Send) and a taskbar jump list (Routines, New conversation, Pause listening, Stop everything) (mockup → System surfaces)
- [ ] **UX-59** · M9 · "What's new in KIVO x.y" dialog shown once after an update, with Release notes / Got it (mockup → System surfaces, DIST-08)

**Overlay: the Island (§2)**

- [x] **UX-05** · M0 · Overlay spike: a transparent, borderless, non-focusable (`WS_EX_NOACTIVATE`), always-on-top, click-through window with no taskbar entry; white-flash and show latency measured (§2, BENCH-07) → done: `apps/kivo-app/src-tauri/src/overlay.rs` (preloaded hidden; transparent, borderless, `focusable(false)` → WS_EX_NOACTIVATE, click-through, topmost re-asserted on show, no taskbar entry; WebView2 set invisible when hidden) + `src/overlay/` page · verified: `kivo-bench overlay` on the release build (2026-09-21): hotkey → Island visible p50 155 ms / p95 155 ms, no white flash in 6 runs over a grey backdrop, never activates, click-through, topmost, never took focus; zero CPU in every process while hidden
- [~] **UX-06** · M1 · The Island window: top center of the monitor with the foreground window, ~8 px from the top, never takes focus, draws nothing when hidden; hidden windows set WebView2 `IsVisible=false` (§2, §3) → partial: the Island window is placed top center, 8 px down, on the monitor under the pointer; never takes focus; fits its height to the Island (up to 50% of the monitor); hidden windows set WebView2 IsVisible=false · missing: the monitor with the foreground window (needs the `Windows` platform trait, TOOL-07)
- [~] **UX-07** · M1 · Island states driven by the runtime's `SessionState`: listening (live transcript), thinking, acting (target-app icon, steps), speaking, awaiting confirmation, error, paused (§2) → partial: `components/island/session.tsx` renders listening, follow-up, thinking, working, speaking, stopping, needs-your-OK and error from the runtime's `SessionState`, with the live mic level on the waveform, plus a mode-change notice · missing: live transcript (VOICE-08), target-app icon and steps (TOOL-*), answers (TTS), confirmation content (SEC-10)
- [ ] **UX-08** · M2 · Island states guest, follow-up (ring countdown) and waiting-for-you (§2, §8.1, CONVERSATION §7)
- [ ] **UX-09** · M1 · Card: ≤ 520 px wide and ≤ 50% of screen height, scrolls, focusable only while typing; header brain/profile chip (routing reason on hover, M3); body with the editable transcript (click to fix and resend), answer and step rows; footer text field, mic toggle, Stop and "Open in Control Center" (§2)
- [ ] **UX-10** · M1 · End: collapses 4 s after the answer, not while hovered, typing or confirming; confirmations never auto-dismiss (§2)
- [ ] **UX-11** · M1 · Fullscreen app / Focus mode: suppressed or tiny pill (setting), sounds only (§2)
- [ ] **UX-12** · M1 · Keyboard: Esc cancels, Ctrl+Enter sends, Tab moves between confirmation buttons (§2)
- [ ] **UX-13** · M4 · Draggable, position remembered per monitor; setting Top center (default) / Bottom center / Remember drag (§2)
- [ ] **UX-14** · M4 · Title-bar overlap: shifts down by the title-bar height while only listening (§2)
- [ ] **UX-15** · M5 · Live activities (media, timer, download, agent progress), each source configurable, off in fullscreen (§2)
- [ ] **UX-16** · M8 · Optional edge glow (2–4 px, ~400 ms on wake, off by default), only if the M0 power measurement is acceptable (§2)
- [ ] **UX-17** · M8 · Overlay style setting: Pill + card / Pill only / Card only / Off (§5)

**Control Center (§3)**

- [~] **UX-18** · M7 · Navigation: 13 items in the §3 groups with in-page tabs, plus Settings (§3) → partial: sidebar with all items and page tabs component (`components/layout/Shell.tsx`, `PageTabs`) · missing: the pages
- [~] **UX-19** · M1 · Home: status orb and "Listening for Hey Kivo", Talk / Pause listening / mode picker, Running and Recent lists (mockup → partial: `pages/Home.tsx`: status orb, state title and detail, Pause/Resume listening, permission-mode picker, Start KIVO when disconnected, version; all from the runtime · missing: Talk button (VOICE-08/41 pipeline), Running and Recent lists (ARCH-23, M5)
- [ ] **UX-20** · M1 · Activity: timeline of turns, tool calls and results from the Activity table (mockup → Activity) (§3, plan §83)
- [ ] **UX-21** · M3 · Chat: threads list, conversation with brain switcher and permission-mode picker (SEC-04), attachments, tool activity, cancel, context meter, Compact now (mockup → Chat; CONVERSATION §0–1)
- [ ] **UX-22** · M3 · Brains page (Brains / Context tabs): providers with found-on-this-PC, add/test/remove, profiles, free options labelled (mockup → Brains)
- [ ] **UX-23** · M3 · Voice page: mic, speaker, wake words (add/edit), STT/TTS engine and voice, personality, models on this PC (download/delete) (mockup → Voice; plan §86)
- [ ] **UX-24** · M5 · Tasks: running and past tasks with status, current step, elapsed time, tool activity, cancel, result (mockup → Tasks; plan §84)
- [ ] **UX-25** · M5 · Agents page: CLI agents, desktop AI apps, sessions (mockup → Agents)
- [ ] **UX-26** · M5 · Routines page and builder (mockup → Routines; ROUTINES §4)
- [ ] **UX-27** · M6 · Extensions page tabs Connectors · MCP servers · Plugins · Skills, each: one-line description + actions, grouped lists with counts, found-on-this-PC sections with Refresh (mockup → Extensions; DISCOVERY)
- [ ] **UX-28** · M4 · Permissions page tabs Mode · Capabilities · Privacy (mockup → Permissions)
- [ ] **UX-29** · M7 · Memory page: vault by tags and folders, suggestions, instructions, Open folder / Open in Obsidian (mockup → Memory; CONVERSATION §6)
- [ ] **UX-30** · M7 · Usage page (BRAIN-37) (mockup → Usage)
- [ ] **UX-31** · M7 · Settings tabs General · Appearance · Island · Sounds · Notifications · Accessibility · Shortcuts · Performance · Diagnostics · About, with the §5 defaults (mockup → Settings)
- [ ] **UX-32** · M7 · Mica on the Control Center (Windows 11), solid on Windows 10 (§3)

**Onboarding (§4)**

- [ ] **UX-33** · M2 · Steps 1–5: Welcome (black, Island demo), Microphone check, How you call KIVO, Hearing & speaking (engine + voice, background download), Your voice (optional enrollment with consent); steps slide 18 px in the direction of travel and the progress dots stretch (DESIGN_SYSTEM §5) (§4)
- [ ] **UX-34** · M3 · Step 6: Connect a brain (optional; sign-in; free options marked) (§4)
- [ ] **UX-35** · M7 · Steps 7–11: Connect your apps and tools, Permission mode, Look & feel, Startup, Try it → Finish opens the Control Center; recommended choices preselected, optional steps skippable (§4)
- [ ] **UX-36** · M7 · Recommendation engine: defaults chosen from hardware, installed software, GPU, RAM, network and providers; the user can override (§4, plan §130)

**Settings defaults (§5)**

- [ ] **UX-37** · M7 · Every §5 control exists with its listed default, persisted in `kivo.toml` through the runtime (§5)

**Companion styles (§6)**

- [ ] **UX-38** · M8 · Companion style setting Pill (default) / Orb (WebGL or Rive) / Character (Rive) / Hidden; all render the same `SessionState`, zero frames at idle (§6)
- [ ] **UX-39** · M8 · Pointing: the companion moves to UIA bounds on the target monitor without covering the target (§6, plan §141)

**Proactive speech (§7)**

- [ ] **UX-40** · M5 · Proactive rules: speech/toast/queue by situation (active, call, fullscreen, away, quiet hours, urgent), grouped to ≤ 1 spoken interruption per 10 min, "what did I miss?", per-source speak/toast/silent (§7)

**Text input and conveniences (§8–8.1)**

- [ ] **UX-41** · M1 · Ctrl+Shift+Space opens the card in text mode with focus in the field; behaves like a spoken request; no speech reply unless enabled (§8)
- [ ] **UX-42** · M5 · Selection shortcut: with text selected, Explain / Rewrite / Translate (clipboard or UIA TextPattern) (§8)
- [ ] **UX-43** · M4 · Undo in the Island with an ~8 s ring, "Kivo, undo that" and the Control Center toast; irreversible actions never offer Undo (§8.1)
- [ ] **UX-44** · M5 · "What can I say?" / "help" / F1: 3–5 examples for the foreground app from the App Capability Registry (§8.1)
- [ ] **UX-45** · M2 · Follow-up without the wake word for N s (Off / 5 / 8 default / 15), ring countdown, VAD-gated (§8.1)
- [ ] **UX-46** · M4 · Target-app icon as the Island's leading icon while acting (§8.1)
- [~] **UX-47** · M7 · Ctrl+K command palette: search every setting, run routines, trigger actions, ask KIVO; black Island material, top center (§8.1) → partial: `components/ui/CommandPalette.tsx` with keyboard navigation and "Ask KIVO" fallback · missing: real commands, settings index, routines
- [ ] **UX-48** · Post · People profiles (voiceprint, memory, preferences, routines, sign-ins, mode, usage per person) (§8.2)

**Internationalization and accessibility (§9–10)**

- [x] **UX-49** · M1 · Every UI string goes through i18n (`i18next`, ICU messages), English source locale; no hard-coded strings (§9) → done: the Control Center, the Island and the gallery read every string from `src/i18n/locales/en.json` (i18next + ICU, `withNodes` for sentences with elements); the runtime's own text (tray, notifications, spoken replies, action titles, refusals) comes from `crates/kivo-core/locales/en.json` through `kivo_core::text` · verified: `pnpm test` (presets have translated labels), `cargo test -p kivo-core --test text_keys` (every key the code uses exists), tray and tool tests assert no raw keys, sweep of `.tsx` for literal text (2026-09-22)
- [x] **UX-50** · M1 · CSS logical properties so RTL works by switching `dir`; dates and numbers via `Intl` (§9) → done: every inline direction in the stylesheets and inline styles is logical (`margin-inline-*`, `inset-inline-*`, `text-align: start`); the switch thumb and the sheet mirror under `:dir(rtl)`; only centring and Base UI’s physical popup sides stay physical. Times and dates use `Intl.DateTimeFormat`, numbers, percentages, units and money use ICU number skeletons (`Intl.NumberFormat`) · verified: `dir="rtl"` in the running UI mirrors the sidebar (border and position flip), typecheck/lint/tests (2026-09-22)
- [ ] **UX-51** · L1 · Language settings show each language's status (Supported / Alpha / Planned) (§9)
- [x] **UX-52** · M1 · ARIA roles, full keyboard navigation and visible focus rings in the overlay and Control Center (§10) → done: Base UI components (ARIA and keyboard) with a focus ring everywhere (white on the Island); the Island is a polite live region, its dots are decorative and its spinner/check are labelled images; Ctrl+Shift+Space puts keyboard focus on the Island’s buttons (arrows/Tab, Enter, Esc) · verified: axe-core audits in Vitest of Home, Activity, Chat, Settings, the component gallery and the Island with a confirmation (`src/a11y.test.tsx`, `src/overlay/Overlay.test.tsx`: no violations; contrast is UX-54), keyboard-mode tests (2026-09-22)
- [ ] **UX-53** · M2 · State changes and final transcripts announced through UIA notifications (§10)
- [ ] **UX-54** · M7 · Contrast ≥ 4.5:1, never color alone, Windows high-contrast respected; warn if overlay and sounds are both off (§5, §10)
- [ ] **UX-55** · M5 · Voice-only use: every overlay action can be triggered by voice (§10)
