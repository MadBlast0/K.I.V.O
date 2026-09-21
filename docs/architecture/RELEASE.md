# Versioning, CI and release pipeline

Status: Draft v1, 2026-09-21. Complements [DISTRIBUTION.md](DISTRIBUTION.md). Everything is
built by **GitHub Actions** from version-controlled tags. Nothing is built by hand.

## 1. Versioning

- **SemVer:** `MAJOR.MINOR.PATCH`, with pre-releases `-beta.N` and `-exp.N`.
- **Commits:** Conventional Commits (`feat:`, `fix:`, `perf:`, `docs:`, `chore:` …), enforced by
  a PR title check.
- **release-please** maintains a "Release PR" that bumps the versions in `Cargo.toml` (workspace),
  `apps/kivo-app/src-tauri/tauri.conf.json` and `package.json`, and writes `CHANGELOG.md`.
  Merging it creates the tag `vX.Y.Z`.
- **Channels:**

  | Channel | Trigger | Update manifest |
  |---|---|---|
  | Stable | Tag `vX.Y.Z` | `stable/latest.json` |
  | Beta | Tag `vX.Y.Z-beta.N` | `beta/latest.json` |
  | Experimental | Nightly from `main` (scheduled) | `experimental/latest.json` |

- **Protocols stay versioned independently:** IPC and the config schema carry
  `protocol_version` / `schema_version` (ARCHITECTURE §3 and §5). A release note flags any
  migration.

## 2. Workflows

| Workflow | Runs on | Jobs |
|---|---|---|
| `ci.yml` | Every PR and push to `main` | Rust: fmt, clippy `-D warnings`, test, `cargo deny` (licenses/advisories) on **windows-latest**, plus **ubuntu-latest / macos-latest** for the portable crates. UI: pnpm install, typecheck, lint, Vitest. Generated IPC types up to date. Smoke benchmarks (BENCHMARKS §4) |
| `pr-title.yml` | PRs | Conventional Commit title check |
| `release-please.yml` | Push to `main` | Maintains the Release PR |
| `release.yml` | Tags `v*` | The build matrix below → **draft** GitHub Release → artifacts + `latest.json` → publish after checks |
| `nightly.yml` | Schedule | Same as release, for the Experimental channel only; artifacts expire after 14 days |
| `codeql.yml` / Dependabot | Schedule and PRs | Security scanning, dependency updates |

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

- **Before the macOS/Linux ports** (ROADMAP post-M9), mac and Linux artifacts are published as
  **"Preview: limited features"** builds. The UI runs, and features without a platform
  implementation show "Not available on this OS yet".
- **Their CI jobs still run from M0**, so portability regressions are caught early.

## 4. Secrets and protections

- **Stored as GitHub Actions secrets** in the `release` environment:
  - `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
  - signing credentials
  - an optional R2/S3 key if manifests move off GitHub Releases
- **Release jobs** run only on tags from `main` (enforced through the environment's branch
  policy). Actions are pinned by commit SHA.
- **The branch ruleset** (already applied: no force-push or deletion on `main`) gains required
  status checks once CI exists.
