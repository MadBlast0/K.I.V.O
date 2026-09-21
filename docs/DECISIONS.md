# KIVO decision log

Decisions made so far, newest first. Each links to the research behind it. When a
decision changes, update the entry and note the date. Do not silently rewrite it.

---

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
  focus, and draws zero frames while idle.
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
  To research: the wake-word engine, the speaker-verification model and the
  per-user STT adaptation method.

## 2026-09-21 — UI stack

**Decision:** React + shadcn/ui (Base UI) + Tailwind v4 + Motion, with the Inter or
Geist font. Voice visuals are adapted from ElevenLabs UI (MIT) and optionally the
LiveKit aura shader (Apache-2.0). The overlay shell, state machine and audio-level
bridge are custom.
