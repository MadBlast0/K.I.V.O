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
  Dependabot.
- Updates: minisign-signed update manifests plus Authenticode-signed binaries; the updater verifies
  both before running the installer.
