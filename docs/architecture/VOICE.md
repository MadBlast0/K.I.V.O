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
- **Pre-roll:** the STT stream starts from the buffer just before the wake phrase begins (the
  keyword spotter's word boundaries are approximate; starting 300 ms before its end clipped the
  request, DECISIONS "Hands-free voice: measured choices"). A "Hey Kivo, open Chrome" said in one
  breath is therefore transcribed in full, and the wake phrase is stripped by alignment with the
  known phrase, by words or by sound.

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
| AEC | WebRTC AEC3 (`sonora`) on KIVO's output mix, stepping aside when no echo path is found (headphones); the OS AEC is not used (DECISIONS "Echo cancellation") | — |
| Wake, built-in | "Hey Kivo" on the open-vocabulary keyword spotter, with its pronunciation variants (no trained model: DECISIONS "No training") | — |
| Wake, custom | sherpa-onnx open-vocabulary KWS | Optional "Enhance" trained model |
| Wake verifier | The speaker check (CAM++) on the whole request; no trained verifier (DECISIONS "No training") | — |
| Speaker verify | CAM++ (ort) | WeSpeaker ResNet34 |
| STT: Ultra Fast | Moonshine Base (EN, MIT); Tiny for the Lightweight profile; Base/Tiny for es, ja, zh, ar, uk, vi, ko (non-commercial, §11) | Parakeet TDT v3 int8 (multilingual) |
| STT: Balanced | Parakeet TDT v3 + DirectML/CUDA | — |
| STT: Accurate | Whisper large-v3-turbo (whisper.cpp) | Voxtral Realtime, Nemotron Streaming (GPU) |
| STT: Native | Windows AI Speech (needs package identity; see DISTRIBUTION.md) | Apple SpeechAnalyzer (macOS) |
| STT: Cloud | Deepgram Flux | AssemblyAI, OpenAI transcribe |
| Turn detector | Silero pause 250 ms + Smart Turn v3 | Cloud engine endpointing |
| TTS: Instant | Supertonic 3 (31 languages; the Multilingual profile, §11) | Piper **only as an optional GPL component** |
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
  wake word. It runs on the AEC-cleaned signal, and a hit cancels the turn. Talking over KIVO
  usually starts a barge-in before the spotter finishes the phrase, so a request that is only a
  stop word ("Kivo, stop", "never mind", "ruko") also ends the turn quietly.

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

1. While TTS is playing, VAD watches the AEC output (the raw microphone when there is no echo
   path, e.g. headphones) with a stricter threshold.
2. When speech starts, the TTS volume ducks to -12 dB within 50 ms.
3. If speech is sustained for 300 ms or longer, or STT yields at least one word, the turn is
   cancelled: TTS stops with a 30 ms fade (whatever KIVO was playing, turn or not), the brain
   stream and tools are cancelled, and a new Listening turn starts using the buffered audio.
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

## 11. Choosing speech engines (owner brief, 2026-09-23)

KIVO never forces one STT or TTS engine, and it never shows a model marketplace. Users pick from
**2–4 curated profiles per slot**; the engine underneath is secondary information. STT, TTS and
the brain stay independent: changing one never requires changing another (§2 traits).

**Profiles (user-facing), each backed by a registry entry:**

| Slot | Profile | Meaning | Candidates to evaluate (not locked) |
|---|---|---|---|
| STT | Recommended / Balanced | accurate, streaming, moderate resources | Moonshine Base (current), Moonshine Streaming sizes, Parakeet TDT v3 |
| STT | Lightweight | lowest CPU/RAM, fast | Moonshine Tiny / Streaming small |
| STT | High accuracy | more compute for better text | Whisper large-v3-turbo, Parakeet on GPU |
| STT | Multilingual | languages beyond English | Parakeet TDT v3, Whisper |
| TTS | Recommended / Natural | human-like, balanced | Kokoro-82M (shipped), Supertonic 3 |
| TTS | Lightweight | fastest first audio, low resources | Windows voices, Supertonic |
| TTS | Multilingual | broad language coverage | Supertonic 3 (reported 31 languages, ~99M params, ONNX on CPU — verify) |
| TTS | Expressive | emotional delivery where supported | Chatterbox (§3) |

**Model registry.** Each engine declares `EngineInfo` plus: profile tags, languages, streaming,
local/cloud/hybrid, execution devices (CPU, DirectML, CUDA), download size, licence, voices
(for TTS), and **KIVO's own benchmark results** on this PC and the reference tiers. Scores and
labels (★ ratings, Excellent/Good) come only from KIVO's thresholds applied to measured results
or clearly labelled qualitative classes; anything unmeasured shows **"Not benchmarked by KIVO"**.
Adding an engine is a registry entry plus an adapter; onboarding, the Voice page, routing and the
resource manager need no changes.

**Recommendation.** An explicit function: hardware (CPU, RAM, GPU, VRAM, disk, battery),
OS, languages, privacy/offline preference, stated priority (speed, accuracy, resources, voice),
installed models and benchmark results → recommended STT and TTS, plus fallbacks. Local models
run on the GPU first (owner, 2026-09-24; DECISIONS "Local models on the GPU first"), switchable
in Settings → Performance: on a PC with a usable graphics card (≥ 3 GB of its own, not on
battery, not the low tier) it recommends whisper.cpp's Whisper on Vulkan (small for Recommended,
large-v3-turbo for accuracy), with a light CPU recognizer as its fallback, and the GPU policy
moves it to the processor while the card isn't available (a game in front, the Gaming or Battery
profile, a busy GPU). ONNX engines DirectML runs badly or not at all (Moonshine, Whisper's ONNX
files, Kokoro) and the always-on detectors (VAD, wake word, speaker check) stay on the processor. It explains itself. Never irreversible.

**Safe switching.** Choosing an engine: check compatibility → download (with the licence shown
first) → load → validate (microphone test, a transcription or synthesis test) → "Ready — Use it".
The previous working engine stays active until the new one validates; a failed switch never
leaves KIVO without working STT/TTS. The active model can't be removed without a working
replacement unless the user confirms.

**Voices.** The TTS engine and the voice are separate choices; voice cards show name, style,
language and a Preview that plays the same sentence ("Hi, I'm KIVO. How can I help?") through
each voice. Speaking speed is a setting.

**Languages.** Primary and secondary languages are chosen separately from engines; if an engine
doesn't cover a language KIVO says so plainly and offers compatible engines — it never switches
silently.

**Fallback.** Primary STT/TTS failure → the configured fallback (Windows voices for TTS; another
installed STT) with a visible notice ("Your voice engine stopped; KIVO is using Windows voices")
— the saved choice is not changed silently (ARCH-09 already speaks failures in-process).

**Privacy labels.** Every option shows Local (audio stays on this PC), Cloud (audio goes to the
provider) or Hybrid, from the engine's real architecture (VOICE-07 enforces the mode).

**Simple and advanced.** Onboarding and the Voice page show profiles; Advanced shows the exact
engine, model, device, model path, chunk/latency settings, resource limits, fallback and cache.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md). Engine choices marked
"default" are confirmed or changed by the M0 benchmarks (BENCH items).

**Pipeline (§1)**

- [x] **VOICE-01** · M0 · Capture through `AudioIo` (WASAPI via windows-rs, cpal fallback) and playback, measured for idle CPU in the audio spike (§1) → done: `crates/kivo-platform-windows/src/audio.rs` (`WindowsAudio`: device lists with friendly names and defaults; WASAPI shared mode, event-driven, 32-bit float at the device rate with AUTOCONVERTPCM, stream thread under MMCSS "Audio", stops on drop) · verified: tests capture 24 000 frames in 500 ms at 48 kHz from the Realtek mic array and play to the default output; `kivo-bench audio`: holding the mic open costs 0.05% of the machine, 10 ms packets (p99 11.2 ms), 100% of frames delivered, playback starts in 39 ms · note: no cpal fallback: on Windows cpal is itself WASAPI, and AUTOCONVERTPCM accepts float on every device (DECISIONS)
- [x] **VOICE-02** · M1 · Resample to 16 kHz mono (`rubato`); 10 ms internal frames batched to 80 ms for models (§1) → done: `kivo-audio::RateConverter` (rubato FFT) turns any device format into 16 kHz mono in 10 ms steps; the detection thread works on those frames and sends the speech worker 80 ms batches · verified: resample tests (`a_48k_stream_becomes_16k_in_10ms_steps` …), the spoken end-to-end test (2026-09-23)
- [x] **VOICE-03** · M1 · Capture on an MMCSS "Audio" thread writing a lock-free ring buffer of ≥ 3 s; detection on one worker thread with EcoQoS while idle (§1) → done: WASAPI capture on an MMCSS "Audio" thread writes a lock-free ring (`capture_ring`, 3 s); detection runs on one worker thread, in EcoQoS (efficiency mode) while it waits and at full speed while listening · verified: capture ring tests, `silence_after_speech_ends_the_utterance` checks the EcoQoS switches, 0 ms idle CPU live (2026-09-23)
- [x] **VOICE-04** · M1 · Energy gate → done: `EnergyGate` in front of Silero VAD v6 on `ort` (bundled model) · verified: gate tests, the Silero model test, the spoken end-to-end test (2026-09-23)
- [x] **VOICE-05** · M2 · Pre-roll: STT starts from the buffer 300 ms before the wake word ends, and the wake phrase is stripped by alignment ("Hey Kivo, open Chrome" in one breath works) (§1) → done: hands-free requests start from the buffer 150 ms before the wake phrase begins (more than the 300 ms before it ends), and `kivo_intent::strip_wake_phrase` removes the phrase by words, sound skeleton, a clipped tail or joined-sound alignment · verified: `hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word` ("Hey Kivo, mute." in one breath), wake-strip tests (2026-09-23)

**Provider traits and engines (§2–3)**

- [x] **VOICE-06** · M1 · Traits `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `SttEngine` (partial/stable/final events), `TtsEngine` (streaming) and `TurnDetector`; every engine declares `EngineInfo` (id, kind, license, languages, streaming, accel, resource estimate) (§2) → done: `kivo-voice::traits`: `VadEngine`, `WakeDetector`, `WakeVerifier`, `SpeakerVerifier`, `TurnDetector`, `SttEngine`/`SttStream` (partial, stable and final events) and streaming `TtsEngine`; every engine declares `EngineInfo` (id, kind, licence, languages, streaming, accel, resource estimate) · verified: engine and language tests (2026-09-23)
- [x] **VOICE-07** · M1 · Cloud engines pass the privacy check before any audio leaves the device (§2, SECURITY §6) → done: `kivo_security::privacy::speech_egress`: a cloud speech engine is used only when the privacy mode (Cloud or Custom) and the Cloud AI capability allow it, checked where the runtime picks engines, so no audio or text reaches one otherwise; a refused cloud voice falls back to the Windows voices · verified: `local_engines_always_pass_and_cloud_ones_follow_the_mode`, `the_privacy_mode_never_blocks_local_speech_engines`, `only_cloud_engines_send_data_off_the_device` (2026-09-23)
- [x] **VOICE-08** · M1 · Default streaming STT engine (the M0 winner among Moonshine v2, Parakeet TDT v3 and Whisper-turbo) running in `kivo-infer`, with partial transcripts shown live (§3) → done: Moonshine Base (MIT) streaming in `kivo-infer` on `ort`, partial transcripts shown live on the Island; it is the provisional default until the deferred M0 comparison runs (DECISIONS "Speech engines on ort", "Benchmarks deferred") · verified: `transcribes_the_sample_recording_when_the_model_is_installed`, the spoken end-to-end test (end of speech → action 40 ms), live (2026-09-23)
- [x] **VOICE-09** · M1 · TTS: system voices (WinRT SpeechSynthesizer / SAPI 5) and Kokoro-82M (EN via misaki, no espeak), streaming from `kivo-infer` (§3) → done: TTS engines in `kivo-infer`: the Windows voices (WinRT, default) and Kokoro-82M (Apache-2.0, quantized ONNX on `ort`) with KIVO's own English phonemizer (misaki's dictionaries and rules ported to Rust, NRL rules for unknown words, no espeak; DECISIONS "Kokoro's phonemizer"); both stream sentence by sentence; Kokoro downloads through the model manager (model, five voices, dictionaries, sha256-checked) when chosen, and the Windows voices speak meanwhile · verified: phonemizer, number and rule tests, `speaks_a_sentence_when_the_model_is_installed`, `replies_can_be_spoken_by_kokoro` through the real worker (2026-09-23)
- [x] **VOICE-10** · M8 · More STT tiers: Parakeet (Balanced, DirectML/CUDA), Whisper large-v3-turbo (Accurate), Deepgram Flux, AssemblyAI and OpenAI (Cloud) (§3) → done: Parakeet TDT 0.6B v3 (NeMo features, greedy TDT; DirectML with CPU fallback) as Multilingual, Whisper large-v3-turbo as High accuracy, and cloud recognizers Deepgram Flux, AssemblyAI streaming (WebSockets) and OpenAI transcribe (HTTP), keys in Credential Manager and tested before they're saved, never in model context · verified: `parakeet`/`whisper` tests pin real transcripts with the real models (and a GPU run), `cloud` tests (7) against mock servers speaking each service's protocol; no live cloud call (needs the owner's keys) (2026-09-24)
- [x] **VOICE-11** · M8 · More TTS tiers: Supertonic-2 (Instant), Chatterbox (Expressive), Cartesia, ElevenLabs, Azure, OpenAI and Deepgram (Cloud) (§3) → done: Chatterbox Turbo (the publisher's ONNX export) as Expressive, with a Windows voice as its reference; cloud voices Cartesia Sonic, ElevenLabs Flash, Azure Neural (region), OpenAI and Deepgram Aura; the Instant tier is Supertonic 3 (M2), which replaced Supertonic-2 (DECISIONS "M8 build") · verified: `chatterbox` round trip (Parakeet hears "the weather is lovely today"), `cloud` tests against mock servers; no live cloud call (needs the owner's keys) (2026-09-24)
- [ ] **VOICE-12** · Post · Windows AI Speech STT (needs package identity, DIST-06) and GPL add-ons (Piper, espeak-ng) as separately downloaded components (§3)

**Activation**

- [x] **VOICE-41** · M1 · Push-to-talk: hold Ctrl+Space to talk (`Hotkeys` trait: RegisterHotKey with a low-level-hook fallback), optional toggle mode, auto-end on silence; registration conflicts (e.g. IME switching on CJK layouts) detected with a rebind prompt (DECISIONS "Activation", UX §5) → done: hold Ctrl+Space (setting `voice.push-to-talk`) to talk through `WindowsHotkeys` (RegisterHotKey on its own thread; release by a 15 ms key check only while held); when another app owns the keys a low-level keyboard hook catches them first (`Binding::Shared`) and Home says so with a rebind prompt that applies new keys at once; toggle mode; utterances end on silence (VAD) · verified: hotkey tests incl. the hook's key logic and a shared combination, `shared_push_to_talk_keys_work_and_are_reported`, `toggle_mode_starts_on_one_press_and_ends_on_the_next`, `silence_after_speech_ends_the_utterance`, live (2026-09-23)

**Wake words (§4)**

- [-] **VOICE-13** · M2 · Built-in "Hey Kivo" model trained with the openWakeWord pipeline (KIVO-owned); it can be disabled but not deleted (§3, §4) → dropped: KIVO trains no models (owner); "Hey Kivo" runs on the open-vocabulary keyword spotter with its pronunciation variants, built in (it can be turned off, not deleted), see DECISIONS.md "No training (owner)", 2026-09-23
- [x] **VOICE-14** · M2 · Two-stage detection: the keyword spotter per word (its pronunciation variants and per-word thresholds), then the speaker check on the whole request (§1, §3; no trained verifier, DECISIONS "No training") → done: stage 1 is `kivo_voice::kws` (KIVO's `ort` port of sherpa-onnx's streaming Zipformer keyword decoder) with each word's variants and threshold; stage 2 is the CAM++ speaker check on the whole request (`voiceid.rs`) · verified: keyword spotter tests against sherpa's detections, `hey_kivo_wakes_kivo…`, voice-ID tests with a fake verifier (2026-09-23)
- [x] **VOICE-15** · M2 · `WakeWord` data model as in §4, stored in the `wake_words` table (§4) → done: `kivo_store::wake::WakeWord` in the `wake_words` table (migration 4), "Hey Kivo" built in · verified: store wake tests (the built-in word can't be deleted) (2026-09-23)
- [x] **VOICE-16** · M2 · Custom wake words via sherpa-onnx KWS: validate (Good / Fair / Risky) → done: `wake.check` (Good/Fair/Risky with reasons, `kivo_voice::wakeword::assess`), `wake.hear` (KIVO says it), `wake.try`, `wake.sample` (encrypted samples), `wake.tune`, `wake.falseAlarms` (minutes of Windows-voice speech through the spotter); the Voice page's Add and Edit dialogs (`components/voice/WakeWords.tsx`) · verified: wakeword tests, `tuned_sensitivity` tests, the Voice page test (turning a word on downloads the listener first) (2026-09-23)
- [-] **VOICE-17** · M8 · Optional "Enhance" background training job for a custom word (§4) → dropped: KIVO trains no models (DECISIONS "No training (owner)") (2026-09-24)
- [x] **VOICE-18** · M2 · Up to 5 enabled wake words; collisions are checked against the other words and the fast-path vocabulary (§4) → done: at most 5 enabled (`MAX_ENABLED`, store and UI); new words are checked against the other words and the fast-path command phrases · verified: wakeword collision tests, store limit test (2026-09-23)
- [x] **VOICE-19** · M2 · Command spotter (sherpa KWS) runs while KIVO speaks or acts, on the AEC-cleaned signal; "Kivo stop", "stop" and "cancel" cancel the turn without the wake word (§4) → done: the stop words ("Kivo stop", "stop", "cancel") run in the spotter only while KIVO is busy, on the echo-cancelled signal; a request that is only a stop word (`kivo_intent::is_stop_request`, EN + HI) ends the turn quietly, since talking over KIVO usually starts a barge-in first · verified: `saying_stop_while_kivo_talks_stops_it` (KIVO silent ~2 s into a 12 s reply), stop-request tests, `stop_words_are_stricter_than_the_default_wake_word` (2026-09-23)

**Enrollment and speaker profile (§5)**

- [x] **VOICE-20** · M2 · Enrollment: consent, then 8 prompts; clips DPAPI-encrypted in `%LOCALAPPDATA%\KIVO\data\voice\`; one-click deletion (§5, SECURITY §5) → done: consent, then 8 prompts recorded through the listener; clips DPAPI-encrypted in the voice folder; Delete removes clips and profile (`voiceid.rs`, `voiceId.*`, `components/voice/Enrollment.tsx` in onboarding and on the Voice page) · verified: voice-ID tests, onboarding test (no recording before consent) (2026-09-23)
- [x] **VOICE-21** · M2 · `SpeakerProfile` (≤ 40 embeddings, CAM++) that grows from high-confidence turns and is rebuilt from clips when the model changes (§5) → done: up to 40 CAM++ embeddings (the joined enrollment plus clips of 2 s or more), grown from confident owner matches, rebuilt from the clips when the model changes · verified: `confident_long_matches_grow_the_profile_up_to_forty`, rebuild test (2026-09-23)
- [x] **VOICE-22** · M2 · Speaker modes Off / Prefer owner (default after enrollment; unknown voices get a guest session) / Owner only (§5) → done: Off / Prefer owner (set when enrollment finishes; others get a guest turn with the Island's Guest chip) / Owner only (other voices dropped); the choice on the Voice page · verified: voice-ID tests, engine guest tests, Island guest test (2026-09-23)
- [x] **VOICE-23** · M3 · STT personalization: the enrollment WER picks the recommended engine; `user_vocabulary` feeds hotwords or the Whisper prompt, and the brain receives it for transcript repair (§5) → done: after enrollment every installed recogniser is scored on the owner's recordings and a clearly better one is recommended; `user_vocabulary` (Voice → Your words) goes to STT start for engines with hotwords/prompts and to the brain for repair (DECISIONS "Your voice picks the recogniser") · verified: `every_installed_recognizer_is_scored_on_the_owners_voice`, recommendation and WER tests (2026-09-23)

**Earcons and sound sets (§6)**

- [x] **VOICE-24** · M1 · Earcons `listen_start`, `listen_stop`, `error`, `done`, `thinking` (off; after 1 s) and `hangup`, each < 300 ms, pre-decoded into the playback mixer; wake → done: KIVO's own generated cues (DECISIONS "Earcons generated, not Kenney"): listen_start, listen_stop, error, done, thinking (off by default, after 1 s of thinking) and hangup, each under 300 ms, rendered once at start and played through the mixer · verified: `every_cue_is_short_audible_and_starts_and_ends_quietly`, the spoken end-to-end test measures press → listening cue at 6 ms (2026-09-23); a wake word plays the same listening cue as its turn starts (M2, `hey_kivo_wakes_kivo_hands_free_and_a_follow_up_needs_no_wake_word`)
- [x] **VOICE-25** · M1 · Capture continues during earcons; voice detection is gated for the earcon plus 50 ms while recognition keeps every sample (DECISIONS "Earcon gating"); AEC removes them from the recognized audio with VOICE-30 (§6) → done: the microphone keeps capturing during cues; voice detection ignores it for the cue plus 50 ms (`Speaker::muting_microphone`) while recognition gets every sample · verified: the spoken end-to-end test (the first word survives the listening cue), speaker tests (2026-09-23)
- [x] **VOICE-26** · M2 · Conversational cues `question`, `approved` and `cancelled` (§6, CONVERSATION §7) → done: `question`, `approved` and `cancelled` cues in every set, played by the decision flow · verified: `every_cue_is_short_audible_and_starts_and_ends_quietly`, `decisions_are_answered_by_voice_but_high_risk_needs_a_click` (2026-09-23)
- [x] **VOICE-27** · M2 · Sound set "Soft" (default) covering every cue plus a notification sound; Settings → done: Soft is the default set, with a notification cue; Settings → Sounds (`pages/Settings.tsx`): master switch, set picker with Preview, volume relative to Windows, each cue on or off with its own preview · verified: `Settings.test.tsx`, sound-set tests (2026-09-23)
- [x] **VOICE-28** · M8 · Sound sets Glass, Pulse, Wood, Minimal and Custom (user `.wav`/`.ogg` per cue); a separately chosen notification sound (§6) → done: sound sets Soft, Glass, Pulse, Wood, Minimal and Custom: the user imports a `.wav` (8/16/24/32-bit, float) or `.ogg` per cue (≤ 2 MB, decoded and checked, copied into KIVO's data folder), "Use the set's" per cue, and a separately chosen notification sound · verified: `sounds` tests (WAV/OGG decode, fixture `chime.ogg`), `speaker` custom-cue test, `Settings.test.tsx` VOICE-28 (2026-09-24)
- [x] **VOICE-29** · M8 · A custom KIVO earcon motif replaces the placeholders before beta (§6) → done: KIVO's own motif (rounded two-note tones, each under 300 ms) is synthesized at startup for every cue, replacing the Kenney placeholders (DECISIONS "Earcons generated, not Kenney") · verified: `speaker` cue tests, heard on the owner's PC (2026-09-24)

**AEC, barge-in and endpointing (§3, §7)**

- [x] **VOICE-30** · M2 · AEC in production: WebRTC AEC3 (`sonora`) with KIVO's output mix as the reference, stepping aside when there is no echo path (headphones); the OS AEC is not used (§1, §3; DECISIONS "Echo cancellation") → done: AEC3 (`sonora`) on the output mix while KIVO's voice plays (+500 ms); `EchoPath` compares played and heard loudness and steps the canceller aside with headphones (it erased 99% of the user's first half-second there); reset when the output device changes; OS AEC not used (DECISIONS "Echo cancellation") · verified: echo tests (38.5 dB echo reduction; headphones keep 100% of the first half-second; a speaker's echo is found and cancelled), `talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request` (headphones), `talking_over_kivo_works_with_speakers_echoing_its_voice` (a simulated room) (2026-09-23)
- [x] **VOICE-31** · M2 · Barge-in: stricter VAD during TTS; duck TTS −12 dB within 50 ms; after ≥ 300 ms of speech or one STT word, cancel the turn (30 ms fade, brain and tools cancelled) and start listening from the buffer; otherwise restore the volume (§7) → done: VAD at 0.75 while KIVO speaks, once the canceller has settled (removing ≥ 10 dB, or no echo path) so KIVO's own voice never interrupts it; the voice ducks −12 dB at the first strong frame; 300 ms of speech cancels the turn, stops the voice with a fade and starts listening from the buffer; otherwise the volume comes back · verified: `talking_over_kivo_interrupts_it_and_is_heard_as_a_new_request` (headphones: barge-in ~1 s after KIVO starts, the whole request heard and acted on), `talking_over_kivo_works_with_speakers_echoing_its_voice` (a room echoing KIVO at −10 dB, 40 ms: no self-interruption, the request acted on) (2026-09-23)
- [x] **VOICE-32** · M2 · The wake threshold rises while KIVO plays audio (§7) → done: while KIVO plays audio a wake-word hit needs a score of 0.6 (`PLAYING_MIN_SCORE`) · verified: keyword tests, the hands-free end-to-end tests (2026-09-23)
- [x] **VOICE-33** · M2 · Endpointing: Silero pause 250 ms + Smart Turn v3 (§3) → done: Silero pause 250 ms, then Smart Turn v3.2 decides (re-checked at 700 ms, end forced at 1.6 s) · verified: Smart Turn feature tests against the reference, `silence_after_speech_ends_the_utterance`, the spoken end-to-end tests (2026-09-23)

**Residency (§8)**

- [x] **VOICE-34** · M1 · Residency: capture, VAD, wake and spotter always resident while listening; STT and TTS warm for 10 min after use (setting); STT prewarm on the wake-stage-1 hit and TTS prewarm on turn start (§8) → done: capture and VAD run only while listening; STT and TTS load when a request starts (`Infer::warm`, the prewarm on turn start), stay warm for the setting's minutes (10 by default) after use and then unload, and with nothing loaded the worker exits · verified: `models_load_on_use_and_unload_after_the_warm_time`, live (2026-09-22); a wake word prewarms the models as its turn starts (M2)
- [x] **VOICE-35** · M8 · GPU models unload on the Gaming profile or when the GPU is busy (§8) → done: the recognizer leaves the GPU on the Gaming profile, with a fullscreen app in front or when the card is over 60 % busy, and comes back under 30 %, checked every 10 s while idle · verified: `models::the_recognizer_leaves_the_gpu_for_games_and_busy_gpus`, `gpu::tests` (2026-09-24)

**Languages (§9)**

- [x] **VOICE-36** · M1 · `LanguagePack { code, stt_engines, tts_voices, kws_model?, grammar, vocabulary, status }` with English first; the router picks the engine for the active language (§9) → done: `LanguagePack { code, name, stt_engines, tts_voices, kws_model, grammar, vocabulary, status }` with English (Alpha) first and Hindi/Punjabi Planned; the runtime picks the STT engine from the active language's pack · verified: `english_ships_first_and_the_rest_are_planned`, `the_router_prefers_the_packs_order_then_anything_that_fits` (2026-09-23)
- [x] **VOICE-37** · M1 · Primary + secondary language settings; automatic language ID only where the engine provides it and it measures reliable (§9) → done: settings `general.language` (primary) and `general.languages` (secondary), validated; no shipped engine offers language ID, so none is used (detected languages would count only when the user speaks them) · verified: config tests, `detected_languages_count_only_when_the_user_speaks_them` (2026-09-23); the language picker is UX-51
- [ ] **VOICE-38** · L2 · Hindi + Punjabi, including Hindi–English and Punjabi–English code-mixing, benchmarked on owner-recorded test sets (§9)
- [ ] **VOICE-39** · L1 · Each language has STT test audio, command utterances and wake positives/negatives before it moves from Alpha to Supported (§9)

**Choosing speech engines (§11)**

- [x] **VOICE-42** · M2 · Speech engine registry: every engine's profile tags, languages, streaming, local/cloud/hybrid, devices, size, licence, voices and KIVO benchmark results; UI and onboarding read only the registry (§11) → done: `kivo_voice::registry` (profiles, privacy, commercial use, languages, devices, size, voices, KIVO's measurements from the `stt`/`tts` bench runs via `Database::latest_benchmark`); `voice.engines` serves it and onboarding and the Voice page read only it · verified: `every_engine_kivo_ships_is_in_the_registry`, `measurements_come_only_from_kivos_benchmarks`, `benchmark_results_are_stored_as_valid_json` (2026-09-23)
- [x] **VOICE-43** · M2 · Curated profiles: 2–4 STT (Recommended, Lightweight, High accuracy, Multilingual) and 2–4 TTS (Recommended/Natural, Lightweight, Multilingual, Expressive), each mapped to a registry engine; unmeasured values read "Not benchmarked by KIVO" (§11) → done: STT Recommended (Moonshine Base) / Lightweight (Tiny) / High accuracy ("Not available yet") / Multilingual (the language's Moonshine); TTS Natural (Kokoro) / Lightweight (Windows voices) / Multilingual (Supertonic 3) / Expressive ("Not available yet"); unmeasured engines read "Not benchmarked by KIVO" · verified: `profiles_map_to_engines_and_say_when_there_is_none`, Voice page test (2026-09-23)
- [x] **VOICE-44** · M2 · Recommendation function: hardware + OS + languages + privacy/offline + priority + installed models + benchmarks → done: `kivo_voice::recommend::recommend` weighs the tier, battery and load, language, privacy mode, priority, installed engines and measurements (slower-than-real-time engines passed over); recommended and fallback STT/TTS with a reason; CPU engines, keeping the GPU free · verified: recommend tests, onboarding test (2026-09-23)
- [x] **VOICE-45** · M2 · Safe engine switching: licence → done: `switch.rs`: check (language, privacy) → download (licence shown first) → load in a second worker → test (a Windows voice says a sentence the recognizer must hear, or the voice must speak audibly) → save; the old engine works until then; the listening model can't be removed without a replacement unless confirmed · verified: `a_new_speech_engine_is_tested_before_it_is_used`, switch tests, Voice page test (licence before download) (2026-09-23)
- [x] **VOICE-46** · M2 · Evaluate Supertonic 3 (licence, languages, size, CPU latency, streaming) and Moonshine Streaming sizes against KIVO's budgets; add the ones that pass to the registry and log the result in DECISIONS (§11) → done: Supertonic 3 added (RTF 0.25, 0.9 s load, EN and HI), Moonshine Tiny and per-language models added, Moonshine Streaming not added (safetensors only); logged in DECISIONS "Speech engines evaluated (VOICE-46)" · verified: `speaks_several_languages_when_the_model_is_here`, `hears_speech_in_its_own_language_when_the_models_are_here` (Base/Tiny EN, Base ES, Tiny JA exact) (2026-09-23)
- [x] **VOICE-47** · M2 · Fallback policy: a failed primary STT/TTS switches to the configured fallback for the session with a visible notice; saved settings are unchanged (§11) → done: an engine that fails to load is swapped for another installed recognizer or the Windows voices for the session (`infer.rs` `load_engines`), with a notice (`SpeechFallback` event → toast; Home says when nothing could hear); settings unchanged · verified: `a_voice_that_fails_to_load_falls_back_to_the_windows_voices_with_a_notice` (2026-09-23)
- [x] **VOICE-48** · M2 · Language compatibility: selected languages are checked against each engine; incompatible choices are explained with compatible alternatives, never switched silently (§11, §9) → done: `registry::language_problem` names the engines that do fit; a switch to an engine without the language is refused with them, and cards say "Not available for <language> yet" · verified: `incompatible_languages_are_explained_with_alternatives`, `engines_are_checked_against_the_language_and_the_slot`, the switch end-to-end test (2026-09-23)
- [x] **VOICE-49** · M3 · Advanced mode: exact engine, model, device, model path, chunk/latency settings, resource limits, fallback and cache (§11) → done: Voice → Advanced shows the exact engines, models, devices, model paths, the listening timings, the thread limit (settable), the fallback (switchable) and how long models stay loaded (settable) · verified: `Voice.test.tsx` (2026-09-23)

**Budgets (§10)**

- [~] **VOICE-40** · M1 · Every §10 budget is measured by `kivo-bench` on the reference tiers and met, or the miss is logged in DECISIONS.md (§10) → partial: every §10 budget has its `kivo-bench` suite, and the end-to-end tests measure the M1 path on this PC (press → cue 6 ms, end of speech → action 40 ms, cancel < 35 ms, 0 ms idle CPU, 67 MB idle runtime) · missing: the reference-tier runs, deferred by the owner (DECISIONS "Benchmarks deferred")
