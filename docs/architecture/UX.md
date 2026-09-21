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

**Chosen concept: "Island"** (owner decision, 2026-09-21; the mockup is
[kivo-wake-concepts.html](../design/mockups/kivo-wake-concepts.html), tab 03). Throughout the
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
| Intelligence | Brains (Brains / Context) · Agents · Voice · Extensions (Installed / Browse; Apps, Tools, Skills) |
| Control | Permissions (Mode / Capabilities / Privacy) · Memory · Usage |
| System | Settings (General / Appearance / Island / Sounds / Notifications / Accessibility / Shortcuts / Performance / Diagnostics / About) |

**Design system (plan §134–135):**

- **Stack:** React + TypeScript, Tailwind v4, shadcn/ui on Base UI, and Motion (`LazyMotion`).
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
| General | Start at sign-in (off) · On close keep running (on) · Tray icon (on) · Language (EN) · Low-memory mode (off) |
| Voice | Wake words list with Add/Edit (Hey Kivo) · Push-to-talk hotkey (Ctrl+Space) · Toggle mode (off) · Auto-end on silence (on) · Mic · Speaker profile mode (Prefer owner, after enrollment) · STT engine · TTS engine + voice · User vocabulary |
| Overlay | Style: Pill + card / Pill only / Card only / Off · Position (bottom-center, remember drag) · Monitor (active) · Wake glow (off) · Reduce motion (follow Windows) · In fullscreen & Focus: hide / tiny pill (hide) |
| Sounds | Master (on) · start/stop/error/done (on) · thinking (off) · volume · sound set |
| Brains | Providers, profiles, routing rules, fallbacks |
| Permissions | Profile (Balanced) · grants list · per-tool overrides · emergency stop hotkey |
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
- **Keyboard:** full keyboard navigation with visible focus rings.
- **Color and contrast:** no information is conveyed by color alone. Contrast is at least 4.5:1,
  and the Windows high-contrast theme is respected.
- **Voice-only use:** every action in the overlay can be triggered by voice.
