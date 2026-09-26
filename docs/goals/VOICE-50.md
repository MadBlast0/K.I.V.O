# VOICE-50 goal prompt

The goal the owner ran on 2026-09-26 to build VOICE-50 (built in commit 228c5f1).

```text
Goal: finish everything buildable in docs/architecture/VOICE.md for KIVO.

Read first, in full: CLAUDE.md, docs/README.md, docs/architecture/VOICE.md (esp. §8, §11, §12 and the Build checklist), docs/research/gpu-speech/REPORT.md, and the 2026-09-24/25 rows of docs/DECISIONS.md. Read your memory notes too.

Scope: VOICE-50, the only open item for this milestone (DirectML is already removed, commit b2af4db):
1. Graphics backend setting: Settings > Performance > "Graphics backend" = Automatic (default) / CUDA / Vulkan / Metal / Processor only, showing only what this PC has. Automatic: NVIDIA -> CUDA once its runtime is installed (Vulkan until then), AMD/Intel -> Vulkan, Mac -> Metal, no usable GPU -> Processor. The "Speech recognition on the graphics card" switch stays the master on/off. Config in kivo-core, IPC + regenerated TS types, en.json strings, UI tests.
2. Fix the wrong-GPU bug: the runtime picks a DXGI adapter index but whisper.cpp only reads "use a GPU" and takes Vulkan device 0. Replace ModelLoad.gpu / Engines.gpu with the backend plus that backend's device, matched by name or LUID; whisper.cpp must run on the card the policy chose.
3. CUDA: an optional kivo-infer-cuda worker (whisper-rs `cuda` feature) and an on-demand "GPU acceleration for NVIDIA" download of the CUDA runtime DLLs (cudart, cuBLAS) through the model manager (pinned URL + sha256, licence shown first), never in the installer; release.yml builds it. CUDA Toolkit 13.4 is installed on the owner's PC (RTX 3060 Laptop, 6 GB, driver 616.92): build and test CUDA on it for real. Check the minimum NVIDIA driver the shipped runtime needs and fall back to Vulkan below it.
4. Metal: macOS builds use whisper.cpp with Metal behind cfg(target_os = "macos"); it cannot be tested here, so say so and mark accordingly.
5. Accel gains Metal; registry::gpu_first and the recommender treat any GPU backend (CUDA, Vulkan, Metal) as the GPU; Voice > Advanced shows device names with proper labels, not raw ids.
6. Supertonic: interruptible runs (kivo_voice::interrupt, as Kokoro has) so a cancel stops it within 100 ms, not 3.6 s; add a test.
Do NOT build GPU voices (step 4 of the report): no Windows GPU voice runtime exists yet; leave it documented. VOICE-12 (post-1.0), VOICE-38/39 (language milestones L1/L2) are out of scope unless the owner says otherwise.

Rules (owner):
- No benchmarks, no release or installer builds, no subagents.
- Disk: check `df -h /c` and `du -sh target` before every build/test. Never run `cargo test --workspace`. Test only the crates you changed (`cargo test -p <crate> --lib`, or one `--test <file>`). Real-model tests only for engines you changed, one engine at a time, with a timeout. Never rebuild on top of a failed build without cleaning. Delete `target/` at the end.
- Build `kivo-infer` before runtime tests that use the worker.
- Keep `cargo clippy --workspace --all-targets` at zero warnings; `cargo fmt --all --check`, `pnpm typecheck`, `pnpm lint`, `pnpm format:check`, and `pnpm test` for UI changes.
- Keep every OS call behind the platform traits. Follow VOICE §12: no new per-model adapters.
- Remove what becomes dead (no leftover code, settings, strings or docs) and search for each removed name.
- Mark VOICE-50 in the checklist with a -> note (what exists, how verified). Never [x] for anything untested or stubbed. Update VOICE.md, the GPU report and DECISIONS.md in the same commit when the build differs from the plan.
- Commit with Conventional Commits under the repo's git identity (MadBlast0), no AI co-author or generated-with lines. Push only when the owner asks.
- Ask the owner when a decision is genuinely theirs; otherwise pick the recommended option and say so.

Done when: all six parts are built and verified (or honestly marked partial with the reason), checks are green, docs are updated, the work is committed, target/ is deleted, and the owner has a short report: built, verified, remaining.
```
