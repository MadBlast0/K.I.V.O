# TTS Engines for KIVO (Rust, Windows-first voice companion) — research notes, Sept 2026

Scope: local and cloud text-to-speech options for streaming LLM output with low first-audio latency, barge-in, and CPU-only laptop support. Licensing pitfalls for a commercially distributed app. Tier recommendation: Instant / Natural / Expressive / System / Cloud.

Method note: ~20 searches/fetches. Primary sources (model cards, GitHub READMEs, vendor pricing pages, Microsoft Learn) are preferred. Some numbers come from secondary blogs and are flagged. Items I know from background knowledge but did not re-verify in this session are in **Gaps**, not in Cited Findings.

---

## 1. Local neural engines: quality, speed, size, streaming, license, Rust path

### Takeaway
Kokoro-82M (Apache-2.0 weights, ~82M params, 8 languages/54 voices) is the best-ranked small open model on public arenas (about #32 of 74 on the Artificial Analysis Speech Arena, above Chatterbox, XTTS v2 and StyleTTS 2), so it is the natural default local voice. Supertonic-2 (66M, RTF ~0.012 on M4 Pro CPU) and Piper are the fastest options. Chatterbox (MIT, with Turbo 350M and Nano 110M variants) is the best permissive option for expressive speech and voice cloning. Kyutai TTS is the only mature option that truly streams text in, but it is 1.8B params, GPU-class, and English/French only. For Rust, the practical path is sherpa-onnx (Apache-2.0, has Rust bindings, supports Kokoro, Piper/VITS, Matcha, KittenTTS, Supertonic, Pocket TTS, ZipVoice) or running the ONNX models directly with the `ort` crate.

### Cited Findings
**Kokoro-82M**
- License Apache 2.0; 82M params; v1.0 (Jan 2025) has 8 languages and 54 voices; v0.19 was Dec 2024 — [HF model card](https://huggingface.co/hexgrad/Kokoro-82M)
- Trained on "permissive/non-copyrighted audio" (public domain, Apache/MIT, synthetic audio from closed commercial TTS), including ~12 h of CC BY 3.0/4.0 data. Training cost ~$1,000 (~1,000 A100 GPU hours) — [HF model card](https://huggingface.co/hexgrad/Kokoro-82M)
- G2P is done by `misaki`; espeak-ng is listed as a dependency — [HF model card](https://huggingface.co/hexgrad/Kokoro-82M)
- Hosted API pricing is "under $1 per million characters" (DeepInfra and others) — [HF model card](https://huggingface.co/hexgrad/Kokoro-82M)
- Official ONNX export exists — [onnx-community/Kokoro-82M-v1.0-ONNX](https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX)
- Kokoro 82M v1.0 is ranked #32 on the Artificial Analysis Speech Arena, Elo 1056.2, 54.4% win rate. It is the highest open model under ~350M params and can run in the browser via WebGPU/WASM — [offlinetts.com summary of AA arena, May/Jul 2026](https://offlinetts.com/blog/tts-arena-leaderboard-2026/) (secondary). A separate July 2026 snapshot puts Kokoro at #48, Elo 1,055 — [search summary of same source](https://offlinetts.com/blog/tts-arena-leaderboard-2026/). The rank differs because the arena added models, but the Elo is stable at ~1055.

**Piper**
- rhasspy/piper (MIT) was archived read-only in October 2025. Development moved to OHF-Voice/piper1-gpl, which is GPL-3.0 and embeds espeak-ng for phonemization — [OHF-Voice/piper1-gpl](https://github.com/OHF-voice/piper1-gpl); [Cekura explainer](https://www.cekura.ai/discover/piper-tts) (secondary)
- The project does not state why it relicensed. espeak-ng being GPL-3.0 is the likely reason. "Shipping Piper inside a closed-source product triggers GPL-3.0's reciprocal obligations" — [Cekura](https://www.cekura.ai/discover/piper-tts) (secondary)
- sherpa-onnx supports Piper/VITS models — [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)

**Supertonic (Supertone Inc.)**
- Supertonic-2: 66M params; English, Korean, Spanish, Portuguese, French; code is MIT, weights are OpenRAIL-M — [HF Supertone/supertonic-2](https://huggingface.co/Supertone/supertonic-2)
- RTF 0.012–0.015 on M4 Pro CPU (2-step); 0.001–0.005 on RTX 4090; up to "167× faster than real-time"; runs on ONNX Runtime — [HF supertonic-2](https://huggingface.co/Supertone/supertonic-2); [HF Supertone/supertonic](https://huggingface.co/Supertone/supertonic)
- 912–1263 chars/s on M4 Pro CPU — [search summary of Supertonic sources](https://huggingface.co/Supertone/supertonic)
- A "Supertonic 3" site exists — [supertonictts.com](https://supertonictts.com/) (not verified whether official)
- sherpa-onnx supports Supertonic, including its text frontend — [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)

**NeuTTS Air (Neuphonic)**
- On-device speech LM on a 0.5B LLM backbone with instant voice cloning. Distributed as Q8 GGUF for CPU — [HF neuphonic/neutts-air](https://huggingface.co/neuphonic/neutts-air)

**KittenTTS / Pocket TTS / ZipVoice / Matcha**
- All are listed as supported in sherpa-onnx. Pocket TTS does English voice cloning; ZipVoice does Chinese+English voice cloning — [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)

**Chatterbox (Resemble AI)**
- MIT license. Variants: Turbo (350M, English, paralinguistic tags like `[laugh]`, one-step mel decoder), Nano (110M, English, "3x faster than realtime on 8 CPU cores"), Multilingual V3 (500M, 23+ languages) — [resemble-ai/chatterbox](https://github.com/resemble-ai/chatterbox)
- Zero-shot voice cloning from about 10 s of reference audio. Emotion "exaggeration" and CFG controls. Built-in Perth neural watermark that survives MP3 compression and editing — [resemble-ai/chatterbox](https://github.com/resemble-ai/chatterbox)
- Chatterbox is #52 on the AA Speech Arena (Elo 1006.4), below Kokoro — [offlinetts.com](https://offlinetts.com/blog/tts-arena-leaderboard-2026/) (secondary)

**Kyutai TTS (Delayed Streams Modeling)**
- tts-1.6b-en_fr is actually 1.8B params (1B backbone + 600M depth transformer). English and French only. Truly streaming: it "starts to output audio as soon as the first few words" arrive. Audio lags the text by 1.28 s. Weights are CC-BY 4.0 — [HF kyutai/tts-1.6b-en_fr](https://huggingface.co/kyutai/tts-1.6b-en_fr)
- Voice conditioning only uses pre-computed embeddings from the tts-voices repo. You cannot clone a voice from an arbitrary sample — [HF model card](https://huggingface.co/kyutai/tts-1.6b-en_fr)
- Python code is MIT; the Rust server is Apache and handles many streaming queries in parallel (used in production for Unmute); an MLX path exists — [kyutai-labs/delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)

**Other arena data points (open weights, AA Speech Arena)**
- Fish Audio S2 Pro #11 (Elo 1128.7), Step Audio EditX #16, NVIDIA Magpie-Multilingual 357M #26, Mistral Voxtral TTS #33, Maya1 #35, Fish Speech 1.5 #51, OpenVoice v2 #60, XTTS v2 #66, StyleTTS 2 #67 (Elo 878.8), MetaVoice #74 — [offlinetts.com](https://offlinetts.com/blog/tts-arena-leaderboard-2026/) (secondary)
- The top 10 are all closed APIs (Inworld 1.5 Max, Gemini 3.1 Flash TTS, ElevenLabs v3, MiniMax Speech 2.8, StepFun…). Another snapshot (July 2026) has Speechify Simba 3.2 first at Elo 1,229 — [offlinetts.com](https://offlinetts.com/blog/tts-arena-leaderboard-2026/). The two snapshots conflict on the #1 model, which is expected on a live leaderboard.
- The HF TTS Arena v2 leaderboard exists but its rows could not be retrieved via fetch — [TTS Arena v2](https://tts-agi-tts-arena-v2.hf.space/leaderboard); [legacy arena](https://huggingface.co/spaces/TTS-AGI/TTS-Arena)

**Picovoice Orca**
- Streaming text input designed for LLMs; claims "4x faster than the best alternatives". Runs on Windows x86_64/arm64, macOS, Linux, Android, iOS, Web and Raspberry Pi. 8 languages; default male and female voices; custom voices and emotion on the Enterprise plan. Needs an AccessKey from Picovoice Console. No Rust SDK is documented — [Picovoice Orca docs](https://picovoice.ai/docs/orca/)

**sherpa-onnx (integration layer)**
- Apache-2.0, with Rust bindings and `rust-api-examples`. Runs on Windows, macOS, Linux, Android, iOS, WASM and NPUs — [k2-fsa/sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)

### Inferences
- **Instant tier:** Supertonic-2 or Piper-class VITS via sherpa-onnx. RTF well under 0.05 on CPU means first audio arrives in a few tens of ms after the first phrase. Supertonic's OpenRAIL-M weight license has use-based restrictions that need legal review. Piper voices vary per voice (see §6).
- **Natural tier (default):** Kokoro-82M via sherpa-onnx or `ort`. It gives the best quality per MB with Apache weights. It is not truly streaming (it synthesizes per chunk), so KIVO must segment the LLM output into phrases (§4).
- **Expressive tier:** Chatterbox Turbo (GPU recommended) or Nano (CPU) under MIT. The watermark helps with the ethics of voice cloning. Chatterbox has no native Rust path; it would need an ONNX export or a Python sidecar.
- Kyutai TTS is the architecturally ideal fit (text-streaming), but at 1.8B params, CC-BY weights and English/French only, it is only a "GPU power-user" option. The Rust server helps.
- Orca is only an option under a commercial agreement. There is no Rust SDK, though the C library could be wrapped via FFI.

### Gaps
- Sesame CSM-1B, Dia (Nari Labs), F5-TTS, MeloTTS, Matcha and StyleTTS2: no primary-source pages were fetched this session for their licenses or RTFs. From background knowledge (unverified here): F5-TTS weights are CC-BY-NC (non-commercial, due to Emilia data); CSM-1B and Dia are Apache-2.0 but GPU-oriented (1B–1.6B); MeloTTS is MIT. **Verify each before adopting.**
- KittenTTS size, license and quality numbers were not fetched. Nor were NeuTTS Air's license (believed Apache-2.0) and RTF on x86 laptops.
- No first-audio latency numbers from one independent benchmark on the same Windows laptop CPU. The Supertonic figures are Apple M4 Pro only. KIVO should run its own benchmark.
- RAM usage figures were not found in primary sources for most models.

---

## 2. System voices (Windows SAPI 5 / OneCore / WinRT; macOS)

### Takeaway
WinRT `Windows.Media.SpeechSynthesis.SpeechSynthesizer` is available on Windows 10 (10240+) and 11. It supports text and SSML 1.1 to a stream, plus voice enumeration. It is a zero-download, zero-license "System" tier. However, it only uses Microsoft-signed installed voices. **Windows 11 Narrator "natural voices" are not officially available to third-party apps.** The only way to reach them is an unofficial hack that uses extracted keys and breaks on updates, so KIVO must not ship that.

### Cited Findings
- SpeechSynthesizer: Windows 10 10.0.10240+; methods `SynthesizeTextToStreamAsync`, `SynthesizeSsmlToStreamAsync` (SSML 1.1); `AllVoices`, `DefaultVoice`, `Voice`; `Options` added in 1703. "Only Microsoft-signed voices installed on the system can be used" — [Microsoft Learn](https://learn.microsoft.com/en-us/uwp/api/windows.media.speechsynthesis.speechsynthesizer)
- Narrator natural voices (e.g. Jenny, Aria, Guy) are embedded Azure voices. They are usable within Narrator but "not currently accessible to third-party applications" — [search summary; MakeUseOf](https://www.makeuseof.com/windows-11-narrator-natural-voices/); [Microsoft Support Narrator guide](https://support.microsoft.com/en-us/windows/chapter-7-customizing-narrator-ce950246-c915-0d44-9be6-fb474387a285)
- NaturalVoiceSAPIAdapter exposes Narrator/Edge natural voices to SAPI 5. It works by "a hack" using "encryption keys extracted from system files", notes "Microsoft hasn't yet allowed third-party apps to use the Narrator/Edge voices", "can stop working at any time", and recent Windows 11 voice updates reportedly broke it — [NaturalVoiceSAPIAdapter README](https://github.com/gexgd0419/NaturalVoiceSAPIAdapter/blob/master/README.md)
- Narrator itself also works with third-party SAPI 5 voices (CereProc, Vocalizer Expressive, etc.) — [search summary of Microsoft Support](https://support.microsoft.com/en-us/windows/complete-guide-to-narrator-e4397a0d-ef4f-b386-d8ae-c172f109bdb1)

### Inferences
- In Rust, call WinRT SpeechSynthesizer via the `windows` crate (Windows.Media.SpeechSynthesis). The result is a WAV stream that KIVO decodes and plays through its own cpal pipeline, which keeps barge-in uniform. The alternative is SAPI 5 `ISpVoice` via COM, which exposes the legacy voice registry and third-party SAPI voices.
- Synthesis is per-call (whole sentence), so feed it phrase-sized chunks.
- System voices are a good fallback: nothing to download, low quality but instant, and fully licensed with the OS.

### Gaps
- macOS AVSpeechSynthesizer (including Personal Voice and premium voices) and Linux speech-dispatcher were not researched this session.
- Whether the WinRT API exposes OneCore voices that SAPI does not (the known registry-token difference) was not re-verified.
- I found no Microsoft statement of plans to open natural voices to apps.

---

## 3. Cloud TTS: first-byte latency and pricing

### Takeaway
For a user-selectable cloud tier, Cartesia Sonic 3 (~40–90 ms TTFA, ~$0.03/min) and ElevenLabs Flash v2.5 (~75 ms model latency, ~150 ms end-to-end, $0.05/1K chars) lead on latency. Deepgram Aura-2 ($0.030/1K chars) and Azure Neural ($16/1M chars; HD $22/1M) are cheaper per character. OpenAI gpt-4o-mini-tts is token-priced and steerable. Top arena quality sits with Inworld, Gemini 3.1 Flash TTS, ElevenLabs v3 and MiniMax.

### Cited Findings
- ElevenLabs API: Flash/Turbo $0.05 per 1K chars; Multilingual v2 and v3 $0.10 per 1K; Flash "~75ms" — [ElevenLabs API pricing](https://elevenlabs.io/pricing/api)
- Cartesia Sonic 3: ~40 ms TTFA on Turbo, ~90 ms standard. ElevenLabs Flash measured ~150 ms end-to-end — [Cartesia vs ElevenLabs roundups](https://burki.dev/blog/41-cartesia-vs-elevenlabs-tts) (secondary; vendor-influenced); [Cartesia comparison page](https://www.cartesia.ai/vs/cartesia-vs-elevenlabs) (vendor)
- Cartesia plans: Free (~27 min), Pro $5/mo, Startup $49, Scale $299 (~10,667 min); ~$0.03/min; 1 credit per character — [Cartesia pricing](https://www.cartesia.ai/pricing); [eesel summary](https://www.eesel.ai/blog/cartesia-sonic-3-pricing) (secondary). A Sonic-3.6 release is referenced — [The Rundown](https://www.therundown.ai/tools/sonic-3-6)
- OpenAI: gpt-4o-mini-tts costs $0.60/1M text-input tokens and $12/1M audio-output tokens; tts-1 $15/1M chars; tts-1-hd $30/1M chars — [OpenAI API pricing](https://developers.openai.com/api/docs/pricing)
- Deepgram: Aura-2 $0.030/1K chars PAYG ($0.027 Growth); Aura-1 $0.015/1K. No latency figures on the pricing page — [Deepgram pricing](https://deepgram.com/pricing)
- Azure: Neural $16/1M chars; Neural HD $22/1M (cut from $30 in March 2026); 500K chars/month free — [TextToLab](https://texttolab.com/blog/azure-text-to-speech-pricing) (secondary); [Azure HD voice update blog](https://techcommunity.microsoft.com/blog/azure-ai-foundry-blog/azure-speech-%E2%80%93-neural-hd-text-to-speech-recent-voice-updates/4505380)
- Rime Arcana: $40/1M chars; Arcana v3 launched — [Rime pricing](https://www.rime.ai/pricing); [Arcana v3](https://www.rime.ai/resources/arcana-v3)
- Quality: Inworld Realtime TTS 1.5 Max (Elo ~1210), Gemini 3.1 Flash TTS (~1206), ElevenLabs v3 (~1178), MiniMax Speech 2.8 HD (~1164) top the AA arena — [offlinetts.com](https://offlinetts.com/blog/tts-arena-leaderboard-2026/) (secondary)

### Inferences
- Rough cost per million chars: Azure $16 < Deepgram Aura-2 $30 < Rime $40 < ElevenLabs Flash $50 < ElevenLabs v3 $100. Cartesia is ~$5–37/M depending on plan (secondary estimate).
- Cartesia and ElevenLabs both offer WebSocket input streaming, which fits token-stream TTS best. I did not verify this in their docs this session.
- Recommended Cloud tier: bring-your-own-key, with Cartesia/ElevenLabs Flash as the low-latency defaults and Azure/OpenAI as alternatives.

### Gaps
- Google Cloud TTS (Chirp 3 HD / Gemini TTS) pricing and latency were not fetched.
- OpenAI gpt-4o-mini-tts TTFB was not found. Inworld TTS pricing was not fetched.
- There are no independent same-region TTFB measurements. The Cartesia and ElevenLabs numbers are vendor claims or from blogs.

---

## 4. Streaming patterns: segmentation, normalization, cancellation, audio out

### Takeaway
Except for Kyutai, Orca and vendor WebSocket APIs, engines synthesize per chunk. KIVO should buffer LLM tokens into short phrases and synthesize the first phrase immediately. It should pipeline the next phrase while the current one plays, and make cancellation a single "generation ID" flip that drops queued text and flushes the audio ring buffer.

### Cited Findings
- Kyutai shows true text-streaming TTS (audio starts after the first few words, ~1.28 s text-to-audio offset), and its Rust server serves many streams in parallel — [HF Kyutai TTS](https://huggingface.co/kyutai/tts-1.6b-en_fr); [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling)
- Orca is designed for streaming LLM text in — [Picovoice Orca docs](https://picovoice.ai/docs/orca/)
- The WinRT synthesizer returns a whole stream per call (text or SSML) — [Microsoft Learn](https://learn.microsoft.com/en-us/uwp/api/windows.media.speechsynthesis.speechsynthesizer)

### Inferences (engineering recommendations; not source-backed this session)
- **Segmentation:** emit the first chunk at the earliest clause boundary (`,;:` or after about 8–12 words) to minimize first audio. After that, emit on sentence boundaries (`.?!` followed by a space/newline, with an abbreviation list and a decimal-number guard) and cap chunks at about 200–300 chars.
- **Text normalization before G2P:** numbers, currency, dates and times, units, abbreviations. Skip or summarize code blocks and URLs (say "link" or the domain only) and strip markdown (`*`, `#`, backticks). Kokoro/misaki does some of this itself; engines vary.
- **Cancellation / barge-in:** each utterance gets a monotonically increasing ID. On VAD speech start: (1) bump the ID, (2) clear the text queue, (3) abort in-flight synthesis (drop results whose ID is stale), (4) clear the playback ring buffer and apply a 5–20 ms fade-out to avoid clicks, (5) close cloud WebSockets or send their cancel message. With WASAPI shared-mode buffers of ~10–20 ms, perceived stop latency is about one buffer.
- **Audio out:** `cpal` (WASAPI on Windows, CoreAudio on macOS, ALSA/Pulse on Linux) with a lock-free SPSC ring buffer (e.g. `rtrb`) fed by the TTS thread. Resample model output (Kokoro 24 kHz; Piper 16/22.05 kHz) to the device rate with `rubato`. For acoustic echo cancellation during barge-in, the playback reference signal must be available to the AEC stage.

### Gaps
- No primary sources were fetched on cpal/WASAPI latency tuning or on published first-audio benchmarks for phrase-chunking strategies.

---

## 5. Voice selection, voice cloning, ethics

### Takeaway
Preset voices (Kokoro's 54, Supertonic, Piper, Orca, Kyutai embeddings) avoid consent problems. Cloning-capable models (Chatterbox, NeuTTS Air, Pocket TTS, ZipVoice, and cloud clones from ElevenLabs/Cartesia) should sit behind explicit consent UX. Chatterbox's built-in watermark is a plus.

### Cited Findings
- Chatterbox does zero-shot cloning from ~10 s of audio and embeds a Perth watermark in all output — [chatterbox](https://github.com/resemble-ai/chatterbox)
- Kyutai deliberately restricts cloning to pre-computed voice embeddings — [HF Kyutai TTS](https://huggingface.co/kyutai/tts-1.6b-en_fr)
- NeuTTS Air offers "instant voice cloning" on-device — [HF NeuTTS Air](https://huggingface.co/neuphonic/neutts-air)
- Kokoro voices come from permissive or synthetic data (some synthetic data came from closed commercial TTS) — [HF Kokoro](https://huggingface.co/hexgrad/Kokoro-82M)

### Inferences
- Cloning UX requirements: the user records their own voice or attests to consent, the app keeps the clone local by default and shows a visible "synthetic voice" indicator, and it never offers celebrity presets.
- Kokoro's partial training on synthetic output from closed commercial TTS is a small provenance risk to note, though the weights are Apache-2.0.

### Gaps
- Per-voice licenses for Piper voices (each voice has its own dataset license, some non-commercial) were not enumerated.

---

## 6. GPL / phonemizer licensing pitfalls for a commercial app

### Takeaway
The main trap is **espeak-ng (GPL-3.0)**. Current Piper (piper1-gpl) is GPL-3.0 because it embeds espeak-ng. Kokoro's G2P library misaki is Apache-2.0, and **espeak-ng is optional for English** (it is only an out-of-vocabulary fallback), but it is needed for several non-English languages. Shipping GPL espeak-ng statically linked in a closed-source KIVO binary would trigger GPL obligations.

### Cited Findings
- piper1-gpl is GPL-3.0 and embeds espeak-ng; the old MIT rhasspy/piper was archived in Oct 2025 — [OHF-Voice/piper1-gpl](https://github.com/OHF-voice/piper1-gpl); [Cekura](https://www.cekura.ai/discover/piper-tts)
- misaki is Apache-2.0. The English G2P runs with `fallback=None` (no espeak), and espeak-ng + phonemizer-fork are optional extras — [hexgrad/misaki](https://github.com/hexgrad/misaki)
- Kokoro uses espeak-ng for English out-of-distribution fallback and for some non-English languages — [search summary of Kokoro sources](https://huggingface.co/hexgrad/Kokoro-82M)
- sherpa-onnx is Apache-2.0. Its README does not state its espeak-ng usage or license — [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
- Supertonic weights are OpenRAIL-M (use-based restrictions), not OSI-open — [HF supertonic-2](https://huggingface.co/Supertone/supertonic-2)
- Kyutai weights are CC-BY 4.0, which requires attribution — [HF Kyutai TTS](https://huggingface.co/kyutai/tts-1.6b-en_fr)

### Inferences
- Safe path for English: Kokoro + misaki-style dictionary G2P with no espeak (port the lexicon to Rust or use sherpa-onnx's Kokoro lexicon frontend). Handle out-of-vocabulary words with a permissive rule-based G2P or letter spelling.
- For multilingual support, choose one of: (a) ship espeak-ng as a separate, user-replaceable dynamically linked DLL/process with source offer and GPL notice (the "mere aggregation" approach; get legal review); (b) prefer models with permissive frontends (Supertonic's own text frontend, Chatterbox's tokenizer-based input); or (c) make espeak-based voices an optional download.
- Avoid piper1-gpl code in the binary. Using Piper **voice models** through sherpa-onnx is fine, subject to per-voice dataset licenses and the espeak question above.
- Supertonic OpenRAIL-M and Kyutai CC-BY require a license screen or attribution in-app. F5-TTS (believed CC-BY-NC, unverified) must be excluded from commercial builds.

### Gaps
- I could not confirm whether sherpa-onnx's Windows prebuilt binaries statically link espeak-ng (piper-phonemize) for VITS/Kokoro multilingual. Check the sherpa-onnx build files and its NOTICE.

---

## 7. Proposed tiers (synthesis for the report writer)

### Takeaway
- **Instant:** Supertonic-2 (66M, RTF ~0.01 on CPU; OpenRAIL-M) or Piper/VITS voices via sherpa-onnx. Aimed at the weakest CPUs and sub-100 ms first audio.
- **Natural (default local):** Kokoro-82M ONNX via sherpa-onnx or `ort` (Apache-2.0; best open small-model arena rank). Use English without espeak.
- **Expressive:** Chatterbox Turbo (GPU) / Nano (CPU), MIT, watermark, consent-gated cloning. Kyutai TTS as an optional GPU streaming engine (CC-BY, English/French).
- **System:** WinRT SpeechSynthesizer or SAPI 5 on Windows 10/11 (no Narrator natural voices); AVSpeechSynthesizer on macOS later.
- **Cloud (BYO key):** Cartesia Sonic 3 or ElevenLabs Flash v2.5 for latency; ElevenLabs v3, Gemini or Inworld for top quality; Azure, Deepgram and OpenAI as cheaper or alternative providers.

### Cited Findings
- See the sections above for each source: [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M), [Supertonic-2](https://huggingface.co/Supertone/supertonic-2), [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx), [Chatterbox](https://github.com/resemble-ai/chatterbox), [Kyutai](https://huggingface.co/kyutai/tts-1.6b-en_fr), [WinRT SpeechSynthesizer](https://learn.microsoft.com/en-us/uwp/api/windows.media.speechsynthesis.speechsynthesizer), [ElevenLabs pricing](https://elevenlabs.io/pricing/api), [Cartesia pricing](https://www.cartesia.ai/pricing)

### Inferences
- A single Rust `TtsEngine` trait (`synthesize(chunk, voice, cancel_token) -> stream of f32 frames`) lets all tiers share the segmenter, normalizer, cancellation and cpal output.

### Gaps
- There is no apples-to-apples Windows laptop benchmark (for example an i5 or Ryzen 5 without a GPU) of Kokoro vs Supertonic vs Piper vs Chatterbox Nano. That is the most important measurement still missing.
