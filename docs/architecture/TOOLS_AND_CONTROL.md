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
| Audio | volume get/set, mute, mic mute, output device switch | `IAudioEndpointVolume`, `IMMDeviceEnumerator`. Windows has no documented API for the system's default output, so a switch moves KIVO's own voice and opens Windows' output settings for the rest (DECISIONS "Output device switch") |
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
| 2. **KIVO browser extension** | The user's real, logged-in browser: tabs, URL/title, selection, readable DOM excerpt, click/type on page elements | Chromium MV3 extension (Chrome, Edge, Brave, `extensions/browser`) + **native messaging host**: `kivo-runtime` started by the browser with the extension's origin (or `--native-messaging`), relaying to the runtime's user-only bridge pipe. Firefox later |
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
- **Input** (SendInput) is the last tier. Its tools belong to the Computer use capability (off by
  default).
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

- [x] **TOOL-01** · M1 · `ToolSpec` and `ToolResult` exactly as in §1 (namespaced id, JSON Schema params/result, risk, side effects, data egress, timeout, cancellable, capability tier, platforms), plus `undo` handler or `irreversible: true` (§1, UX §8.1) → done: `kivo_core::tool::ToolSpec` (namespaced id, JSON Schema params/result, risk, side effects, data egress, timeout, cancellable, capability tier, platforms, reversibility, capability) and `ToolResult`; every Undoable tool has an undo handler (`Tool::undo`, run through `registry::undo` with its own permit) that restores the recorded previous state, and restart/shutdown are Irreversible · verified: `every_undoable_tool_has_an_undo_and_no_other_does`, `undoable_tools_take_back_what_they_did`, `an_undo_is_a_permitted_call_of_its_own`, `every_tool_is_fully_declared` (2026-09-23); the Undo experience is UX-43 (M4)
- [x] **TOOL-02** · M1 · The executor API requires the `Decision` from `authorize()`; nothing executes without one (type-level) (§1, SECURITY §2) → done: `registry::execute` and `registry::undo` take a `Permit`, which only `kivo_security::authorize`/`confirmed` can create and which must name the exact call; there is no other way to run a tool · verified: `a_permit_runs_only_its_own_call`, `an_undo_is_a_permitted_call_of_its_own` (2026-09-23)
- [x] **TOOL-03** · M1 · Errors carry a user-readable message and a machine code; raw HRESULTs are logged, never spoken (§1) → done: `ToolError { code, message, detail }`: the message is plain words from the text catalog (spoken and shown), the OS detail (HRESULTs) goes to the log only; the engine adds what to do next (plan §146) · verified: `failures_are_worded_for_people`, `platform_error` tests (2026-09-23)
- [x] **TOOL-04** · M4 · Tool router with the capability ladder (Native/OS API → App CLI → UIA → Browser DOM → A11y → Vision → Input) and per-app overrides (§2) → done: `control.act` (`crates/kivo-tools/src/router.rs`) walks the ladder Native → URI → App CLI → UIA → Browser DOM → Vision → Input with the app's registry preferences first, skips tiers whose capability is off, stops at a password field and reports which tier did it and what was tried · verified: `uia_is_used_first_for_a_desktop_app_and_says_so`, `custom_drawn_apps_fall_to_vision_and_input_only_when_allowed`, `password_fields_stop_the_ladder`, `the_ladder_puts_an_apps_preferences_first` (2026-09-23)
- [x] **TOOL-05** · M4 · App Capability Registry as data files (`apps/*.toml`: exe/AUMID match, launch method, CLI verbs, UIA hints, quirks) for the core apps; users and plugins can add entries (§2) → done: `crates/kivo-tools/apps/*.toml` (13 core apps: VS Code, Windows Terminal, git, GitHub CLI, Spotify, Chrome, Edge, Brave, Firefox, Explorer, Notepad, Settings, Calculator) compiled in, plus the user's `%APPDATA%\KIVO\apps` files, which override core ones; exe/AUMID match, launch, CLI verbs with risk, URI verbs, UIA hints, preferred tiers, quirks (DECISIONS "App registry as data") · verified: `the_core_registry_loads_and_matches_programs_and_names`, `user_entries_are_added_and_override_core_ones_and_bad_files_are_reported` (2026-09-23)

**Native Windows tools (§3)**

- [x] **TOOL-06** · M1 · Apps: launch, close, focus, restart, list installed (Start-menu `.lnk` + `shell:AppsFolder`, UWP via AUMID, fuzzy match with aliases) (§3) → done: `apps.launch`, `apps.close`, `apps.restart` (close, wait for it to go, open again) and the installed-app index from the Start menu `.lnk`s and `shell:AppsFolder` (UWP by AUMID), fuzzy-matched with aliases; focusing an app's window is `windows.focus` · verified: `the_start_menu_lists_real_apps_without_uninstallers`, `restarting_closes_then_opens_the_app`, index tests, the spoken end-to-end test (2026-09-23)
- [x] **TOOL-07** · M1 · Windows: list, focus (foreground-lock rules), minimize, maximize, close; DWM cloaking check (§3) → done: `WindowsWindows`: list (visible, uncloaked via DWMWA_CLOAKED, front to back), focus (foreground-lock handled with AttachThreadInput), minimize, maximize, restore, close; the runtime is per-monitor DPI aware so positions are physical pixels · verified: `windows_are_listed_front_to_back_with_their_app`, window tool tests (2026-09-23)
- [x] **TOOL-08** · M4 · Windows: move to monitor, snap (§3) → done: `windows.move_to_monitor` (monitors numbered left to right, window kept whole and centred) and `windows.snap` (halves and quarters of the work area), both undoable; `WindowsDisplays` (EnumDisplayMonitors, GetWindowPlacement, SetWindowPos without activating) · verified: `snapping_and_moving_use_the_work_area_and_undo`, `snapping_splits_the_work_area`, `there_is_one_primary_monitor_numbered_from_one` (2026-09-23)
- [x] **TOOL-09** · M1 · Audio: volume get/set, mute, mic mute (§3) → done: `audio.volume_set/up/down`, `audio.mute/unmute`, `audio.mic_mute/unmute` through `WindowsControl` (Core Audio endpoint volume), each undoable · verified: `the_speaker_and_microphone_levels_are_readable`, tool tests (tests never change the real volume) (2026-09-23)
- [x] **TOOL-10** · M4 · Audio: output device switch via documented APIs (§3) → done: `audio.outputs` lists the devices; `audio.set_output` moves KIVO's own voice there and opens Windows' output settings, since Windows has no documented default-device API (spec §3 updated, DECISIONS "Output device switch") · verified: `output_switch_moves_kivos_voice_and_opens_the_picker`, `lists_devices_with_one_default_per_direction` (2026-09-23)
- [x] **TOOL-11** · M1 · Media: play/pause/next/previous and now-playing via `GlobalSystemMediaTransportControlsSessionManager` (§3) → done: `media.play_pause/next/previous/now_playing` through `GlobalSystemMediaTransportControlsSessionManager` · verified: tool tests with the fake, live "what's playing" (2026-09-22)
- [x] **TOOL-12** · M1 · System: lock and sleep; restart and shutdown as High risk with confirmation (§3) → done: `system.lock` and `system.sleep` (Low risk), `system.restart` and `system.shutdown` (High risk, Irreversible, always confirmed, a spoken yes isn't enough) · verified: tool tests (power actions are recorded by the fake, never performed), policy tests (2026-09-23)
- [x] **TOOL-13** · M4 · System: brightness where supported, battery, Focus/DND state (§3) → done: `system.brightness` / `system.set_brightness` (built-in panels through WMI `WmiMonitorBrightness`, undoable; external monitors say they can't be changed), `system.battery` (GetSystemPowerStatus), `system.focus_state` (Windows' notification state) · verified: `brightness_battery_and_focus` (tool), `brightness_is_a_percentage_or_unsupported`, `battery_is_consistent_when_present`, `focus_state_reads` on this PC; the WMI write is not run against the real display (owner rule: tests never change real settings) (2026-09-23)
- [x] **TOOL-14** · M1 · Screen: screenshot of screen, window or region (Windows.Graphics.Capture) (§3) → done: `screen.screenshot` (the monitor in front, or a region) and `screen.screenshot_window` via Windows.Graphics.Capture, one frame on request, saved as PNG to Pictures\Screenshots · verified: `captures_one_frame_of_the_active_monitor`, `a_region_is_cropped_from_its_monitor`, `one_window_is_captured_at_its_size`, `screenshots_of_the_screen_a_window_or_a_region` (2026-09-23)
- [x] **TOOL-15** · M4 · OCR via Windows.Media.Ocr (offline) (§3) → done: `WindowsOcr` on `Windows.Media.Ocr` (the given language, else the user's), line boxes from word boxes; `screen.ocr` and screen reading use it · verified: `recognizes_text_drawn_into_an_image` (GDI-rendered text read back), `a_malformed_image_is_refused` (2026-09-23)
- [x] **TOOL-16** · M4 · Files: search (Windows Search `SystemIndex` + fallback walk), open, reveal, create, rename, move, copy; delete always to the Recycle Bin; path-traversal and protected-path guard (§3) → done: `files.search` (Windows index via `search-ms:`, else a bounded walk), `read`, `open`, `reveal`, `create`, `rename`, `move`, `copy` (undoable) and `delete` (Recycle Bin via `IFileOperation`, confirmed up front); the path guard, allowed and private folders, protected paths and bulk → High (DECISIONS "Files: scoped, recycled") · verified: `create_rename_move_copy_and_undo_on_the_synthetic_tree`, `delete_goes_to_the_recycle_bin`, `the_guard_refuses_traversal_devices_networks_and_streams`, `files_stay_inside_the_allowed_folders_and_out_of_private_ones`, `recycling_a_temp_file_moves_it_out_of_its_folder` (2026-09-23)
- [x] **TOOL-17** · M4 · Clipboard read/write; reads are tagged `Untrusted` (§3) → done: `clipboard.read` (untrusted, `the clipboard` source; taints the turn) and `clipboard.write` (undoable) on `WindowsClipboard` (Unicode text), each switchable in Capabilities → Clipboard · verified: `clipboard_reads_are_untrusted_and_writes_undo`, `destinations_from_content_are_refused` (a clipboard read taints the turn) (2026-09-23)
- [x] **TOOL-18** · M1 · Notifications: show a Windows toast (§3) → done: `notifications.show` through `WindowsNotifications` (Windows toasts, AUMID KIVO.Desktop), gated by the Notifications capability · verified: `toast_xml_escapes_text_and_wires_buttons`, `notifications_respect_their_capability` (2026-09-23)

**UI Automation (§4)**

- [x] **TOOL-19** · M4 · A dedicated UIA thread (COM MTA) using the `uiautomation` crate (license verified in M0) with `windows-rs` for gaps (§4) → done: `WindowsUiAutomation` (`crates/kivo-platform-windows/src/uia.rs`): one COM-MTA thread owning the `uiautomation` client, a 12 s answer deadline and IUIAutomation2 provider timeouts; `windows-rs` for elevation and window checks (DECISIONS "UI Automation on its own thread") · verified: `uia_journey_on_the_dummy_app` against `testenv/app` (2026-09-23)
- [x] **TOOL-20** · M4 · UIA tools `uia.find`, `uia.get_tree` (depth-limited, pruned), `uia.invoke`, `uia.set_value`, `uia.toggle`, `uia.select`, `uia.expand`, `uia.scroll_into_view`, `uia.get_bounds` (§4) → done: `uia.find` (name fuzzy, role, AutomationId), `uia.get_tree` (depth 6, 150 nodes, unnamed containers lifted), `invoke`, `set_value` (undoable, never passwords), `toggle` (undoable), `select`, `expand` (undoable), `scroll_into_view`, `get_bounds`, plus `context.selection` · verified: `uia_journey_on_the_dummy_app` (find, tree, set value, invoke, toggle, select, expand/collapse, bounds, Export writes its file), tool tests in `uia_tools.rs` (2026-09-23)
- [x] **TOOL-21** · M4 · UIA events (FocusChanged, scoped StructureChanged, WindowOpened/Closed, PropertyChanged) subscribed on demand into the event bus; no polling (§4) → done: focus, window opened/closed, and window-scoped structure and property events, subscribed only on request (`uia.wait`) and removed after; each also goes onto the event bus (`SystemEvent::Automation`) · verified: `uia_journey_on_the_dummy_app` (the Export window's WindowOpened), `structure_and_property_events_are_scoped_to_one_window`, `waiting_is_event_driven_and_unsubscribes` (2026-09-23)
- [x] **TOOL-22** · M4 · Elevated windows (UIPI) are reported clearly; KIVO never auto-elevates; custom-drawn apps fall back to Vision/Input (§4) → done: a window whose process is elevated above KIVO is refused up front with "<app> is running as administrator …" (`PlatformError::Elevated`); KIVO never elevates; an empty tree is reported as custom-drawn so the ladder falls to Vision/Input · verified: `this_process_is_not_elevated_in_tests`, `trees_are_compact_and_custom_drawn_windows_are_flagged`, `custom_drawn_apps_fall_to_vision_and_input_only_when_allowed`; a live elevated window isn't tested (tests never run as administrator) (2026-09-23)

**Browser (§5)**

- [x] **TOOL-23** · M1 · Open URL and web search via the default browser (§5) → done: `browser.open_url` (http/https only) and `browser.search` open the default browser; the address is a destination the hard limits check · verified: `only_web_addresses_are_opened`, tool and grammar tests (2026-09-23)
- [x] **TOOL-24** · M4 · KIVO Chromium MV3 extension (Chrome, Edge, Brave) + native messaging host (`kivo-runtime --native-messaging`): tabs, URL/title, selection, readable DOM excerpt (~4k tokens), click/type on elements (§5) → done: MV3 extension `extensions/browser` (tabs, active tab, readable excerpt ≈ 4k tokens, selection, click and type, never password fields) and the native messaging host in `kivo-runtime` relaying to the user-only bridge pipe with the session token; `browser.*` tools (DECISIONS "The browser extension and its host") · verified: `extension.test.ts` (7, on testenv pages), `the_extension_answers_through_the_bridge`, `the_host_relays_between_the_browser_and_the_runtime`, `a_connection_without_the_token_is_refused`, browser tool tests (2026-09-23)
- [x] **TOOL-25** · M4 · CDP automation (`chromiumoxide`) only in a KIVO-managed profile (§5) → done: `ManagedChrome` (`chromiumoxide`) runs Chrome/Edge/Brave already installed in a KIVO profile folder, respecting certificate errors, never downloading a browser; `browser.auto.*` tools under Autonomous browsing · verified: `kivos_browser_reads_clicks_and_types_in_its_own_profile` (headless, throwaway profile, local page), `the_managed_browser_opens_only_web_addresses` (2026-09-23)
- [x] **TOOL-26** · M4 · UIA on the browser window as the fallback when the extension is absent; all page content is `Untrusted` (§5) → done: without the extension, the active tab's address and title come from the browser window through UIA and the page text from its UIA tree; all page content is untrusted · verified: `without_the_extension_the_active_tab_comes_from_uia`, `with_the_extension_pages_are_read_as_untrusted_excerpts` (2026-09-23)
- [ ] **TOOL-27** · Post · Firefox extension (§5)

**Watchers (§6)**

- [x] **TOOL-28** · M5 · Event-driven watchers: download finished, folder changed (`notify`), process exited / build finished, window/app state (UIA/WinEvent), time/reminders (persisted scheduler; missed runs reported on start) (§6) → done: `watchers.rs`: download finished and folder changed (`notify`), process exit / build finished (kernel waits), window opened/closed (UIA events), time (stored, missed ones reported on start) · verified: `watchers.rs` tests, `a_process_is_listed_and_its_exit_is_waited_for`, `the_build_watcher_waits_without_ai_and_tells_you` (2026-09-23)
- [x] **TOOL-29** · M5 · Every task has an owner, permissions granted at creation, history, status and cancellation; watchers use no LLM while waiting (§6) → done: every task has an owner, its grants, step history, status and cancel; watchers make no brain calls while waiting · verified: `the_build_watcher_waits_without_ai_and_tells_you` (zero brain requests), `Tasks.test.tsx` (2026-09-23)

**Shell (§7)**

- [x] **TOOL-30** · M4 · `shell.run { command, shell: pwsh|cmd, cwd, timeout }` inside a Job Object (kill-on-close, memory/CPU limits), output captured, cancellable, logged (§7, ARCHITECTURE §1) → done: `shell.run { command, shell: pwsh|cmd, cwd, timeoutMs, env }` via `WindowsCommands`: started suspended, joined to its own Job Object (kill on close, memory cap, CPU cap), output captured (64 KiB), cancellable, killed with its tree; logged through the audit (DECISIONS "Commands in Job Objects") · verified: `cmd_runs_in_its_folder_with_per_call_env`, `powershell_reports_exit_codes_and_stderr`, `timeouts_and_cancellation_kill_the_whole_tree`, `kill_all_ends_running_commands`, `output_is_capped`, `cancellation_stops_a_running_command` (2026-09-23)
- [x] **TOOL-31** · M4 · Shell risk parser: read-only commands Low; writes, deletes, network, registry, `Invoke-Expression`, encoded commands or elevation Medium/High; unparseable High; never elevated; secrets only as per-call env vars (§7) → done: the parser (`shell::assess`): read-only Low; writes, installs, network Medium; deletes with recursion or wildcards, registry, services, disks, users, `Invoke-Expression`, encoded commands, download-and-run and elevation High; unreadable High; a protected working folder High; read-only mode by default; secrets only as per-call environment variables, scrubbed from output · verified: `read_only_commands_are_low`, `writes_installs_and_network_are_medium`, `dangerous_commands_are_high`, `quoting_hides_nothing`, `the_call_risk_comes_from_the_command`, `a_run_goes_through_the_job_runner_with_secrets_as_env_only`, `read_only_mode_refuses_anything_that_changes_something` (2026-09-23)

**Vision and input (§8)**

- [x] **TOOL-32** · M4 · Vision on request only: capture region/window → local OCR or a vision brain within the privacy class; no continuous capture (§8) → done: `screen.read` (one capture, UIA excerpt + local OCR, dropped after), `screen.ocr`, and `screen.look` (one downscaled PNG to a vision brain only when it sees and the settings allow; `Part::Image` in the Anthropic, OpenAI and Gemini adapters); never continuous · verified: `reading_uses_the_tree_and_local_ocr_and_keeps_nothing`, `looking_needs_cloud_vision_allowed_and_sends_a_small_png`, `screenshots_reach_vision_models_in_each_providers_shape`, `screenshots_go_only_to_a_brain_allowed_to_see` (2026-09-23)

**MCP (§9)**

- [x] **TOOL-34** · M6 · `rmcp` client with stdio and Streamable HTTP transports (§9) → done: `crates/kivo-mcp/src/client.rs` on `rmcp` 3.4 with stdio and Streamable HTTP transports · verified: `kivo-mcp/tests/servers.rs` (stdio fixture) and `tests/signin.rs` (HTTP)
- [x] **TOOL-35** · M6 · MCP tools become `mcp.<server>.<tool>` ToolSpecs, default Medium risk (user can lower per tool), `data_egress` for remote servers (§9) → done: `mcp.<server>.<tool>` ToolSpecs (`crates/kivo-mcp/src/tool.rs`), default Medium, lowerable per tool, `data_egress` for remote servers · verified: kivo-mcp unit tests, `m6_extensions::an_imported_server_is_reviewed_then_used_by_a_brain`
- [x] **TOOL-36** · M6 · Tool descriptions treated as untrusted: shown on install, description hashes compared and changes flagged (§9) → done: descriptions shown for review and labelled as the server's; approval by SHA-256 of name, description and schema (`hash.rs`); a changed tool is switched off until reviewed again · verified: `m6_extensions::a_description_changed_after_approval_is_switched_off`, hash unit tests, `Extensions.test.tsx`
- [x] **TOOL-37** · M6 · KIVO MCP server exposing selected KIVO tools to CLI agents, all calls through the permission engine (§9) → done: `kivo-runtime --mcp-server --agent <id>` (`mcp_bridge.rs`, `crates/kivo-mcp/src/server.rs`) relays to `Engine::shared_call` with initiator Mcp through the permission engine · verified: `m6_extensions::kivos_mcp_server_shares_memory_only_with_allowed_agents`, args test

**Test environment (§10)**

- [x] **TOOL-38** · M4 · `testenv/` dummy Win32/WinUI app with known AutomationIds (buttons, fields, lists, dialogs, fake Export), local HTML pages (forms, injection pages, fake login), synthetic file tree, sandboxed shell dir (§10) → done: `testenv/`: the dummy Win32 app (`kivo-test-app`, AutomationIds 101–110 and 201–203, never takes focus, Export writes only into `--out`), `pages/` (form, fake login, injection), `documents/malicious.md`, the synthetic `files/` tree and the `shell/` folder, with the testkit launcher · verified: `uia_journey_on_the_dummy_app`, file tests on a copy of the tree, `extension.test.ts` on the pages (2026-09-23)
- [x] **TOOL-40** · M4 · Security test suite v1: injection pages, malicious documents, shell injection strings, path traversal, permission-bypass attempts (§10, SECURITY) → done: the security suite v1 in two halves. Tools (`crates/kivo-tools/src/security_suite.rs`): 31 smuggled-change shell strings (chaining, newlines, pipes into `iex`, script blocks, subexpressions in double quotes, en-dash flags, `git --output`, cmd `&`) never pass as read-only and the read-only shell runs none; destructive and obfuscated ones are High; a brain's extra arguments can't lower a risk; traversal, device, share, stream, drive-relative and variable paths are refused by every file tool; renames can't leave the folder; disguised schemes never open. End to end (`apps/kivo-runtime/tests/security_suite.rs`, the `testenv/` fixtures): the malicious document and the injection page reach the brain fenced (a forged `</UNTRUSTED >` can't close the fence — `ContextItem::render` now defuses any spelling), the tainted round offers no shell, delete or sending and refuses them without asking; switched-off, unknown and unoffered tools are refused; arguments claiming approval still get the strong question, and forged answers approve nothing. The suite found and fixed parser gaps (script blocks, `$()` in double quotes, `@()`, `iex(`, `InvokeScript`, unicode dashes, `git --output`) · verified: the 8 tools tests and 4 end-to-end tests, `untrusted_text_cant_leave_its_fence`; CI runs both as a named step (2026-09-23)
- [x] **TOOL-41** · M6 · Hostile MCP server fixture tests (§10) → done: `testenv/mcp --hostile`: poisoned description, a tool named `shell.run`, a 500k flood, a 10-minute call, injected instructions, a failure asking for a password, a bad schema and a rug pull · verified: `kivo-mcp/tests/servers.rs::a_hostile_server_is_contained` and `m6_extensions::a_description_changed_after_approval_is_switched_off` (M6-X1)
