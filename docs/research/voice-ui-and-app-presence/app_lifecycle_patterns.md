# App Presence and Lifecycle Patterns for Always-On Assistant / Launcher / Voice Apps on Windows (for KIVO)

Research date: 2026-09-21. Scope: how comparable Windows apps behave from launch onward (first vs subsequent launch, tray presence, close behavior, autostart, invocation, invoked UI), Microsoft guidance, user complaints, onboarding, "running in background" communication, and single-instance behavior. Recommendations for KIVO (Rust runtime + Tauri v2 UI) are in the Inferences subsections.

## 1. How comparable apps behave (Copilot, ChatGPT, Claude, PowerToys, Raycast, Flow Launcher, Wispr Flow, Superwhisper, Voice Access, Cortana)

### Takeaway
The dominant 2025-2026 pattern is a two-surface model: a full "main window" (hub/chat/settings) plus a small, always-on-top "quick" surface (floating voice pill, companion bar, launcher box) summoned by a global hotkey, wake word, or tray click, with a notification-area (tray) icon as the persistent anchor. Microsoft's own Copilot app now exposes explicit user settings for both "Auto start on login" and "On close, keep the app running", which is the clearest first-party precedent for KIVO.

### Cited Findings

**Microsoft Copilot app (Windows 11, current)**
- "Hey, Copilot" wake word is opt-in: "off by default", enabled in Copilot Settings under Voice mode; rolled out starting version 1.25051.10.0 via Microsoft Store — [Windows Insider Blog, 2025-05-14](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
- On wake word, "the Copilot Voice Floating UI appear[s] on the bottom of your screen" with "a small chime, or a voice greeting"; end by tapping X on the floating call UI or by staying silent a few seconds (auto-ends) — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
- Saying "Goodbye" also ends the session — [Microsoft Support: Copilot Wake Word](https://support.microsoft.com/en-us/microsoft-copilot/copilot-wake-word-hey-copilot)
- Requirements: PC powered on and unlocked, and "the Copilot app is running (open, minimized, or running in the background)" — i.e., wake word depends on a resident background process — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/); [Microsoft Support](https://support.microsoft.com/en-us/microsoft-copilot/copilot-wake-word-hey-copilot)
- Privacy model: on-device wake-word spotter with a 10-second in-memory audio buffer that is "never recorded or stored locally"; audio goes to cloud only after detection — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
- Windows shows Copilot's microphone use in the system tray when "Hey Copilot" is enabled — [Microsoft Support](https://support.microsoft.com/en-us/microsoft-copilot/copilot-wake-word-hey-copilot)
- Alt+Space was given to Copilot's floating "quick view", which stays on top until dismissed or toggled with the same shortcut; quick view also reachable from a Copilot tray icon — [The Register, 2024-12-11](https://www.theregister.com/2024/12/11/microsoft_copilot_keyboard_shortcut/); [Windows Latest, 2025-01-09](https://www.windowslatest.com/2025/01/09/microsoft-really-wants-you-to-use-altspace-to-open-copilot-anytime-on-windows-11/)
- Alt+Space behavior is configurable (Full Window / Quick View / None) under Account > Settings > Copilot Keyboard Shortcuts; the hardware Copilot key opens the main window — [ElevenForum](https://www.elevenforum.com/t/change-how-copilot-opens-with-alt-spacebar-shortcut-in-windows-11.34184/); [Windows Latest](https://www.windowslatest.com/2025/01/09/microsoft-really-wants-you-to-use-altspace-to-open-copilot-anytime-on-windows-11/)
- "Auto start on login" setting added in app version 1.25014.121.0: starts Copilot at sign-in, running in the background with a tray icon — [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-auto-start-copilot-on-login-in-windows-11.19626/); press coverage [Windows Report](https://windowsreport.com/copilot-will-now-auto-start-on-windows-11-login-thanks-to-a-new-setting/), [Windows Latest](https://www.windowslatest.com/2025/02/13/windows-11s-microsoft-copilot-now-auto-runs-in-the-background-but-its-still-a-web-crap/)
- Press reaction to autostart was negative in tone (e.g., TechRadar headline: "the option nobody was crying out for") — [TechRadar](https://www.techradar.com/computing/windows/windows-11-is-set-to-offer-the-option-nobody-was-crying-out-for-having-copilot-automatically-load-in-the-background-when-the-pc-boots). Note: I could not verify (ghacks returned 403) whether autostart shipped enabled by default.
- "On close, keep the app running" setting (Copilot app version 146+): when on, closing leaves Copilot in background with tray icon to reopen or quit; when off, close fully quits — [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-keep-copilot-app-running-on-close-in-windows-11.45683/); [Geek Rewind](https://geekrewind.com/how-to-enable-or-disable-keep-copilot-app-running-on-close-in-windows-11/)

**ChatGPT desktop app for Windows (OpenAI)**
- Companion window opened with Alt+Space "when you have the ChatGPT app open"; can ask anything, upload files, generate images, start new chat; remembers last position and resets to bottom-center; hotkey changeable under Settings > App > Companion window hotkey; won't work if another app already registered the shortcut — [OpenAI Help Center: Using the ChatGPT Windows app](https://help.openai.com/en/articles/9982051) (page returned 403 to direct fetch; details taken from search snippet of that page) ; corroborated by [WebNots](https://www.webnots.com/10-tips-to-use-chatgpt-windows-app/)
- A native "launch at Windows sign-in" setting is an open feature request on OpenAI's GitHub, suggesting it was not a first-class setting at time of the issue — [openai/codex issue #43181](https://github.com/openai/codex/issues/43181) (low confidence on current state)

**Claude desktop app (Anthropic)**
- Quick entry: "a small Claude window that opens on top of whatever app you're in and stays in front as you switch apps"; Mac: double-tap Option, Caps Lock to speak; Windows/Linux: a customizable shortcut set in Settings; voice/screenshot in quick entry are Mac-only per the tutorial — [Claude Academy](https://academy.claude.com/tutorials/navigating-the-claude-desktop-app); [Claude Help Center (Mac quick entry)](https://support.claude.com/en/articles/12626668-use-quick-entry-with-claude-desktop-on-mac)
- Windows: Claude icon lives in the system tray; tray/menu-bar visibility toggled in Desktop Settings — [Claude Academy](https://academy.claude.com/tutorials/navigating-the-claude-desktop-app)
- Cautionary bug report: after closing the main window, relaunching from Start did not show the main window; a tray icon appeared silently; tray click opened only a (non-functional) quick entry bar; recovery required killing Claude.exe. Closed as "not planned/invalid" because filed in the wrong repo — [anthropics/claude-code #39336](https://github.com/anthropics/claude-code/issues/39336)
- Feature requests: users want a toggle so clicking the taskbar icon opens the full app instead of quick entry — [#22420](https://github.com/anthropics/claude-code/issues/22420); [#74908](https://github.com/anthropics/claude-code/issues/74908)

**PowerToys Run / Command Palette (Microsoft)**
- Command Palette (PowerToys v0.90+) is the successor to PowerToys Run; default activation Win+Alt+Space; requires PowerToys enabled and running in background; has an optional tray icon ("Show system tray icon") — [Microsoft Learn: Command Palette overview](https://learn.microsoft.com/en-us/windows/powertoys/command-palette/overview); [Windows Latest](https://www.windowslatest.com/2025/04/01/powertoys-brings-command-palette-to-windows-11-and-its-better-than-built-in-search/)
- PowerToys Run (legacy) default was Alt+Space — [Microsoft Learn: PowerToys Run](https://learn.microsoft.com/en-us/windows/powertoys/run)

**Raycast for Windows**
- Default hotkey Alt+Space; General settings include hotkey, launch-at-login, and system tray option; cannot bind raw Win key alone — [Raycast Manual: Settings](https://manual.raycast.com/settings); [Windows Forum](https://windowsforum.com/threads/raycast-on-windows-a-keyboard-first-command-bar-for-faster-workflow.387394/)

**Flow Launcher (open source)**
- Alt+Space default; settings for start on system startup, hide tray icon, and "Hide Flow Launcher when focus is lost"; settings reached via tray right-click — [MakeUseOf](https://www.makeuseof.com/windows-flow-launcher-guide/); [XDA](https://www.xda-developers.com/change-5-flow-launcher-settings-max-productivity/)
- Real-world pain points: hotkey registration conflicts (Alt+Space) and focus not returning to the original window after cancel — [Flow Launcher #2622](https://github.com/Flow-Launcher/Flow.Launcher/issues/2622); [#4615](https://github.com/Flow-Launcher/Flow.Launcher/issues/4615)

**Wispr Flow (Windows)**
- Two surfaces: the "Hub" main window (history, personalization, settings) and a floating "Flow Bar" for dictation in other apps; tray icon gives quick access — [Wispr Flow Help: Navigating the app](https://docs.wisprflow.ai/articles/5096240724-navigating-the-wispr-flow-app-desktop-ios-and-android)
- On Windows with Launch at login enabled, Flow "starts hidden in the tray and never opens the main window on its own"; turning off launch at login restores the hub opening on every launch — per search snippet of Wispr Flow help docs ([help article list](https://docs.wisprflow.ai/articles/7492559320-quit-and-relaunch-wispr-flow)); not re-verified on a fetched page
- Default hotkey Ctrl+Shift+Space (per snippet); Flow Bar can be hidden "for 1 hour" or turned off in Settings > System > Show Flow Bar — [Wispr Flow Help: Flow Bar troubleshooting](https://docs.wisprflow.ai/articles/5002934560-why-is-the-wispr-bar-is-not-appearing-or-disappearing); [Shortcuts](https://docs.wisprflow.ai/articles/2612050838-supported-unsupported-keyboard-hotkey-shortcuts)
- Dedicated help article on "Quit and relaunch" implies quitting is a tray/menu action distinct from closing — [Wispr Flow Help](https://docs.wisprflow.ai/articles/7492559320-quit-and-relaunch-wispr-flow)

**Superwhisper (Windows, launched ~Dec 2025)**
- Windows 10+ x64/ARM64, runs from the system tray, Ctrl+Space push-to-talk default — [AlternativeTo news](https://alternativeto.net/news/2025/12/ai-powered-transcription-tool-superwhisper-has-officially-launched-a-windows-version)
- Recording window has main and mini views; mini window shows waveform, hover reveals stop button, right-click menu — [Superwhisper docs: Recording Window](https://superwhisper.com/docs/get-started/interface-rec-window)

**Windows Voice Access (built-in accessibility)**
- Optional "Start voice access after you sign in to your PC"; UI is a bar docked at the top of the screen showing mic state, recognized commands, and progress; sleep/listen states with "Voice access wake up" — [Microsoft Support: Set up voice access](https://support.microsoft.com/en-us/accessibility/windows/voice-access/set-up-voice-access); [Get started](https://support.microsoft.com/en-us/accessibility/windows/voice-access/get-started-with-voice-access)

**Cortana (historical)**
- "Hey Cortana" was temporarily removed in 2020 Cortana beta, later restored — [Voicebot.ai](https://voicebot.ai/2020/05/05/microsoft-cortana-temporarily-loses-wake-word/); [MSPowerUser](https://mspoweruser.com/hey-cortana-working-once-again-says-microsoft/)
- Standalone Cortana app in Windows deprecated/retired in 2023, shortly after Windows Copilot was announced; opening it shows a deprecation message — [AlternativeTo, 2023-06](https://alternativeto.net/news/2023/6/rip-cortana-microsoft-announces-retirement-of-the-voice-assistant-on-windows-10-and-windows-11-)

### Inferences
- Hotkeys cluster on a small, contested set: Alt+Space (Copilot, ChatGPT, Raycast, Flow Launcher, legacy PowerToys Run), Win+Alt+Space (Command Palette), Ctrl+Space / Ctrl+Shift+Space (Superwhisper, Wispr Flow, Claude common choice). KIVO should not assume Alt+Space; pick a less contested default, detect registration failure, and prompt the user to rebind.
- Voice-first apps (Copilot Voice, Wispr, Superwhisper, Voice Access) all show a small floating/docked "listening" surface rather than the main window when invoked. KIVO's invoked UI should be a compact always-on-top voice pill/orb (bottom-center like Copilot/ChatGPT or near the caret like dictation tools), with a clear path to expand into the main window.
- A wake word requires a resident process; Copilot's wording "open, minimized, or running in the background" is a good model for explaining this to users.
- The Claude bug (#39336) shows the main failure mode of the two-surface model: relaunch/tray click must always have a reliable path to the full main window.

### Gaps
- Could not fetch OpenAI help page directly (403); ChatGPT Windows close-to-tray and autostart specifics unverified.
- Could not confirm whether Copilot's "Auto start on login" shipped on-by-default (ghacks 403).
- Claude desktop Windows close-button behavior and launch-at-login defaults not documented in fetched official sources.
- Raycast Windows: close/first-run specifics not found beyond settings list.

## 2. Microsoft guidance on notification area icons, startup apps, and close-to-tray; user expectations and complaints

### Takeaway
Microsoft's (Windows 7-era, still published) guidance says the notification area is for status/notifications and for features without desktop presence, users must stay in control (hide icon, Exit, suspend), and minimizing to tray should be opt-in and tied to Minimize, not Close. In practice, first-party Microsoft apps (Teams, Copilot) close-to-tray by default with an explicit toggle, and users repeatedly complain that X not quitting is surprising.

### Cited Findings
- The design guide "was created for Windows 7 and has not been updated"; much still applies in principle — [Microsoft Learn: Notification Area](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Notification area is for "notifications and status, as well as an access point for system- and program-related features that have no presence on the desktop"; it "isn't intended for quick program or command access" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Most icons are hidden in overflow by default and "there is no way for your program to perform this promotion automatically" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- "Background task status and access" is a valid pattern: background processes need icons when they have no desktop presence — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Minimizing to the notification area "is no longer recommended"; if offered, only when single-instance, long-running, icon shows status, icon can be a notification source, and it is opt-in; "Use the Minimize button on the application's title bar, not the Close button" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Interaction rules: left click must display something (flyout/window) or icon "appear[s] unresponsive"; double-click = default command; right-click = context menu with default bolded; show windows near the notification area — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Context menu order: Open (bold, default) / Run; separator; Suspend/Resume; opt-in notifications; "Display icon in notification area"; separator; Options; Exit. "All background tasks must have a Suspend/Resume command." Exit quits for the current session — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Icons: provide 16/20/24 px; don't flash; no long-running animations; use bottom-right status overlays (warning/error/disabled/offline); avoid pure red/yellow/green in base icon; tooltip format "Program name - Status summary" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Startup: StartupTask API for packaged apps; UWP apps show a consent dialog via RequestEnableAsync; packaged desktop apps get no dialog and may be enabled in manifest; states include DisabledByUser (user must re-enable manually in Settings) and DisabledByPolicy (don't re-request) — [Microsoft Learn: StartupTask](https://learn.microsoft.com/en-us/uwp/api/windows.applicationmodel.startuptask?view=winrt-26100); [Windows Developer Blog 2017](https://blogs.windows.com/windowsdeveloper/2017/08/01/configure-app-start-log/)
- Teams exposes "On close, keep the application running"; Discord exposes "Minimize to Tray"/"Close to tray" — [Microsoft Q&A](https://learn.microsoft.com/en-sg/answers/questions/7312/cant-close-teams-window); [Discord feedback](https://support.discord.com/hc/en-us/community/posts/360029987811-Add-an-option-to-fully-close-Discord-on-x)
- User complaint: close-to-tray setting is "way too hidden"; "Almost universally X is expected to close an application"; inconsistency where Alt+F4 still exits while X hides — [Discord community feedback](https://support.discord.com/hc/en-us/community/posts/360029987811-Add-an-option-to-fully-close-Discord-on-x); [Discord Alt+F4 post](https://support.discord.com/hc/en-us/community/posts/360040720652-Also-close-to-tray-when-Alt-F4-is-pressed-and-the-relevant-setting-is-enabled)
- Windows 11 tray overflow complaints: icons hidden behind chevron by default; users must enable each in Settings > Personalization > Taskbar > Other system tray icons; Windows resets some icons to hidden (often after app updates change the exe path) — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/3860271/how-do-i-make-windows-11-system-tray-icons-stop-go); [Microsoft Tech Community](https://techcommunity.microsoft.com/discussions/windowsinsiderprogram/windows-11-occasionally-resets-some-tray-icons-to-be-hidden/3887241); [ElevenForum](https://www.elevenforum.com/t/unhidden-system-tray-icons-disappearing-after-every-reboot.18813/)
- Autostart press reaction to Copilot was negative ("nobody was crying out for") — [TechRadar](https://www.techradar.com/computing/windows/windows-11-is-set-to-offer-the-option-nobody-was-crying-out-for-having-copilot-automatically-load-in-the-background-when-the-pc-boots)

### Inferences
- There is tension between the (old) official guideline (close should close; tray minimize opt-in) and modern practice (Copilot/Teams/Discord keep running on close by default with a toggle). For a wake-word assistant, "keep running on close" is functionally required for the core feature, so KIVO should default to it but make it explicit, discoverable, and reversible (Copilot's exact phrasing "On close, keep the app running" is a good label).
- Because the tray icon will likely sit in overflow, KIVO cannot rely on the tray as the primary "it's still running" cue; the hotkey, wake word, and a one-time notification must carry that load.
- KIVO's tray menu should follow the MS order, with a bolded "Open KIVO", "Pause listening / Resume listening" (maps to Suspend/Resume and doubles as a privacy control), Settings, and "Quit KIVO". Use icon overlays for mic-muted/offline/error states rather than animations.
- Keep close behavior consistent across X, Alt+F4, and taskbar "Close window" to avoid the Discord inconsistency.

### Gaps
- No current (Windows 11-era, Fluent) Microsoft Learn page found that supersedes the Windows 7 notification area guide; I found none, so the Win7 guide remains the most specific official source.
- No Microsoft Store policy text found explicitly requiring consent for autostart of unpackaged Win32 apps.

## 3. First-run / onboarding flow

### Takeaway
Comparable apps open the full main window on first run (sign-in, permissions, shortcut setup), and only later start silently in the tray; Wispr Flow explicitly starts hidden in the tray only when launched at login, while manual launches open the hub.

### Cited Findings
- Wispr Flow: with Launch at login enabled, starts hidden in tray and doesn't open main window on its own; manual launch/disabled autostart opens the hub — [Wispr Flow Help (search snippet)](https://docs.wisprflow.ai/articles/7492559320-quit-and-relaunch-wispr-flow)
- Wispr Flow has a dedicated first-dictation setup guide (mic, hotkey, permissions) — [Wispr Flow Help: Setup guide](https://docs.wisprflow.ai/articles/3152211871-setup-guide)
- Wispr Flow troubleshooting exists for "app doesn't start up after signing in and clicking 'Open Wispr Flow'" — sign-in handoff is a known fragile point — [Wispr Flow Help](https://docs.wisprflow.ai/articles/2809372297-what-to-do-if-the-app-doesn-t-start-up-after-signing-in-and-clicking-open-wispr-flow)
- Voice Access: first run downloads speech model and offers an interactive guide; autostart is a separate opt-in checkbox — [Microsoft Support: Set up voice access](https://support.microsoft.com/en-us/accessibility/windows/voice-access/set-up-voice-access)
- Copilot's wake word and Voice Access autostart are both off by default (opt-in) — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/); [Microsoft Support](https://support.microsoft.com/en-us/accessibility/windows/voice-access/set-up-voice-access)
- MS guidance: "Don't turn everything on by default and expect users to turn features off" — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)

### Inferences
- Recommended KIVO first run: open main window -> welcome -> microphone permission + mic test -> choose/confirm global hotkey (with conflict detection) -> opt-in wake word (explain on-device spotting, as Copilot does) -> opt-in "Start KIVO when I sign in" -> explain close behavior ("KIVO keeps running in the notification area so you can call it anytime") -> try-it moment (invoke via hotkey) -> end.
- Launch source should determine initial UI: autostart launch (e.g., pass a `--autostart`/`--hidden` arg in the startup registration) -> start hidden in tray; manual launch (Start menu, desktop shortcut, relaunch) -> show main window.

### Gaps
- No official documentation found describing ChatGPT's, Claude's, or Raycast's Windows first-run screens in detail.

## 4. Communicating "running in background"

### Takeaway
Copilot and Teams make background residency a named setting and keep a tray icon; the Windows OS itself surfaces microphone-in-use in the tray. I found no official source documenting a one-time "still running" toast in these specific apps, though it's a common pattern.

### Cited Findings
- Copilot: with "On close, keep the app running" on, the tray icon remains "to reopen or quit Copilot from" — [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-keep-copilot-app-running-on-close-in-windows-11.45683/)
- Windows shows microphone-in-use via tray when "Hey Copilot" is enabled (persistent privacy indicator) — [Microsoft Support](https://support.microsoft.com/en-us/microsoft-copilot/copilot-wake-word-hey-copilot)
- MS guidance: a hidden icon's notifications are still temporarily promoted by Windows, so a notification can reveal an overflowed icon — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Failure mode when there's no GUI path back: apps with minimize_on_close can end with the app running and unreachable, multiple instances — [search result summary; danielgjackson/minimize-to-tray](https://github.com/danielgjackson/minimize-to-tray) (low confidence on specific attribution)

### Inferences
- KIVO should show a one-time toast on the first close-to-tray: "KIVO is still running. Say 'Hey KIVO' or press <hotkey> anytime. Change this in Settings." with actions "Quit KIVO" / "Settings". Optionally, on first close show a small dialog with "Keep running in background" vs "Quit" plus "Don't ask again", persisting the choice.
- The persistent mic-in-use tray indicator Windows shows while a wake word listens is a trust cue; KIVO should reinforce it with its own icon overlay state (listening / paused / muted).

### Gaps
- No primary source found documenting the exact first-close toast text used by Copilot, ChatGPT, Claude, or Wispr Flow.

## 5. Single-instance behavior and relaunch while running

### Takeaway
Always-on apps must be single-instance, and a relaunch should activate the existing instance and bring its main window forward, including when the window is hidden; Tauri v2 provides a plugin for this, with a known focus pitfall for hidden windows.

### Cited Findings
- MS guidance: tray-minimizing apps must be single-instance — [Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification)
- Tauri v2 Single Instance plugin: must be registered first; its init callback runs when a second instance is launched (and closed by the plugin) and receives args/cwd, where you can focus the existing window — [Tauri docs: Single Instance](https://v2.tauri.app/plugin/single-instance/)
- Close-to-tray in Tauri: handle `WindowEvent::CloseRequested`, call `window.hide()` then `api.prevent_close()` — [tauri-apps discussion #2684](https://github.com/tauri-apps/tauri/discussions/2684)
- Known issue: `set_focus()` from the single-instance callback may fail when the window is hidden (need `show()` + `unminimize()` before `set_focus()`) — [tauri-apps/tauri #12936](https://github.com/tauri-apps/tauri/issues/12936)
- Real-world bug: with close-to-tray on, relaunching created multiple instances instead of restoring the hidden window — [AOSSIE-Org/PictoPy #1549](https://github.com/AOSSIE-Org/PictoPy/issues/1549)
- Claude Desktop bug: relaunch after close did not restore main window, only a broken quick entry — [anthropics/claude-code #39336](https://github.com/anthropics/claude-code/issues/39336)

### Inferences
- KIVO relaunch contract: second launch forwards args to the running instance -> instance calls show(), unminimize(), set_focus() on the main window (not the quick/voice surface) -> exits. Deep-link/file args are routed through the same callback.
- Since KIVO has a separate Rust runtime plus Tauri UI, enforce single instance at the runtime level too (named mutex / IPC pipe), so the UI process and the voice runtime can't duplicate (which would mean two wake-word listeners on one mic).
- Recommended overall KIVO lifecycle (synthesized):
  1. First launch: main window + onboarding.
  2. Manual subsequent launch: main window shown (or focused if already running).
  3. Autostart at login (opt-in, via registry Run key or StartupTask with a hidden flag): no window; tray icon; hotkey + wake word active.
  4. Close (X / Alt+F4): hide to tray by default (setting "On close, keep KIVO running"), one-time toast on first time.
  5. Invocation: hotkey or wake word -> compact always-on-top voice pill (chime + listening state), dismiss on silence/"goodbye"/Esc/X; expand button to open main window. Tray left-click -> main window (not a quick bar, avoiding Claude's #22420 complaint); right-click -> MS-style menu with Pause listening and Quit.
  6. Quit: only from tray menu / in-app menu; fully stops runtime and mic.

### Gaps
- No official documentation found on how Copilot, ChatGPT, or Wispr Flow handle a second launch while running (focus behavior); inferred as standard single-instance activation but unverified.
