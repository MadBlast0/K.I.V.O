# GPU speech: backends and building blocks (owner, 2026-09-25)

Local speech models run on the graphics card by default, with the processor as the fallback, the
way desktop AI apps do it (Handy, Ollama, LM Studio): **GGML engines** (whisper.cpp, llama.cpp,
TTS.cpp) with a GPU backend chosen per machine. DirectML is not used for local models
(DECISIONS "GGML on Vulkan, no DirectML"). Build item: VOICE-50.

## Backends

| Machine | Default | Also selectable | Notes |
|---|---|---|---|
| NVIDIA GPU (Windows, Linux) | **CUDA** | Vulkan, CPU | Fastest on NVIDIA. Needs NVIDIA's cuBLAS DLLs (the worker links the CUDA runtime statically; the driver brings the rest), downloaded on demand like a model (≈0.5–1 GB), not in the installer |
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
| [mmwillet/TTS.cpp](https://github.com/mmwillet/TTS.cpp) | Text-to-speech on GGML (Kokoro, Parler, Dia, Orpheus). **Not usable yet** (checked 2026-09-25): its README calls it a proof of concept, supported on macOS only (Windows ✗), CUDA ✗, Vulkan planned, Kokoro without Metal, no streaming; MIT, GPL only with eSpeak NG. Watch it |
| [ggml-org/llama.cpp](https://github.com/ggml-org/llama.cpp) + [llama-cpp-2](https://github.com/utilityai/llama-cpp-rs) | The path for local LLM brains. Its `tts` tool is a demo that writes a file (no streaming, no library API), now for Qwen3-TTS 1.7B and Pocket TTS; the OuteTTS model it used to show is CC BY-NC 4.0, so not for KIVO (checked 2026-09-25) |

Checked 2026-09-25: whisper.cpp on Vulkan builds and runs in KIVO; transcribe-rs is MIT, 0.3 on
crates.io, ONNX engines (Parakeet, Moonshine, Canary, SenseVoice, …) plus whisper.cpp, each engine
still code behind its `SpeechModel` trait.

**Voices on the GPU wait for a runtime.** No GGML voice runtime runs on Windows' GPUs today, so
Kokoro and Supertonic stay on the processor (3.5× and 4× faster than real time there) until one
does. The target is unchanged: GPU first, the processor and RAM as the fallback, as for speech
recognition.

## Order of work (VOICE-50)

1. ~~Remove DirectML~~ (done 2026-09-25, below).
2. ~~The backend setting and detection; the CUDA worker and its on-demand runtime download~~
   (built 2026-09-26, below).
3. ~~Give Supertonic interruptible runs~~ (built 2026-09-26).
4. A GGML voice once a runtime runs on Windows' GPUs (TTS.cpp with Vulkan/CUDA, or another):
   measure first audio, real-time factor and cancel (≤ 100 ms) against Kokoro on the processor.
5. Look at transcribe-rs and Handy's model manager for patterns worth reusing.

## DirectML removed (2026-09-25)

Searched by name (`directml`, `DirectMl`, `dml`, `accel::`, `load_on`, `Device::Gpu`) and by what
depends on it without naming it (the `ort` feature's binaries, the adapter index, the UI's device
names, notices, build and release files):

- `ort`'s `directml` feature is off, so no KIVO code can reach DirectML. pyke's ONNX Runtime
  downloads have no plain build for Windows x64 (the smallest is `directml`, `ort-sys`'s
  `dist.tsv`), so that library still carries DirectML's provider unused; dropping it too would
  mean building ONNX Runtime from source. Every ONNX engine's real-model tests pass on the new build
  (2026-09-26).
- `crates/kivo-voice/src/accel.rs` deleted; its CPU half is `onnx.rs` (`onnx::session`), which all
  eleven ONNX engines now use instead of their own copies of the same builder.
- `Accel::DirectMl`, `Parakeet::load_on`, `Whisper::load_on`, the DirectML Parakeet test, the
  worker's `device()`, `can_use_gpu`'s Parakeet case, the recommender's DirectML check, the
  bench's "encoder on DirectML" row, and the comments and docs that described them.
- **Kept:** ONNX Whisper, on the CPU (the only High-accuracy and Hindi recognizer on ARM64, where
  whisper.cpp isn't built); DXGI adapter listing (GPU detection); `Recovery::Cpu` (any backend).
  No DirectML DLL was ever bundled, and the notices had no DirectML entry.

## Backend setting and CUDA (2026-09-26)

What was built (DECISIONS "VOICE-50: GPU backends"):

| Where | What |
|---|---|
| `kivo-core` config | `performance.graphics-backend`: auto / cuda / vulkan / metal / processor; `gpu_allowed()` |
| `kivo-platform` | `GpuInfo` gains the vendor (PCI id) and the driver version (DXGI); `nvidia_driver()` |
| `kivo-ipc` | `GpuTarget { backend, device }` replaces the DXGI adapter index; `InferWelcome.backend` |
| `kivo-voice` | Features `vulkan` / `cuda` / `metal` over `whisper-cpp`; `BUILT_GPU`; the card found by name among ggml's GPU devices (`gpu_devices`, `same_card`); `Accel::Metal`, `Accel::is_gpu`; Supertonic's interruptible runs |
| `kivo-store` | `.zip` unpacking; the `cuda-runtime-13` pack (`GpuRuntime`) from NVIDIA's redistributables |
| `apps/kivo-infer-cuda` | The worker's source built with CUDA |
| `kivo-runtime` | `gpu::target` / `backend_for` / `options`; the CUDA worker chosen per backend, NVIDIA's DLLs on its PATH, restart on change; `Recovery::Vulkan`; the pack offered only with an NVIDIA card; `performance.status` → `graphics` |
| UI | Settings → Performance → Graphics backend and GPU acceleration for NVIDIA (licence first, the shared `DownloadDialog`); Voice → Advanced names devices |
| Release | release.yml installs the pinned CUDA Toolkit, builds, signs and publishes `kivo-infer-cuda-x64.exe`, and compiles its address and hash into the runtime; `pnpm build:cuda` for development |

Still open: GPU voices (step 4), the macOS platform layer that would give Metal a GPU list,
transcribe-rs / Handy (step 5), and whisper.cpp's stop on the processor (~950 ms: its CPU backend
checks the flag only between large steps; 4.7 ms on CUDA).
