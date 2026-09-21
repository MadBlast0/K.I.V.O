# Tauri v2 on Windows (WebView2): feasibility of KIVO's ambient overlay UI and background-app lifecycle

Scope: Tauri 2.x (docs.rs currently shows tauri 2.11.6), Windows 10/11, WebView2 Evergreen. Research date 2026-09-21.
Overall verdict (inference, detailed below): **feasible**. Every required piece (floating transparent always-on-top pill, tray, autostart, global hotkey with press/release, single instance, close-to-tray) has a first-party Tauri API or plugin. The weak spots are all on the overlay side: white flash on show, focus stealing when click-through is toggled, no native per-pixel hit-testing, acrylic lag, and exclusive-fullscreen games. Each has a workaround. A full-screen WebView2 edge-glow is the least attractive part; a small native Rust layer is the fallback.

## 1. Transparent, borderless, always-on-top, skip-taskbar windows; the white flash, white frame and shadow artifacts

### Takeaway
All the window flags exist as WindowConfig fields and runtime setters (`transparent`, `decorations`, `alwaysOnTop`, `skipTaskbar`, `shadow`, `visible`, `backgroundColor`). The main Windows-specific defects are a **white flash when a hidden transparent window is shown** (still open as of Tauri 2.8.5, issue #14515), and a **1px white border plus rounded corners when `shadow: true` is used on an undecorated window on Windows 11**. Fixes: `shadow: false`, a transparent WebView2 default background (set early through the `WEBVIEW2_DEFAULT_BACKGROUND_COLOR` env var), create the window hidden, and show it only after the frontend signals it has painted.

### Cited Findings
- WindowConfig fields: `transparent`, `decorations`, `alwaysOnTop`, `skipTaskbar`, `focus` ("receives focus upon creation"), `focusable`, `visible`, `shadow`, `backgroundColor`, `windowEffects`, `visibleOnAllWorkspaces`, `contentProtected`, `preventOverflow`. The config reference notes `shadow` does not work with undecorated windows on some platforms. — [Tauri config reference](https://v2.tauri.app/reference/config/)
- `set_shadow` on Windows: "`false` has no effect on decorated windows; `true` adds a 1px white border and rounded corners on Windows 11." This is the source of the "white frame" around undecorated overlays. — [docs.rs tauri 2.11.6 WebviewWindow](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)
- `set_background_color` on Windows: the alpha channel is ignored for the window layer. Windows 8+ rejects alpha values other than 0 or 255, and Windows 7 has no transparency. — [docs.rs tauri WebviewWindow](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)
- Open bug: "Creating a hidden transparent window results in a white flash when displayed" (Windows 10 19045, Tauri 2.8.5, WebView2 139). No workaround or linked PR was posted, and it was still marked needs-triage. — [tauri#14515](https://github.com/tauri-apps/tauri/issues/14515)
- Related reports: a white screen appears when calling `show()` in v2 ([tauri#9393](https://github.com/tauri-apps/tauri/issues/9393)); a transparent window still had a white background until resized ([tauri#8308](https://github.com/tauri-apps/tauri/issues/8308), [tauri#4881](https://github.com/tauri-apps/tauri/issues/4881)); a dark-mode white flash at startup ([tauri#6027](https://github.com/tauri-apps/tauri/issues/6027)); a discussion on preventing the white flash ([discussion #13226](https://github.com/orgs/tauri-apps/discussions/13226)). The common community workaround is `"visible": false, "transparent": true` in tauri.conf.json, then showing the window from code once it is ready.
- Enabling `transparent: true` was reported to *improve* resize performance and to remove black/white bar artifacts during resize. — [tauri#13270](https://github.com/tauri-apps/tauri/issues/13270)
- WebView2's `DefaultBackgroundColor` accepts only alpha 0 or 255, not semi-transparent values. When it is transparent, WebView2 shows the host window content behind the page. Microsoft's spec notes that setting the color through the API can still leave a white flicker before it takes effect, and that "setting the color via environment variable solves this issue". The env var is `WEBVIEW2_DEFAULT_BACKGROUND_COLOR`, a hex ARGB value where values under 8 digits get `00` alpha prepended, so they come out transparent. — [WebView2Feedback BackgroundColor spec](https://github.com/MicrosoftEdge/WebView2Feedback/blob/main/specs/BackgroundColor.md); [ICoreWebView2Controller2](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2controller2)

### Inferences
- Recommended overlay window recipe: `decorations: false, transparent: true, shadow: false, alwaysOnTop: true, skipTaskbar: true, visible: false, focus: false, focusable: false, resizable: false`. The CSS `html, body` background must also be `transparent`. Set `std::env::set_var("WEBVIEW2_DEFAULT_BACKGROUND_COLOR", "0")` in `main()` before `tauri::Builder` so WebView2 starts transparent. This works because wry/WebView2 reads the variable at environment creation. That mechanism is inferred from the Microsoft spec, not from Tauri docs.
- To avoid the flash on the first show: create the overlay at startup, hidden. Have the frontend render one transparent frame (for example two `requestAnimationFrame`s after mount), then `invoke('overlay_ready')`, and never call `show()` before that point. For later shows, keep the DOM mounted and animate opacity in CSS rather than reloading.
- If a drop shadow is wanted under the pill, draw it in CSS (`box-shadow`/`filter: drop-shadow`) inside a slightly larger transparent window. Do not use the DWM shadow.

### Gaps
- No upstream fix was found for #14515. Whether the env-var approach fully removes the flash for a *hidden then shown* window, as opposed to only the startup flash, was not confirmed by a primary source. It needs a spike.

## 2. Click-through overlays (`set_ignore_cursor_events`) and partial click-through

### Takeaway
`set_ignore_cursor_events(true)` makes the *whole* window click-through (WS_EX_TRANSPARENT-style). Tauri has no per-region hit-testing. A long-standing feature request asks for this ([#2090](https://github.com/tauri-apps/tauri/issues/2090)), and a "forward" option was requested separately ([#6164](https://github.com/tauri-apps/tauri/issues/6164)). The working pattern is to poll the global cursor position against the pill's rectangle(s) and toggle ignore on and off. Two alternatives avoid the problem: size the overlay window to exactly the pill, or use separate windows for the interactive pill and the non-interactive glow.

### Cited Findings
- API: `set_ignore_cursor_events` ("Ignores the window cursor events") and `cursor_position` ("relative to the top-left hand corner of the desktop", which can be negative on multi-monitor setups). — [docs.rs tauri WebviewWindow](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)
- Feature requests: ignoring mouse events on transparent areas ([tauri#2090](https://github.com/tauri-apps/tauri/issues/2090)); a forward option for `setIgnoreCursorEvents` ([tauri#6164](https://github.com/tauri-apps/tauri/issues/6164)); forwarding mouse/keyboard events to windows underneath ([discussion #11507](https://github.com/tauri-apps/tauri/discussions/11507)); a bug report that `setIgnoreCursorEvents()` does not work ([tauri#11461](https://github.com/tauri-apps/tauri/issues/11461)); a request to expose cursor position ([tauri#9250](https://github.com/tauri-apps/tauri/issues/9250)).
- The DeskPet project implemented partial click-through in Tauri 2 by toggling `setIgnoreCursorEvents` from a `cursorPosition()` poll against the pet/menu bounds. Key caveat: "Hover events cannot turn click-through back off — once ignore is on the webview stops receiving mouse move", so the check has to be a poll. A drag keeps ignore off so pointer capture is not lost. — [Scyyyy4/deskpet PR #3](https://github.com/Scyyyy4/deskpet/pull/3)
- CSS `pointer-events: none` does not pass clicks to other applications, so it cannot replace window-level click-through. — [deskpet PR #3 summary via search](https://github.com/Scyyyy4/deskpet/pull/3)
- Toggling click-through makes tao rebuild the window's extended styles, which wipes any manually added `WS_EX_NOACTIVATE`/`WS_EX_TOOLWINDOW` (see section 3). — [vinzdg/codenotch PR #199](https://github.com/vinzdg/codenotch/pull/199)

### Inferences
- Recommended approach: run the poll **in Rust**, not JS, on a tokio interval of about 30 to 60 Hz while the overlay is visible, and stop it when hidden. Compare `cursor_position()` (physical pixels) with the pill rect reported by the frontend (`getBoundingClientRect() * devicePixelRatio + outer_position()`). Call `set_ignore_cursor_events` only on transitions.
- Simpler alternative for KIVO: if the pill is a compact card, size the window exactly to it and do not make it click-through. Only the full-screen glow window needs permanent `set_ignore_cursor_events(true)`, and that one never needs toggling.
- A native alternative with true per-pixel hit testing is `WM_NCHITTEST` returning `HTTRANSPARENT` from a subclassed wndproc. That is not exposed by Tauri and would need windows-rs subclassing of the HWND. It was not verified against WebView2's child HWND, which receives input itself.

### Gaps
- No measurements were found for the CPU cost of a 60 Hz cursor poll. It is presumed negligible but unmeasured.

## 3. Showing without stealing focus; topmost above fullscreen apps, Start menu and UAC

### Takeaway
Tauri 2.11 adds a `focusable` window config field and a `set_focusable` setter. On Windows, tao then adds `WS_EX_NOACTIVATE` on every style rebuild, so it survives click-through toggles. That is the correct, supported way to build a non-activating KIVO pill. `focus: false` alone is unreliable on Windows ([#11566](https://github.com/tauri-apps/tauri/issues/11566), [#7519](https://github.com/tauri-apps/tauri/issues/7519)). Always-on-top covers borderless-windowed fullscreen, but *exclusive* fullscreen games bypass DWM and will cover the overlay. The Start menu and UAC secure desktop sit in higher z-order bands, or on a separate desktop, that a normal app cannot reach without UIAccess.

### Cited Findings
- codenotch (a Tauri app with a notch-style overlay) fixed focus stealing on Windows by setting `"focusable": false`, described as "a Tauri 2.11 configuration field". tao then adds `WS_EX_NOACTIVATE` on every extended-style rebuild via the `!FOCUSABLE` branch in `window_state.rs`. Their earlier manual approach, setting `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` once at startup, broke because `set_ignore_cursor_events` toggles made tao rebuild the styles and drop the flags, so clicks then stole keyboard focus from the user's editor. — [vinzdg/codenotch PR #199](https://github.com/vinzdg/codenotch/pull/199)
- `set_focusable` exists ("Sets whether the window can be focused"). The macOS caveat does not apply to Windows. — [docs.rs tauri 2.11.6](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)
- Known focus bugs: `focus: false` in tauri config does not disable focus at startup on Windows ([tauri#11566](https://github.com/tauri-apps/tauri/issues/11566)); windows do not respect the `focus` property ([tauri#7519](https://github.com/tauri-apps/tauri/issues/7519)); a window loses focus when another is created with `focus: false`, with a possible deadlock ([tauri#5120](https://github.com/tauri-apps/tauri/issues/5120)); `focusable: false` is broken on macOS ([tauri#14102](https://github.com/tauri-apps/tauri/issues/14102)).
- Win32 semantics: `WS_EX_NOACTIVATE` stops a top-level window from becoming the foreground window when clicked. `WS_EX_TOOLWINDOW` keeps it out of the taskbar and Alt+Tab. `WS_EX_TRANSPARENT`/`WS_EX_LAYERED` are the click-through primitives. — [Microsoft Learn: Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles); `SetWindowPos` with `HWND_TOPMOST | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW` shows or raises without activating — [Microsoft Learn: SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- Exclusive fullscreen: the game "takes direct control of the whole display and bypass[es] the desktop compositor", so the topmost flag no longer governs what is on screen. Either the overlay gets covered or the game is forced out of exclusive mode, with an FPS and latency cost. — [Staged blog](https://staged.gg/blog/always-on-top-over-fullscreen-game) (vendor blog, secondary source). Tauri issues: a request to show a window on top of a full-screen app ([tauri#5793](https://github.com/tauri-apps/tauri/issues/5793)); `setAlwaysOnTop` not working ([tauri#9439](https://github.com/tauri-apps/tauri/issues/9439)); macOS fullscreen workspaces ([tauri#11488](https://github.com/tauri-apps/tauri/issues/11488)).
- Z-order bands: Windows puts the Start menu and its taskbar in `ZBID_IMMERSIVE_MOGO`, above normal topmost windows (`ZBID_DESKTOP`). UIAccess apps get `ZBID_UIACCESS`, which Magnifier and the on-screen keyboard use. — [ADeltaX blog: Window z-order in Windows 10](https://blog.adeltax.com/window-z-order-in-windows-10/) (reverse-engineering blog, undocumented API). A UIAccess app "can run as the topmost application in the z-order at any time". UIAccess requires a signed binary installed in a secure location such as Program Files, and has no effect on the SYSTEM-run UAC secure desktop. — [Microsoft Learn: Security Considerations for Assistive Technologies](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview); [UAC settings](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/settings-and-configuration)
- Hacks that reach higher bands via the undocumented `CreateWindowInBand`/token duplication exist ([arcanine300/CreateWindowInBand](https://github.com/arcanine300/CreateWindowInBand), [xmc0211/WindowTopMost](https://github.com/xmc0211/WindowTopMost/)). They are unsuitable for a shipped consumer app.

### Inferences
- Use `"focusable": false` (requires tauri >= 2.11) on the pill and glow windows. Show them with `window.show()`. If `show()` still activates, fall back to windows-rs `ShowWindow(hwnd, SW_SHOWNOACTIVATE)` plus `SetWindowPos(hwnd, HWND_TOPMOST, 0,0,0,0, SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE|SWP_SHOWWINDOW)`, taking the HWND from `window.hwnd()`.
- If the pill needs text input (a typed follow-up), temporarily call `set_focusable(true)` + `set_focus()`, then revert. Otherwise keep it voice-only and non-activating.
- Re-assert topmost periodically, or on `EVENT_SYSTEM_FOREGROUND` through `SetWinEventHook`, because other topmost windows and fullscreen apps can reorder the stack. This is a common Win32 pattern. No Tauri-specific source was found.
- Accept that KIVO will not appear over exclusive-fullscreen games, the open Start menu or UAC prompts. Detect fullscreen (`SHQueryUserNotificationState` returning `QUNS_RUNNING_D3D_FULL_SCREEN` / `QUNS_BUSY`) and fall back to audio-only feedback or a toast. UIAccess is technically possible but requires signing and Program Files install, and it enlarges the security surface.

### Gaps
- There is no official Tauri changelog citation for `focusable` landing in 2.11. It is confirmed only through the codenotch PR and the existence of `set_focusable` in docs.rs 2.11.6.
- No source confirms the behavior of Tauri topmost windows over Windows 11 borderless-fullscreen games specifically. The expectation is that they work, since that mode is DWM-composited.

## 4. Acrylic / Mica / Blur via window-vibrancy

### Takeaway
window-vibrancy 0.8.0 provides `apply_blur` (Win7, Win10 1809+), `apply_acrylic` (Win10 1809+), and `apply_mica`/`apply_tabbed` (Win11 only). Acrylic has well-documented lag when dragging or resizing on Win10 1903+ and Win11 22000, and it was broken or laggy on Win11 22H2. For a small fixed-size, non-draggable pill, the lag matters little. For resizable or draggable surfaces, prefer Mica on Win11 or a CSS `backdrop-filter`-style fake.

### Cited Findings
- Function and OS matrix: `apply_blur` (Windows 7, 10 v1809+), `apply_acrylic` (10 v1809+), `apply_mica` (11 only), `apply_tabbed` (11 only), each with a matching `clear_*`. Crate version 0.8.0. — [docs.rs window-vibrancy](https://docs.rs/window-vibrancy/latest/window_vibrancy/)
- `apply_acrylic()`/`clear_acrylic()` "have bad performance when resizing/dragging the window on Windows 10 v1903+ and Windows 11 build 22000". — [window-vibrancy README/crates.io](https://crates.io/crates/window-vibrancy/0.1.1); [window-vibrancy#47 Acrylic lag](https://github.com/tauri-apps/window-vibrancy/issues/47)
- Win11 22H2: the Acrylic effect did not work and the Blur effect was laggy, reportedly lagging the whole PC. — [window-vibrancy#45](https://github.com/tauri-apps/window-vibrancy/issues/45)
- Electron-side mitigations: throttle move/resize events to the monitor refresh rate. — [vscode-vibrancy discussion #80](https://github.com/EYHN/vscode-vibrancy/discussions/80)
- Tauri also exposes effects natively: `windowEffects` config / `set_effects` ("Requires the window to be transparent"). — [docs.rs tauri](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)

### Inferences
- Mica samples the *desktop wallpaper*, not what is directly behind the window, so for a floating pill over arbitrary apps it will look like a flat tint. Acrylic (a true behind-window blur) is the look that matches "ambient overlay". Use acrylic only on a fixed-position, non-resizable pill, and provide a solid or semi-opaque CSS fallback on Win10 and when `apply_acrylic` returns `Err`.
- Do not apply any vibrancy to a full-screen glow window.

### Gaps
- No quantitative GPU or frame-time data was found for acrylic on current Win11 24H2/25H2 builds, nor any confirmation of whether the 22H2 regressions are fixed.

## 5. Cost of a full-screen transparent overlay for an edge glow

### Takeaway
A full-screen, per-pixel-transparent, topmost WebView2 window is technically possible (transparent + ignore cursor events + focusable false). But it keeps a Chromium compositor and GPU surface of full monitor size alive, and it needs one window per monitor. No authoritative measurements were found. Recommended alternatives: four thin edge windows (for example 8 to 24 px strips per edge per monitor), or a small native DirectComposition/wgpu window. Either way, animate only while listening and hide the window completely when idle.

### Cited Findings
- WebView2 uses the GPU for rendering by default, and "GPU drivers and additional buffers must be allocated, which requires additional memory". Microsoft advises CSS animations over JS and throttling work. — [Microsoft Learn: WebView2 performance best practices](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- Native overlays in Rust exist, for example ZeroIn-Crosshair (winit + wgpu with premultiplied alpha, softbuffer fallback) as a lightweight click-through overlay. — [ZeroIn-Crosshair](https://github.com/mrdhnto/ZeroIn-Crosshair)
- `cursor_position` and window coordinates can be negative on multi-monitor layouts. — [docs.rs tauri](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html)

### Inferences
- Tauri exposes `available_monitors()`, `Monitor::position/size/scale_factor` and `ScaleFactorChanged` events. Size glow windows per monitor in physical pixels and recreate or reposition them on display-change events.
- Battery: a continuously animating full-screen layer forces DWM to recompose the whole screen every frame, which defeats panel self-refresh and multiplane overlay optimizations. This is general knowledge and was not sourced here. Keep the glow window hidden (`hide()`, not opacity 0) when not listening, and cap the animation.

### Gaps
- No measured numbers (GPU %, watts, MB) were found for a full-screen transparent WebView2 on Windows. This is a spike item: measure with Task Manager GPU engine, `powercfg /srumutil`, and PresentMon.
- No authoritative source was found on per-monitor-DPI edge cases for Tauri transparent windows specifically.

## 6. Memory per WebView2 window; preloaded hidden overlay vs on-demand

### Takeaway
All windows in one Tauri app share one WebView2 environment, and therefore one browser, GPU and network process set. Each extra window mainly adds a renderer process plus its JS heap. Microsoft's guidance is to avoid re-creating WebView2 controls, because a cold start (spinning up processes and caches) is the main latency, and WebView2 has explicit APIs for trimming idle instances. So **preload the overlay hidden at startup and show/hide it**, rather than creating it on demand. On-demand creation is also exactly where the global-shortcut deadlock (section 7) bites.

### Cited Findings
- "Each WebView2 control creates its own set of processes… Resource usage generally grows as more WebView2 instances are created." Use one `CoreWebView2Environment` across controls to save memory. — [Microsoft Learn: WebView2 performance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- A cold start is a "common performance bottleneck": "WebView2 must spin up its processes and disk caches, which can introduce a noticeable delay". Microsoft recommends reusing a WebView2 over destroying and re-creating it, and not using WebView2 for splash or simple dialogs. — [Microsoft Learn: WebView2 performance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- For idle instances, set `MemoryUsageTargetLevel = Low` (may drop caches or swap to disk), or use `TrySuspendAsync()` to suspend the renderer, then `Resume()`. Both are best-effort. — [Microsoft Learn: WebView2 performance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)
- A default Tauri app shows about 6 WebView2 processes. A maintainer said this is normal Chromium architecture ("one main process, one gpu process, one network process etc."). — [tauri discussion #7904](https://github.com/tauri-apps/tauri/discussions/7904)
- An example of real-world bloat reported for another Tauri app: [hoppscotch#6340](https://github.com/hoppscotch/hoppscotch/issues/6340). Figures of 100 to 300 MB for "lightweight" WebView2 apps come only from low-quality SEO blogs and are not trustworthy.

### Inferences
- Keep the overlay as a hidden, preloaded window with lightweight vanilla/Svelte HTML rather than a full SPA bundle. Tauri does not directly expose `MemoryUsageTargetLevel`/`TrySuspend`. They could be reached through `webview.with_webview(|w| w.controller() ...)` using `webview2-com` (this is wry's Windows escape hatch, and the exact call chain was not verified).
- Show latency for a preloaded window should be one `ShowWindow` plus one composited frame, with no network or JS boot involved. Creating a window on demand pays renderer-process creation plus page load.

### Gaps
- No primary-source measurements were found for per-window MB or show latency (preloaded vs on-demand) in Tauri 2 on Windows. KIVO should measure these: private working set of the `msedgewebview2.exe` renderer per window, and time from hotkey to first frame.

## 7. Plugins and lifecycle: tray, autostart, global shortcut (push-to-talk), single instance, notification, close-to-tray, ExitRequested

### Takeaway
All are first-party and work on Windows. Push-to-talk is possible because the handler receives `ShortcutState::Pressed` and `ShortcutState::Released`, and Windows registration uses `MOD_NOREPEAT` so holding the key does not auto-repeat. Limits: shortcuts are RegisterHotKey-style chords, so a bare modifier (for example Right-Ctrl alone) or a mouse button cannot be registered. The plugin also holds a mutex while the handler runs, so heavy or reentrant handlers deadlock. Autostart has an open bug where the Run entry disappears after one boot.

### Cited Findings
- **Global shortcut:** `tauri_plugin_global_shortcut::Builder::new().with_handler(|app, shortcut, event| match event.state() { ShortcutState::Pressed => …, ShortcutState::Released => … })`, then `app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL), Code::KeyN))`. JS use needs the capability permissions `global-shortcut:allow-register`/`allow-unregister`/`allow-is-registered`, since "No features are enabled by default". — [Tauri global-shortcut plugin docs](https://v2.tauri.app/plugin/global-shortcut/)
- The release event was a long-standing request ([tauri#4364](https://github.com/tauri-apps/tauri/issues/4364)) and is now supported through `ShortcutState::Released`. The backing crate is [tauri-apps/global-hotkey](https://github.com/tauri-apps/global-hotkey).
- Windows auto-repeat: `MOD_NOREPEAT` was added to Windows hotkey registration so a held accelerator does not re-fire. — [tao PR #602](https://github.com/tauri-apps/tao/pull/602)
- Deadlock: `tauri-plugin-global-shortcut` 2.3.2 (global-hotkey 0.8.0) holds the `shortcuts` mutex while invoking the handler. Calling `register`/`unregister`/`is_registered` from the handler, or creating a WebView from it (which pumps messages via `webview2_com::wait_with_pump` and redelivers `WM_HOTKEY`), deadlocks the main thread. Workaround: defer the work with `tauri::async_runtime::spawn` + `run_on_main_thread`. — [plugins-workspace#3590](https://github.com/tauri-apps/plugins-workspace/issues/3590)
- Other known issues: handler called twice on press ([plugins-workspace#1748](https://github.com/tauri-apps/plugins-workspace/issues/1748)), double fire on macOS ([tauri#10025](https://github.com/tauri-apps/tauri/issues/10025)), X11 release-order problems ([global-hotkey#39](https://github.com/tauri-apps/global-hotkey/issues/39)), registration failures ([discussion #10017](https://github.com/tauri-apps/tauri/discussions/10017)).
- **Tray:** Cargo feature `tauri = { features = ["tray-icon"] }`. `TrayIconBuilder::new().menu(&menu).show_menu_on_left_click(false).on_menu_event(...).on_tray_icon_event(|tray, e| if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e { … })`. By default the menu shows on both left and right click. — [Tauri system tray guide](https://v2.tauri.app/learn/system-tray/)
- **Single instance:** `app.handle().plugin(tauri_plugin_single_instance::init(|app, args, cwd| { … }))`. It "must be the first one to be registered to work well". The callback receives the second launch's argv and cwd, and is used to focus or show the existing window. — [Tauri single-instance plugin](https://v2.tauri.app/plugin/single-instance/)
- **Autostart:** `tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"]))` and `enable()`/`disable()`/`is_enabled()`. On Windows it writes `HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run`. — [Tauri autostart plugin](https://v2.tauri.app/plugin/autostart/); [tauri-docs autostart.mdx](https://github.com/tauri-apps/tauri-docs/blob/v2/src/content/docs/plugin/autostart.mdx)
- Autostart bug: on Windows the Run entry was removed after one boot, so users had to re-enable it. Opened Nov 2023 and still open, with no root cause documented. — [plugins-workspace#771](https://github.com/tauri-apps/plugins-workspace/issues/771). Also "autostart not working on Windows" ([plugins-workspace#24](https://github.com/tauri-apps/plugins-workspace/issues/24)).
- **Close-to-tray:** `.on_window_event(|window, event| if let WindowEvent::CloseRequested { api, .. } = event { window.hide().unwrap(); api.prevent_close(); })`. — [tauri discussion #2684](https://github.com/tauri-apps/tauri/discussions/2684). A report of `prevent_close()` being ineffective: [tauri#12334](https://github.com/tauri-apps/tauri/issues/12334).
- **ExitRequested:** in `.build(ctx)?.run(|app, event| if let RunEvent::ExitRequested { api, code, .. } = event { api.prevent_exit(); })`, the app keeps running when all windows are closed, which suits tray-only apps. Caveat: this also blocks normal exit unless you distinguish an explicit `app.exit(0)` (where `code` is `Some`) from last-window-closed (where `code` is `None`). — [tauri discussion #11489](https://github.com/tauri-apps/tauri/discussions/11489); [ExitRequested commit](https://github.com/tauri-apps/tauri/commit/892c63a0538f8d62680dce5848657128ad6b7af3); [tauri#13511](https://github.com/tauri-apps/tauri/issues/13511)
- **Notification:** `tauri-plugin-notification` exists as a first-party plugin. It was not researched in depth here (see Gaps).

### Inferences
- Push-to-talk: choose a chord such as `Ctrl+Space` or `Alt+`` `, or an F-key (F13 to F24 are ideal if the user has macro keys). Start capture on `Pressed` and stop on `Released`. Measure the duration between them to tell a *tap* (toggle mode) from a *hold* (PTT). Keep the handler to a channel send (`tx.send(PttEvent::Down)`) so it never re-enters the plugin or creates windows.
- A bare-modifier or mouse-button PTT (Discord-style "hold Right Alt") would need a low-level hook (`SetWindowsHookEx(WH_KEYBOARD_LL)` through windows-rs, or crates such as `rdev`/`willhook`). LL hooks must return within the `LowLevelHooksTimeout`, so run them on a dedicated thread with its own message loop. They also do not see input to elevated windows unless KIVO is elevated. This is general Win32 knowledge and was not sourced here.
- Autostart: pass a `--autostart`/`--minimized` arg so launch-at-login starts tray-only with no window. On every launch, check `is_enabled()` against the user preference and re-enable if needed, to mitigate #771.
- Plugin order: single-instance first, then autostart, global-shortcut, notification, and the rest.

### Gaps
- Windows specifics of `tauri-plugin-notification` (action buttons, AUMID requirement for unpackaged apps, behavior under Focus Assist) were not researched.
- No confirmation was found whether the #3590 deadlock is fixed in the latest global-shortcut release.
- The root cause of autostart #771 (for example a `StartupApproved\Run` disable flag written by Task Manager) was not confirmed.

## 8. Alternatives if WebView2 falls short: native Rust overlay and hybrid

### Takeaway
A hybrid is the pragmatic fallback: keep Tauri/WebView2 for the main UI, settings and conversation card, and render only the ambient glow (and optionally the pill) natively. The native renderer can be (a) a raw windows-rs window with `WS_EX_LAYERED|WS_EX_TRANSPARENT|WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW|WS_EX_TOPMOST` driven by DirectComposition/Direct2D, or (b) winit + wgpu (or egui on top of it) with a transparent surface. Both run in the same Rust process as the Tauri runtime.

### Cited Findings
- Win32 click-through is done with `WS_EX_TRANSPARENT` (usually with `WS_EX_LAYERED`), and opacity via `UpdateLayeredWindow`/`SetLayeredWindowAttributes`. — [Microsoft Learn: Extended Window Styles](https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles); [Microsoft Q&A: DirectComposition click-through in transparent areas](https://learn.microsoft.com/en-au/answers/questions/2153247/directcomposition-click-through-in-transparent-are)
- winit transparency on Windows has a history of regressions, including a PR restoring fully transparent windows ([winit#1815](https://github.com/rust-windowing/winit/pull/1815)), transparent borderless issues ([winit#851](https://github.com/rust-windowing/winit/issues/851)), and the transparent example not working ([winit#2502](https://github.com/rust-windowing/winit/issues/2502)). A click-through feature request is at [winit#1434](https://github.com/rust-windowing/winit/issues/1434). Current winit has `Window::set_cursor_hittest(false)` for click-through (from the winit API, not re-verified here).
- A working Rust example is ZeroIn-Crosshair: winit + wgpu with premultiplied-alpha output and a softbuffer fallback, as a click-through overlay on Windows and Linux. — [ZeroIn-Crosshair](https://github.com/mrdhnto/ZeroIn-Crosshair)
- Microsoft itself advises against WebView2 for trivial UI ("Don't use WebView2 for initial UI … use lightweight XAML or Win32 screens"). — [Microsoft Learn: WebView2 performance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance)

### Inferences
- Integration: tao (Tauri's windowing layer) and winit each want to own the event loop. Rather than running two loops, create the native overlay HWND with windows-rs on Tauri's main thread (`app.run_on_main_thread`), since Tauri already pumps Win32 messages there. Render with DirectComposition + Direct2D, or wgpu with a `CompositeAlphaMode::PreMultiplied` surface from `raw-window-handle`. Drive the glow intensity from the Rust audio pipeline directly (RMS level to a shader uniform) with no IPC, which also removes WebView IPC latency for voice-reactive visuals.
- Suggested phasing for KIVO:
  1. Build the pill and card in Tauri/WebView2 (non-focusable, transparent, preloaded).
  2. Prototype the edge glow as four thin WebView2 strips per monitor.
  3. If GPU or battery measurements are bad, replace only the glow with native DirectComposition/wgpu.
- egui is a reasonable choice for a native *interactive* pill (via eframe/egui-wgpu), but it would duplicate the design system, so use it only if WebView2 latency or focus problems prove unfixable.

### Gaps
- No published blog or case study was found of a production Tauri app shipping a native DirectComposition overlay alongside WebView2 windows. The integration path above is an inference.
- No benchmarks were found comparing WebView2 vs native overlay GPU and power cost.
