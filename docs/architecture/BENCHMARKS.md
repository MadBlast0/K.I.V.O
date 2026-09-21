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
| `overlay` | show latency (hotkey → first frame), white-flash check, GPU utilization/power while idle vs animating | Tauri overlay spike + PresentMon |
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

See [VOICE.md §9](VOICE.md). In addition:

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
