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
