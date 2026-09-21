# Speech-to-Text (STT) Engines for KIVO (Rust, Windows-first, local + cloud)

Research date: 2026-09-21. All numbers come from the cited pages. A value marked "(secondary)" comes from an aggregator or blog, not from the vendor or model card.

## Local engines: accuracy, size, license, hardware, languages

### Takeaway
The Open ASR Leaderboard's top models (Granite Speech 4.1 2B, Cohere Transcribe, Canary-Qwen-2.5B, Qwen3-ASR) are within about 1 WER point of each other, but they are 2B+ LLM-decoder models and are not streaming. For a desktop companion, the useful local options are NVIDIA Parakeet TDT 0.6B v3, Nemotron Speech Streaming 0.6B, Moonshine v2, Whisper large-v3-turbo, SenseVoice-Small and Voxtral Realtime 4B. Of these, Parakeet v3 int8 via ONNX is the best CPU accuracy-to-speed option available today.

### Cited Findings
**Leaderboard context**
- Cohere Transcribe (2B, released March 2026) averaged 5.42% WER. Granite Speech 4.1 2B averaged 5.33% at RTFx 231. ARK-ASR-3B scored 5.04% and MOSS-Transcribe-preview-2B about 5.0%. None of these stream. All are Apache 2.0. — [MarkTechPost, Jul 2026 (secondary)](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- Canary-Qwen-2.5B: 5.63% WER, RTFx 418, English only, CC-BY-4.0, not streaming. — [MarkTechPost (secondary)](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- Qwen3-ASR-1.7B scored 5.76% WER and covers 52 languages/dialects. Qwen3-ASR-0.6B reaches about 2000x RTFx. Both are Apache 2.0. — [MarkTechPost (secondary)](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- Granite Speech 4.1 2B-NAR (non-autoregressive) reaches about 1820 RTFx and covers 5 languages. — [MarkTechPost (secondary)](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- The top of the leaderboard is separated by less than 1 WER point, so the choice depends on language coverage, license and speed. — [MarkTechPost / search summary](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/); see also [HF Open ASR Leaderboard blog](https://huggingface.co/blog/open-asr-leaderboard)
- Whisper large-v3 (1.55B, 99 languages, MIT) has been overtaken by about 10 models on the leaderboard. — [MarkTechPost (secondary)](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)

**NVIDIA Parakeet TDT 0.6B v3**
- 600M parameters, CC-BY-4.0, 25 European languages (including English, Russian and Ukrainian), with automatic punctuation, capitalization and word-level timestamps. — [HF model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- Open ASR average WER: 6.34% on the model card, 6.32% on MarkTechPost. RTFx 3,332.74 on GPU. — [HF model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); [MarkTechPost](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- It is offline (TDT, full attention), but NeMo provides a chunked "streaming with Parakeet" inference script with configurable chunk and context sizes. — [HF model card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- ONNX ports:
  - sherpa-onnx int8 export: [csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8)
  - istupakov ONNX export: [istupakov/parakeet-tdt-0.6b-v3-onnx](https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx)
  - The weights-only int8 export has a 669 MB encoder instead of 2.55 GB. It needs ONNX Runtime 1.22 or later (sherpa-onnx 1.12 or later). It quantizes only the weights because stock dynamic-INT8 exports "could drop multi-second speech spans". — [ekhodzitsky int8 card](https://huggingface.co/ekhodzitsky/parakeet-tdt-0.6b-v3-onnx-weights-only-int8)
- Memory: one user reported the sherpa-onnx int8 Parakeet 0.6B using about 1.2 GB of RAM. — [sherpa-onnx issue #2626](https://github.com/k2-fsa/sherpa-onnx/issues/2626)
- CPU speed:
  - Transcribing 3.845 s of audio in sherpa-onnx took 1.249 s (RTF 0.325). — [ekhodzitsky int8 card (search summary)](https://huggingface.co/ekhodzitsky/parakeet-tdt-0.6b-v3-onnx-weights-only-int8)
  - transcribe-rs benchmarks for Parakeet int8: about 30x real-time on an M4 Max, about 20x on a Ryzen 5700X, about 5x on a Skylake i5-6500, and about 5x on the Jetson Nano CPU. — [transcribe-rs README](https://github.com/cjpais/transcribe-rs)
- Parakeet TDT 1.1B has RTFx above 2,000 but ranks about 23rd for accuracy. — [search summary of Northflank/AssemblyAI blogs (secondary)](https://northflank.com/blog/best-open-source-speech-to-text-stt-model-in-2026-benchmarks)

**NVIDIA Nemotron Speech Streaming EN 0.6B (true streaming)**
- Cache-aware FastConformer-RNNT, 600M parameters, 24 encoder layers, English only. Licensed under the NVIDIA Open Model License, which permits commercial use. — [HF model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)
- Chunk size is chosen at runtime: 80 ms, 160 ms, 560 ms or 1.12 s. Average WER by chunk size: 8.43% at 0.08 s, 7.67% at 0.16 s, 7.07% at 0.56 s, 6.93% at 1.12 s. — [HF model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)
- The card lists only NVIDIA GPUs (V100, A100, A6000, DGX Spark). It offers a GGUF build for C++ inference via NeMo-Speech.cpp and does not mention ONNX. An MLX port exists. — [HF model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b); [nemotron-asr-mlx](https://github.com/199-biotechnologies/nemotron-asr-mlx)
- In a GPU production pipeline, median final-transcript latency was about 24 ms and voice-to-voice latency about 500 ms. Cache-aware processing is up to 3x more efficient than buffered streaming. — [NVIDIA HF blog](https://huggingface.co/blog/nvidia/nemotron-speech-asr-scaling-voice-agents)
- A newer `nvidia/nemotron-3.5-asr-streaming-0.6b` exists. I did not fetch its details. — [HF](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b)

**Moonshine v2 (Useful Sensors, Feb 2026): streaming, built for edge devices**
- Sizes: Tiny 33.6M, Small 123.4M, Medium 244.9M parameters. English only; multilingual variants are planned. The paper says only "permissive license". — [arXiv 2602.12241](https://arxiv.org/html/2602.12241v1)
- Average WER across 8 Open ASR sets: Tiny 12.01%, Small 7.84%, Medium 6.65%. LibriSpeech-clean: 4.49%, 2.49% and 2.08% respectively. — [arXiv 2602.12241](https://arxiv.org/html/2602.12241v1)
- Response latency on Apple M3: Tiny 50 ms, Small 148 ms, Medium 258 ms. For comparison, Whisper Tiny takes 289 ms and Whisper Large v3 takes 11,286 ms. Algorithmic lookahead is at most 80 ms (sliding-window attention). — [arXiv 2602.12241](https://arxiv.org/html/2602.12241v1)

**Whisper family (whisper.cpp / faster-whisper)**
- On long-form YouTube-commons audio, large-v3-turbo runs at 129.5x real-time vs 55.3x for v3, with nearly identical accuracy. — [Whisper Notes blog (secondary)](https://whispernotes.app/blog/introducing-whisper-large-v3-turbo)
- The whisper.cpp large-v3-turbo-q5_0 file is 547 MiB. On an older CPU (i5-460M), q5_0/q5_1 were 3 to 5.5x slower than other quantizations and q4_0 was fastest. — [whisper.cpp discussion #3752](https://github.com/ggml-org/whisper.cpp/discussions/3752)
- Whisper is chunked offline (30 s windows). It does not natively stream. — inferred from the Moonshine paper comparing Whisper response latency ([arXiv](https://arxiv.org/html/2602.12241v1))

**SenseVoice-Small (Alibaba FunAudioLLM)**
- Non-autoregressive. Processes 10 s of audio in 70 ms, 15x faster than Whisper-Large. Trained on more than 400k hours and covers 50+ languages, strongest for zh/yue/en/ja/ko. Also does emotion and audio-event detection. License is custom ("model-license"), not OSI. — [HF model card](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)

**Voxtral Mini 4B Realtime 2602 (Mistral)**
- Natively streaming with a causal audio encoder. Delay is configurable from 240 ms to 2.4 s; 480 ms is recommended. 13 languages, Apache 2.0. — [HF model card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- At 480 ms it is described as competitive with ElevenLabs Scribe v2 Realtime and with offline Whisper. MarkTechPost lists 7.68% WER and describes the model as a 3.4B LM plus a 970M encoder. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602); [MarkTechPost](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- Community ports exist in GGUF (llama.cpp) and ONNX. — [GGUF](https://huggingface.co/freddm/Voxtral-Mini-4B-Realtime-2602-GGUF); [ONNX community](https://huggingface.co/onnx-community/Voxtral-Mini-4B-Realtime-2602-ONNX)

**Kyutai STT**
- stt-1b-en_fr: about 1B parameters, 0.5 s delay, semantic VAD. stt-2.6b-en: English only. Both CC-BY-4.0, about 6.40% WER. — [HF stt-1b-en_fr](https://huggingface.co/kyutai/stt-1b-en_fr); [MarkTechPost](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- The production server is written in Rust (moshi-server, websockets). It serves 64 streams at 3x real-time on an L40S. The semantic VAD is available only in the Rust server. — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling/)
- A user reported that real-time mic STT on the Rust server was "painfully slow" on their hardware, which suggests the models need a GPU. — [issue #123](https://github.com/kyutai-labs/delayed-streams-modeling/issues/123)

**sherpa-onnx streaming Zipformer**
- Streaming (online) transducer models are provided in fp16 and int8, with encoder, decoder and joiner files, for many languages. — [sherpa-onnx Zipformer model list](https://k2-fsa.github.io/sherpa/onnx/pretrained_models/online-transducer/zipformer-transducer-models.html)

**OS built-ins**
- **Windows AI APIs `Microsoft.Windows.AI.Speech.SpeechRecognitionModel`**
  - Requires Windows 11 24H2 (build 26100) or later and WinAppSDK 1.7.1 or later.
  - Supports batch and streaming recognition. The streaming `Recognized` event fires per completed phrase.
  - Runs on the NPU on Copilot+ PCs, where the model is preinstalled. On other PCs it runs on the CPU after an on-demand download through Windows Update. GPU is not supported.
  - Recommended CPU: 4+ cores, 3 GHz+ base clock, 32 MB+ L3 cache.
  - The app must be MSIX-packaged with the `systemAIModels` capability.
  - The API returns `NotSupportedOnCurrentSystem`, and the docs recommend falling back to the legacy Windows SDK speech API or a cloud service in that case.
  - [Microsoft Learn, updated 2026-07-07](https://learn.microsoft.com/en-us/windows/ai/apis/speech-recognition)
- **Legacy `Windows.Media.SpeechRecognition` (WinRT)**: does not support custom engines and requires the microphone capability plus user consent for speech. — [MS Q&A](https://learn.microsoft.com/en-us/answers/questions/2181090/speech-recognition-engine-question-on-windows)
- **Apple SpeechAnalyzer (macOS/iOS 26)**
  - Modules: SpeechTranscriber (long-form), DictationTranscriber (short utterances) and SpeechDetector (VAD).
  - WER on clear speech: 2.12%, vs Whisper Small 3.74% and legacy SFSpeechRecognizer 9.02%. On harder speech: 4.56% vs Whisper Small 7.95%. Tested on an M2 Pro.
  - [GIGAZINE (secondary)](https://gigazine.net/gsc_news/en/20260714-apple-speech-analyzer-benchmark/); [Argmax](https://www.argmaxinc.com/blog/apple-and-argmax)
  - Developer forum threads report it being relatively slow in some pipelines. — [Apple forums](https://developer.apple.com/forums/thread/794720)

### Inferences
- **Summary table.** All values come from the findings above; "?" means no figure was found.

| Engine | Params / disk | Avg WER (Open ASR-style) | Streaming? | Languages | License | Best HW path |
|---|---|---|---|---|---|---|
| Moonshine v2 Tiny / Small / Medium | 34M / 123M / 245M | 12.0 / 7.84 / 6.65% | True streaming (80 ms lookahead) | EN | permissive | CPU (ORT) |
| Nemotron Speech Streaming 0.6B | 600M | 6.93% (1.12 s) to 8.43% (80 ms) | True streaming, cache-aware | EN | NVIDIA Open Model | CUDA; GGUF for CPU |
| Parakeet TDT 0.6B v3 | 600M; int8 encoder about 669 MB | 6.32–6.34% | Offline (chunked possible) | 25 EU | CC-BY-4.0 | CPU int8 ORT; CUDA/DirectML |
| Whisper large-v3-turbo | about 809M; q5_0 547 MiB | slightly worse than v3 | Chunked offline | 99 | MIT | whisper.cpp CUDA/Vulkan/Metal |
| SenseVoice-Small | small (NAR) | ? | Offline, very fast | 50+ (best on CJK) | custom | CPU ORT |
| Voxtral Mini 4B Realtime | about 4B | 7.68% | True streaming (240 ms–2.4 s) | 13 | Apache 2.0 | GPU (vLLM, GGUF) |
| Kyutai STT 1B / 2.6B | 1B / 2.6B | about 6.4% | True streaming (0.5 s / 2.5 s delay) | EN/FR / EN | CC-BY-4.0 | GPU (Rust moshi-server) |
| Granite 4.1 2B / Cohere Transcribe / Canary-Qwen | 2–2.5B | 5.3–5.6% | Offline | 5–14 / EN | Apache / CC-BY | GPU |
| Windows AI SpeechRecognitionModel | OS-managed | ? | Streaming (phrase-level finals) | ? | OS | NPU (Copilot+) / CPU |
| Apple SpeechAnalyzer | OS-managed | 2.12% (clear) in 1 test | Streaming | many | OS | Apple Neural Engine |

- Licensing for a shipped product:
  - Moonshine, Voxtral, Granite, Qwen3-ASR and Whisper are the least restrictive.
  - Parakeet and Canary (CC-BY-4.0) require attribution only.
  - Before redistributing, KIVO should check SenseVoice's custom license and the NVIDIA Open Model License (Nemotron).
- The LLM-decoder leaderboard leaders (2B+) are unsuitable as a default for low-end CPU laptops, although they are candidates for an optional GPU "High Accuracy" tier.

### Gaps
- I could not load the live Open ASR Leaderboard (the Space did not render), so leaderboard WER/RTFx come from model cards and MarkTechPost.
- I found no verified figures for:
  - Canary-1B-flash / Canary-1B-v2 WER and CPU speed
  - distil-whisper large-v3.5 numbers
  - faster-whisper CPU RTF on a typical low-end laptop
  - Vosk/Kaldi WER
  - Streaming Zipformer WER on the Open ASR sets
  - Granite Speech CPU feasibility
- No measured peak RAM/VRAM figures were found for most models, apart from Parakeet int8 (about 1.2 GB RAM, one user report).
- I found no source confirming ONNX Runtime QNN or OpenVINO execution of Parakeet or Moonshine on NPUs. NPU support is unverified except for the Windows built-in API.
- Language coverage of the Windows AI Speech API is not stated on the page I fetched.

## True streaming vs chunked offline, and CPU latency

### Takeaway
The models that are truly streaming (they emit partials as the user speaks, with bounded latency) are Moonshine v2, Nemotron Speech Streaming, the sherpa-onnx streaming Zipformer/Paraformer models, Voxtral Realtime, Kyutai STT, the Windows AI streaming API and Apple SpeechAnalyzer. Parakeet TDT, Whisper, SenseVoice and the leaderboard LLM-ASR models are offline and need VAD-segmented or chunked re-decoding. That is acceptable for short commands because they are very fast on CPU.

### Cited Findings
- Moonshine v2 uses sliding-window ("ergodic") encoder attention to bound latency, with 80 ms maximum lookahead. Response latencies on M3 are 50, 148 and 258 ms for Tiny, Small and Medium, at 8–29% compute load. — [arXiv 2602.12241](https://arxiv.org/html/2602.12241v1)
- Nemotron's chunk size (80, 160, 560 or 1120 ms) is chosen at runtime without retraining. — [HF card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)
- Voxtral Realtime's delay is configurable from 240 ms to 2.4 s, with 480 ms as the sweet spot. — [HF card](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- Kyutai's delay is 0.5 s for the 1B model. MarkTechPost gives 0.5–2.5 s across the variants. — [HF](https://huggingface.co/kyutai/stt-1b-en_fr); [MarkTechPost](https://www.marktechpost.com/2026/07/23/best-open-speech-recognition-asr-models-in-2026-wer-languages-latency-and-license-compared/)
- Parakeet v3 is offline with optional chunked streaming via NeMo scripts. — [HF card](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3)
- Parakeet int8 CPU throughput ranges from about 5x real-time on a 2015-era Skylake i5-6500 to about 20x on a Ryzen 5700X. — [transcribe-rs](https://github.com/cjpais/transcribe-rs)
- SenseVoice-Small takes 70 ms for 10 s of audio (GPU figure per the model card context). — [HF card](https://huggingface.co/FunAudioLLM/SenseVoiceSmall)
- Whisper Large v3 response latency is 11.3 s on M3 in the streaming-latency setup of the Moonshine paper. — [arXiv](https://arxiv.org/html/2602.12241v1)
- Windows AI streaming delivers "final results as complete phrases are recognized", so the docs indicate phrase-level finals rather than word-level partials. Latency scales with the host CPU. — [MS Learn](https://learn.microsoft.com/en-us/windows/ai/apis/speech-recognition)

### Inferences
- **Time to final for a 3 s command on a low-end CPU.** An offline model running at 5x real-time adds about 0.6 s after endpointing. At 20x it adds about 0.15 s. This is my own arithmetic from the RTF figures above.
- **Streaming models.** These keep the time to final bounded by the chunk size plus compute, for example about 50–260 ms for Moonshine on Apple M3.
- **Practical pattern.** Use a streaming model (Moonshine v2 or a Zipformer) for live partials and UI feedback. On VAD end-of-speech, optionally re-decode the utterance with Parakeet v3 for the final text (a "two-pass" design).

### Gaps
- No independent first-partial latency measurements on Windows x86 CPUs were found for Moonshine v2, Zipformer or Nemotron. The Moonshine figures are for Apple M3 only.
- No CPU latency figures for Voxtral Realtime or Kyutai were found.

## Cloud STT options: latency, price, turn detection

### Takeaway
For conversational use, Deepgram Flux and AssemblyAI Universal-Streaming are the best fits:
- Both have integrated semantic end-of-turn detection and latency around 300 ms.
- Flux costs $0.0065/min (English) and AssemblyAI $0.15/hr, which works out to about $0.0025/min.

OpenAI's realtime transcription costs $0.017/min. Its gpt-4o-transcribe family costs $0.003–0.006/min but is file-based.

### Cited Findings
- **Deepgram Flux**
  - $0.0065/min (English), $0.0078/min (Multilingual, 10 languages, GA April 29 2026).
  - Median end-of-turn detection under 300 ms, with 1.5 s p95.
  - Integrated end-of-turn detection means no external VAD is needed.
  - [LLMReference (secondary)](https://www.llmreference.com/provider/deepgram/flux-asr); [Deepgram pricing](https://deepgram.com/pricing); [Coval](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **Deepgram Nova-3 / Flux streaming band**: $0.0048–0.0078/min, sub-300 ms latency. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **AssemblyAI Universal-Streaming**
  - $0.15/hr, billed on session duration, with unlimited concurrency.
  - About 300 ms latency, immutable transcripts, and intelligent endpointing that combines acoustic and semantic features.
  - [AssemblyAI Universal-Streaming](https://www.assemblyai.com/universal-streaming); [turn detection blog](https://www.assemblyai.com/blog/turn-detection-endpointing-voice-agent)
- **AssemblyAI Universal-3 Pro Streaming**: 300–600 ms median latency, 5.6% mean English WER, $0.45/hr. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **OpenAI** (official pricing page)
  - gpt-live-transcribe $0.017/min
  - gpt-realtime-whisper $0.017/min
  - gpt-transcribe $0.0045/min
  - gpt-4o-transcribe $0.006/min
  - gpt-4o-mini-transcribe $0.003/min
  - whisper-1 $0.006/min
  - [OpenAI pricing](https://developers.openai.com/api/docs/pricing)
  - Coval claims sub-150 ms realtime latency. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **Google Chirp 3**: about 250–400 ms streaming, $0.36/hr batch, about $1/hr standard real-time. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **Azure MAI-Transcribe-1**: 3.8% average WER on FLEURS (25 languages), billed per GPU-hour. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **ElevenLabs Scribe v2 Realtime**: sub-150 ms, 30 languages, prices cut 45% after May 7. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **Speechmatics (Ursa 2)**: sub-1 s real-time, Flow at $0.0537/min. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)
- **Cartesia Ink-Whisper**: $0.13/hr. — [Coval (secondary)](https://www.coval.ai/blog/best-speech-to-text-providers-in-2026-independent-benchmarks-and-how-to-choose/)

### Inferences
- KIVO's cloud tier should default to Deepgram Flux or AssemblyAI Universal-Streaming. Built-in turn detection removes the need for local VAD endpointing when cloud is selected.
- At personal-assistant volumes (for example 60 min/day), cloud STT costs roughly $0.15–0.40/month. This is my own arithmetic.

### Gaps
- Several cloud figures come only from Coval, a vendor-neutral benchmark blog: Google, Azure, ElevenLabs, Speechmatics and Cartesia. I did not verify them on the vendors' own pricing pages.
- Azure Speech's standard real-time per-hour price and the exact ElevenLabs per-hour price were not found.
- Soniox was not covered.
- I found no independent head-to-head first-partial latency benchmark across cloud vendors from a single source.

## Rust ecosystem and what OSS dictation apps use

### Takeaway
The most direct path for KIVO is **transcribe-rs**, which is used by Handy, a Tauri/Rust dictation app. It wraps ONNX Runtime (Parakeet, Canary, Cohere, Moonshine including streaming, SenseVoice, GigaAM) and whisper.cpp behind one API, and it can select CUDA, DirectML, CoreML, ROCm, Vulkan or WebGPU at runtime. For streaming Zipformer or Paraformer, the official `sherpa-onnx` Rust crate is the alternative.

### Cited Findings
- **transcribe-rs**
  - Engines: Parakeet, Canary, Cohere, Moonshine, SenseVoice, GigaAM, Whisper, Whisperfile, and OpenAI (cloud).
  - The ONNX Runtime (ort) backend powers the ONNX models. Whisper runs through whisper.cpp with Metal, Vulkan or CUDA.
  - Accelerators: CUDA, ROCm, Metal, Vulkan, DirectML, CoreML, WebGPU, selected via `set_ort_accelerator(OrtAccelerator::Auto)`.
  - Moonshine streaming is exposed as `StreamingModel::load(...)`.
  - [transcribe-rs](https://github.com/cjpais/transcribe-rs)
- **Handy**
  - Tauri app for Windows, macOS and Linux.
  - Offers Whisper Small, Medium, Turbo and Large with GPU acceleration, and "Parakeet V3 – CPU-optimized model with excellent performance and automatic language detection".
  - Also offers Parakeet Unified EN 0.6B (731 MB, Q8_0).
  - Uses Silero VAD to filter silence.
  - [Handy GitHub](https://github.com/cjpais/Handy)
- **sherpa-onnx crates**
  - `sherpa-onnx` provides safe Rust bindings over the sherpa-onnx C API, with RAII types. — [docs.rs](https://docs.rs/sherpa-onnx/latest/sherpa_onnx/)
  - `sherpa-transducers` wraps the streaming Zipformer transducers and has optional CUDA and DirectML features. Its README says DirectML is "entirely untested". — [crates.io](https://crates.io/crates/sherpa-transducers)
- **Kyutai** ships a Rust inference server (moshi-server). — [delayed-streams-modeling](https://github.com/kyutai-labs/delayed-streams-modeling/)
- **whisper.cpp alternatives on Mac**: WhisperKit (Argmax) is an alternative for Apple devices, and Argmax works with SpeechAnalyzer. — [Argmax](https://www.argmaxinc.com/blog/apple-and-argmax)

### Inferences
- Suggested stack:
  - `transcribe-rs` for local models
  - `ort` with the DirectML execution provider for Windows GPU/iGPU acceleration of ONNX models
  - `whisper-rs`/whisper.cpp with Vulkan for Whisper on AMD/Intel GPUs
  - Silero VAD via ORT
  - websocket clients for Deepgram and AssemblyAI
- Using the Windows AI Speech API from Rust would need WinRT projection (the `windows` crate) plus MSIX packaging with the `systemAIModels` capability. That limits it to packaged builds.

### Gaps
- The exact Rust crate Vibe uses (probably whisper-rs) was not verified in this session.
- whisper-rs's current version and features were not fetched.
- Whether transcribe-rs exposes the QNN or OpenVINO execution providers was not stated.

## Model distribution, quantization, residency

### Takeaway
Ship no large model in the installer. Download on demand, prefer int8 ONNX (or q4_0/q8_0 GGML for Whisper), and keep the selected STT model resident while the companion is active. Pay a one-time consent-plus-download step, as the Windows AI API does.

### Cited Findings
- Model sizes:
  - Parakeet v3 encoder: 669 MB int8 vs 2.55 GB fp32 (weights-only int8). — [HF](https://huggingface.co/ekhodzitsky/parakeet-tdt-0.6b-v3-onnx-weights-only-int8)
  - ONNX int8 Parakeet v3 is listed at 1.02 GB total in one benchmark. — [search summary of groxaxo repo](https://github.com/groxaxo/parakeet-tdt-0.6b-v3-fastapi-openai/blob/main/README.md)
  - Whisper large-v3-turbo q5_0 is 547 MiB. — [whisper.cpp #3752](https://github.com/ggml-org/whisper.cpp/discussions/3752)
  - Handy's Parakeet Unified EN is 731 MB. The Whisper sizes Handy lists (Small 487 MB, Turbo 1600 MB, Large 1100 MB) appear to mix quantization levels. — [Handy](https://github.com/cjpais/Handy)
  - Moonshine v2 Tiny has 34M parameters (a few tens of MB). — [arXiv](https://arxiv.org/html/2602.12241v1)
- Naive dynamic-int8 exports of Parakeet can drop speech spans. Weights-only int8 avoids this. — [HF](https://huggingface.co/ekhodzitsky/parakeet-tdt-0.6b-v3-onnx-weights-only-int8)
- Parakeet int8 in sherpa-onnx uses about 1.2 GB of RAM. — [sherpa-onnx #2626](https://github.com/k2-fsa/sherpa-onnx/issues/2626)
- On old CPUs, q4_0 is the fastest whisper.cpp quantization and q5 variants are 3 to 5.5x slower. — [whisper.cpp #3752](https://github.com/ggml-org/whisper.cpp/discussions/3752)
- Windows AI downloads its CPU model on demand through Windows Update, with a recommended consent dialog. The user can remove it under Settings > System > AI Components. — [MS Learn](https://learn.microsoft.com/en-us/windows/ai/apis/speech-recognition)

### Inferences
- The cold-load cost (reading 0.5–1 GB from disk and creating the ORT session) plausibly takes seconds on HDD or low-end machines. KIVO should preload the STT model at app start, or on wake-word arm, and unload it only under memory pressure or after a long idle period. I have no measured load times.

### Gaps
- No published load-time (cold-start) measurements were found for Parakeet, Moonshine or Whisper.

## Recommended tiers for KIVO

### Takeaway
- **Ultra Fast CPU default:** Moonshine v2 Small streaming (English), or Parakeet v3 int8 for multilingual European languages. Silero VAD in both cases.
- **Balanced:** Parakeet TDT 0.6B v3 int8 (ORT, with DirectML/CUDA when present), optionally with Moonshine partials.
- **High Accuracy:** GPU-only. Whisper large-v3-turbo (whisper.cpp, CUDA/Vulkan/Metal) for 99 languages, or Voxtral Realtime / Nemotron streaming.
- **Cloud:** Deepgram Flux or AssemblyAI Universal-Streaming, with OpenAI realtime as an option.
- **Platform native:** Windows AI Speech (NPU on Copilot+ PCs) and Apple SpeechAnalyzer on macOS.

### Cited Findings
- Ultra Fast: Moonshine v2 Small has 123M parameters, 7.84% WER and 148 ms response latency (M3). Tiny has 34M parameters, 12% WER and 50 ms. — [arXiv](https://arxiv.org/html/2602.12241v1)
- Balanced: Parakeet v3 has 6.34% WER, 25 languages, and runs at about 5–20x real-time on CPU in int8. — [HF](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3); [transcribe-rs](https://github.com/cjpais/transcribe-rs)
- High Accuracy streaming on GPU:
  - Nemotron: 6.93% WER at a 1.12 s chunk. — [HF](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)
  - Voxtral Realtime: 13 languages at a 480 ms delay. — [HF](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602)
- Cloud:
  - Deepgram Flux: $0.0065/min with end-of-turn detection under 300 ms. — [LLMReference](https://www.llmreference.com/provider/deepgram/flux-asr)
  - AssemblyAI Universal-Streaming: $0.15/hr, about 300 ms. — [AssemblyAI](https://www.assemblyai.com/universal-streaming)
- Native: the Windows AI Speech API runs on the NPU on Copilot+ PCs, but it requires MSIX. — [MS Learn](https://learn.microsoft.com/en-us/windows/ai/apis/speech-recognition)

### Inferences
- The default CPU pick depends on the language:
  - English-only users: Moonshine v2 Small streaming gives real partials at the lowest cost.
  - Multilingual users: Parakeet v3 int8 run on VAD segments is the best default. It is too heavy for truly low-end machines (about 1.2 GB RAM, 5x real-time on a Skylake i5), so fall back to Moonshine Tiny or Whisper base/small q4_0 there.
- Where sherpa-onnx has a streaming Zipformer for the user's language, it is a viable ultra-low-resource streaming fallback.
- Hardware detection should drive the tier: NPU-capable Windows 11 24H2 with MSIX, NVIDIA GPU, or CPU core count and cache size (the MS Learn CPU guidance of 4 cores, 3 GHz and 32 MB L3 is a useful threshold).

### Gaps
- No single source benchmarks all candidate models on the same low-end Windows laptop. KIVO should run its own benchmark on the target hardware classes before finalizing the defaults.
