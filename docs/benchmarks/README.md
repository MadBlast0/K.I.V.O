# Benchmark results

Reports written by `kivo-bench` ([BENCHMARKS.md](../architecture/BENCHMARKS.md)): one file per
day and machine, `<date>-<machine>.md`, with the raw samples in
`bench-results/<date>-<machine>.json` at the repository root. Re-running a suite on the same day
replaces its section.

## Running

```bash
cargo build --release -p kivo-bench
target/release/kivo-bench machine          # this PC's fingerprint
target/release/kivo-bench list             # the suites
target/release/kivo-bench ipc audio        # run suites (5 runs after 1 warmup by default)
```

Saved results need at least 5 runs; `--no-save` prints only. Run from the repository root so the
reports land here.

| Suite | Needs |
|---|---|
| `ipc`, `audio` | Nothing (a microphone and speakers for `audio`) |
| `idle`, `overlay` | KIVO running (release build: `pnpm --filter kivo-app tauri build --no-bundle`, then `target/release/kivo-app.exe`), the PC left alone; `idle` takes 30 min |
| `stt`, `tts`, `vad`, `wake` | The speech data below |

## Speech data

`pnpm bench:data` downloads and unpacks everything into `%LOCALAPPDATA%\KIVO\bench` (or
`KIVO_BENCH_DATA`), about 2 GB, outside the repository:

| What | Source |
|---|---|
| Moonshine v2 base EN (quantized), Parakeet TDT 0.6B v3 (int8), Whisper turbo | sherpa-onnx `asr-models` release |
| Kokoro-82M int8 EN v0.19 | sherpa-onnx `tts-models` release |
| Keyword spotting, zipformer GigaSpeech 3.3M | sherpa-onnx `kws-models` release |
| Silero VAD v6 | github.com/snakers4/silero-vad (tag v6.0) |
| LibriSpeech test-clean | openslr.org/12 |

The speech suites use sherpa-onnx (Apache-2.0, with ONNX Runtime) in `kivo-bench` only. Its TTS
phonemizes with espeak-ng (GPL-3.0), so it stays out of KIVO's shipped binaries (DECISIONS
"Kokoro phonemizer").
