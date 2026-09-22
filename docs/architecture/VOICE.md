# Voice subsystem spec

Status: Draft v1, 2026-09-21. Research: [voice-pipeline-engines/REPORT.md](../research/voice-pipeline-engines/REPORT.md),
[voice-ui-and-app-presence/REPORT.md](../research/voice-ui-and-app-presence/REPORT.md) (earcons).
Engine defaults are **provisional until M0 benchmarks** ([ROADMAP.md](../ROADMAP.md)).

## 1. Pipeline

```text
AudioIo capture (native fmt) → resample 16 kHz mono (rubato) → EchoCancel (ref = KIVO output mix)
  → RingBuffer (≥ 3 s)
  → EnergyGate → VAD (Silero v6)
  → WakeStage1 (per-word detectors) → WakeStage2 (verifier + speaker check) ──► Wake
  → [earcon, capture never paused] → STT stream (kivo-infer) → Endpointing (VAD pause + Smart Turn)
  → Final transcript → intent router
TTS stream (kivo-infer) → playback mixer (TTS + earcons) → AudioIo render  ⟶ also AEC reference
```

- **Frames** are 10 ms internally and batched to 80 ms for models (the idle-power lever).
- **Threads:**
  - capture runs on an MMCSS "Audio" thread and writes the ring buffer lock-free;
  - detection runs on one worker thread with EcoQoS while idle;
  - model inference runs in kivo-infer.
- **Pre-roll:** the STT stream starts from the buffer 300 ms before the wake word ends. A
  "Hey Kivo, open Chrome" said in one breath is therefore transcribed in full, and the wake phrase
  is stripped by alignment with the known phrase.

## 2. Provider traits

```text
trait VadEngine        { fn process(&mut self, frame) -> VadProb }
trait WakeDetector     { fn process(&mut self, frames) -> Option<WakeHit{word_id, score, span}> }
trait WakeVerifier     { fn verify(&self, audio, hit) -> VerifyResult }
trait SpeakerVerifier  { fn embed(&self, audio) -> Embedding; fn score(&self, e, profile) -> f32 }
trait SttEngine        { fn capabilities() -> SttCaps; fn start(opts, cancel) -> SttStream }   // partial/stable/final events (plan §31)
trait TtsEngine        { fn capabilities() -> TtsCaps; fn synth_stream(text_chunks, voice, cancel) -> AudioStream }
trait TurnDetector     { fn end_probability(&self, audio_tail, transcript) -> f32 }
```

- Every engine declares `EngineInfo { id, kind: Local|Cloud|System, license, languages,
  streaming, accel: [Cpu, DirectML, Cuda, Npu, …], resource_estimate }`. The Control Center
  shows this in the selection screens (plan §24 and §33).
- Cloud engines go through the privacy check in the router before any audio leaves the device.

## 3. Engine catalogue (v1)

| Slot | Default (provisional) | Also shipped/optional |
|---|---|---|
| VAD | Silero v6 (ort) | — |
| AEC | OS AEC (Win11 22621+ where the device exposes it), else WebRTC AEC3 (`sonora`) | — |
| Wake, built-in | "Hey Kivo" model trained by KIVO with the openWakeWord pipeline | — |
| Wake, custom | sherpa-onnx open-vocabulary KWS | Optional "Enhance" trained model |
| Wake verifier | Few-shot template (enrollment) + stage-2 model | — |
| Speaker verify | CAM++ (sherpa-onnx or ort) | WeSpeaker ResNet34 |
| STT: Ultra Fast | Moonshine v2 Small, streaming (EN) | Parakeet TDT v3 int8 (multilingual) |
| STT: Balanced | Parakeet TDT v3 + DirectML/CUDA | — |
| STT: Accurate | Whisper large-v3-turbo (whisper.cpp) | Voxtral Realtime, Nemotron Streaming (GPU) |
| STT: Native | Windows AI Speech (needs package identity; see DISTRIBUTION.md) | Apple SpeechAnalyzer (macOS) |
| STT: Cloud | Deepgram Flux | AssemblyAI, OpenAI transcribe |
| Turn detector | Silero pause 250 ms + Smart Turn v3 | Cloud engine endpointing |
| TTS: Instant | Supertonic-2 | Piper **only as an optional GPL component** |
| TTS: Natural | Kokoro-82M (EN via misaki, no espeak) | — |
| TTS: Expressive | Chatterbox (Nano CPU / Turbo GPU) | Kyutai TTS (GPU) |
| TTS: System | WinRT SpeechSynthesizer / SAPI 5 | AVSpeechSynthesizer (macOS) |
| TTS: Cloud | Cartesia Sonic 3 | ElevenLabs Flash, Azure, OpenAI, Deepgram Aura |

The Rust integration path runs through `ort` and `sherpa-onnx` crates, plus `transcribe-rs` for STT
(evaluate it in M0 against a direct `ort` implementation).

## 4. Wake words

**Data model:**

```text
WakeWord { id, phrase, phonetic_spelling?, engine: Trained|Kws, model_path?, enabled,
           sensitivity (0–1 mapped to threshold/boost), samples: [SampleRef], verifier_template,
           created_at, quality: Good|Fair|Risky, fa_test_result }
```

- **Built-in word:** "Hey Kivo" cannot be deleted, but it can be disabled.
- **Custom word flow:** validate → hear it → try it → record 3–5 samples → tune → false-alarm test
  → optional Enhance job ([DECISIONS.md](../DECISIONS.md)).
- **Limits:** up to 5 enabled wake words by default (a CPU budget). Collisions are checked against
  the other words and against the fast-path command vocabulary.
- **"Stop" and emergency words:** while KIVO is speaking or acting, a small always-listening
  **command spotter** (sherpa KWS) watches for "Kivo stop", "stop" and "cancel" **without** the
  wake word. It runs on the AEC-cleaned signal, and a hit cancels the turn.

## 5. Enrollment and speaker profile

- Consent first, then 8 prompts (see the research report §2). Clips are DPAPI-encrypted.
- `SpeakerProfile { embeddings: Vec<Embedding> (≤ 40), threshold, model_id, created_at }`. It grows
  from turns above a high-confidence margin, and it is rebuilt from stored clips when the model
  changes.
- **Modes** (a setting):
  - **Off**: anyone can use KIVO.
  - **Prefer owner** (default after enrollment): unknown voices get a guest session with no
    memory or preferences and a confirmation for medium-risk actions.
  - **Owner only**: unknown voices are ignored after the wake word, with a soft earcon.
- **Speaker verification never authorizes high-risk actions** ([SECURITY.md](SECURITY.md)).
- **STT personalization:**
  - The enrollment transcript WER picks the recommended engine.
  - A `user_vocabulary` list (names, apps, projects) feeds hotwords/boosting where the engine
    supports it, and otherwise the Whisper prompt.
  - The brain receives the vocabulary for transcript repair.

## 6. Earcons

- **Sounds:** `listen_start` (rising 2 notes), `listen_stop` (1 note), `error` (low descending 2
  notes), `done` (optional), `thinking` (off by default, starts after 1 s), `hangup`.
- **Timing:** each sound is under 300 ms and plays together with the matching state change. The
  target is ≤ 150 ms from wake to audio. Sounds are pre-decoded into the playback mixer.
- **Echo handling:** capture continues during an earcon. AEC removes it, and the VAD/STT input is
  additionally gated for the earcon's duration plus 50 ms.
- **Placeholders:** the Kenney CC0 packs, replaced by a custom KIVO motif before beta.
- **Conversational cues** (added 2026-09-21): `question` (a rising 2-note "needs your answer"
  cue), `approved` (a soft tick) and `cancelled` (a short descending tone). See
  [CONVERSATION.md §7](CONVERSATION.md).
- **Sound sets:** the user picks a set, and each set covers every cue plus a **notification**
  sound for proactive messages:

  | Set | Character |
  |---|---|
  | **Soft** (default) | Rounded marimba-like tones |
  | Glass | Bright, airy bells |
  | Pulse | Short electronic blips |
  | Wood | Warm, percussive |
  | Minimal | Single quiet clicks |
  | Custom | User-imported `.wav`/`.ogg` per cue |

  Settings → Sounds offers a set picker with preview, per-cue on/off and override, and volume
  relative to the system. The notification sound can be set separately.

## 7. Barge-in and cancellation

1. While TTS is playing, VAD watches the AEC output with a stricter threshold.
2. When speech starts, the TTS volume ducks to -12 dB within 50 ms.
3. If speech is sustained for 300 ms or longer, or STT yields at least one word, the turn is
   cancelled: TTS stops with a 30 ms fade, the brain stream and tools are cancelled, and a new
   Listening turn starts using the buffered audio.
4. If the speech is not sustained, the volume is restored.

- The wake-word threshold rises while KIVO itself is playing audio, to prevent self-triggering.

## 8. Residency (plan §26 and §93)

| Component | Policy |
|---|---|
| Capture, VAD, wake word, command spotter | Always resident while listening is on |
| STT model | Warm 10 min after last use (setting); prewarm starts on the wake-stage-1 hit |
| TTS model | Warm 10 min after last use; prewarm on turn start |
| Speaker model | Loaded with the wake stages |
| GPU models | Unloaded on the `Gaming` profile or when the GPU is busy (plan §91) |

## 9. Languages

Design for every language from the start, and ship languages one at a time
([DECISIONS.md](../DECISIONS.md)): English → Hindi + Punjabi → European → CJK → Arabic/RTL.

- **Engines are chosen per language.** A `LanguagePack { code, stt_engines, tts_voices, kws_model?,
  grammar, vocabulary, status: Planned|Alpha|Supported }` maps each language to the engines that
  support it. Engine `EngineInfo.languages` is the source of truth, and the router picks the best
  engine for the active language.
- **Choosing the language:** the user sets a primary language plus optional secondary ones. There
  is automatic language ID only where the engine provides it (Whisper, SenseVoice) and it measures
  reliable. Otherwise the user's primary language is used.
- **Code-mixing** (Hindi–English "Hinglish", Punjabi–English): treated as a first-class case. It
  needs engines that handle mixed-script and mixed-language speech, benchmarked with owner-recorded
  test sets. Candidates to evaluate: Whisper large-v3/turbo, AI4Bharat IndicConformer-family models,
  and cloud engines. **To research before the Hindi/Punjabi milestone.**
- **TTS:** the Kokoro multilingual voices (including Hindi) need espeak-ng (GPL). So non-English
  local TTS ships as an optional add-on, or uses a permissively licensed alternative identified in
  that language's research. Cloud and system voices cover the gap.
- **Wake words:** "Hey Kivo" works for any language, since it's a name. **Custom wake words** in
  other languages depend on KWS model coverage; sherpa-onnx models exist mainly for English and
  Chinese. Other scripts may need the trained-model path.
- **Fast-path grammars:** these are data files per language (`grammar/<lang>/*.toml`), and the
  intent exemplars are multilingual (the embedding model must be multilingual when a second
  language ships).
- **Test sets:** each language needs STT test audio, command utterances, and wake-word
  positives/negatives before it moves from Alpha to Supported.

## 10. Budgets (targets to validate in M0/M1)

| Metric | Target |
|---|---|
| Idle CPU (listening on, no speech) | ≤ 2% of one mid-range laptop's total CPU |
| Idle RAM (runtime, excluding UI) | ≤ 150 MB |
| Wake → earcon | ≤ 150 ms |
| Wake → overlay visible | ≤ 200 ms |
| End of speech → final transcript (local, CPU) | ≤ 300 ms |
| Fast-path command → action done | ≤ 500 ms after the final transcript |
| End of speech → first TTS audio (cloud brain) | ≤ 1.2 s p50 |
| Cancel → silence | ≤ 100 ms |
| Wake false accepts | ≤ 0.5 / hour on the negative corpus |
| Wake false rejects | ≤ 5% on the positive corpus |

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md). Engine choices marked
"default" are confirmed or changed by the M0 benchmarks (BENCH items).

**Pipeline (§1)**

- [x] **VOICE-01** · M0 · Capture through `AudioIo` (WASAPI via windows-rs, cpal fallback) and playback, measured for idle CPU in the audio spike (§1) → done: `crates/kivo-platform-windows/src/audio.rs` (`WindowsAudio`: device lists with friendly names and defaults; WASAPI shared mode, event-driven, 32-bit float at the device rate with AUTOCONVERTPCM, stream thread under MMCSS "Audio", stops on drop) · verified: tests capture 24 000 frames in 500 ms at 48 kHz from the Realtek mic array and play to the default output; `kivo-bench audio`: holding the mic open costs 0.05% of the machine, 10 ms packets (p99 11.2 ms), 100% of frames delivered, playback starts in 39 ms · note: no cpal fallback: on Windows cpal is itself WASAPI, and AUTOCONVERTPCM accepts float on every device (DECISIONS)
- [x] **VOICE-02** · M1 · Resample to 16 kHz mono (`rubato`); 10 ms internal frames batched to 80 ms for models (§1) → done: `kivo-audio::RateConverter` (rubato FFT) turns any device format into 16 kHz mono in 10 ms steps; the detection thread works on those frames and sends the speech worker 80 ms batches · verified: resample tests (`a_48k_stream_becomes_16k_in_10ms_steps` …), the spoken end-to-end test (2026-09-23)
- [x] **VOICE-03** · M1 · Capture on an MMCSS "Audio" thread writing a lock-free ring buffer of ≥ 3 s; detection on one worker thread with EcoQoS while idle (§1) → done: WASAPI capture on an MMCSS "Audio" thread writes a lock-free ring (`capture_ring`, 3 s); detection runs on one worker thread, in EcoQoS (efficiency mode) while it waits and at full speed while listening · verified: capture ring tests, `silence_after_speech_ends_the_utterance` checks the EcoQoS switches, 0 ms idle CPU live (2026-09-23)
- [x] **VOICE-04** · M1 · Energy gate → done: `EnergyGate` in front of Silero VAD v6 on `ort` (bundled model) · verified: gate tests, the Silero model test, the spoken end-to-end test (2026-09-23)
- [ ] **VOICE-05** · M2 · Pre-roll: STT starts from the buffer 300 ms before the wake word ends, and the wake phrase is stripped by alignment ("Hey Kivo, open Chrome" in one breath works) (§1)

**Provider traits and engines (§2–3)**

- [x] **VOICE-06** · M1 · Traits `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `SttEngine` (partial/stable/final events), `TtsEngine` (streaming) and `TurnDetector`; every engine declares `EngineInfo` (id, kind, license, languages, streaming, accel, resource estimate) (§2) → done: `kivo-voice::traits`: `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `TurnDetector`, `SttEngine`/`SttStream` (partial, stable and final events) and streaming `TtsEngine`; every engine declares `EngineInfo` (id, kind, licence, languages, streaming, accel, resource estimate) · verified: engine and language tests (2026-09-23)
- [x] **VOICE-07** · M1 · Cloud engines pass the privacy check before any audio leaves the device (§2, SECURITY §6) → done: `kivo_security::privacy::speech_egress`: a cloud speech engine is used only when the privacy mode (Cloud or Custom) and the Cloud AI capability allow it, checked where the runtime picks engines, so no audio or text reaches one otherwise; a refused cloud voice falls back to the Windows voices · verified: `local_engines_always_pass_and_cloud_ones_follow_the_mode`, `the_privacy_mode_never_blocks_local_speech_engines`, `only_cloud_engines_send_data_off_the_device` (2026-09-23)
- [x] **VOICE-08** · M1 · Default streaming STT engine (the M0 winner among Moonshine v2, Parakeet TDT v3 and Whisper-turbo) running in `kivo-infer`, with partial transcripts shown live (§3) → done: Moonshine Base (MIT) streaming in `kivo-infer` on `ort`, partial transcripts shown live on the Island; it is the provisional default until the deferred M0 comparison runs (DECISIONS "Speech engines on ort", "Benchmarks deferred") · verified: `transcribes_the_sample_recording_when_the_model_is_installed`, the spoken end-to-end test (end of speech → action 40 ms), live (2026-09-23)
- [ ] **VOICE-09** · M1 · TTS: system voices (WinRT SpeechSynthesizer / SAPI 5) and Kokoro-82M (EN via misaki, no espeak), streaming from `kivo-infer` (§3)
- [ ] **VOICE-10** · M8 · More STT tiers: Parakeet (Balanced, DirectML/CUDA), Whisper large-v3-turbo (Accurate), Deepgram Flux, AssemblyAI and OpenAI (Cloud) (§3)
- [ ] **VOICE-11** · M8 · More TTS tiers: Supertonic-2 (Instant), Chatterbox (Expressive), Cartesia, ElevenLabs, Azure, OpenAI and Deepgram (Cloud) (§3)
- [ ] **VOICE-12** · Post · Windows AI Speech STT (needs package identity, DIST-06) and GPL add-ons (Piper, espeak-ng) as separately downloaded components (§3)

**Activation**

- [x] **VOICE-41** · M1 · Push-to-talk: hold Ctrl+Space to talk (`Hotkeys` trait: RegisterHotKey with a low-level-hook fallback), optional toggle mode, auto-end on silence; registration conflicts (e.g. IME switching on CJK layouts) detected with a rebind prompt (DECISIONS "Activation", UX §5) → done: hold Ctrl+Space (setting `voice.push-to-talk`) to talk through `WindowsHotkeys` (RegisterHotKey on its own thread; release by a 15 ms key check only while held); when another app owns the keys a low-level keyboard hook catches them first (`Binding::Shared`) and Home says so with a rebind prompt that applies new keys at once; toggle mode; utterances end on silence (VAD) · verified: hotkey tests incl. the hook's key logic and a shared combination, `shared_push_to_talk_keys_work_and_are_reported`, `toggle_mode_starts_on_one_press_and_ends_on_the_next`, `silence_after_speech_ends_the_utterance`, live (2026-09-23)

**Wake words (§4)**

- [ ] **VOICE-13** · M2 · Built-in "Hey Kivo" model trained with the openWakeWord pipeline (KIVO-owned); it can be disabled but not deleted (§3, §4)
- [ ] **VOICE-14** · M2 · Two-stage detection: per-word detectors, then the verifier and speaker check (§1, §3)
- [ ] **VOICE-15** · M2 · `WakeWord` data model as in §4, stored in the `wake_words` table (§4)
- [ ] **VOICE-16** · M2 · Custom wake words via sherpa-onnx KWS: validate (Good / Fair / Risky) → hear it (TTS) → try it → record 3–5 samples → tune sensitivity → false-alarm test (§4)
- [ ] **VOICE-17** · M8 · Optional "Enhance" background training job for a custom word (§4)
- [ ] **VOICE-18** · M2 · Up to 5 enabled wake words; collisions are checked against the other words and the fast-path vocabulary (§4)
- [ ] **VOICE-19** · M2 · Command spotter (sherpa KWS) runs while KIVO speaks or acts, on the AEC-cleaned signal; "Kivo stop", "stop" and "cancel" cancel the turn without the wake word (§4)

**Enrollment and speaker profile (§5)**

- [ ] **VOICE-20** · M2 · Enrollment: consent, then 8 prompts; clips DPAPI-encrypted in `%LOCALAPPDATA%\KIVO\data\voice\`; one-click deletion (§5, SECURITY §5)
- [ ] **VOICE-21** · M2 · `SpeakerProfile` (≤ 40 embeddings, CAM++) that grows from high-confidence turns and is rebuilt from clips when the model changes (§5)
- [ ] **VOICE-22** · M2 · Speaker modes Off / Prefer owner (default after enrollment; unknown voices get a guest session) / Owner only (§5)
- [ ] **VOICE-23** · M3 · STT personalization: the enrollment WER picks the recommended engine; `user_vocabulary` feeds hotwords or the Whisper prompt, and the brain receives it for transcript repair (§5)

**Earcons and sound sets (§6)**

- [x] **VOICE-24** · M1 · Earcons `listen_start`, `listen_stop`, `error`, `done`, `thinking` (off; after 1 s) and `hangup`, each < 300 ms, pre-decoded into the playback mixer; wake → done: KIVO's own generated cues (DECISIONS "Earcons generated, not Kenney"): listen_start, listen_stop, error, done, thinking (off by default, after 1 s of thinking) and hangup, each under 300 ms, rendered once at start and played through the mixer · verified: `every_cue_is_short_audible_and_starts_and_ends_quietly`, the spoken end-to-end test measures press → listening cue at 6 ms (2026-09-23); wake → audio for the wake word joins with VOICE-13 (M2)
- [x] **VOICE-25** · M1 · Capture continues during earcons; voice detection is gated for the earcon plus 50 ms while recognition keeps every sample (DECISIONS "Earcon gating"); AEC removes them from the recognized audio with VOICE-30 (§6) → done: the microphone keeps capturing during cues; voice detection ignores it for the cue plus 50 ms (`Speaker::muting_microphone`) while recognition gets every sample · verified: the spoken end-to-end test (the first word survives the listening cue), speaker tests (2026-09-23)
- [ ] **VOICE-26** · M2 · Conversational cues `question`, `approved` and `cancelled` (§6, CONVERSATION §7)
- [ ] **VOICE-27** · M2 · Sound set "Soft" (default) covering every cue plus a notification sound; Settings → Sounds offers the set picker with preview, per-cue toggles and volume relative to the system (§6)
- [ ] **VOICE-28** · M8 · Sound sets Glass, Pulse, Wood, Minimal and Custom (user `.wav`/`.ogg` per cue); a separately chosen notification sound (§6)
- [ ] **VOICE-29** · M8 · A custom KIVO earcon motif replaces the placeholders before beta (§6)

**AEC, barge-in and endpointing (§3, §7)**

- [ ] **VOICE-30** · M2 · AEC in production: the OS AEC on Windows 11 22621+ where the device exposes it, else WebRTC AEC3 (`sonora`), with KIVO's output mix as the reference (§1, §3)
- [ ] **VOICE-31** · M2 · Barge-in: stricter VAD during TTS; duck TTS −12 dB within 50 ms; after ≥ 300 ms of speech or one STT word, cancel the turn (30 ms fade, brain and tools cancelled) and start listening from the buffer; otherwise restore the volume (§7)
- [ ] **VOICE-32** · M2 · The wake threshold rises while KIVO plays audio (§7)
- [ ] **VOICE-33** · M2 · Endpointing: Silero pause 250 ms + Smart Turn v3 (§3)

**Residency (§8)**

- [x] **VOICE-34** · M1 · Residency: capture, VAD, wake and spotter always resident while listening; STT and TTS warm for 10 min after use (setting); STT prewarm on the wake-stage-1 hit and TTS prewarm on turn start (§8) → done: capture and VAD run only while listening; STT and TTS load when a request starts (`Infer::warm`, the prewarm on turn start), stay warm for the setting's minutes (10 by default) after use and then unload, and with nothing loaded the worker exits · verified: `models_load_on_use_and_unload_after_the_warm_time`, live (2026-09-22); the wake-stage prewarm joins with the wake word (VOICE-13, M2)
- [ ] **VOICE-35** · M8 · GPU models unload on the Gaming profile or when the GPU is busy (§8)

**Languages (§9)**

- [x] **VOICE-36** · M1 · `LanguagePack { code, stt_engines, tts_voices, kws_model?, grammar, vocabulary, status }` with English first; the router picks the engine for the active language (§9) → done: `LanguagePack { code, name, stt_engines, tts_voices, kws_model, grammar, vocabulary, status }` with English (Alpha) first and Hindi/Punjabi Planned; the runtime picks the STT engine from the active language's pack · verified: `english_ships_first_and_the_rest_are_planned`, `the_router_prefers_the_packs_order_then_anything_that_fits` (2026-09-23)
- [x] **VOICE-37** · M1 · Primary + secondary language settings; automatic language ID only where the engine provides it and it measures reliable (§9) → done: settings `general.language` (primary) and `general.languages` (secondary), validated; no shipped engine offers language ID, so none is used (detected languages would count only when the user speaks them) · verified: config tests, `detected_languages_count_only_when_the_user_speaks_them` (2026-09-23); the language picker is UX-51
- [ ] **VOICE-38** · L2 · Hindi + Punjabi, including Hindi–English and Punjabi–English code-mixing, benchmarked on owner-recorded test sets (§9)
- [ ] **VOICE-39** · L1 · Each language has STT test audio, command utterances and wake positives/negatives before it moves from Alpha to Supported (§9)

**Budgets (§10)**

- [~] **VOICE-40** · M1 · Every §10 budget is measured by `kivo-bench` on the reference tiers and met, or the miss is logged in DECISIONS.md (§10) → partial: every §10 budget has its `kivo-bench` suite, and the end-to-end tests measure the M1 path on this PC (press → cue 6 ms, end of speech → action 40 ms, cancel < 35 ms, 0 ms idle CPU, 67 MB idle runtime) · missing: the reference-tier runs, deferred by the owner (DECISIONS "Benchmarks deferred")
