# GPU speech: backends and building blocks (owner, 2026-09-25)

Local speech models run on the graphics card by default, with the processor as the fallback, the
way desktop AI apps do it (Handy, Ollama, LM Studio): **GGML engines** (whisper.cpp, llama.cpp,
TTS.cpp) with a GPU backend chosen per machine. DirectML is not used for local models
(DECISIONS "GGML on Vulkan, no DirectML"). Build item: VOICE-50.

## Backends

| Machine | Default | Also selectable | Notes |
|---|---|---|---|
| NVIDIA GPU (Windows, Linux) | **CUDA** | Vulkan, CPU | Fastest on NVIDIA. Needs the CUDA runtime DLLs (cudart, cuBLAS), downloaded on demand like a model (≈0.5–1 GB), not in the installer |
| AMD or Intel GPU (Windows, Linux) | **Vulkan** | CPU | Works on every vendor through the driver's Vulkan loader |
| Mac (Apple silicon) | **Metal** | Vulkan (MoltenVK), CPU | Metal is Apple's GPU API; macOS builds are planned, untested here (no Mac) |
| No usable GPU, battery, game in front, GPU busy | **CPU** | — | The GPU policy (PLAN-09) already moves speech off the GPU in these cases |

Later, optional: ROCm/HIP (AMD, Linux), SYCL (Intel), OpenVINO — only if measurements show a clear
gain over Vulkan.

**Detection:** KIVO already reads every adapter (vendor, VRAM) for the GPU policy. NVIDIA with enough
VRAM → CUDA (after its runtime is downloaded; until then Vulkan); other vendors → Vulkan; Mac →
Metal. **Settings → Performance → Graphics backend**: Automatic (default) / CUDA / Vulkan / Metal /
Processor only, showing only the options this machine has. The existing switch "Speech on the
graphics card" stays as the master on/off.

**Build:** one worker per backend is simplest: `kivo-infer` (Vulkan, today) and an optional
`kivo-infer-cuda` downloaded with the CUDA runtime; macOS builds `kivo-infer` with Metal. whisper-rs
exposes `cuda`, `vulkan` and `metal` features; llama.cpp's Rust bindings do the same. The always-on
small models (VAD, wake word, speaker check) stay on the CPU: waking the GPU for them costs more
power than it saves.

## Repositories to use

| Repository | Use in KIVO |
|---|---|
| [ggml-org/whisper.cpp](https://github.com/ggml-org/whisper.cpp) + [whisper-rs](https://github.com/tazz4843/whisper-rs) | Speech-to-text on CUDA/Vulkan/Metal. **Already in KIVO** (Vulkan) |
| [cjpais/Handy](https://github.com/cjpais/Handy) (MIT, Tauri + Rust) | Reference for the model download/switch UX and the GPU/CPU choice — the app the owner likes |
| [cjpais/transcribe-rs](https://github.com/cjpais/transcribe-rs) | Handy's Rust speech-to-text layer (whisper.cpp + Parakeet behind one trait); compare with `kivo-voice` and reuse where simpler |
| [mmwillet/TTS.cpp](https://github.com/mmwillet/TTS.cpp) | **First choice for text-to-speech on GGML**: Kokoro (and Parler, Dia) as GGUF. Check its Vulkan/CUDA support, licence, first audio and cancel |
| [ggml-org/llama.cpp](https://github.com/ggml-org/llama.cpp) + [llama-cpp-2](https://github.com/utilityai/llama-cpp-rs) | Fallback text-to-speech (OuteTTS) with mature CUDA/Vulkan/Metal backends; also the path for local LLM brains |

Nothing above is verified yet beyond whisper.cpp on Vulkan compiling in KIVO (2026-09-25); each
repository's licence and backend support is checked when VOICE-50 is built.

## Order of work (VOICE-50)

1. TTS.cpp: build with Vulkan, run Kokoro GGUF, measure first audio, real-time factor and cancel
   (budget ≤ 100 ms) against today's Kokoro on the CPU; if it can't do Vulkan, OuteTTS via llama.cpp.
2. The backend setting and detection above; the CUDA worker and its on-demand runtime download.
3. Remove Parakeet's DirectML encoder path; give Supertonic interruptible runs (its cancel takes 3.6 s).
4. Look at transcribe-rs and Handy's model manager for patterns worth reusing.
