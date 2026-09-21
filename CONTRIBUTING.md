# Contributing to KIVO

Thanks for your interest! KIVO is in **early development** (Phase 0 → M0). The architecture is
documented, but the code is just starting, so the best contributions right now are feedback on
the specs and help with benchmarks.

## Before you start

- **Plan and decisions:**
  - Read [docs/KIVO_Project_Plan.md](docs/KIVO_Project_Plan.md) (the product);
  - read [docs/architecture/](docs/architecture/ARCHITECTURE.md) (the engineering specs);
  - check [docs/DECISIONS.md](docs/DECISIONS.md) before proposing to change a settled decision,
    and say why it should change.
- **Scope:** open an issue before large changes, so the approach can be agreed first.

## Ground rules

- **Architecture invariants** ([ARCHITECTURE.md §8](docs/architecture/ARCHITECTURE.md)) must hold.
  In particular:
  - nothing may bypass the permission engine;
  - secrets never enter model context or logs;
  - OS-specific code goes behind platform traits.
- **Licenses:**
  - Do not add GPL/AGPL dependencies to the default build, because `cargo deny` will fail.
  - Model weights must have licenses that permit redistribution or download, and attribution
    requirements must be recorded in their model manifest.
- **Privacy:**
  - No telemetry or network calls without an explicit user setting.
  - Test fixtures must not contain real personal data or real voices without consent.

## Development workflow (applies once M0 lands)

- Rust stable, `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo deny check`.
- UI: pnpm, TypeScript strict, ESLint, Prettier, Vitest.
- Branch from `main` and keep PRs focused. Describe **what** changed, **why**, and **how it was
  tested**, and include benchmark numbers for performance-sensitive changes.
- Update the relevant spec or `DECISIONS.md` in the same PR when behavior or a decision changes.

## Computer-control and security testing

Automation tests run only against `testenv/` (dummy apps and pages) or in Windows Sandbox or a VM,
**never against your real desktop by default**. See
[TOOLS_AND_CONTROL.md §10](docs/architecture/TOOLS_AND_CONTROL.md).

## Languages

Help with languages is especially welcome: test recordings, command phrasings and native-speaker
review. See [VOICE.md §9](docs/architecture/VOICE.md). Recordings are contributed only with
explicit consent, and under a license stated in the PR.

## Code of conduct and security

- Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
- Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in public
  issues.

## License

The project license is **not chosen yet** (open source; permissive vs copyleft is being decided).
Until a LICENSE file is added, contributions can't be merged. A contributor agreement or DCO
policy will be set when the license is chosen.
