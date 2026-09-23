# Benchmark harness and performance budgets

Status: Draft v1, 2026-09-21. Implements plan §96–103. **Engine defaults are chosen by this
harness, not by guesswork.**

## 1. `kivo-bench` (CLI, `apps/kivo-bench`)

| Suite | Measures | Data |
|---|---|---|
| `stt` | cold load time, first partial, final latency after end-of-speech, RTF, WER (overall + accented subsets), peak RAM/VRAM, CPU% | LibriSpeech test-clean subset; accented English (e.g. L2-ARCTIC / Common Voice accents); 50 KIVO command utterances; optional user enrollment clips |
| `tts` | first-audio latency (short/medium/long text), RTF, RAM, cancel-to-silence | Fixed text set incl. numbers/URLs/code |
| `wake` | false accepts/hour, false rejects %, CPU% | Negative: ≥ 10 h of speech/podcast/TV audio; positive: recorded "Hey Kivo" set (varied speakers/mics/distances) + synthetic |
| `vad` / `aec` | detection latency, false triggers with TTS playing, echo leakage into STT | Scripted playback + capture loop |
| `idle` | CPU%, RAM, wakeups/sec, package power over 30 min, listening on | Real runtime, idle desktop |
| `overlay` | show latency (hotkey → first frame), white-flash check, GPU utilization/power while idle vs animating | The running Island, driven by synthetic push-to-talk; screen sampling; Windows performance counters (GPU engine load, RAPL package power) |
| `e2e` | T0–T10 spans (plan §97) for the three journeys of plan §102 | Scripted audio injected via virtual mic, fake/real brains |
| `brain` | TTFT, tokens/s, tool-call latency, structured-output validity, cancellation latency, error rate | Fixed prompts per provider |

- **Output:** JSON results go into the `benchmarks` table and `bench-results/<date>-<machine>.json`.
  A Markdown summary is written to `docs/benchmarks/`.
- **Machine fingerprint:** CPU, GPU, RAM, OS build, power source.
- **Repeatability:** every run is ≥ 5 runs, reporting p50/p95, with warmup excluded from the cold
  metrics.

## 2. Reference hardware

| Tier | Example | Purpose |
|---|---|---|
| Low | 4-core laptop CPU, 8 GB RAM, iGPU, Windows 10 or 11 | Defines the "Ultra Fast" defaults; must meet budgets |
| Mid | 8-core laptop, 16 GB, iGPU | Balanced defaults |
| High | Desktop, NVIDIA GPU 8 GB+ | Accurate tier, GPU policies |
| NPU (optional) | Copilot+ PC | Windows AI Speech evaluation |

Your development PC is the first "High" or "Mid" data point. A low-end machine or VM profile must
be secured before defaults are frozen.

## 3. Budgets

See [VOICE.md §10](VOICE.md). In addition:

| Metric | Budget |
|---|---|
| Installer size | < 40 MB |
| Runtime idle RAM | ≤ 150 MB |
| UI process idle RAM (overlay preloaded, CC closed) | measure in M0; target ≤ 120 MB |
| Cold start to "ready" (runtime) | ≤ 1.5 s |
| Control Center open (warm) | ≤ 300 ms |

## 4. Regression gate

- CI runs a **smoke subset** on every PR: the intent-router latency, the IPC round trip, and the
  state-machine and cancellation tests.
- The full suites run before each release, on the reference machines.
- A regression of more than 10% on a budgeted metric blocks the release (plan §159).

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [x] **BENCH-01** · M0 · `kivo-bench` CLI with the harness: machine fingerprint (CPU, GPU, RAM, OS build, power source), ≥ 5 runs with p50/p95, warmup excluded from cold metrics, JSON into the `benchmarks` table and `bench-results/<date>-<machine>.json`, Markdown summary into `docs/benchmarks/` (§1) → done: `apps/kivo-bench` (`machine.rs` fingerprint from `kivo-platform-windows::WindowsSystemInfo` + capabilities; `harness.rs` warmup discarded then N runs; `stats.rs` nearest-rank p50/p95/min/max/mean; `report.rs` merges each day's `bench-results/<date>-<machine>.json`, rewrites `docs/benchmarks/<date>-<machine>.md`; rows in the new `benchmarks` table, migration 2); saved runs require ≥ 5 runs · verified: 8 unit tests; `kivo-bench ipc --runs 10 --warmup 2` on this PC wrote all three outputs (ping round trip p50 28.7 µs)
- [~] **BENCH-02** · M0 · `stt` suite: cold load, first partial, final latency, RTF, WER (overall + accented), peak RAM/VRAM, CPU% on LibriSpeech subset, accented English and 50 KIVO commands (§1) → partial: `apps/kivo-bench/src/speech/stt.rs` (Moonshine v2, Parakeet TDT v3, Whisper turbo via sherpa-onnx; cold load, end-of-speech → final, RTF, WER with normalization, peak memory on LibriSpeech test-clean) + `wer.rs` (tested) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; accented-English WER needs a licensed set (L2-ARCTIC / Common Voice); first-partial time needs a streaming engine
- [~] **BENCH-03** · M0 · `tts` suite: first audio (short/medium/long), RTF, RAM, cancel-to-silence (§1) → partial: `apps/kivo-bench/src/speech/tts.rs` (Kokoro-82M int8: first audio for short/medium/long text, RTF, cancel → silence, memory) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`
- [~] **BENCH-04** · M0 · `wake` suite: false accepts/hour on ≥ 10 h negative audio, false rejects % on the recorded "Hey Kivo" set, CPU% (§1) → partial: `apps/kivo-bench/src/speech/wake.rs` ("Hey Kivo" with sherpa KWS; synthetic positives from every Kokoro voice × 3 speeds through a room, clean and over speech; LibriSpeech negatives; false rejects, false accepts/hour, CPU; tokenizer tested) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; ≥ 10 h mixed negatives (5.4 h of read speech available)
- [~] **BENCH-05** · M0 · `vad` / `aec` suite: detection latency, false triggers with TTS playing, echo leakage into STT (§1) → partial: `apps/kivo-bench/src/speech/vad_aec.rs` (Silero VAD v6 + WebRTC AEC3 via sonora on a simulated room: onset/end latency, false triggers and STT leakage from echo with and without AEC, ERLE, barge-in onset) · missing: not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`; the OS AEC on real devices (VOICE-30)
- [~] **BENCH-06** · M0 · `idle` suite: CPU%, RAM, wakeups/s and package power over 30 min with listening on (§1) → partial: `apps/kivo-bench/src/suites/idle.rs`; 30-min run saved (release build, 2026-09-21): runtime 0.00% CPU, 6.8 MB, 0 wakeups/s; app + WebView2 0.00% CPU, 230 MB (over the 120 MB UI budget, BENCH-12) · missing: "listening on" (always-on wake listening arrives in M2)
- [x] **BENCH-07** · M0 · `overlay` suite: hotkey → done: `apps/kivo-bench/src/suites/overlay.rs` + `win.rs` (synthetic push-to-talk; screen sampling over a grey backdrop for first frame, white flash and exit; window style and focus checks; GPU 3D load and RAPL package power from Windows performance counters instead of PresentMon, DECISIONS "Benchmark counters") · verified: saved release-build run (2026-09-21): 155 ms p50 to visible, no flash, +1.1 W package power while animating (was +12 W before the redraw fixes)
- [x] **BENCH-08** · M1 · `e2e` suite: T0–T10 spans for the three plan §102 journeys with scripted audio via a virtual mic (§1) → done: `kivo-bench e2e` drives `kivo-e2e` (the runtime's scripted journeys: Windows' voice into a scripted microphone, the real turn engine, speech worker and models, fake apps and system controls) through the three plan §102 journeys and reports T spans from the end of speech; medium and complex end as "unhandled" until the brain router (M3) and agents (M5) exist · verified: 5 runs on the owner's PC (2026-09-23): simple end of speech → action done 37 ms p50 / 38 ms p95; medium → intent 95 ms; complex → intent 172 ms; `spans_become_samples_from_the_end_of_speech`
- [ ] **BENCH-09** · M3 · `brain` suite: TTFT, tokens/s, tool-call latency, structured-output validity, cancellation latency, error rate (§1)
- [~] **BENCH-10** · M0 · Reference machines recorded: the owner's PC (mid/high) and the 4-core / 8 GB / no-GPU VM (low, labelled approximate) (§2, DECISIONS) → partial: the owner's PC is recorded in every report (Ryzen 7 6800H, 16 threads, 15 GB, RTX 3060 6 GB + Radeon iGPU, Windows 11 25H2); `--tier low` emulates the 4-core tier on it, labelled approximate (DECISIONS "Wake-word data") · missing: a low-tier run (not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`)
- [~] **BENCH-11** · M0 · The first benchmark report is committed to `docs/benchmarks/` and the engine defaults in DECISIONS.md are updated with measured numbers (§1) → partial: first report committed: `docs/benchmarks/2026-09-21-amd-ryzen-7-6800h.md` (ipc, audio, overlay, idle) + `bench-results/` · missing: the engine results and updated engine defaults in DECISIONS (needs BENCH-02–05; not run: the owner deferred benchmark runs until after the build milestones (2026-09-22); `pnpm bench:data` fetches the data, then `kivo-bench <suite>`)
- [ ] **BENCH-12** · M1 · Budgets met: runtime idle RAM ≤ 150 MB, UI idle RAM ≤ 120 MB (overlay preloaded, CC closed), runtime cold start ≤ 1.5 s, Control Center open (warm) ≤ 300 ms (§3)
- [~] **BENCH-13** · M0 · CI smoke subset on every PR: intent-router latency, IPC round trip, state-machine and cancellation tests (§4) → partial: `ci.yml` runs `kivo-bench ipc` after the tests (state-machine and cancellation tests are unit tests in the same job) · missing: the intent-router latency (the router is M1); not yet run in CI (runs on the next version tag)
- [ ] **BENCH-14** · M9 · Full suites before each release on the reference machines; a > 10% regression on a budgeted metric blocks the release (§4)
- [ ] **BENCH-15** · M3 · "Benchmark this engine" in the Control Center: STT first-partial and final latency, RTF, WER, CPU/RAM/VRAM, noise; TTS first audio, RTF, CPU/RAM/VRAM, long text, interruption; stored locally and shown against KIVO's thresholds (VOICE §11)
