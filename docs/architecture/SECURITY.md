# Security model

Status: Draft v1, 2026-09-21. Implements plan §56–61, §69–70, §104–106 and §117. Research:
[architecture-and-platform/REPORT.md §4](../research/architecture-and-platform/REPORT.md),
[speaker enrollment](../research/voice-pipeline-engines/speaker_enrollment_personalization.md).

## 1. Threat model (summary)

| Threat | Examples | Primary controls |
|---|---|---|
| Prompt injection | A web page, email, document or clipboard content says "delete files" or "send this to x@evil" | Provenance/taint + policy at the tool boundary; no instruction-following from untrusted content |
| Model error | The brain hallucinates a destructive command | Risk tiers, confirmation, Recycle Bin, dry-run previews, validation |
| Voice spoofing / bystanders | TV audio, another person, a replayed or cloned voice | Speaker verification (convenience only); High risk needs an on-screen action or Windows Hello |
| Malicious extensions | Hostile MCP server, tool poisoning, malicious plugin | Per-server permissions, untrusted descriptions, change detection, egress flag |
| Local attacker (same user) | Another process talking to KIVO's pipe | Per-user pipe DACL + session token; no remote clients |
| Secret leakage | Keys in prompts, logs, diagnostics | `Secret<T>`, store-by-handle, redaction layer, diagnostics review |
| Supply chain | Compromised crates or npm packages, a malicious update | `cargo deny`/`audit`, lockfiles, pinned CI, signed updates + Authenticode |

**Out of scope:** an attacker with admin or the same user's full code execution (they already own
the account), and physical access.

## 1.1 Permission modes (owner decision, 2026-09-21)

The mode model is borrowed from Claude Code. The user picks **how much KIVO may do without
asking**, and can switch at any time from the Island, the tray, chat, Home, or **Ctrl+Shift+M**.
The Island shows the current mode while KIVO acts (hidden for Auto).

| Mode | Behavior |
|---|---|
| **Ask every time** | Every non-read action asks |
| **Accept edits** | File edits and app/window actions in allowed places run automatically; commands, settings and anything High ask |
| **Plan first** | Read-only. KIVO produces a plan (shown in the Island/card), and nothing changes until the user approves the plan. Approval grants exactly the planned steps |
| **Auto** (default) | Safe/Low/Medium run automatically; **High still asks** |
| **Bypass permissions** | Everything runs without asking, **including High**. Explicit opt-in dialog (optional Windows Hello), **auto-expires** (15 min / 1 h default / until turned off), red **BYPASS** chip in the Island and tray, every action audited. Not available to guest sessions or remote clients |

**Hard limits, enforced in every mode including Bypass** (these are not permissions, and the
brain can never change them):

1. The emergency stop.
2. Capabilities that are turned off ([CAPABILITIES.md](CAPABILITIES.md)).
3. The blocked-apps list.
4. Never typing into password fields.
5. **Destination binding:** no outbound sends to recipients or hosts that came from untrusted
   content ([§4](#4-prompt-injection-defense-camel-lite-v1)).
6. Per-task cost caps.

The **"High always confirms"** rule below applies to every mode except Bypass. Bypass is the
user's explicit, time-limited override, and invariant 4 still holds: *AI output* can never lower
the mode, because only the user can, through UI or hotkey, never through a tool call or voice
alone.

## 2. Permission engine

```text
authorize(ToolCall, Context) -> Decision { Allow | Confirm(ConfirmSpec) | Deny(reason) }
Context = { session: owner|guest, speaker_confidence, profile: MaxSafety|Balanced|PowerUser|Custom,
            turn_taint: Clean|Tainted(sources), privacy_mode, data_classes_in_args, initiated_by: UserDirect|Brain|Task|Mcp,
            task_grants, tool_overrides }
```

> **Note:** the permission *modes* in §1.1 replace the earlier Maximum Safety / Balanced / Power
> User profiles. The mapping: Ask ≈ Maximum Safety, Auto ≈ Balanced (default), Accept edits and
> Plan are new, and Bypass is the explicit override. The table below still describes how taint and
> guest status tighten any mode.

**Default policy (Auto mode):**

| Risk | Clean turn, user-initiated | Tainted turn or brain-initiated | Guest session |
|---|---|---|---|
| Safe | Allow | Allow | Allow |
| Low | Allow | Allow | Confirm |
| Medium | Allow once the user has granted "always for this tool+scope", else Confirm | **Confirm** | Deny |
| High | **Confirm (on-screen click or Windows Hello; voice "yes" not enough)** | **Confirm, strong** + show provenance | Deny |

- **Profiles:** Maximum Safety moves every row down one level of strictness. Power User allows
  Medium by default on clean turns. **High always needs confirmation in every profile.**
- **Grants** are scoped by tool, argument pattern (for example a folder) and duration (once,
  session, 24 h, always). They are stored in `permissions_grants`, and the user can view and
  revoke them.
- **Task grants:** permissions granted when a background task is created. The task cannot exceed
  them.
- **Voice confirmations:** Medium can be approved by voice when speaker recognition says it's
  the owner (or the device user is signed in with recognition off). For High, a voice approval
  triggers Windows Hello (face recognition is hands-free), and voice alone is never enough.
  Guests can't approve, and speech heard during KIVO's own TTS is ignored
  ([CONVERSATION.md §7](CONVERSATION.md)).
- **Confirmation UX:** the card shows the exact action in plain words, the target, why it is
  needed, and its provenance (for example "requested after reading example.com"). Buttons are
  Allow once / Always for… / Deny. Confirmations never auto-dismiss.

## 3. Risk escalation by arguments

- **File operations** on protected paths (Windows, Program Files, user profile root, or other apps'
  config) become High, and bulk operations over 20 files become High.
- **Shell:** see TOOLS_AND_CONTROL.md §7.
- **Network egress** of `personal`/`sensitive` classified data becomes High, and it is blocked
  entirely in Strict Private mode.
- **Any tool whose destination** (email address, URL, recipient) came from untrusted content and
  not from the user's words becomes **Deny**, unless the user explicitly re-states it.

## 4. Prompt-injection defense ("CaMeL-lite", v1)

1. **Provenance on every value:** user speech/typing = `User`; KIVO state = `System`; web pages,
   files, emails, clipboard, OCR, tool outputs and MCP results = `Untrusted`.
2. **Taint:** when untrusted content enters the brain context, the turn becomes `Tainted`, and
   the permission rules above tighten.
3. **Destination binding:** outbound actions (send, post, upload, HTTP to a non-allowlisted host)
   must target destinations present in `User`-provenance text for the task. Otherwise they are
   denied.
4. **Framing:** untrusted content is wrapped in delimited, labelled blocks. The system prompt says
   it is data. This helps a little, but it is **not relied on** for safety.
5. **Minimal tools:** tainted turns get only the tools the task needs (plan §123).
6. **Later hardening (post-beta):** a plan-then-execute mode in which the privileged brain writes a
   plan (a restricted DSL) *before* reading untrusted data, and a quarantined model extracts values
   that cannot change the control flow. This is the full CaMeL approach.

## 5. Secrets (plan §61)

- API keys and tokens live in Windows Credential Manager (`keyring`), config holds only
  `secret://kivo/<provider>/<name>` handles, and the values are loaded only inside the adapter or
  tool executor that needs them.
- The `Secret<String>` newtype has no `Display`, `Debug` (prints `***`) or `Serialize`, and it is
  zeroized on drop.
- Secrets never go into prompts, logs, activity, diagnostics or IPC to the UI. The UI can only
  *set* a secret (write-only) and test it (the runtime performs the test).
- Voice enrollment data is DPAPI-encrypted (CurrentUser scope), with one-click deletion (biometric
  data: GDPR Art. 9, BIPA).
- **Known limitation:** another process running as the same user can read Credential Manager
  entries. This is documented.

## 6. Privacy and data classification (plan §69–70)

- **Data classes:** `public, normal, personal, sensitive, credential, highly_sensitive`.
- **How items are classified:** by source (password fields → credential; documents in folders the
  user marks private → sensitive), plus lightweight local detectors (keys, card numbers, ID
  patterns) and user labels.
- **Modes:** Cloud / Local / Strict Private / Custom. The router and the egress checks enforce the
  mode, and **privacy always overrides failover convenience**.
- **Cloud STT/TTS** also count as egress, so they are subject to the same mode.

## 7. Audit log (plan §106)

- An append-only `audit` table records:
  `ts, turn_id/task_id, tool, args_summary (redacted), risk, decision, confirmed_by (click|hello|grant|policy), result, error`.
- Each row stores `hash = sha256(prev_hash || row)`, and the chain is verified in diagnostics
  (this detects tampering, it does not prevent it).
- The Control Center shows it under Activity → Audit.

## 8. Emergency stop (plan §105)

| Trigger | Default |
|---|---|
| Global hotkey | **Ctrl+Alt+Shift+Esc** (configurable; low-level hook so it works even if RegisterHotKey is taken) |
| Voice | "Kivo stop" / "stop" / "cancel" via the command spotter, no wake word needed while active |
| Tray | "Stop everything" |
| Overlay/Control Center | Stop button |

**Effect:**

1. Cancel all turns and tasks (tokens).
2. Stop TTS immediately.
3. Kill tool Job Objects.
4. Send `session/cancel` to ACP agents.
5. Stop input injection.
6. Background tasks go to Paused (they don't resume automatically).
7. Log an audit entry.

## 9. Hardening checklist (enforced in CI or review)

- Tauri:
  - strict CSP, no remote content in app windows;
  - minimal `capabilities/*.json` per window (the overlay window gets almost nothing);
  - `withGlobalTauri: false`;
  - no `shell` plugin in the UI.
- IPC: user-only pipe DACL, `PIPE_REJECT_REMOTE_CLIENTS`, session token, schema validation, a
  message size limit.
- Native messaging host: the manifest allows only KIVO's extension ID; payload limits.
- Dependencies: `cargo deny` (licenses, advisories, bans), `pnpm audit`, lockfiles committed,
  Dependabot alerts.
- Updates: minisign-signed update manifests plus Authenticode-signed binaries; the updater verifies
  both before running the installer.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Permission modes (§1.1)**

- [x] **SEC-01** · M1 · Permission modes Ask every time and Auto (default) in the engine (§1.1) → done: `PermissionMode` Ask every time and Auto (default), plus Accept edits and Plan first, decided in `kivo_security::authorize` (§1.1 table; DECISIONS "Auto means Medium runs") · verified: `each_mode_asks_for_what_it_promises`, `the_default_policy_table` (2026-09-23)
- [ ] **SEC-02** · M4 · Permission modes Accept edits and Plan first (Plan: read-only, plan shown, approval grants exactly the planned steps) (§1.1)
- [ ] **SEC-03** · M5 · Bypass permissions: explicit opt-in dialog (optional Windows Hello), auto-expiry (15 min / 1 h default / until off), red BYPASS chip in the Island and tray, every action audited, unavailable to guests and remote clients (§1.1)
- [x] **SEC-04** · M1 · Switching mode only through UI or hotkey (Ctrl+Shift+M), from the Island, tray, chat and Home; never through a tool call or voice alone; the Island shows the mode while acting (hidden for Auto) (§1.1) → done: the mode lives in the runtime and kivo.toml; the user switches it from Home, the tray submenu, the command palette and Ctrl+Shift+M (with an Island notice); the Island shows the mode chip while KIVO acts (hidden in Auto) and a click opens Home to switch it (the overlay itself can't switch); no tool or voice path can change it; Bypass needs its confirmation step (SEC-03) · verified: core, RPC, tray and hotkey tests, `shows the permission mode, except in Auto (SEC-04)` (2026-09-23); the Chat page's mode picker comes with Chat (UX-21, M3)
- [x] **SEC-05** · M1 · Hard limits enforced in every mode: emergency stop, disabled capabilities, blocked apps, no typing into password fields, destination binding, per-task cost caps (§1.1) → done: checked first in `authorize`, in every mode including Bypass: the emergency stop, disabled capabilities, blocked apps (by id or name) and destination binding (an address from untrusted content is refused); no M1 tool types into fields or spends money, so the password-field check lands with typing (TOOL-33, M4) and cost caps with paid work (tasks, cloud brains) · verified: `hard_limits_hold_in_every_mode_including_bypass` (2026-09-23)

**Permission engine (§2–3)**

- [x] **SEC-06** · M1 · `authorize(ToolCall, Context) -> Decision { Allow | Confirm(ConfirmSpec) | Deny(reason) }` with the §2 `Context`, in `kivo-security` (§2) → done: `kivo_security::authorize(&ToolSpec, &ToolCall, &Context) -> Decision { Allow(Permit) | Confirm(ConfirmSpec) | Deny(Denial) }` with the §2 context (mode, session kind, taint, capabilities, grants, hard limits) · verified: the policy tests, and every call in the engine goes through it (end-to-end tests) (2026-09-23)
- [x] **SEC-07** · M1 · Default policy table (risk × clean/tainted/guest) implemented and unit-tested; High always confirms except in Bypass (§2) → done: the risk × clean/tainted/guest table, with High always confirming except in Bypass and grants never lifting High · verified: `the_default_policy_table`, `grants_lift_medium_asks_but_never_high`, `a_spoken_yes_is_not_enough_for_high_risk` (2026-09-23)
- [ ] **SEC-08** · M4 · Grants scoped by tool, argument pattern and duration (once / session / 24 h / always) in `permissions_grants`; the user can view and revoke them (§2)
- [ ] **SEC-09** · M5 · Task grants: a background task cannot exceed the permissions granted at creation (§2)
- [x] **SEC-10** · M1 · Confirmation card: exact action in plain words, target, why, provenance; Allow once / Always for… / Deny; never auto-dismisses (§2) → done: the Island's confirmation card shows the exact action, the target when the action doesn't name it, why and who asked; Allow once / Always allow for <target> / Deny; it stays until answered (a waiting turn never collapses) and answers go through `confirmed` · verified: `asks with the action, why and who asked, and never goes away by itself (SEC-10)`, policy confirmation tests (2026-09-23)
- [ ] **SEC-11** · M4 · Windows Hello confirmation for High risk (§2, CONVERSATION §7)
- [ ] **SEC-12** · M4 · Risk escalation by arguments: protected paths and bulk (> 20 files) become High; egress of personal/sensitive data becomes High (blocked in Strict Private); untrusted-origin destinations become Deny unless the user restates them (§3)

**Prompt injection (§4)**

- [ ] **SEC-13** · M4 · Provenance on every value (User / System / Untrusted) and turn taint when untrusted content enters the context (§4)
- [ ] **SEC-14** · M4 · Destination binding: outbound actions only to destinations present in `User`-provenance text for the task (§4)
- [ ] **SEC-15** · M4 · Untrusted content wrapped in delimited, labelled blocks; tainted turns get the minimal tool set (§4)
- [ ] **SEC-16** · Post · Full CaMeL plan-then-execute mode with a quarantined extractor (§4)

**Secrets (§5)**

- [x] **SEC-17** · M3 · Secrets in Credential Manager (`keyring`) referenced as `secret://kivo/<provider>/<name>`; loaded only inside the adapter or tool executor (§5) → done: `WindowsSecrets` on Credential Manager (`kivo/<provider>/<name>`), referenced as `secret://kivo/…` in config, read only inside the brain adapter · verified: `keys_round_trip_through_credential_manager`, brains tests (2026-09-23)
- [x] **SEC-18** · M0 · `Secret<String>` newtype: no `Display`, `Debug` prints `***`, no `Serialize`, zeroized on drop (§5) → done: `crates/kivo-core/src/secret.rs` · verified: tests for Debug `***`, expose, zeroize; no Display/Serialize impls
- [x] **SEC-19** · M3 · Secrets never reach prompts, logs, activity, diagnostics or IPC to the UI; the UI can only set (write-only) and ask the runtime to test (§5) → done: keys never enter prompts, logs, Activity or IPC; the UI can only set and ask for a test · verified: brains view test (no key material), `the_control_centers_brain_requests` (not in list or config), `Brains.test.tsx` (write-only field) (2026-09-23)

**Privacy (§6)**

- [ ] **SEC-20** · M7 · Data classes `public … highly_sensitive`, classified by source, local detectors (keys, card numbers, ID patterns) and user labels (§6)
- [ ] **SEC-21** · M7 · Privacy modes Cloud / Local / Strict Private / Custom enforced in the router and egress checks (cloud STT/TTS included); privacy overrides failover (§6)

**Audit (§7)**

- [x] **SEC-22** · M1 · Append-only `audit` table with the §7 fields and `hash = sha256(prev_hash || row)` (§7) → done: the append-only `audit` table (migration 3, triggers refuse UPDATE/DELETE) with the §7 fields and `hash = sha256(prev_hash || row)`; `verify_audit` finds the first broken row · verified: `audit_rows_chain_and_the_chain_verifies`, `the_audit_log_refuses_edits_and_detects_tampering`, the end-to-end test verifies the chain (2026-09-23)
- [ ] **SEC-23** · M8 · Chain verification in diagnostics (§7)
- [ ] **SEC-24** · M7 · Activity → Audit view in the Control Center (§7)

**Emergency stop (§8)**

- [x] **SEC-25** · M1 · Emergency stop from the Ctrl+Alt+Shift+Esc hotkey (low-level hook fallback), tray "Stop everything" and the overlay/Control Center Stop button (§8) → done: the emergency stop (`Engine::stop_everything`: the turn, the voice, and tasks once they exist) from Ctrl+Alt+Shift+Esc (with the keyboard-hook fallback when another app owns it), the tray's Stop everything, Home's Stop everything button and the command palette; the Island's Stop cancels the current request · verified: core, tray and hotkey tests, `cancelling_stops_every_layer_within_100_ms` (emergency stop < 1 ms), the RPC stop test (2026-09-23)
- [x] **SEC-26** · M2 · Voice trigger via the command spotter (§8, VOICE-19) → done: the command spotter's stop words cancel a busy turn without the wake word (VOICE-19), and "stop" as a whole request ends the turn · verified: `saying_stop_while_kivo_talks_stops_it` (2026-09-23)
- [ ] **SEC-27** · M4 · Full effect: cancel all turns and tasks, stop TTS, kill tool Job Objects, `session/cancel` to ACP agents, stop input injection, pause background tasks, audit entry (§8)

**Hardening (§9)**

- [x] **SEC-28** · M0 · Tauri: strict CSP, no remote content, `withGlobalTauri: false`, no `shell` plugin, minimal `capabilities/*.json` per window (the overlay window gets almost nothing) (§9) → done: strict CSP, `withGlobalTauri` off, no shell plugin; the app manifest (`build.rs`) exposes only `ui_ready`, `runtime_request`, `runtime_start`, `overlay_fit`; `capabilities/default.json` (main: core + window controls + three app commands) and `capabilities/overlay.json` (overlay: listen/unlisten, `ui_ready`, `overlay_fit` only) · verified: both windows work with exactly these grants (release build, 2026-09-21)
- [x] **SEC-29** · M0 · IPC hardening: user-only DACL, reject remote clients, session token, schema validation, message size limit (§9, ARCH-14–16) → done: user-only DACL, PIPE_REJECT_REMOTE_CLIENTS, first-instance pipes, session token, strict schema parsing (unknown fields and malformed messages close the connection), 1 MiB frame limit · verified: 20 IPC tests over the real transport, stable over 20 runs
- [ ] **SEC-30** · M4 · Native messaging host manifest allows only KIVO's extension ID; payload limits (§9)
- [x] **SEC-31** · M0 · Dependencies: `cargo deny`, `pnpm audit`, lockfiles committed, Dependabot alerts (§9) → done: `cargo deny` and `pnpm audit --audit-level high` in CI (on version tags), `Cargo.lock` + `pnpm-lock.yaml` committed and used with --locked/--frozen-lockfile, Dependabot alerts · verified: CI green; `cargo deny check` and `pnpm audit` also run locally
- [ ] **SEC-32** · M9 · Updates: minisign-signed manifests + Authenticode binaries, both verified before running the installer (§9)
