# Distribution: packaging, updates, models, signing

Status: Draft v1, 2026-09-21. Implements plan §21, §110–113 and §19–20. Research:
[architecture-and-platform/REPORT.md §5](../research/architecture-and-platform/REPORT.md).

## 1. Installers

| Format | Role | Notes |
|---|---|---|
| **NSIS, per-user** | **Primary download** | No admin prompt, installs to `%LOCALAPPDATA%\Programs\KIVO`, silent in-place upgrades by `tauri-plugin-updater` |
| MSI (per-machine) | Enterprise / IT deployment | Built from the same bundle; updater-aware |
| MSIX / sparse package | **Later spike** | Gives *package identity* for the Windows AI Speech API (NPU STT), richer notifications and Store listing. Via the winapp CLI or `tauri-windows-bundle`. Decide after M3 |

**Amendment to the master plan:** plan §21 and §110 say "MSI". NSIS per-user is now primary,
because it needs no admin rights, suits a per-user runtime, and has the best updater path. MSI
remains for IT.

**Bundle contents:**

- `KIVO.exe` (Tauri);
- `kivo-runtime.exe` and `kivo-infer.exe` (sidecars via `externalBin`);
- the WebView2 bootstrapper (Windows 10);
- sounds, icons and the default intent grammar.

**Target size:** under 40 MB. **No models of any kind ship in the installer** (owner decision,
2026-09-23): not STT, TTS or LLM models, and not the small ones either (Silero VAD, the "Hey Kivo"
wake model, keyword spotting, speaker verification). The user chooses what KIVO downloads, in
onboarding or on the Voice page; a model brings what it needs with it (speech recognition brings
Silero VAD, §4).

**How it is built (M1):**

- `pnpm build` runs `scripts/sidecars.mjs` (release builds of `kivo-runtime` and `kivo-infer`,
  copied to `src-tauri/binaries/<name>-<target-triple>.exe`), then `tauri build` with
  `src-tauri/tauri.bundle.conf.json` merged in. The sidecars and the installer settings live only
  in that file, so `pnpm dev` and `cargo` builds don't need release binaries.
- `mainBinaryName` is `KIVO`, so the app is `KIVO.exe` beside `kivo-runtime.exe` and
  `kivo-infer.exe`. No models are bundled; the smoke test checks there are none.
- KIVO's sounds are synthesized by the runtime and the English grammar is compiled into it, so
  neither is a separate file. The icons come from the Tauri bundle.
- NSIS per user, with the WebView2 bootstrapper downloaded silently when WebView2 is missing
  (Windows 10). Tauri's template adds the Start-menu shortcut, the uninstaller and in-place
  upgrades. KIVO's hooks (`src-tauri/windows/hooks.nsh`) stop the runtime tree before files are
  replaced or removed, and remove the startup entry on a real uninstall (upgrades keep it).
- **Startup.** "Open KIVO when Windows starts" is off by default and asked in onboarding.
  Deployments can pass `/STARTUP` (`KIVO_x.y.z_x64-setup.exe /S /STARTUP`) to register it. On its
  first start the runtime adopts that entry as the setting, so it isn't removed.

## 2. Updates (plan §112–113)

- **The runtime is the updater** (`apps/kivo-runtime/src/updater.rs`, DECISIONS "M9 build"): it
  holds the executables' locks and must wind turns down first, and the webview never runs an
  installer (SECURITY §9). It uses `tauri-plugin-updater`'s formats unchanged: the bundler's
  minisign `.sig` files (`createUpdaterArtifacts`), a `latest.json`-format manifest per channel
  (`stable.json`, `beta.json`, `experimental.json` on the `updates` release, written by
  `scripts/update-manifest.mjs`), and the installers run as that plugin runs them (NSIS
  `/P /R /UPDATE`, MSI `msiexec /i … /passive AUTOLAUNCHAPP=True`). The minisign private key lives
  outside the repo (backed up in a password manager); its public half is compiled in from
  `src-tauri/updater.pub`. A build without it can't update itself.
- **Channels:** Stable, Beta and Experimental, each at its own manifest URL on GitHub Releases. The
  user chooses the channel in Settings.
- **Update flow** (the runtime holds the exe locks):
  1. Download and verify.
  2. Ask, or install at idle if the user opted in.
  3. The runtime finishes or cancels turns and persists state.
  4. The runtime exits.
  5. The installer runs `/UPDATE`.
  6. Relaunch.
  7. Health check.
  8. If the new version fails to start the runtime twice, **roll back**: the previous installer is
     kept for 1 version and reinstalled.
- **Config and database migrations** run forward only, with a backup taken first.

## 3. Code signing

- **Authenticode:**
  - **Azure Artifact Signing** ($9.99/mo) if eligible. Individuals are limited to the US and
    Canada; organizations qualify in more countries.
  - Otherwise, an OV code-signing certificate on a hardware token or cloud HSM.
- **SmartScreen:** neither gives instant trust. Reputation builds across consistently signed
  releases, so publish betas signed.
- **Sign everything:** all three executables, the installers and the native-messaging host.

## 4. Models and optional components (plan §19–22)

**Model manager** (`kivo-store::models`):

- **Manifest per model:** `{ id, version, kind, files[{url, sha256, size}], license, attribution, min_ram, accel, languages }`.
- **Downloads:** resumable (HTTP range), sha256-verified, stored atomically in
  `%LOCALAPPDATA%\KIVO\models`.
- **Only on the user's choice:** nothing is downloaded at startup or in the background unasked.
  A manifest lists the models it `requires`, and those install with it.
- **Sources:** Hugging Face or KIVO's GitHub release mirror.
- **License display:** CC-BY or OpenRAIL models show their license and attribution before
  download, and in About.
- **Removal:** the Voice settings show disk usage and a Remove button.

**Dependency manager** (Node.js for ACP adapters, Git, Ollama, etc.):

1. Detect it.
2. Explain why it's needed.
3. Get consent.
4. Install via **winget** where available, or link to the vendor.
5. Verify, configure and test it.

Nothing is installed silently.

## 5. Crash reporting and telemetry

- **Default: none.** Crash dumps stay local. The diagnostics bundle is created on request, reviewed
  by the user, and shared manually.
- Optional anonymous metrics (latency percentiles, feature usage) are **opt-in** only, and a
  post-beta decision.

## 6. Licensing compliance

- Generated `THIRD_PARTY_NOTICES.txt` is included in the installer and shown in About. KIVO's own
  `pnpm licenses:gen` writes it from the licence and notice files each shipped crate and npm
  package carries (identical texts once), which is what `cargo about` would collect, without
  another tool to install (DECISIONS "M9 build").
- **`cargo deny` rules:** GPL/AGPL are denied in the default dependency graph. The optional GPL
  components (espeak-ng, Piper) ship only as separately downloaded, dynamically loaded
  add-ons, after legal review.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Installers (§1)**

- [~] **DIST-01** · M1 · NSIS per-user installer (no admin, `%LOCALAPPDATA%\Programs\KIVO`) built from the Tauri bundle, with `kivo-runtime.exe` and `kivo-infer.exe` as `externalBin` sidecars (§1) → partial: `pnpm build` = `scripts/sidecars.mjs` (release `kivo-runtime`/`kivo-infer` → `src-tauri/binaries/<name>-<triple>.exe`) + `tauri build --config src-tauri/tauri.bundle.conf.json` (NSIS `installMode: currentUser` → `%LOCALAPPDATA%\Programs\KIVO`, `externalBin` sidecars, `mainBinaryName: KIVO`) · verified: the app compiles with the bundle config merged (tauri-build resolves the sidecars and resources), the runtime and app find each other and the worker beside them (2026-09-23) · missing: the first real installer build, which runs on the first `v*` tag (`release.yml`; DECISIONS "Installer and release at M1")
- [~] **DIST-02** · M1 · Bundle contents: WebView2 bootstrapper (Windows 10), sounds, icons, default intent grammar; no models of any kind: every model is a download the user chooses (§1, DECISIONS "No models in the installer") → partial: the bundle carries the three executables and icons; the WebView2 bootstrapper is downloaded silently when missing; sounds are synthesized by the runtime and the grammar is compiled in; no models at all: Silero VAD is now a model-manager download installed with any speech-recognition model (`requires`), and the runtime downloads nothing unasked (DECISIONS "No models in the installer") · verified: `speech_recognition_brings_the_voice_activity_model`, bundle config compiles (2026-09-23) · missing: contents checked in a built installer (the smoke test asserts no model files; first `v*` tag)
- [~] **DIST-03** · M9 · Installer size under 40 MB, checked in the release pipeline (§1, BENCHMARKS §3) → partial: `release.yml` fails the build when the NSIS or MSI installer is over 40 MB (no models are bundled) · verified: the step reads the bundle folder the smoke test already uses (2026-09-24) · missing: the first tagged run of `release.yml` (tags only; the owner releases)
- [~] **DIST-04** · M9 · MSI per-machine for IT, updater-aware (§1) → partial: the bundle builds the per-machine MSI beside the NSIS installer; the updater recognizes an MSI install (under Program Files), takes the manifest's `windows-<arch>-msi` installer and runs `msiexec /i … /passive /promptrestart AUTOLAUNCHAPP=True` · verified: `the_channel_and_install_kind_choose_the_manifest_and_installer`, `installers_run_as_the_updater_runs_them` (2026-09-24) · missing: an MSI built and installed (the first tagged run of `release.yml` (tags only; the owner releases))
- [~] **DIST-05** · M1 · Installer registers the startup option, Start-menu shortcut and uninstaller; supports in-place upgrades (§1, plan §110) → partial: Tauri's NSIS template adds the Start-menu shortcut, uninstaller and in-place upgrades; `src-tauri/windows/hooks.nsh` stops the runtime tree before files are replaced or removed, registers startup with `/STARTUP` and removes it on a real uninstall (upgrades keep it); the runtime adopts the installer's entry on its first start (`Lifecycle::adopt_installer_startup`) · verified: `the_installers_startup_choice_is_kept_on_the_first_start` (2026-09-23) · missing: `scripts/smoke-install.ps1` run against a built installer (first `v*` tag)
- [ ] **DIST-06** · Post · MSIX / sparse package spike for package identity (Windows AI Speech, richer notifications, Store) (§1)

**Updates (§2)**

- [~] **DIST-07** · M9 · `tauri-plugin-updater` with a minisign-signed `latest.json` per channel (Stable, Beta, Experimental), channel chosen in Settings (§2) → partial: the runtime is the updater (DECISIONS "M9 build"): channels Stable/Beta/Experimental (the installed build's by default, chosen in Settings → About), `<channel>.json` manifests in `tauri-plugin-updater`'s format written by `scripts/update-manifest.mjs` from the bundler's minisign signatures and published on the `updates` release, verified with `minisign-verify` against the key compiled in from `src-tauri/updater.pub` · verified: `an_update_is_found_verified_installed_and_proves_itself` and `a_tampered_or_foreign_installer_is_refused_and_deleted` (real minisign keys and signatures, a local server), `update-manifest.test.mjs`, Updates UI tests (2026-09-24) · missing: the owner's minisign key (`pnpm tauri signer generate`, public half as `updater.pub`, private in the `release` environment) and a published manifest
- [~] **DIST-08** · M9 · Update flow: download + verify → ask (or install at idle if opted in) → runtime finishes/cancels turns and persists state → exits → installer `/UPDATE` → relaunch → health check (§2) → partial: download + verify → a notification (Install now / When idle) or install at idle when chosen (KIVO and the PC idle 10 min, no task running) → a running turn gets 10 s → the pending update is recorded → the installer starts detached (`/P /R /UPDATE`, relaunching KIVO) → the runtime quits → after the restart the runtime marks the update healthy after 15 s and What's new shows · verified: the updater tests with a recording launcher, `kivo_core::update` tests (2026-09-24) · missing: a real update from one released version to the next
- [~] **DIST-09** · M9 · Rollback: if the runtime fails to start twice after an update, reinstall the previous version (kept for one version) (§2) → partial: before installing, the running version's installer is kept (or fetched and verified from its release); the app counts its starts of an unproven update's runtime, retries one that doesn't come up within 20 s, and after two failed starts runs the kept installer and exits; only the current and previous installers stay · verified: `two_failed_starts_roll_back_to_the_previous_installer_once`, `without_a_kept_installer_there_is_nothing_to_roll_back_to`, `an_update_that_never_installed_is_forgotten`, the kept-installer check in the updater test (2026-09-24) · missing: a forced failed update rolled back on a real install
- [x] **DIST-10** · M0 · Config and database migrations run forward only, with a backup first (§2, ARCH-29) → done: config migrations back up to `kivo.toml.vN.bak` first; database upgrades take a `VACUUM INTO` copy (`kivo.db.vN.bak`) first; both forward-only (a newer database is refused, a newer config is left untouched) · verified: tests `older_files_are_backed_up_then_migrated_in_order`, `a_backup_is_a_complete_readable_copy`, `a_database_from_a_newer_kivo_is_not_downgraded`

**Signing (§3)**

- [~] **DIST-11** · M9 · Authenticode signing (Azure Artifact Signing or OV certificate) of all three executables, the installers and the native-messaging host; betas published signed (§3) → partial: `scripts/sign.ps1` signs with Azure Artifact Signing or an OV certificate (SHA-256, timestamped) and is Tauri's `signCommand` for KIVO.exe, both sidecars (kivo-runtime is also the native-messaging host) and the installers when the owner's secrets are set; `release.yml` then checks every file with `Get-AuthenticodeSignature` · verified: the workflow and script read against Tauri's signCommand contract (2026-09-24) · missing: the owner's signing route and credentials, and the first tagged run of `release.yml` (tags only; the owner releases)

**Models and dependencies (§4)**

- [x] **DIST-12** · M1 · Model manager (`kivo-store::models`): per-model manifest, resumable HTTP-range downloads, sha256 verification, atomic install into `%LOCALAPPDATA%\KIVO\models`, sources Hugging Face or KIVO's release mirror (§4) → done: `kivo_store::models`: per-model manifests (Moonshine, Kokoro), resumable HTTP-range downloads, sha256 checks, a retry for a damaged file, files in subfolders, atomic install into `%LOCALAPPDATA%\KIVO\models`; sources pinned Hugging Face revisions and GitHub commits · verified: model-store tests (install, resume, damage, cancel, subfolders), the real Moonshine download (2026-09-23)
- [x] **DIST-13** · M1 · Licenses and attribution (CC-BY, OpenRAIL) shown before download; Voice → done: the Voice page lists every model with its licence, size and whether it is on this PC; Download first shows the licence, attribution and source; Remove confirms and says what changes; progress and residency update by push · verified: `Voice.test.tsx` (licence before download, list), accessibility audit of Voice (2026-09-23)
- [x] **DIST-14** · M8 · Dependency manager (Node.js, Git, Ollama, …): detect → explain → consent → install via winget or vendor link → verify, configure, test; version and health checked; nothing installed silently (§4, plan §20) → done: the dependency manager (`installer.rs`) for Node.js, Git and Ollama: detect → explain → consent per command → winget (or the vendor's page when winget can't) → verify the version → configure PATH; versions and health shown in Agents → "Tools KIVO uses"; nothing installs without the user's OK · verified: installer unit tests, `installing_a_cli_agent_explains_asks_runs_and_verifies`

**Telemetry and compliance (§5–6)**

- [x] **DIST-15** · M1 · No telemetry by default; crash dumps stay local; the diagnostics bundle (generated on request and reviewed before sharing) is ARCH-40 (§5) → done: KIVO sends nothing anywhere by default: the only network use is a model download the user chooses and web pages they open; crash dumps and reports stay in the crashes folder · verified: a search of every network client in the tree (only the model downloader), crash tests (2026-09-23)
- [~] **DIST-16** · M9 · Generated `THIRD_PARTY_NOTICES` (`cargo about` + npm license checker) in the installer and in About (§6) → partial: `pnpm licenses:gen` writes THIRD_PARTY_NOTICES.txt from the licence and notice files each shipped crate and npm package carries (700 packages, identical texts once, 1.3 MB; DECISIONS "M9 build"), bundled as a resource and opened from Settings → About → Licences → Full licence texts (`about.notices`, shipped `.txt`/`.md`/`.pdf` only) · verified: generated on this PC, `m7_settings` opens it, `only_shipped_documents_open` (2026-09-24) · missing: checked inside a built installer (the first tagged run of `release.yml` (tags only; the owner releases))
- [x] **DIST-17** · M0 · `cargo deny` denies GPL/AGPL in the default graph; GPL components only as separately downloaded add-ons after legal review (§6, ARCH-35) → done: `deny.toml` denies GPL/AGPL by allow-list · verified: `cargo deny check licenses` in CI
