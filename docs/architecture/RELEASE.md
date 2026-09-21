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
- **The branch ruleset** (already applied: no force-push or deletion on `main`) gains required
  status checks once CI exists.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [~] **REL-01** · M0 · Conventional Commits, enforced by `pr-title.yml` (§1, §2) → partial: Conventional Commits enforced on pushed commits by the `commit-messages` job in `ci.yml` and on PR titles by `pr-title.yml` · missing: the PR-title check has not run yet (no PR opened)
- [~] **REL-02** · M0 · `release-please` Release PR bumping `Cargo.toml` (workspace), `tauri.conf.json` and `package.json`, writing `CHANGELOG.md`; merging tags `vX.Y.Z` (§1) → partial: `release-please.yml` + config (vX.Y.Z tags, bumps Cargo.toml / tauri.conf.json / both package.json, refreshes Cargo.lock on the release branch); first run succeeded · missing: a Release PR (needs the first feat/fix commit; earlier commits were not Conventional)
- [~] **REL-03** · M0 · `ci.yml` on every PR and push to `main`: Rust fmt, clippy `-D warnings`, tests and `cargo deny` on windows-latest, plus ubuntu-latest and macos-latest for the portable crates; UI install, typecheck, lint, Vitest; generated IPC types up to date; smoke benchmarks (§2, §3) → partial: `ci.yml` jobs green: Rust fmt/clippy -D warnings/test on Windows, portable crates on Ubuntu + macOS, cargo-deny, UI typecheck/lint/format/test/build/audit, ROADMAP sync check · missing: generated-IPC-types check (ARCH-18) and smoke benchmarks (BENCH-13), added with those items
- [ ] **REL-04** · M0 · Required status checks added to the `main` ruleset once CI exists; Actions pinned by commit SHA (§4)
- [ ] **REL-05** · M1 · `release.yml` on `v*` tags, Windows x64 first: sidecars built and copied to `src-tauri/binaries/<name>-<target-triple>`, `tauri-action` → draft GitHub Release with NSIS `.exe`, `.msi` and `latest.json` (§2)
- [ ] **REL-06** · M1 · Install/launch smoke test on a clean Windows runner (silent install, start the runtime, IPC health, uninstall) and checksums before publishing a draft (§2)
- [ ] **REL-07** · M9 · Full matrix: Windows ARM64, macOS universal `.dmg` + `.app.tar.gz`, Linux `.AppImage`/`.deb`/`.rpm`; mac/Linux built but kept as 14-day workflow artifacts, not published (§2, §3)
- [ ] **REL-08** · M9 · `nightly.yml` for the Experimental channel (artifacts expire after 14 days) (§2)
- [ ] **REL-09** · M9 · Platform signing steps (Authenticode `signCommand`; later Apple notarization; Linux `SHA256SUMS`/GPG) (§2)
- [ ] **REL-10** · M9 · SBOM (`cargo cyclonedx` + npm) and `THIRD_PARTY_NOTICES` uploaded with each release (§2)
- [ ] **REL-11** · M9 · `release` environment secrets (`TAURI_SIGNING_PRIVATE_KEY` + password, signing credentials), release jobs only on tags from `main`, a `production` approval for Stable (§2, §4)
- [x] **REL-12** · M0 · `codeql.yml` and Dependabot version updates (§2) → done: `codeql.yml` (JS/TS, Rust, Actions), `.github/dependabot.yml` (cargo, npm, actions; grouped) · verified: CodeQL run succeeded, Dependabot update runs started
