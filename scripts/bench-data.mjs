#!/usr/bin/env node
// Downloads and unpacks the models and test audio the kivo-bench speech suites use
// (docs/benchmarks/README.md). Everything goes to %LOCALAPPDATA%\KIVO\bench (or $KIVO_BENCH_DATA),
// outside the repository. Files already present are kept.
//
//   pnpm bench:data

import { spawnSync } from "node:child_process";
import { copyFileSync, createWriteStream, existsSync, mkdirSync, renameSync, statSync } from "node:fs";
import { join } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const root =
  process.env.KIVO_BENCH_DATA ??
  join(process.env.LOCALAPPDATA ?? join(process.env.HOME ?? ".", ".local", "share"), "KIVO", "bench");
const downloads = join(root, "downloads");
const models = join(root, "models");
const data = join(root, "data");

const EDACC =
  "https://huggingface.co/datasets/edinburghcstr/edacc/resolve/d9ae7bd344f0562b766ec93ee5ce8f2f9568ce66";
const WHISPER_CPP =
  "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1";
const SHERPA = "https://github.com/k2-fsa/sherpa-onnx/releases/download";
/** [url, where it is unpacked (or copied, for a single file)] */
const FILES = [
  [`${SHERPA}/asr-models/sherpa-onnx-moonshine-base-en-quantized-2026-02-27.tar.bz2`, models],
  [`${SHERPA}/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2`, models],
  [`${SHERPA}/asr-models/sherpa-onnx-whisper-turbo.tar.bz2`, models],
  [`${SHERPA}/tts-models/kokoro-int8-en-v0_19.tar.bz2`, models],
  [`${SHERPA}/kws-models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01.tar.bz2`, models],
  ["https://github.com/snakers4/silero-vad/raw/v6.0/src/silero_vad/data/silero_vad.onnx", models],
  ["https://www.openslr.org/resources/12/test-clean.tar.gz", data],
  // More read speech for the wake suite's negatives (10.7 h with test-clean).
  ["https://www.openslr.org/resources/12/test-other.tar.gz", data],
  // Accented English (BENCH-02): one test shard of EdAcc, the University of Edinburgh's corpus of
  // conversations between speakers of many accents (CC BY-SA 4.0), at a pinned revision.
  [`${EDACC}/data/test-00004-of-00010-806407c9bc68112a.parquet`, join(data, "edacc")],
  // whisper.cpp's models for the GPU engine (the revision KIVO's model manager pins).
  ...["ggml-small-q8_0.bin", "ggml-large-v3-turbo-q5_0.bin", "ggml-base.en.bin"].map((f) => [
    `${WHISPER_CPP}/${f}`,
    join(models, "whisper-cpp"),
  ]),
];

for (const dir of [downloads, models, data]) mkdirSync(dir, { recursive: true });

for (const [url, target] of FILES) {
  const name = url.split("/").pop();
  const file = join(downloads, name);
  if (!existsSync(file) || statSync(file).size === 0) {
    process.stdout.write(`downloading ${name}… `);
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
    await pipeline(Readable.fromWeb(response.body), createWriteStream(`${file}.part`));
    renameSync(`${file}.part`, file);
    console.log(`${Math.round(statSync(file).size / 1048576)} MB`);
  } else {
    console.log(`have ${name}`);
  }
  if (/\.(onnx|parquet|bin)$/.test(name)) {
    mkdirSync(target, { recursive: true });
    const dest = join(target, name);
    if (!existsSync(dest)) copyFileSync(file, dest);
    continue;
  }
  // Model archives unpack to a folder named after them; LibriSpeech to "LibriSpeech".
  const unpacked = name.startsWith("test-")
    ? join(target, "LibriSpeech", name.replace(/\.tar\.gz$/, ""))
    : join(target, name.replace(/\.tar\.bz2$/, ""));
  if (existsSync(unpacked)) continue;
  process.stdout.write(`unpacking ${name}… `);
  // Windows' own bsdtar: a GNU tar earlier on PATH (Git's) reads "C:" as a remote host.
  const tar = process.platform === "win32" ? join(process.env.SystemRoot ?? "C:\\Windows", "System32", "tar.exe") : "tar";
  const result = spawnSync(tar, ["-xf", file, "-C", target], { stdio: "inherit" });
  if (result.status !== 0) throw new Error(`tar failed for ${name}`);
  console.log("done");
}
console.log(`\nBenchmark data is in ${root}`);
