# Audio Infrastructure for KIVO (capture/playback, VAD, AEC/NS, endpointing, barge-in, idle power)

Research date: 2026-09-21. Scope: Rust runtime, Windows 11 first, then Windows 10, macOS/Linux later.
Note on method: about 20 search/fetch calls. Items marked "(unverified)" in Inferences are from background knowledge that was not re-confirmed against a source this session. Treat them as leads, not facts.

## Audio I/O in Rust: cpal vs direct WASAPI, device switching, hot-plug, resampling, macOS/Linux

### Takeaway
cpal is usable as the cross-platform layer, but on Windows it has had real, recent bugs right where a voice assistant runs: following the default device, and Communications-class (headset) endpoints. KIVO should use cpal 0.18.x behind its own `AudioBackend` trait and keep a direct-WASAPI (windows-rs) backend option on Windows. That backend is needed anyway for the OS AEC APIs, which work at the IAudioClient level.

### Cited Findings
- cpal issue #1200: on Windows 11 24H2 (build 10.0.26200, the same build as the KIVO dev machine), cpal v0.17.2+ captured **silence** from USB Communications-class mics (headsets, webcams). Cause: PR #1097 turned on `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` by default. When a non-Communications app opens a Communications endpoint at its Communications mix format (16 kHz mono) with that flag, the engine delivered zero samples, about 82 dB of attenuation. Closed by PR #1201. The Intel Smart Sound mic array was not affected. — [cpal #1200](https://github.com/RustAudio/cpal/issues/1200)
- cpal PR #1350 ("Restore reroute notifications on WASAPI") was merged on 2026-09-06 into `stable-0.18`, with a companion PR #1355 for master. Before it, a default-device change wrongly raised `StreamInvalidated`. Now `DeviceChanged` means the stream stays alive on the new default, and `DeviceNotAvailable` means no replacement device exists. — [cpal PR #1350](https://github.com/RustAudio/cpal/pull/1350)
- Earlier cpal work, PR #754, proposed creating default devices with `ActivateAudioInterfaceAsync` so WASAPI routes the stream automatically. Issue #740 ("Respect Windows output device selection") describes audio staying on the old device after the user switched outputs. — [cpal PR #754](https://github.com/RustAudio/cpal/pull/754/files); [cpal #740](https://github.com/RustAudio/cpal/issues/740)
- A downstream app (WaveFlow) on cpal 0.17.1 reports that opening the default endpoint binds the stream to that endpoint, with no `IMMNotificationClient` registered and no polling. The menu can show one output while audio plays on another. — [WaveFlow #612](https://github.com/InstaZDLL/WaveFlow/issues/612)
- Microsoft: an app that wants its stream to follow the default device must implement that itself through `IMMNotificationClient::OnDefaultDeviceChanged`, unless it uses the automatic stream routing path. — [MS Learn: Relevant device notifications for stream routing](https://learn.microsoft.com/en-us/windows/win32/coreaudio/relevant-device-notifications-for-stream-routing)
- An engineering write-up covers recovering a `!Send` cpal stream after device errors by rebuilding it on the owning thread. — [bhanueso.dev: cpal stream recovery](https://bhanueso.dev/broadcasts/cpal-stream-recovery/)
- Recognized Windows audio-thread practice: MMCSS gives audio threads prioritized CPU access so buffers are filled by their deadlines. — [Wikipedia: MMCSS](https://en.wikipedia.org/wiki/Multimedia_Class_Scheduler_Service)

### Inferences
- Pin cpal at **>= 0.18 with the #1350 fix**, or vendor a patched copy. Also add a **silence-probe watchdog**: if RMS stays near digital zero for more than 2 s while the endpoint's peak meter shows signal, reopen the stream without AUTOCONVERTPCM, or reopen it in the device's native format and resample in-process. This guards against the #1200 class of bug.
- Always capture in the device's native format (often 48 kHz, stereo or multichannel) and downmix/resample to 16 kHz mono in-process with `rubato`. Never ask WASAPI for 16 kHz. This avoids the AUTOCONVERTPCM path completely. (rubato is the standard Rust resampler; not re-verified this session.)
- Use **shared mode**, not exclusive, for both capture and render. Exclusive mode blocks other apps, such as a user's Teams call, and it bypasses the OS effects pipeline, including Windows AEC. Shared-mode engine latency (about 10 ms period) is negligible next to the 300-800 ms voice-to-voice budget. (Exclusive vs shared tradeoff: unverified this session.)
- Bluetooth: when a capture stream opens on a BT headset mic, Windows switches the headset from A2DP to HFP, which drops playback to narrowband/wideband telephony quality. (Well-known behaviour, unverified this session.) Mitigation: default the capture device to the laptop's built-in mic even when the output is a BT headset, and tell the user if they explicitly choose a BT mic.
- Linux: cpal's ALSA host reaches PipeWire through pipewire-alsa. The downstream issue [pokeemerald-rs #1022](https://github.com/LunchBox951/pokeemerald-rs/issues/1022) tracks enabling PipeWire/PulseAudio default-host routing, which shows default-device following on Linux also needs work.

### Gaps
- No measured shared-mode vs exclusive-mode round-trip latency numbers found for cpal on Windows 11.
- Did not confirm whether cpal 0.18 exposes `IAudioClient` or signal-processing-mode/category settings (AudioCategory_Communications). It very likely does not, so a custom WASAPI path is required for OS AEC.
- No authoritative Microsoft doc fetched on the HFP profile switch or on Windows 11's "Bluetooth LE Audio / super wideband" improvements.

## VAD: Silero v5/v6, TEN VAD, WebRTC VAD; gating the wake-word detector

### Takeaway
Silero VAD v6 (ONNX through `ort`) is the safe default: mature, widely used, with several Rust wrappers. TEN VAD is lighter and detects the end of speech faster, but has no Rust bindings and a non-standard license. Treat it as an FFI option for later. Use VAD to *confirm* wake-word hits, and optionally to duty-cycle heavier models, but do not hard-gate the wake-word model off with VAD.

### Cited Findings
- Silero VAD v6.0 (2025-08-25): 16% fewer errors on noisy real-life data and 11% fewer on multi-domain validation vs v5. v6.2 (2025-12-10) added "ifless" ONNX and tinygrad 16 kHz models. v6.2.1 (Feb 2026) made onnxruntime optional. — [silero-vad releases](https://github.com/snakers4/silero-vad/releases); [v6.0 release](https://github.com/snakers4/silero-vad/releases/tag/v6.0); [changelog issue](https://github.com/snakers4/silero-vad/issues/2)
- Silero: one audio chunk (30+ ms) takes under 1 ms on a single CPU thread. — [silero-vad README](https://github.com/snakers4/silero-vad)
- Silero accepts only fixed windows: 512 samples at 16 kHz (32 ms), 256 at 8 kHz. — [voice_activity_detector crate](https://crates.io/crates/voice_activity_detector) / [docs.rs](https://docs.rs/voice_activity_detector)
- Rust Silero wrappers: `voice_activity_detector` (Silero v5, standalone), `silero-vad-rust` (ONNX through safe `ort` bindings), `silero-vad-rs` (ort), `vad-silero-rs`. — [crates.io voice_activity_detector](https://crates.io/crates/voice_activity_detector); [silero-vad-rust](https://crates.io/crates/silero-vad-rust); [silero-vad-rs](https://crates.io/crates/silero-vad-rs/0.1.2); [vad-silero-rs](https://crates.io/crates/vad-silero-rs); [GitHub nkeenan38](https://github.com/nkeenan38/voice_activity_detector)
- TEN VAD RTF: Windows i7-10710U 0.0150; M1 0.0160; Ryzen 9 5900X 0.0150. Head-to-head on a Xeon Gold 6348: TEN 0.0086 vs Silero 0.0127. Library about 306 KB (Linux) to about 508 KB (Windows), vs Silero about 2.2 MB ONNX. 16 kHz only, 10 or 16 ms hop (160/256 samples). Bindings for Python, C, JS/WASM, Java and Go, with **no Rust binding**. License is "Apache 2.0 with additional conditions". — [TEN VAD on Hugging Face](https://huggingface.co/TEN-framework/ten-vad)
- TEN's own claim: it detects speech→non-speech transitions quickly, while Silero lags by "several hundred milliseconds" and misses short pauses between segments. TEN claims better precision than both WebRTC VAD and Silero. This is the vendor's benchmark. — [TEN VAD HF README](https://huggingface.co/TEN-framework/ten-vad/blob/main/README.md); [PyPI ten-vad](https://pypi.org/project/ten-vad/)
- Independent/competitor comparisons exist from Picovoice (vendor of Cobra VAD) and VoxRT. Both are vendors comparing against their own product. — [Picovoice VAD 2026](https://picovoice.ai/blog/best-voice-activity-detection-vad/); [VoxRT comparison](https://voxrt.com/vad-comparison); [Silero metrics wiki](https://github.com/snakers4/silero-vad/wiki/Performance-Metrics)
- openWakeWord includes Silero VAD. With `vad_threshold` set, a wake-word positive fires **only if VAD also scores above threshold at the same time**. This is meant to cut false positives from non-speech noise. One RPi 3 core runs 15-20 openWakeWord models in real time. — [openWakeWord README](https://github.com/dscripka/openWakeWord/blob/main/README.md)
- Counter-signal: wyoming-openwakeword users report one core pegged at 100% on a Pi Zero 2W. — [wyoming-openwakeword #47](https://github.com/rhasspy/wyoming-openwakeword/issues/47); [#30](https://github.com/rhasspy/wyoming-openwakeword/issues/30)

### Inferences
- On a laptop, Silero at 32 ms windows (about 31 inferences/s at under 1 ms each) is roughly 1-3% of one core, well under 1% of total CPU on an 8+ core machine. The CPU-saving case for gating wake word *behind* VAD is therefore weak. The wake-word model (for example openWakeWord's mel+embedding frontend) usually costs more than VAD, but only a few percent of one core too.
- **Gating risk:** if VAD must fire before the wake-word model sees audio, VAD onset latency plus threshold misses clip the first 100-300 ms of "Hey KIVO". Always run wake word on a **ring buffer**, for example 1.5-2 s pre-roll. If you gate, feed the wake-word model the buffered pre-roll when VAD turns on, never just the frames after onset. The openWakeWord approach (require VAD agreement *at the moment of detection*) keeps recall while cutting false accepts, and is the lower-risk pattern.
- A cheap energy/RMS gate (under 0.1% CPU) in front of Silero, used only in deep idle such as screen locked or on battery saver, is a reasonable further tier.
- WebRTC VAD (the `webrtc-vad` crate, GMM-based, 10/20/30 ms frames) is nearly free but much less accurate in noise. Useful only as the cheap first-stage gate. (Crate details unverified this session.) Note the WebRTC APM in `webrtc-audio-processing`/`sonora` also carries a VAD.

### Gaps
- No independent (non-vendor) accuracy benchmark of Silero v6 vs TEN VAD found.
- Silero v6 per-frame CPU on Windows x64 through `ort` not measured in any source found. Must benchmark locally.
- Whether any Rust crate already ships the Silero v6 (not v5) ONNX is not confirmed. `voice_activity_detector` states v5.

## Echo cancellation and noise suppression

### Takeaway
Two-tier AEC. On Windows 11 22621+, open capture in Communications category/mode and point OS AEC at KIVO's render endpoint with `IAcousticEchoCancellationControl`, when the endpoint supports it. As the universal fallback, and on Windows 10, macOS and Linux, run WebRTC AEC3 in-process: either `webrtc-audio-processing` (C++ bundled) or the pure-Rust `sonora` port. Because KIVO owns its TTS output, it always has a sample-exact reference signal, which makes in-process AEC3 very effective. Add NS only for the STT path, not for wake word unless tests show gains.

### Cited Findings
- `IAcousticEchoCancellationControl::SetEchoCancellationRenderEndpoint(endpointId)` sets the render endpoint whose loopback is the AEC reference. Obtain it through `IAudioClient::GetService` after `Initialize`. `E_NOINTERFACE` means the capture endpoint's AEC (if present) does not allow reference control. NULL lets Windows pick. An invalid ID returns E_INVALIDARG. Minimum client: **Windows Build 22621** (22H2). — [MS Learn interface](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nn-audioclient-iacousticechocancellationcontrol); [method](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iacousticechocancellationcontrol-setechocancellationrenderendpoint)
- Conflict: the Microsoft classic sample README says build **22540** or later is required. — [Windows-classic-samples AEC README](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/AcousticEchoCancellation/README.md). The API reference says 22621. Treat 22621 as the safe floor.
- A WinRT equivalent exists: `Windows.Media.Effects.AcousticEchoCancellationConfiguration.SetEchoCancellationRenderEndpoint`. — [WinRT doc](https://learn.microsoft.com/en-us/uwp/api/windows.media.effects.acousticechocancellationconfiguration.setechocancellationrenderendpoint?view=winrt-26100)
- Windows 11 (build 22000+) CAPX APIs let an AEC APO receive an OS-provided reference loopback as an auxiliary input. **Windows 10 does not support these APIs.** On Windows 10, AEC APOs got the reference through private driver channels, which usually works only for the integrated speaker and not USB/Bluetooth output, or by opening a loopback stream. The loopback is pre-volume by default. Post-volume loopback is optional and not available on all endpoints. — [MS Learn: Windows 11 APIs for APOs](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/windows-11-apis-for-audio-processing-objects)
- Implication from the same doc: whether OS AEC exists at all depends on the OEM/driver shipping an AEC APO. Windows exposes the framework but the effect is vendor-supplied. — [same](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/windows-11-apis-for-audio-processing-objects); signal processing modes background: [Audio signal processing modes](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/audio-signal-processing-modes)
- Windows 10-compatible OS option: the **Voice Capture DSP** (`CLSID_CWMAudioAEC`) is a DMO with AEC, mic-array processing, NS, AGC and VAD, each toggleable. It has filter mode (app feeds mic and speaker streams) and source mode (DMO drives the devices). AEC is single-channel only. It is legacy Media Foundation/DMO technology. — [MS Learn Voice Capture DSP](https://learn.microsoft.com/en-us/windows/win32/medfound/voicecapturedmo); [system mode property](https://learn.microsoft.com/en-us/windows/win32/medfound/mfpkey-wmaaecma-system-modeproperty)
- `webrtc-audio-processing` (tonarino) wraps PulseAudio's repackaging of the WebRTC APM: AEC, NS, AGC, VAD. The `bundled` feature builds the C++ source and mangles symbols. `experimental-aec3-config` exposes raw EchoCanceller3 parameters, with no semver guarantee. — [README](https://github.com/tonarino/webrtc-audio-processing/blob/main/README.md); [crates.io](https://crates.io/crates/webrtc-audio-processing); [docs.rs](https://docs.rs/webrtc-audio-processing)
- **sonora**: a pure-Rust port of WebRTC APM from the M145 branch, with AEC3, NS (Wiener), AGC2 (RNN-VAD-based) and a high-pass filter. The team reports the full 2,400+ C++ test suite passes. It processes a 10 ms frame in 4.2 µs (16 kHz mono) and 13.3 µs (48 kHz) on an M4 Max, 1.07-1.24x slower than C++. SSE2/AVX2/NEON. BSD-3-Clause. MSRV 1.91. — [CallBroAI/sonora](https://github.com/CallBroAI/sonora); [dignifiedquire/sonora](https://github.com/dignifiedquire/sonora)
- RNNoise is available in pure Rust as `nnnoiseless`. DeepFilterNet is written in Rust (tract inference). DeepFilterNet has a single-thread RTF of 0.19 on an i5-8250U (secondary source; DFN3 described as "slightly heavier"). — [forasoft article](https://www.forasoft.com/learn/ai-for-video-engineering/articles-ai/real-time-noise-suppression-krisp-rnnoise-deepfilternet); [DeepFilterNet repo](https://github.com/rikorose/deepfilternet); [DeepFilterNet paper (Interspeech 2023)](https://www.isca-archive.org/interspeech_2023/schroter23b_interspeech.pdf)
- Example: a pure-Rust DeepFilterNet3 + WASAPI real-time mic denoiser for Windows exists. — [noisegate](https://github.com/Yashsomalkar/noisegate); ONNX Runtime variant: [deepfilter-rt](https://github.com/shimondoodkin/deepfilter-rt)
- macOS VoiceProcessingIO pitfalls: creating a VPIO unit **ducks all other system audio** on Mac. Volume becomes very low with a BT headset output plus built-in mic. Developers question whether it is production-ready on macOS. — [Apple forum 664346](https://developer.apple.com/forums/thread/664346); [Apple forum 66953](https://developer.apple.com/forums/thread/66953); [JUCE forum](https://forum.juce.com/t/cannot-get-kaudiounitsubtype-voiceprocessingio-code-working/63789); [Apple doc](https://developer.apple.com/documentation/audiotoolbox/kaudiounitsubtype_voiceprocessingio)
- What real open-source assistants do: Home Assistant Voice Preview Edition puts AEC, stationary noise removal and AGC in **hardware**, on an XMOS XU316 DSP next to dual mics with an ESP32-S3, and ships open firmware for both chips. — [HA Voice PE](https://www.home-assistant.io/voice-pe/); [HA blog 2024-12-19](https://www.home-assistant.io/blog/2024/12/19/voice-preview-edition-the-era-of-open-voice/)

### Inferences
- **Recommended AEC design:** KIVO renders TTS itself, so feed the exact PCM it hands to the render stream into AEC3's `process_render_frame` as the far-end, 10 ms frames at 16 or 48 kHz. Measure and set the stream delay from WASAPI `GetStreamLatency` plus buffer padding. AEC3's delay estimator absorbs the rest. This works on Win10/11/macOS/Linux alike and does not depend on OEM APOs. Limitation: it only cancels **KIVO's own** audio, not music from Spotify. For that you need a system loopback capture (WASAPI loopback on the render endpoint) as reference, or the OS AEC.
- Prefer `sonora` if its maturity holds up in testing, since it has no C++ toolchain dependency and eases cross-compiling. Keep `webrtc-audio-processing` (bundled) as a fallback. Both run 10 ms frames, which fits a 16 kHz / 160-sample hop.
- Windows OS AEC path: open the capture `IAudioClient` with `AudioCategory_Communications` through `IAudioClient2::SetClientProperties`, then try `GetService(IAcousticEchoCancellationControl)`. Check whether AEC is actually present with the Windows 11 effects-discovery APIs before trusting it. Do **not** stack OS AEC and AEC3 blindly. Double processing can distort speech and hurt STT. Pick one per session.
- Pitfall: Communications category can trigger Windows "communications activity" ducking of other apps (the Sound control panel default is to reduce other sounds by 80%). Users may find it intrusive for an always-on assistant. (Ducking default unverified this session.)
- NS: aggressive NS (DeepFilterNet) can remove cues modern STT models such as Whisper or Parakeet rely on. Use AEC3's built-in light NS, or none, for STT. Make RNNoise (`nnnoiseless`, cheap, 48 kHz 10 ms frames) or DeepFilterNet opt-in toggles, and A/B test on WER.
- macOS: avoid VPIO for the always-on path because it ducks system audio. Use cpal/CoreAudio plus in-process AEC3.

### Gaps
- No source confirmed how many shipping Windows 11 laptops expose `IAcousticEchoCancellationControl` on their built-in mic (it depends on OEM APO support).
- The Windows 11 effects-discovery API (`IAudioEffectsManager`, `AUDIO_EFFECT_TYPE_ACOUSTIC_ECHO_CANCELLATION`) was not fetched in detail. Confirm the header and min build.
- No head-to-head WER impact data for RNNoise vs DeepFilterNet vs none on modern ASR found.
- What OVOS/Mycroft desktop and Rhasspy do for AEC in software (for example PulseAudio `module-echo-cancel`) was not verified.

## Endpointing / turn detection

### Takeaway
Baseline: Silero VAD silence timeout, about 500-800 ms. Upgrade: **Pipecat Smart Turn v3.x**, an 8 MB int8 ONNX audio model (open weights, data and training code; exact licence not verified) that takes up to 8 s of turn audio and runs in about 12-60 ms on CPU. It runs directly in Rust through `ort` once VAD reports a short pause, about 200-300 ms. LiveKit's detectors carry a restrictive "LiveKit Model License" and the text one is heavier. Deepgram Flux is cloud-only.

### Cited Findings
- Smart Turn v3: Whisper-Tiny encoder base with linear classifier head, about 8M params, **8 MB int8** (QAT) ONNX, "nearly 50x smaller" than v2. Up to 8 s input. CPU latency 12.6 ms (c7a.2xlarge), 15.2 ms (c8g.2xlarge), 33.8 ms (t3.2xlarge), 59.8 ms (c8g.medium), 94.8 ms (t3.medium). 23 languages, 94-97% accuracy for most (Bengali 84%, Vietnamese 81%, Arabic 89%). Open weights, data and training script. Meant to run together with a VAD such as Silero. — [Daily blog: Smart Turn v3](https://www.daily.co/blog/announcing-smart-turn-v3-with-cpu-inference-in-just-12ms/); [GitHub smart-turn](https://github.com/pipecat-ai/smart-turn)
- v3.1 has improved accuracy. — [Daily blog v3.1](https://www.daily.co/blog/improved-accuracy-in-smart-turn-v3-1/). Pipecat's `LocalSmartTurnAnalyzerV3` runs ONNX locally, about 65 ms on a Pipecat Cloud 1x instance. — [Pipecat Smart Turn overview](https://docs.pipecat.ai/api-reference/server/utilities/turn-detection/smart-turn-overview); [local_smart_turn_v3 ref](https://reference-server.pipecat.ai/en/stable/api/pipecat.audio.turn.smart_turn.local_smart_turn_v3.html)
- LiveKit (current docs): a new **audio** turn detector, `v1` (hosted on LiveKit Inference) and `v1-mini` (local CPU). It needs VAD with `min_silence_duration` of at least 0.25 s. Endpointing delay adapts over 0.3-2.5 s. The **text** `MultilingualModel` (Qwen2.5-0.5B-Instruct) is now marked **deprecated**: 396 MB on disk, about 50-160 ms per turn, under 500 MB RAM. Both use the "LiveKit Model License". — [LiveKit turn detector docs](https://docs.livekit.io/agents/logic/turns/turn-detector/)
- Conflict: older LiveKit material gives the multilingual text model as **281 MB**. — [LiveKit docs (older path)](https://docs.livekit.io/agents/build/turns/turn-detector.md). The same material describes distillation from a Qwen2.5-7B teacher and recommends compute-optimized, not burstable, instances to avoid inference timeouts. — [LiveKit blog: improved EOT model](https://blog.livekit.io/improved-end-of-turn-model-cuts-voice-ai-interruptions-39/); [HF livekit/turn-detector](https://huggingface.co/livekit/turn-detector)
- LiveKit claims its improved end-of-turn model cut voice-AI interruptions by 39%. — [LiveKit blog](https://blog.livekit.io/improved-end-of-turn-model-cuts-voice-ai-interruptions-39/)

### Inferences
- Pipeline: Silero reports speech end after about 200 ms of silence, then Smart Turn v3.x runs on the buffered turn audio (at most the last 8 s). If P(complete) is above threshold, commit the turn. Otherwise wait and re-check, with a hard silence cap of about 1.5-2 s. This gives about 250-350 ms endpointing on complete utterances without cutting off "uh, let me think" pauses.
- Smart Turn is ONNX with a Whisper-style log-mel frontend. In Rust you need the mel feature extraction (port from the Python reference) plus `ort`. Moderate effort, no blockers.
- Because KIVO also runs streaming STT, a cheap text heuristic (trailing conjunctions, "and…", "um") can complement the audio model without LiveKit's licence.
- Deepgram Flux (STT with built-in end-of-turn) and Kyutai's streaming models (Unmute/semantic VAD) are relevant alternatives, but **no source was fetched this session**. See Gaps.

### Gaps
- No sources fetched for Deepgram Flux (cloud-only, end-of-turn latency claims) or Kyutai semantic VAD / delayed-streams models. Neither can be characterized here.
- Size and latency of LiveKit `v1-mini` audio detector not published in the docs fetched. Whether an ONNX export exists is unknown.
- Smart Turn v3.1/v3.2 latency and language changes not extracted in detail.
- Exact Smart Turn licence: GitHub repo text not fetched. The blog says "open weights, data, and training script".

## Barge-in

### Takeaway
Detect barge-in on the **AEC-cleaned** mic signal, so KIVO's own TTS is removed. Require sustained speech, such as more than 200-300 ms of VAD plus optional 1-2 recognized words, before a hard stop, and duck TTS instantly on first VAD onset. Stop propagates as a cancellation token that flushes the render buffer, cancels TTS synthesis, and aborts or truncates the LLM stream, then records what was actually spoken.

### Cited Findings
- Pipecat: by default any user speech immediately interrupts the bot. Backchannels ("yeah", "okay", "mm-hmm") make that a problem. `MinWordsInterruptionStrategy` requires a minimum word count, and the first strategy that returns true triggers the interruption. As of v0.0.99 it is deprecated in favour of `pipecat.turns.user.MinWordsUserTurnStartStrategy` through `turn_start_strategies`. — [Pipecat interruption strategies](https://docs.pipecat.ai/server/utilities/interruption-strategies); [ref docs](https://reference-server.pipecat.ai/en/latest/api/pipecat.audio.interruptions.min_words_interruption_strategy.html); [v0.0.69 release](https://newreleases.io/project/github/pipecat-ai/pipecat/release/v0.0.69)
- Pipecat users report interruption failures with certain LLM services and pipeline runners, showing cancellation propagation is a common bug source. — [pipecat #2295](https://github.com/pipecat-ai/pipecat/issues/2295); [#3191](https://github.com/pipecat-ai/pipecat/issues/3191); [#2249](https://github.com/pipecat-ai/pipecat/issues/2249)
- LiveKit's audio turn detector addresses false interruptions by recognizing mid-turn pauses. — [LiveKit docs](https://docs.livekit.io/agents/logic/turns/turn-detector/)
- openWakeWord's VAD-agreement mechanism is an existing pattern for requiring two detectors to agree before acting. — [openWakeWord README](https://github.com/dscripka/openWakeWord/blob/main/README.md)

### Inferences
- **Self-trigger prevention layers:** (1) AEC3 with KIVO's own PCM as reference. (2) Raise the VAD threshold, for example 0.5 to 0.7, while TTS plays. (3) Optionally compare the near-end/far-end energy ratio (echo return loss) and ignore VAD when mic energy tracks TTS energy. (4) Require N ms of sustained speech. (5) Optionally require at least one or two STT words (Pipecat-style) before a hard stop.
- **Duck, then stop:** on first VAD onset during TTS, ramp TTS gain down about 12-20 dB over about 30 ms. That is instant feedback, and it also improves AEC convergence and STT. If the barge-in is confirmed within about 300-600 ms, stop: flush the render ring buffer, cancel the TTS request, cancel the LLM stream, and mark the assistant message as truncated at the last played word. If it is not confirmed (cough, backchannel), ramp back up and continue. That is Pipecat's "false interruption / resume" idea.
- Use a Rust `tokio_util::sync::CancellationToken` tree per turn (turn → LLM → TTS → playback) so one cancel reaches every stage. Playback must drop queued audio within one period, about 10 ms, instead of draining. Keep the render queue short (at most about 200 ms buffered) so stop latency stays low.
- Wake word during TTS: keep running wake word on the AEC output so "Hey KIVO, stop" works as a guaranteed barge-in path when VAD-based barge-in is disabled, for example in speakerphone mode without good AEC.

### Gaps
- No quantitative false-barge-in rates for AEC3-plus-VAD on laptop speakers found.
- The LiveKit "adaptive interruption handling" feature was not fetched in detail.

## Reference architectures and latency budgets

### Takeaway
Industry target: about 800 ms median voice-to-voice, roughly four stages of about 200 ms each. Streaming overlap is what makes it achievable. For a local-first desktop app, the audio front-end (capture + AEC + VAD + endpointing) should take at most about 300 ms of that, and endpointing wait dominates.

### Cited Findings
- Kwindla Hultman Kramer (Pipecat co-creator) recommends an 800 ms median voice-to-voice target, split about 200 ms each across transport/media, STT+endpointing, LLM, and TTS. — [Chanl blog summarizing](https://www.channel.tel/blog/voice-ai-pipeline-stt-tts-latency-budget)
- Example breakdowns: STT 60-120 ms, LLM first token 100-250 ms, TTS first chunk 40-100 ms, network 20-60 ms. Another: VAD and turn-taking 150-300 ms, LLM TTFT 150-400 ms, TTS TTFA 100-200 ms. Streaming overlap makes 600-800 ms reachable. — [Chanl](https://www.channel.tel/blog/voice-ai-pipeline-stt-tts-latency-budget); [Hamming AI](https://hamming.ai/resources/voice-ai-latency-whats-fast-whats-slow-how-to-fix-it); [dev.to](https://dev.to/tigranbs/sub-second-voice-agent-latency-a-practical-architecture-guide-4cg1)
- Pipecat production guidance: P95 TTFB under 300 ms, P95 final under 800 ms for 3 s utterances. — [Luong Hong Thuan guide](https://luonghongthuan.com/en/blog/pipecat-voice-agent-production-scalable-guide/)
- Human turn gaps average about 200 ms. Up to 500 ms feels natural, and above 500 ms is noticed. — [Chanl](https://www.channel.tel/blog/voice-ai-pipeline-stt-tts-latency-budget)
- Home Assistant Assist on Voice PE: hardware DSP (XMOS) front-end, with wake word on the device and processing that can stay fully local. — [HA Voice PE](https://www.home-assistant.io/voice-pe/)
- Rhasspy/Wyoming ecosystem uses openWakeWord (with an optional Silero gate) as a Wyoming service. — [wyoming-openwakeword issues](https://github.com/rhasspy/wyoming-openwakeword/issues/47); [openWakeWord](https://github.com/dscripka/openWakeWord)
- Pipecat standard stack: Silero VAD + Smart Turn + interruption strategies. LiveKit Agents: VAD + turn detector + STT endpointing. — [Pipecat Smart Turn docs](https://docs.pipecat.ai/api-reference/server/utilities/turn-detection/smart-turn-overview); [LiveKit docs](https://docs.livekit.io/agents/logic/turns/turn-detector/)

### Inferences
- Suggested KIVO front-end budget: capture buffer 10-20 ms, AEC/NS under 1 ms compute, VAD frame 32 ms, endpointing 200-300 ms (VAD pause plus Smart Turn), playback start 20-40 ms. Total front-end overhead about 300-400 ms. That leaves about 400-500 ms for STT finalization, LLM TTFT and TTS TTFA to hit about 800 ms.
- Many latency blog posts are secondary aggregators (Chanl, Hamming, dev.to). The 800 ms / 4x200 ms framing is consistent across them and attributed to Pipecat's co-creator. Treat it as industry consensus, not measured data.

### Gaps
- No official Home Assistant developer-doc latency numbers for the Assist pipeline were fetched.
- No Rust-native open-source voice assistant with published end-to-end latency found this session.
- OVOS/Mycroft current audio architecture not researched.

## Idle power: CPU for always-on capture + VAD + wake word; reduction tricks

### Takeaway
Always-on capture + Silero + a small wake-word model should land in the low single-digit percent of **one** core, well under 1% of total CPU on a modern 8-16 thread laptop. The bigger idle cost is **wakeups**, not FLOPs. Batch 32-80 ms of audio per inference, put the ML worker thread on EcoQoS (Windows 11) while idle, and keep only the capture callback on MMCSS.

### Cited Findings
- Silero: under 1 ms per 30+ ms chunk on one thread. — [silero-vad](https://github.com/snakers4/silero-vad)
- TEN VAD RTF about 0.015 on a Windows i7-10710U, so about 1.5% of one core. — [TEN VAD HF](https://huggingface.co/TEN-framework/ten-vad)
- openWakeWord: 15-20 models real-time on a single RPi 3 core. — [openWakeWord README](https://github.com/dscripka/openWakeWord/blob/main/README.md)
- Windows QoS levels: High (foreground or **audible** processes), Medium, Low, Utility, **Eco** (explicitly tagged, always efficient cores; Windows 11), Media, and **Deadline** (audio threads needing performance). A process that plays audio is automatically HighQoS. Thread-level EcoQoS is set with `SetThreadInformation(ThreadPowerThrottling, THREAD_POWER_THROTTLING_EXECUTION_SPEED)`. — [MS QoS doc](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/ProcThread/quality-of-service.md); [SetThreadInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setthreadinformation); [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation)
- EcoQoS lowers CPU frequency or uses efficient cores. It is meant for work that does not contribute to foreground UX, and should not be used for performance-critical or foreground experiences. — [Introducing EcoQoS](https://devblogs.microsoft.com/performance-diagnostics/introducing-ecoqos/)

### Inferences
- **Realistic budget:** set a target of **at most 1-2% total CPU** (Task Manager) while idle-listening on a 4P+8E Win11 laptop, and **at most 0.5%** in "deep idle" (energy-gated) mode. Validate with Windows Performance Recorder and `powercfg /energy` wakeup counts. This is an engineering target derived from the per-model figures above, not a sourced benchmark.
- **Tricks:** (1) Use 20 ms WASAPI periods, not 3-10 ms, for capture while idle. (2) Wake the ML thread every 80 ms and run 2-3 Silero windows in one go; openWakeWord already works on 80 ms frames (unverified this session). (3) Set `ort` to intra-op threads = 1, no spinning (`session.intra_op.allow_spinning = 0`), because ONNX Runtime's default thread-pool spin burns idle CPU. (4) Apply thread-level EcoQoS to the idle VAD/wake-word thread and drop it when a turn starts. Never apply it to the capture/render callback thread. Mark that thread with MMCSS ("Audio" or "Pro Audio") through `AvSetMmThreadCharacteristicsW`. (5) An energy pre-gate plus a pre-roll ring buffer lets Silero/wake word sleep in silent rooms. (6) Consider pausing capture on "microphone off" hotkey or lid-closed events. That also removes the Windows mic-in-use privacy indicator, which users will notice on an always-on app.
- Because KIVO shows the mic-in-use indicator permanently, the UX and privacy framing matters as much as CPU.

### Gaps
- No published measurement of total-system CPU/power for an always-on Silero + openWakeWord desktop app on Windows 11 found.
- EcoQoS vs MMCSS interaction on the same process (audio thread MMCSS/Deadline, ML thread Eco) is not documented explicitly. Needs a local test.
- ONNX Runtime spin-wait idle cost is from background knowledge and was not re-sourced this session.
