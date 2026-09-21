# Tools and computer control

Status: Draft v1, 2026-09-21. Implements plan §44–59, §67–68, §116 and §120–123. Research:
[architecture-and-platform/REPORT.md §3](../research/architecture-and-platform/REPORT.md).

## 1. Tool schema (plan §122)

```text
ToolSpec {
  id: "windows.focus_window",   // namespaced: system.*, apps.*, windows.*, files.*, media.*, uia.*, browser.*, vision.*, input.*, shell.*, mcp.<server>.*
  description (short, model-facing), params: JSON Schema, result: JSON Schema,
  risk: Safe | Low | Medium | High,          // plan §56; may be raised dynamically by args
  side_effects: [None | LocalRead | LocalWrite | Destructive | ExternalComms | Financial | SecuritySensitive],
  data_egress: bool,                          // sends data off-device
  timeout_ms, cancellable: bool,
  capability_tier: Native | OsApi | AppCli | Uia | BrowserDom | A11y | Vision | Input,   // plan §45
  platforms: [Windows, …]
}
ToolResult { status: Ok | Err(kind, user_message), data, provenance, duration }
```

- **Tool calls:** every call is `authorize()` → `execute(cancel)` → `ToolResult`. The executor
  API requires the `Decision` produced by the permission engine, so nothing can run without one.
- **Error messages:** errors carry a **user-readable message** (plan §146) and a machine code.
  Raw HRESULTs are logged, never spoken.

## 2. Tool router and capability ladder

- **Resolution:** for a capability ("click the Export button in Photoshop"), the router asks the
  **App Capability Registry** (plan §120) what the app supports, then tries the tiers in order:
  Native/OS API → App CLI → UIA → Browser DOM → A11y → Vision → Input.
- **Overrides:** a registry entry may declare that a specific method is more reliable for that app.
- **Registry format:** data files (`apps/*.toml`) with the executable/AUMID match, launch method,
  CLI verbs, UIA hints (AutomationIds), and known quirks. The core registry ships with KIVO, and
  users or plugins can add entries.

## 3. Native Windows tools (v1)

| Area | Tools | Implementation |
|---|---|---|
| Apps | launch, close, focus, restart, list installed | Start-menu `.lnk` + `shell:AppsFolder` index (UWP via AUMID + `IApplicationActivationManager`); fuzzy match with aliases; ShellExecute |
| Windows | list, focus, minimize, maximize, close, move to monitor, snap | EnumWindows + DWM cloaking check. Focus uses UIA `SetFocus` / `AllowSetForegroundWindow` handling (foreground lock rules) |
| Audio | volume get/set, mute, mic mute, output device switch | `IAudioEndpointVolume`, `IMMDeviceEnumerator` (device switch via documented APIs only) |
| Media | play/pause/next/prev, now-playing | `GlobalSystemMediaTransportControlsSessionManager` (WinRT) |
| System | lock, sleep, restart*, shutdown*, brightness (where supported), battery, Focus/DND state | *High-risk, confirmation required |
| Screen | screenshot (screen/window/region), OCR | Windows.Graphics.Capture; Windows.Media.Ocr (offline, free) |
| Files | search, open, reveal, create, rename, move, copy, delete→Recycle Bin | Windows Search index (`SystemIndex`) + fallback walk; **delete always goes to the Recycle Bin**; path-traversal and protected-path guard |
| Clipboard | read, write | Reads are untrusted content (taint) |
| Notifications | show toast | tauri-plugin-notification via the UI, or WinRT directly |

## 4. UI Automation (plan §47–48)

- **Threading:** a dedicated UIA thread (COM MTA) in the runtime. The `uiautomation` crate is the
  first choice (license to verify in M0), and raw `windows-rs` covers anything it lacks.
- **Tools:**
  - `uia.find` (by name/role/AutomationId with fuzzy name), `uia.get_tree` (depth-limited and
    pruned to be compact enough for a model);
  - `uia.invoke`, `uia.set_value`, `uia.toggle`, `uia.select`, `uia.expand`, `uia.scroll_into_view`;
  - `uia.get_bounds` (for companion pointing, plan §141).
- **Events:** FocusChanged, StructureChanged (scoped), WindowOpened/Closed and PropertyChanged,
  subscribed on demand and fed into the event bus. There is no polling.
- **Limits:**
  - UIPI blocks automating elevated windows from a non-elevated KIVO. KIVO reports this clearly
    and never auto-elevates.
  - Custom-drawn apps (games, some Electron/Qt apps) fall back to Vision/Input.

## 5. Browser (plan §49–50)

| Layer | Use | Implementation |
|---|---|---|
| 1. URL/launch | Open URL, search | ShellExecute with the default browser |
| 2. **KIVO browser extension** | The user's real, logged-in browser: tabs, URL/title, selection, readable DOM excerpt, click/type on page elements | Chromium MV3 extension (Chrome, Edge, Brave) + **native messaging host** (`kivo-runtime --native-messaging`). Firefox later |
| 3. CDP automation | Autonomous or background browsing in a **KIVO-managed profile** | `chromiumoxide`. Chrome 136+ blocks CDP on the default profile, so this never touches the user's main profile |
| 4. UIA on browser window | Fallback when the extension is absent | UIA |

- **Untrusted content:** all page content is `Untrusted` provenance, and page text is never
  treated as instructions ([SECURITY.md](SECURITY.md)).
- **Excerpts:** the extension returns structured excerpts (readability extraction, capped at
  roughly 4k tokens), not whole pages (plan §50).

## 6. Background tasks and watchers (plan §67–68)

| Watcher | Source (event-driven) |
|---|---|
| Download finished | Filesystem notifications on the download dirs + browser extension download events |
| Folder changed | `ReadDirectoryChangesW` (the `notify` crate) |
| Process exited / build finished | Process handle wait / Job Object notifications; terminal output via a KIVO-launched shell |
| Window/app state | UIA / WinEvent hooks |
| Time/reminders | Scheduler (tokio timers, persisted; missed runs are reported on the next start) |

- Every task has an owner, permissions (granted at creation), history, status and cancellation.
- Watchers use **no LLM while waiting**. The brain is invoked only when the event fires, and only
  if the task needs reasoning.

## 7. Shell (plan §59)

- `shell.run { command, shell: pwsh|cmd, cwd, timeout }` runs in a Job Object, captures output, is
  cancellable, and is logged.
- **Risk classification:** a parser plus rules. Read-only commands (`Get-*`, `dir`, `git status`)
  are Low. Anything that writes, deletes, touches the network or the registry, or uses
  `Invoke-Expression`, encoded commands or elevation is Medium/High. An unparseable command
  defaults to High.
- Commands never run elevated. Secrets are injected only as environment variables, per call, by
  handle.

## 8. Vision and input (plan §51–52)

- **Vision** runs on request only: capture a region or window → OCR (local) or a vision-capable
  brain (subject to the privacy class). There is no continuous capture.
- **Input** (SendInput) is the last tier.
  - Before clicking, KIVO re-validates that the target window is foreground and its bounds are
    unchanged.
  - Typing into password fields is **blocked** (UIA `IsPassword`).
  - Every input action is at least Medium risk unless the user initiated it directly.

## 9. MCP (plan §53–55)

- **Client:** `rmcp` with stdio and Streamable HTTP transports, managed in the Control Center (list,
  status, tools, permissions, logs, health).
- **Tool mapping:** MCP tools become `mcp.<server>.<tool>` ToolSpecs.
  - The **default risk is Medium** unless the user lowers it per tool.
  - `data_egress: true` for remote servers.
  - The server's own descriptions are treated as untrusted text (tool-poisoning defense): they are
    shown to the user on install, and changes are flagged.
- **KIVO MCP server:** exposes selected KIVO tools to CLI agents, with all calls going through the
  permission engine.

## 10. Safe test environment (plan §116)

- `testenv/` contains:
  - a Win32/WinUI **dummy app** with known AutomationIds (buttons, text fields, lists, dialogs, a
    fake "Export" button);
  - local HTML pages (forms, prompt-injection pages, a fake login);
  - a synthetic file tree;
  - a sandboxed shell working dir.
- Integration tests target `testenv` only, and destructive tests run in **Windows Sandbox** or a
  VM. CI runs the UIA tests on `windows-latest` against the dummy app.
- Security tests (plan §117) include: injection pages, malicious documents, a hostile MCP server
  fixture, shell injection strings, path traversal, and attempts to bypass permissions.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Tool schema and router (§1–2)**

- [ ] **TOOL-01** · M1 · `ToolSpec` and `ToolResult` exactly as in §1 (namespaced id, JSON Schema params/result, risk, side effects, data egress, timeout, cancellable, capability tier, platforms), plus `undo` handler or `irreversible: true` (§1, UX §8.1)
- [ ] **TOOL-02** · M1 · The executor API requires the `Decision` from `authorize()`; nothing executes without one (type-level) (§1, SECURITY §2)
- [ ] **TOOL-03** · M1 · Errors carry a user-readable message and a machine code; raw HRESULTs are logged, never spoken (§1)
- [ ] **TOOL-04** · M4 · Tool router with the capability ladder (Native/OS API → App CLI → UIA → Browser DOM → A11y → Vision → Input) and per-app overrides (§2)
- [ ] **TOOL-05** · M4 · App Capability Registry as data files (`apps/*.toml`: exe/AUMID match, launch method, CLI verbs, UIA hints, quirks) for the core apps; users and plugins can add entries (§2)

**Native Windows tools (§3)**

- [ ] **TOOL-06** · M1 · Apps: launch, close, focus, restart, list installed (Start-menu `.lnk` + `shell:AppsFolder`, UWP via AUMID, fuzzy match with aliases) (§3)
- [ ] **TOOL-07** · M1 · Windows: list, focus (foreground-lock rules), minimize, maximize, close; DWM cloaking check (§3)
- [ ] **TOOL-08** · M4 · Windows: move to monitor, snap (§3)
- [ ] **TOOL-09** · M1 · Audio: volume get/set, mute, mic mute (§3)
- [ ] **TOOL-10** · M4 · Audio: output device switch via documented APIs (§3)
- [ ] **TOOL-11** · M1 · Media: play/pause/next/previous and now-playing via `GlobalSystemMediaTransportControlsSessionManager` (§3)
- [ ] **TOOL-12** · M1 · System: lock and sleep; restart and shutdown as High risk with confirmation (§3)
- [ ] **TOOL-13** · M4 · System: brightness where supported, battery, Focus/DND state (§3)
- [ ] **TOOL-14** · M1 · Screen: screenshot of screen, window or region (Windows.Graphics.Capture) (§3)
- [ ] **TOOL-15** · M4 · OCR via Windows.Media.Ocr (offline) (§3)
- [ ] **TOOL-16** · M4 · Files: search (Windows Search `SystemIndex` + fallback walk), open, reveal, create, rename, move, copy; delete always to the Recycle Bin; path-traversal and protected-path guard (§3)
- [ ] **TOOL-17** · M4 · Clipboard read/write; reads are tagged `Untrusted` (§3)
- [ ] **TOOL-18** · M1 · Notifications: show a Windows toast (§3)

**UI Automation (§4)**

- [ ] **TOOL-19** · M4 · A dedicated UIA thread (COM MTA) using the `uiautomation` crate (license verified in M0) with `windows-rs` for gaps (§4)
- [ ] **TOOL-20** · M4 · UIA tools `uia.find`, `uia.get_tree` (depth-limited, pruned), `uia.invoke`, `uia.set_value`, `uia.toggle`, `uia.select`, `uia.expand`, `uia.scroll_into_view`, `uia.get_bounds` (§4)
- [ ] **TOOL-21** · M4 · UIA events (FocusChanged, scoped StructureChanged, WindowOpened/Closed, PropertyChanged) subscribed on demand into the event bus; no polling (§4)
- [ ] **TOOL-22** · M4 · Elevated windows (UIPI) are reported clearly; KIVO never auto-elevates; custom-drawn apps fall back to Vision/Input (§4)

**Browser (§5)**

- [ ] **TOOL-23** · M1 · Open URL and web search via the default browser (§5)
- [ ] **TOOL-24** · M4 · KIVO Chromium MV3 extension (Chrome, Edge, Brave) + native messaging host (`kivo-runtime --native-messaging`): tabs, URL/title, selection, readable DOM excerpt (~4k tokens), click/type on elements (§5)
- [ ] **TOOL-25** · M4 · CDP automation (`chromiumoxide`) only in a KIVO-managed profile (§5)
- [ ] **TOOL-26** · M4 · UIA on the browser window as the fallback when the extension is absent; all page content is `Untrusted` (§5)
- [ ] **TOOL-27** · Post · Firefox extension (§5)

**Watchers (§6)**

- [ ] **TOOL-28** · M5 · Event-driven watchers: download finished, folder changed (`notify`), process exited / build finished, window/app state (UIA/WinEvent), time/reminders (persisted scheduler; missed runs reported on start) (§6)
- [ ] **TOOL-29** · M5 · Every task has an owner, permissions granted at creation, history, status and cancellation; watchers use no LLM while waiting (§6)

**Shell (§7)**

- [ ] **TOOL-30** · M4 · `shell.run { command, shell: pwsh|cmd, cwd, timeout }` inside a Job Object (kill-on-close, memory/CPU limits), output captured, cancellable, logged (§7, ARCHITECTURE §1)
- [ ] **TOOL-31** · M4 · Shell risk parser: read-only commands Low; writes, deletes, network, registry, `Invoke-Expression`, encoded commands or elevation Medium/High; unparseable High; never elevated; secrets only as per-call env vars (§7)

**Vision and input (§8)**

- [ ] **TOOL-32** · M4 · Vision on request only: capture region/window → local OCR or a vision brain within the privacy class; no continuous capture (§8)
- [ ] **TOOL-33** · M4 · Input (SendInput) last tier: re-validate foreground window and bounds before clicking; block typing into password fields (UIA `IsPassword`); at least Medium unless user-initiated (§8)

**MCP (§9)**

- [ ] **TOOL-34** · M6 · `rmcp` client with stdio and Streamable HTTP transports (§9)
- [ ] **TOOL-35** · M6 · MCP tools become `mcp.<server>.<tool>` ToolSpecs, default Medium risk (user can lower per tool), `data_egress` for remote servers (§9)
- [ ] **TOOL-36** · M6 · Tool descriptions treated as untrusted: shown on install, description hashes compared and changes flagged (§9)
- [ ] **TOOL-37** · M6 · KIVO MCP server exposing selected KIVO tools to CLI agents, all calls through the permission engine (§9)

**Test environment (§10)**

- [ ] **TOOL-38** · M4 · `testenv/` dummy Win32/WinUI app with known AutomationIds (buttons, fields, lists, dialogs, fake Export), local HTML pages (forms, injection pages, fake login), synthetic file tree, sandboxed shell dir (§10)
- [ ] **TOOL-39** · M4 · CI runs the UIA journey against the dummy app on `windows-latest`; destructive tests run only in Windows Sandbox or a VM (§10)
- [ ] **TOOL-40** · M4 · Security test suite v1: injection pages, malicious documents, shell injection strings, path traversal, permission-bypass attempts (§10, SECURITY)
- [ ] **TOOL-41** · M6 · Hostile MCP server fixture tests (§10)
