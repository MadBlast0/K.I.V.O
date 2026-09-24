# Versioning, CI and release pipeline

Status: Draft v1, 2026-09-21. Complements [DISTRIBUTION.md](DISTRIBUTION.md). Everything is
built by **GitHub Actions** from version-controlled tags. Nothing is built by hand.

> **Updated 2026-09-21 (owner decision):** workflows run **only when a version tag is pushed**
> (or by hand from the Actions tab), never on pushes, PRs or a schedule. Versions are set by hand
> with `pnpm release:version`, and the owner tags. release-please, the PR-title check and
> Dependabot update PRs were removed. The project starts at `0.0.0`.

## 1. Versioning

- **SemVer:** `MAJOR.MINOR.PATCH`, with pre-releases `-beta.N` and `-exp.N`.
- **Commits:** Conventional Commits (`feat:`, `fix:`, `perf:`, `docs:`, `chore:` …), kept by
  convention (the root `CLAUDE.md`).
- **Releasing:** `pnpm release:version X.Y.Z` sets the version in `Cargo.toml` (workspace),
  `apps/kivo-app/src-tauri/tauri.conf.json` and both `package.json` files, and refreshes
  `Cargo.lock`. The owner then commits, tags `vX.Y.Z` and pushes the tag, which runs the
  workflows.
- **Channels:**

  | Channel | Trigger | Update manifest |
  |---|---|---|
  | Stable | Tag `vX.Y.Z` | `stable/latest.json` |
  | Beta | Tag `vX.Y.Z-beta.N` | `beta/latest.json` |
  | Experimental | Tag `vX.Y.Z-exp.N` | `experimental/latest.json` |

- **Protocols stay versioned independently:** IPC and the config schema carry
  `protocol_version` / `schema_version` (ARCHITECTURE §3 and §5). A release note flags any
  migration.

## 2. Workflows

| Workflow | Runs on | Jobs |
|---|---|---|
| `ci.yml` | Tags `v*` (and by hand) | Rust: fmt, clippy `-D warnings`, test, `cargo deny` (licenses/advisories) on **windows-latest**, plus **ubuntu-latest / macos-latest** for the portable crates. UI: pnpm install, typecheck, lint, Vitest. Generated IPC types up to date. Smoke benchmarks (BENCHMARKS §4) |
| `release.yml` | Tags `v*` | The build matrix below → **draft** GitHub Release → artifacts + `latest.json` → publish after checks |
| `codeql.yml` | Tags `v*` (and by hand) | Security scanning. Dependabot **alerts** stay on in the repository settings; there are no automatic update PRs |

**Release build matrix** (the official `tauri-apps/tauri-action`, which uploads bundles to one
draft release and generates the updater JSON:
[tauri-action](https://github.com/marketplace/actions/tauri-action),
[Tauri GitHub pipeline docs](https://v2.tauri.app/distribute/pipelines/github/)):

| Runner | Target | Artifacts |
|---|---|---|
| `windows-latest` | `x86_64-pc-windows-msvc` | **NSIS `-setup.exe`** (primary), **`.msi`** (IT) |
| `windows-11-arm` | `aarch64-pc-windows-msvc` | NSIS `.exe`, `.msi` (Copilot+ / ARM laptops) |
| `macos-latest` | `universal-apple-darwin` | `.dmg`, `.app.tar.gz` (updater) |
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | **`.AppImage`**, **`.deb`**, **`.rpm`** |
| later | Flatpak (Flathub), `aarch64` Linux | — |

**Per-job steps:**

1. Checkout, then set up the Rust toolchain from `rust-toolchain.toml`, with the target.
2. Restore the cargo and pnpm caches.
3. **Build the sidecars** (`kivo-runtime`, `kivo-infer`) for the target and copy them to
   `src-tauri/binaries/<name>-<target-triple>`, which is Tauri's `externalBin` convention.
4. Run `tauri-action`, with `TAURI_SIGNING_PRIVATE_KEY` + password (the updater minisign key)
   from secrets.
5. **Platform signing**, once available:
   - **Windows:** Authenticode through Azure Artifact Signing or an OV certificate, with `signCommand` in the Tauri config.
   - **macOS:** Developer ID signing plus **notarization**. This needs an Apple Developer account
     at $99/yr; unsigned mac builds show Gatekeeper warnings.
   - **Linux:** optional GPG signatures, and `SHA256SUMS`.
6. Upload a **SBOM** (`cargo cyclonedx` + npm) and `THIRD_PARTY_NOTICES`.

**Checks before publishing a draft release:**

- an install/launch smoke test on clean Windows runners (install silently, start the runtime,
  query its IPC health, uninstall);
- checksums;
- a manual approval environment (`production`) for Stable.

## 3. Platform readiness

- **Owner decision (2026-09-21):** macOS and Linux artifacts are **built in CI but not
  published** until their ports land (ROADMAP post-M9).
  - They are kept as workflow artifacts (14-day retention) for testing only.
  - Releases contain Windows installers only until then.
- **Their CI jobs run from M0**, so portability regressions are caught early.

## 4. Secrets and protections

- **Stored as GitHub Actions secrets** in the `release` environment:
  - `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  - signing credentials
  - an optional R2/S3 key if manifests move off GitHub Releases
- **Release jobs** run only on tags from `main` (enforced through the environment's branch
  policy). Actions are pinned by commit SHA.
- **The branch ruleset** (already applied: no force-push or deletion on `main`). There are no
  required status checks, because CI runs on tags, not on pushes to `main`.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [x] **REL-01** · M0 · Conventional Commits for every commit, kept by convention (§1) → done: rule in the root `CLAUDE.md`; CI enforcement removed with the tag-only workflows (DECISIONS: "GitHub Actions only on version tags") · verified: every commit since `a1fbee5` follows it (`git log --format=%s`)
- [x] **REL-02** · M0 · `pnpm release:version X.Y.Z` sets the version in `Cargo.toml` (workspace), `tauri.conf.json` and both `package.json` files and refreshes `Cargo.lock`; the owner commits and pushes the tag `vX.Y.Z` (§1) → done: `scripts/set-version.mjs`; release-please removed (DECISIONS: "GitHub Actions only on version tags"); versions reset to 0.0.0 · verified: ran it with a test version and back; `git diff` showed exactly the four version lines and the workspace entries in Cargo.lock; a bad version is refused
- [~] **REL-03** · M0 · `ci.yml` on every `v*` tag (and by hand): Rust fmt, clippy `-D warnings`, tests and `cargo deny` on windows-latest, plus ubuntu-latest and macos-latest for the portable crates; UI install, typecheck, lint, Vitest; generated IPC types up to date; smoke benchmarks (§2, §3) → partial: `ci.yml` (tags `v*` + manual): Rust fmt/clippy -D warnings/tests on Windows, portable crates on Ubuntu + macOS, cargo-deny, generated-IPC-types check, IPC smoke benchmark, UI typecheck/lint/format/test/build/audit, ROADMAP sync · verified: the jobs were green on pushes before the tag-only switch · missing: a run on a version tag since the switch
- [-] **REL-04** · M0 · Required status checks added to the `main` ruleset once CI exists; Actions pinned by commit SHA (§4) → dropped: CI runs only on version tags, so there are no checks on `main` to require (DECISIONS: "GitHub Actions only on version tags"); every action stays pinned by commit SHA
- [~] **REL-05** · M1 · `release.yml` on `v*` tags, Windows x64 first: sidecars built and copied to `src-tauri/binaries/<name>-<target-triple>`, `tauri-action` → partial: `.github/workflows/release.yml` (tags `v*` only, actions pinned by SHA): sidecars for `x86_64-pc-windows-msvc`, `tauri-action` build, smoke test, `SHA256SUMS.txt`, then `gh release create --draft` · verified: config compiles; the workflow has not run yet (first tag) · missing: a first tagged run
- [~] **REL-06** · M1 · Install/launch smoke test on a clean Windows runner (silent install, start the runtime, IPC health, uninstall) and checksums before publishing a draft (§2) → partial: `scripts/smoke-install.ps1` installs silently with `/STARTUP`, checks files, shortcut, uninstall entry and startup entry, starts the runtime and checks `kivo-runtime --health` (IPC hello + ping, `kivo_ipc::health`), checks the startup choice was adopted, upgrades in place while it runs, uninstalls and checks nothing is left; `release.yml` runs it before checksums and the draft · verified: `health_reports_a_running_runtime_and_gives_up_on_a_missing_one`; `--health` against a live runtime (exit 0) and none (exit 1) (2026-09-23) · missing: a run on a clean runner (first tag)
- [~] **REL-07** · M9 · Full matrix: Windows ARM64, macOS universal `.dmg` + `.app.tar.gz`, Linux `.AppImage`/`.deb`/`.rpm`; mac/Linux built but kept as 14-day workflow artifacts, not published (§2, §3) → partial: `release.yml` builds Windows x64 and ARM64 (both published), macOS universal and Linux x64 (AppImage/deb/rpm) as 14-day workflow artifacts that are never published and whose failures don't block a release · verified: the workflow (2026-09-24) · missing: the first tagged run of `release.yml` (tags only; the owner releases); the macOS and Linux ports are post-M9, so those builds are expected to fail until then
- [~] **REL-08** · M9 · Experimental channel from `vX.Y.Z-exp.N` tags through `release.yml` (artifacts expire after 14 days) (§2) → partial: `vX.Y.Z-exp.N` tags are the Experimental channel (`channelOf` in `update-manifest.mjs`), published as pre-releases with `experimental.json`, and Experimental releases older than 14 days are deleted on each publish · verified: `update-manifest.test.mjs` (2026-09-24) · missing: the first tagged run of `release.yml` (tags only; the owner releases)
- [~] **REL-09** · M9 · Platform signing steps (Authenticode `signCommand`; later Apple notarization; Linux `SHA256SUMS`/GPG) (§2) → partial: Windows Authenticode through `signCommand` (DIST-11) and SHA256SUMS.txt for every release; Apple notarization and GPG wait for the ports · verified: the workflow (2026-09-24) · missing: the owner's signing credentials and the first tagged run of `release.yml` (tags only; the owner releases)
- [~] **REL-10** · M9 · SBOM (`cargo cyclonedx` + npm) and `THIRD_PARTY_NOTICES` uploaded with each release (§2) → partial: the `sbom` job writes CycloneDX SBOMs for kivo-runtime, kivo-infer and kivo-app (`cargo cyclonedx`) and the app's npm packages (`cdxgen`), plus THIRD_PARTY_NOTICES.txt, all uploaded with the draft release · verified: the workflow (2026-09-24) · missing: the first tagged run of `release.yml` (tags only; the owner releases)
- [~] **REL-11** · M9 · `release` environment secrets (`TAURI_SIGNING_PRIVATE_KEY` + password, signing credentials), release jobs only on tags from `main`, a `production` approval for Stable (§2, §4) → partial: the build jobs read the signing secrets from the `release` environment; a `check` job refuses a tag that isn't on main; the `publish` job waits for the `production` environment's approval for every channel · verified: the workflow (2026-09-24) · missing: the owner creating both environments, their secrets and the reviewers in the repository settings
- [x] **REL-12** · M0 · `codeql.yml` on version tags, and Dependabot alerts (§2) → done: `codeql.yml` (JS/TS, Rust, Actions) on `v*` tags and by hand; Dependabot alerts stay on in the repository; update PRs removed (DECISIONS: "GitHub Actions only on version tags") · verified: CodeQL run succeeded earlier; `dependabot.yml` removed
