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

## 9. Budgets (targets to validate in M0/M1)

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
