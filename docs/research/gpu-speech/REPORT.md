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
2. The backend setting and detection above; the CUDA worker and its on-demand runtime download
   (speech recognition: whisper.cpp).
3. Give Supertonic interruptible runs (its cancel takes 3.6 s).
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
  mean building ONNX Runtime from source. Every ONNX engine was re-tested after the change.
- `crates/kivo-voice/src/accel.rs` deleted; its CPU half is `onnx.rs` (`onnx::session`), which all
  eleven ONNX engines now use instead of their own copies of the same builder.
- `Accel::DirectMl`, `Parakeet::load_on`, `Whisper::load_on`, the DirectML Parakeet test, the
  worker's `device()`, `can_use_gpu`'s Parakeet case, the recommender's DirectML check, the
  bench's "encoder on DirectML" row, and the comments and docs that described them.
- **Kept:** ONNX Whisper, on the CPU (the only High-accuracy and Hindi recognizer on ARM64, where
  whisper.cpp isn't built); DXGI adapter listing (GPU detection); `Recovery::Cpu` (any backend).
  No DirectML DLL was ever bundled, and the notices had no DirectML entry.

## Still to change with the backend setting (step 3)

| Where | Change |
|---|---|
| `crates/kivo-ipc/src/infer.rs` `ModelLoad.gpu`, runtime `infer.rs` `Engines.gpu`, `models.rs`, `gpu.rs` `choose()` | Today a **DXGI** adapter index that whisper.cpp only reads as "use a GPU": whisper.cpp then takes **Vulkan device 0**, and Vulkan lists cards in another order than DXGI, so a laptop can run on its integrated GPU. Pass the backend and that backend's device (matched by name or LUID) |
| `crates/kivo-voice/src/engine.rs` | Add `Accel::Metal` |
| `crates/kivo-voice/src/registry.rs` `gpu_first` | Any GPU backend (CUDA, Vulkan, Metal), not only Vulkan |
| `apps/kivo-app/src/components/voice/Details.tsx` (Advanced) | Devices are shown raw (`vulkan`, `cpu`); give them `en.json` labels |
| Settings → Performance, `en.json` `gpuSpeech*` | The Graphics backend picker; the switch covers voices too once they move to the GPU |
| `.github/workflows/release.yml` | The CUDA Toolkit for the `kivo-infer-cuda` build |
| `crates/kivo-voice/src/kokoro/mod.rs`, `interrupt.rs`, `crates/kivo-store/src/models.rs` | ONNX Kokoro and its manifest once a GGML voice replaces it, with a store migration; `interrupt.rs` stays while an ONNX voice does |

Finish each step with `cargo clippy --workspace --all-targets` (zero warnings), `pnpm lint`, and a
search for each removed name returning nothing.
