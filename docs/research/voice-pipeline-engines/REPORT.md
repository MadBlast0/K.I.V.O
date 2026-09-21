# KIVO voice pipeline: engines and architecture

Research date: 2026-09-21. This report summarizes five source notes in this folder, where every
claim is linked to its source:
[wake_word_engines.md](wake_word_engines.md) ·
[speaker_enrollment_personalization.md](speaker_enrollment_personalization.md) ·
[stt_engines.md](stt_engines.md) ·
[tts_engines.md](tts_engines.md) ·
[audio_infrastructure.md](audio_infrastructure.md)

## Summary

KIVO can build an entirely local voice pipeline from permissively licensed, cross-platform
components. It can be reached from Rust through two runtimes: **ONNX Runtime (`ort`)** and
**sherpa-onnx**. sherpa-onnx alone covers keyword spotting, VAD, speaker ID, streaming STT and TTS.

The main conclusions:

1. **Picovoice is out.** Its free tier closed on 30 June 2026, its SDK refuses to run without an
   online-validated key, and it is now enterprise-only. That removes Porcupine (wake word), Eagle
   (speaker ID) and Orca (TTS).
2. **Two wake-word paths:**
   - A **trained "Hey Kivo" model**, built with the openWakeWord pipeline, gives the lowest false
     accepts for the phrase most users keep.
   - **sherpa-onnx open-vocabulary keyword spotting** lets users type any custom wake word and use
     it immediately. A few recorded samples then tune and verify it.
3. **Speaker verification is a convenience filter, not a security gate.** Replay and TTS spoofing
   defeat unprotected systems 89–100% of the time. Sensitive actions still need on-screen
   confirmation or Windows Hello.
4. **Personalizing STT for an accent** works best by choosing the engine that measures best on the
   user's enrollment recordings, adding custom vocabulary, and letting the LLM repair transcripts.
   Per-user fine-tuning needs far more audio than onboarding collects.
5. **Defaults to benchmark:**
   - STT: Moonshine v2 (streaming, English) or Parakeet TDT v3 int8 (25 European languages).
   - TTS: Kokoro-82M.
   - VAD: Silero v6.
   - Echo cancellation: WebRTC AEC3.
   - Turn detection: Pipecat Smart Turn v3.

   No source benchmarks these on a common low-end Windows laptop, so KIVO must measure them before
   fixing defaults.
6. **Licensing traps:**
   - espeak-ng is GPL-3.0, which made maintained Piper GPL. Kokoro's English phonemizer avoids it.
   - openWakeWord's pretrained models are non-commercial, so KIVO trains its own.
   - Several weights are CC-BY or OpenRAIL-M and need attribution or use notices.

## 1. Wake word, including user-defined wake words

### Engine landscape

| Engine | Custom words | License | Notes |
|---|---|---|---|
| **openWakeWord** | Trained per phrase from synthetic TTS clips (about 1 h on a Colab GPU) | Code Apache-2.0; **pretrained models non-commercial** | Best trained accuracy (<0.5 false accepts/h, <5% false rejects target); includes Silero VAD gate and per-user verifier models; lightly maintained |
| **sherpa-onnx KWS** | **Typed text at runtime**, with per-keyword boost and threshold | Apache-2.0 | About 3 MB models; official Rust crate; Windows, macOS, Linux; no published false-alarm figures |
| local-wake / EfficientWord-Net | 3–4 recorded samples (few-shot) | MIT / Apache-2.0 | 98.6% on clean same-speaker audio; good as a second-stage verifier |
| microWakeWord | Trained | Unclear | Microcontroller-oriented, "early release" |
| Porcupine (Picovoice) | Console-trained | Enterprise-only since 2026 | **Excluded** |
| DaVoice, Sensory | Vendor-trained | Commercial | Excluded unless a commercial deal is wanted |
| Windows voice activation API | Needs Microsoft onboarding | — | Not practical |

### Architecture: gated, two-stage detection

Apple ("Hey Siri" uses a tiny always-on DNN, then a larger confirming DNN), Microsoft (which
recommends apps re-verify platform detections), and openWakeWord all use a cheap first pass followed
by a strict second pass:

```text
mic 16 kHz mono ─► energy gate ─► Silero VAD ─► Stage 1 detector ─► Stage 2 verify ─► WAKE
                  (always)       (on energy)   (on speech;          (on hit, over
                                                pre-roll buffer)     1.5–2 s buffer)
Stage 1: trained "Hey Kivo" model (openWakeWord via ort), or sherpa-onnx KWS for custom words
Stage 2: stricter second model, and/or enrollment template match, and/or speaker verification
Free third check: the first words of the streaming STT
```

VAD should confirm or gate the detector **with a pre-roll ring buffer**, so the start of
"Hey Kivo" is never clipped. It should not hard-switch the detector off.

### Custom wake-word feature (Add / Edit wake words)

1. **Type the phrase.** A validator checks it using phonemes from G2P:
   - syllables: reject 1, warn at 2, prefer 3–4;
   - at least 6 phonemes, with varied vowels;
   - edit distance from common words and names;
   - collision with other enabled wake words and with KIVO commands.

   The result is shown as **Good / Fair / Risky**.
2. **Hear it.** TTS speaks the phrase, and the user can adjust the phonetic spelling until the
   pronunciation matches.
3. **It works immediately** through sherpa-onnx KWS.
4. **Record 3–5 samples.** These are used to:
   - tune the threshold and boost so the samples trigger;
   - build a few-shot template or verifier for stage 2;
   - run a short false-alarm test on bundled background speech.
5. **Optional "Enhance this wake word" job.** It trains a dedicated openWakeWord classifier head in
   the background, using local TTS clips and precomputed negative features. The estimate is tens of
   minutes on a CPU; this is unverified. The training stack is Python, so it needs a bundled
   runtime or a port to Rust (candle, burn, or ONNX Runtime training).
6. **Management.** Users can list, enable/disable, rename, re-record, set per-word sensitivity and
   delete wake words. Several wake words can be active at once.

"Hey Kivo" (/h eɪ k iː v oʊ/, 3 syllables, about 6 phonemes) passes the guidance, although "Kivo"
itself is short.

## 2. Speaker enrollment and personalization

### Speaker verification models (local)

| Model | Size | EER (VoxCeleb1-O) | License | Path |
|---|---|---|---|---|
| **CAM++ (3D-Speaker)** | 7.2M params | 0.65% | Apache-2.0 | sherpa-onnx speaker-ID API, or `ort` |
| WeSpeaker ResNet34 | — | — | CC-BY-4.0 weights (attribution) | ONNX available |
| ECAPA-TDNN (SpeechBrain) | — | 0.80% | Apache-2.0 | Export to ONNX |
| TitaNet-Large (NVIDIA) | 23M | 0.66% | CC-BY-4.0 | NeMo export |
| Picovoice Eagle | — | — | $6k/yr+, metered | **Excluded** |

**Recommendation:** CAM++, with WeSpeaker ResNet34 as the alternative.

### What the big assistants do

- **Apple:** five phrases ("Hey Siri" ×3, "Hey Siri, how is the weather today?",
  "Hey Siri, it's me."). The profile then grows from accepted real uses up to 40 samples, and the
  enrollment audio is stored so the profile can be rebuilt when the model changes. Apple reports
  about 1 false accept per month and 5.2% false rejects.
- **Google Voice Match:** four phrases, a Retrain option, and guest mode with no personal results.
  Google warns that recordings or similar voices can pass.
- **Amazon Voice ID:** about ten on-screen phrases. This figure is unverified.

### Spoofing

Replay attacks reached 89–100% false accepts on unprotected systems, and modern TTS cloning fools
both people and models. **Speaker verification decides whose preferences and memory to use and
filters out TV and other people. It never authorizes a risky action on its own.**

### STT personalization: what actually works

| Method | Works with | Verdict |
|---|---|---|
| Pick the engine by measured WER on the user's enrollment prompts | Any | **Do this.** Cheap, and it uses the known prompt text |
| Custom vocabulary / hotwords | sherpa-onnx transducers (beam search only); NeMo word boosting for Parakeet TDT (≥ 2.5.0); Whisper initial prompt (≤ 224 tokens) | **Do this** for names, apps and jargon |
| LLM transcript repair with user context | Any | **Do this** in the brain path |
| Per-user LoRA fine-tune | Whisper, Parakeet | 15–37% relative WER gains in papers, but with hours of audio. A minute of enrollment is not enough. **Later**, optional, and fed by consented usage data |

### Recommended enrollment flow

A consent screen explains that a voiceprint is biometric data (GDPR Art. 9 explicit consent;
Illinois BIPA names voiceprints), that it stays on the device and is encrypted, and how to delete it.
Then **8 prompts of 2–4 s each** (30–45 s of speech):

- the wake word ×3
- the wake word plus a command ×3
- "It's me, ⟨name⟩" ×1
- one or two sentences containing digits, names and the user's own vocabulary

Each clip becomes its own sample. The profile then grows from high-confidence matches (up to about
40). The settings offer **Retrain my voice** and **Delete voice data**. Unknown voices get a guest
mode with no personal data. Storage uses Windows DPAPI, the macOS Keychain or the Linux Secret
Service.

## 3. Speech-to-text

### Tiers (proposed; confirm by benchmark)

| Tier | Engine | Why | Caveats |
|---|---|---|---|
| **Ultra Fast (CPU default)** | **Moonshine v2 Small**, streaming | Real partials; 7.84% WER; about 148 ms response on an M3 | English only |
| | or **Parakeet TDT 0.6B v3 int8** | 6.34% WER, 25 European languages; about 5× real time on an i5-6500, 20× on a Ryzen 5700X; about 1.2 GB RAM | Not streaming (VAD-segmented); use the weights-only int8 export, since other int8 exports drop speech; CC-BY-4.0 |
| **Balanced** | Parakeet v3 via ONNX Runtime + DirectML or CUDA | Same model, accelerated | — |
| **High Accuracy (GPU)** | Whisper large-v3-turbo (whisper.cpp) · Voxtral Mini 4B Realtime · Nemotron Speech Streaming 0.6B | Accuracy, multilingual, streaming | Heavier; Nemotron is GPU-only |
| **Platform native** | Windows AI Speech API · Apple SpeechAnalyzer | NPU on Copilot+ PCs, no download | Windows: 24H2+, **MSIX packaging with the `systemAIModels` capability**, per-phrase results |
| **Cloud** | Deepgram Flux ($0.0065/min, built-in end-of-turn detection <300 ms) · AssemblyAI Universal-Streaming ($0.15/h) · OpenAI gpt-4o-(mini-)transcribe ($0.003–0.006/min) | Accuracy and turn detection | Needs internet; privacy rules apply |

The leaderboard leaders (Granite Speech 4.1 2B, Cohere Transcribe, Canary-Qwen-2.5B) have about 5.3–5.6%
WER but are over 2B parameters and do not stream, so they don't suit a resident default.

**Rust path:** **transcribe-rs** (the library behind the Handy dictation app) already wraps Parakeet,
Canary, Moonshine including streaming, SenseVoice and Whisper, with runtime selection of CUDA, DirectML,
CoreML, Vulkan and ROCm. The official `sherpa-onnx` crate covers the streaming Zipformer models. Models
are downloaded on demand, never bundled in the installer, per the plan's §21.

## 4. Text-to-speech

### Tiers

| Tier | Engine | Notes |
|---|---|---|
| **Instant** | Supertonic-2 (66M, about 80× real time on CPU) or Piper voices via sherpa-onnx | Supertonic weights are OpenRAIL-M (use restrictions); **Piper is now GPL-3.0** (see below) |
| **Natural (default local)** | **Kokoro-82M** (ONNX) | Apache-2.0 weights, 8 languages, 54 voices; best-ranked small open model (#32 in the Artificial Analysis arena, ahead of Chatterbox, XTTS v2 and StyleTTS 2) |
| **Expressive** | Chatterbox (MIT; Turbo 350M on GPU, Nano 110M on CPU) · Kyutai TTS (GPU, 1.8B) | Chatterbox clones from about 10 s of audio and watermarks its output. Kyutai is the only local model that truly starts speaking from a text stream (English/French only) |
| **System** | WinRT SpeechSynthesizer / SAPI 5 · macOS AVSpeechSynthesizer | Zero download. **Windows 11 Narrator natural voices are not available to third-party apps**; the adapter hack uses extracted keys, so avoid it |
| **Cloud** (user's key) | Cartesia Sonic 3 (about 40–90 ms first audio) · ElevenLabs Flash v2.5 (about 75 ms) · Azure Neural · Deepgram Aura-2 · OpenAI gpt-4o-mini-tts | Pricing is in the notes |

### Licensing trap: espeak-ng

espeak-ng is **GPL-3.0**. The maintained Piper (`OHF-Voice/piper1-gpl`) became GPL because it embeds
it, and the original MIT repo was archived in October 2025. Kokoro's English phonemizer **misaki is
Apache-2.0 and needs no espeak**, while other Kokoro languages do. The options:

- start English-first without espeak;
- ship espeak-ng as a separately installed, dynamically loaded optional component, after legal review;
- choose phonemizer-free models.

**Also to check:** whether the sherpa-onnx Windows builds statically link espeak-ng.

### Streaming and interruption

- **Chunking:** send the first clause as soon as it forms, then whole sentences.
- **Normalization:** convert numbers, units, code, paths and URLs into speakable text before synthesis.
- **Cancellation:** every utterance carries a turn ID; barge-in cancels that ID, drops queued audio,
  and fades out over about 20–50 ms.
- **Playback:** a short output queue so a stop takes effect quickly.

## 5. Audio infrastructure

| Concern | Recommendation | Why |
|---|---|---|
| Capture/playback | `cpal` 0.18.x behind a KIVO `AudioBackend` trait, **plus a direct WASAPI backend** (windows-rs) | cpal had Windows bugs in exactly KIVO's areas (#1200: silent USB mics on 24H2/26200, fixed; default-device following fixed 2026-09-06). Capture in the device's native format, resample to 16 kHz mono with `rubato`, and run a silence watchdog that reopens the stream |
| VAD | **Silero v6** via `ort` (load the model directly, since most wrappers still ship v5) | TEN VAD is faster, but it has no Rust bindings and a modified Apache license |
| Echo cancellation (AEC) | **Tier 1:** Windows 11 OS AEC pointed at KIVO's output (`IAcousticEchoCancellationControl`; treat build 22621 as the minimum; only where the laptop maker ships the effect). **Tier 2 (everywhere else, including Win10, macOS, Linux):** in-process **WebRTC AEC3**, using the pure-Rust `sonora` port (passes the C++ test suite, 1.07–1.24× slower) or `webrtc-audio-processing` | Never run both at once. Avoid macOS VoiceProcessingIO for always-on listening, because it ducks all system audio |
| Noise suppression | Optional: WebRTC NS, or RNNoise (`nnnoiseless`) | Off by default; measure the effect on STT |
| Turn detection | Silero pause of 200–300 ms, then **Pipecat Smart Turn v3** (8 MB int8 ONNX, 12–95 ms on CPU, 23 languages) | Its Whisper-style feature extraction must be ported to Rust; its license still needs confirming. LiveKit's detector is deprecated and restrictively licensed |
| Barge-in | Detect on the **echo-cancelled** signal → **duck** TTS immediately → **stop** after sustained speech or 1–2 recognized words | Avoids stopping on coughs; one cancellation token per turn runs through STT, brain, tools and TTS |
| Latency budget | About **800 ms** from end of user speech to first audio: about 200 ms each for transport, STT + endpointing, LLM first token and TTS first audio; the local front end takes ≤ 300–400 ms | Industry target from Pipecat and LiveKit |
| Idle power | Target **1–2% CPU** for capture + VAD + wake word | Silero <1 ms per 30 ms chunk. Levers: 80 ms batches, turn off ORT thread spinning, EcoQoS on the detection thread, MMCSS only on the capture thread. No measured end-to-end figure exists, so benchmark it |

## Proposed default pipeline

```text
cpal/WASAPI capture (native format)
  → rubato 16 kHz mono → [AEC3 or OS AEC, reference = KIVO output]
  → ring buffer (≥ 2 s pre-roll)
  → energy gate → Silero v6 VAD
  → Stage 1: "Hey Kivo" openWakeWord model  |  sherpa-onnx KWS (custom words)
  → Stage 2: verifier + CAM++ speaker check (convenience filter)
  → start chime (capture never pauses) → streaming STT (Moonshine v2 / Parakeet v3 / cloud)
  → Silero pause + Smart Turn v3 endpoint
  → intent router → fast path | brain
  → phrase-chunked TTS (Kokoro / system / cloud) → cpal playback (also the AEC reference)
  ⟲ barge-in: VAD on AEC'd signal → duck → cancel turn token
```

## Licensing checklist

| Component | License | Action |
|---|---|---|
| openWakeWord code | Apache-2.0 | OK |
| openWakeWord pretrained models | Non-commercial | **Train KIVO's own** |
| sherpa-onnx, Silero, CAM++, Kokoro weights, misaki, Chatterbox | Apache-2.0 / MIT | OK, keep notices |
| Parakeet v3, WeSpeaker, TitaNet, Kyutai TTS | CC-BY-4.0 | Show attribution in the app |
| Supertonic | OpenRAIL-M | Show use restrictions to users |
| espeak-ng, piper1-gpl | GPL-3.0 | Avoid in the core; optional separate component only after review |
| Smart Turn v3, TEN VAD, microWakeWord | Unconfirmed or modified | Verify before adopting |
| Picovoice (all) | Enterprise-only | Excluded |

## What must be measured (no source has it)

1. The false accepts per hour of sherpa-onnx text KWS against a trained openWakeWord model, for the same phrase.
2. STT candidates on a **low-end CPU-only Windows laptop**: first partial, final latency, RAM,
   cold-load time.
3. TTS candidates on the same laptop: first-audio latency, real-time factor, RAM.
4. The end-to-end idle CPU of capture + AEC + VAD + wake word.
5. CPU-only training time for a custom openWakeWord head.
6. How many real Windows 11 laptops expose the OS AEC effect.

These belong in KIVO's benchmark harness (plan §98–102), which should exist before defaults are
fixed.
