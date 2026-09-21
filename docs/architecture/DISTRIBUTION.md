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
- sounds, icons, the default intent grammar, the "Hey Kivo" wake model, and Silero VAD.

**Target size:** under 40 MB. There are **no STT, TTS or LLM models** in the installer.

## 2. Updates (plan §112–113)

- The updater is `tauri-plugin-updater`. The manifest (`latest.json`) is signed with KIVO's
  minisign key (the private key lives outside the repo, backed up in a password manager).
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

- Generated `THIRD_PARTY_NOTICES` (via `cargo about` and a license checker for npm) are included in
  the installer and shown in About.
- **`cargo deny` rules:** GPL/AGPL are denied in the default dependency graph. The optional GPL
  components (espeak-ng, Piper) ship only as separately downloaded, dynamically loaded
  add-ons, after legal review.
