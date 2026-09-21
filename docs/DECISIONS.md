# KIVO decision log

Decisions made so far, newest first. Each links to the research behind it. When a
decision changes, update the entry and note the date. Do not silently rewrite it.

---

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
  (Engines researched; see "Voice pipeline direction" above.)

## 2026-09-21 — UI stack

**Decision:** React + shadcn/ui (Base UI) + Tailwind v4 + Motion, with the Inter or
Geist font. Voice visuals are adapted from ElevenLabs UI (MIT) and optionally the
LiveKit aura shader (Apache-2.0). The overlay shell, state machine and audio-level
bridge are custom.
