# Capabilities and privacy controls ("what KIVO may do")

Status: Draft v1, 2026-09-21. Owner requirement: iOS-style controls where the user can **enable
or disable every capability**, including the expensive "AI sees and uses my screen". This sits
*above* the permission engine in [SECURITY.md](SECURITY.md): a disabled capability means its
tools are **not registered** for any brain, routine, agent or remote client.

## 1. Capability catalogue

Control Center → Permissions → **Capabilities**. Each row has an on/off toggle, a short description, a status
("used 3 min ago"), and a **cost/privacy badge**: Local, Cloud, Costly, Sensitive.

| Capability | Default | Badge | Controls when on |
|---|---|---|---|
| Microphone listening (wake word) | Off until onboarding | Local | Per wake word; pause schedule |
| Push-to-talk | On | Local | Hotkey |
| Speak responses (TTS) | On | Local/Cloud | Engine; quiet hours |
| Apps & windows | On | Local | Blocked apps list |
| System controls (volume, media, lock, sleep) | On | Local | — |
| Power actions (restart/shutdown) | On (always confirm) | Sensitive | — |
| Files: read | On (user folders) | Sensitive | Allowed folders; private folders |
| Files: modify | On (confirm) | Sensitive | Allowed folders |
| Clipboard | Off | Sensitive | Read / write separately |
| Browser: open links | On | Local | — |
| Browser: read & act on pages (extension) | Off until the extension is installed | Sensitive | Per-site allow/block |
| Browser: autonomous browsing (KIVO profile) | Off | Cloud | Allowed domains |
| UI Automation (read & click app controls) | On | Local | Per-app allow/block |
| **Screen awareness** (on-request screenshot + OCR/UIA "what's on my screen") | Off | Sensitive | Local OCR only / allow cloud vision |
| **Computer use** (AI operates mouse and keyboard from screenshots) | **Off** | **Costly · Cloud · Sensitive** | Per-app allow list, max steps, max cost per task, "watch mode" (confirm each action) |
| Shell / terminal commands | Off | Sensitive | Read-only only / full with confirmation |
| Background tasks & watchers | On | Local | Max concurrent |
| Routines | On | Local | Per routine |
| Memory (remember things) | On (explicit only) | Sensitive | Proposals on/off |
| Cloud AI brains | On if configured | Cloud · Costly | Per provider; privacy mode |
| Realtime voice conversation | Off | Cloud · Costly | Provider; auto-end after silence |
| CLI coding agents | Off until configured | Cloud · Costly | Per agent; allowed project folders |
| MCP servers | Off until added | Varies | Per server, per tool |
| Integrations (Google, Microsoft, Spotify, GitHub, …) | Off until connected | Cloud · Sensitive | Per integration and scope |
| Speaker recognition | Off until enrollment | Sensitive (biometric) | Mode; delete data |
| Remote access (phone) | Off | Sensitive | Paired devices |
| Notifications & proactive speech | On (quiet) | Local | See [UX.md §7](UX.md) |

## 2. Behavior rules

- **A disabled capability is removed, not just denied.** Its tools vanish from brain tool lists,
  the fast path, routines and MCP exposure. When a request needs it, KIVO says so plainly: "Screen
  awareness is off. Turn it on?" A one-tap link opens the toggle. The toggle **never turns on
  automatically**.
- **Presets** apply sets of toggles:
  - **Minimal:** voice + apps + system.
  - **Balanced** (default).
  - **Power user:** adds shell, screen awareness and clipboard.
  - **Custom.**
- **Active-use indicators:**
  - While a sensitive capability is in use, the tray icon and the pill show a small indicator:
    screen (eye), input control (hand), shell (terminal).
  - Computer use also shows a **persistent banner, "KIVO is controlling your screen — Stop
    (Ctrl+Alt+Shift+Esc)"**, and a colored screen border.
- **Per-app scopes:** UIA, screen awareness and computer use honour allow and block lists. The
  default block list covers password managers, banking apps and Windows Security.
- **Audit:** every toggle change is written to the audit log.

## 3. Screen awareness (on request)

"What's on my screen?", "Explain this error", "Read this to me":

1. The active window (or a chosen region) is captured once.
2. The **UIA tree excerpt** and **local OCR** (Windows.Media.Ocr) run first. This is local and
   free.
3. If the question needs visual understanding and the privacy mode allows it, the screenshot goes
   to a vision-capable brain. The card says "Sent a screenshot of VS Code to Claude".
4. Screenshots are held in memory only, never saved (unless debug capture is explicitly on).

## 4. Computer use (opt-in, last resort)

- **When it's used:** only when the capability ladder has no semantic method, **and** the
  capability is on, **and** the app is allowed.
- **Loop:** screenshot → the vision brain proposes an action → the permission engine checks it →
  execute → verify. Limits (all user-configurable):
  - max 25 steps per task;
  - max estimated $0.50 per task;
  - a 2-minute timeout.
- **Watch mode** (default for the first 10 tasks): every proposed action appears as an overlay
  marker at the target, and the user approves it with a click, Enter, or voice for Low risk.
- **Engines:** Anthropic computer-use tool, OpenAI computer use and Gemini computer use, behind a
  `ComputerUseProvider` trait. The estimated cost is shown before the task starts.
- **Always true:** typing into password fields is blocked, the emergency stop works, and steps are
  audited.

### 4.1 Computer-use experience and options (owner request, 2026-09-21)

**While it's working:**

- The Island becomes a **controller live activity**: "Controlling Notes · step 4/25 · ≈ $0.08",
  with **Pause** and **Stop**.
- A **target highlight** (a pulsing ring) marks the element it's about to act on.
- A black **callout** says what it will do ("Click *Export*"). In watch mode the callout has
  **Allow / Skip**.
- An optional second **KIVO cursor** shows where it acts, without taking over the user's pointer
  until the action runs.

**Options** (Capabilities → Computer use → Options):

| Group | Options |
|---|---|
| Visibility | Screen frame: Off / Subtle (default) / Full · Show KIVO's cursor · Highlight the target · Island controls |
| Control | Approve each step: Always / First 10 tasks (default) / Never · Pause when I use the mouse (default on) · Speed: Careful / Normal / Fast |
| Limits | Max steps per task (25) · Max cost per task ($0.50) · Time limit (2 min) |
| Apps | Allowed apps list · Never-touch list (password managers, banking, Windows Security by default) |

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [x] **CAP-01** · M1 · Capability model in the runtime: every §1 capability with its default; a disabled capability's tools are **not registered** for any brain, routine, agent, MCP exposure or remote client (§1, §2) → done: `kivo_core::Capability` (all 27 §1 capabilities with their defaults) in the settings; the tool registry only hands out tools whose capability is on, so a disabled one's tools don't exist for any caller (brains, routines, agents, MCP and remote clients all go through the registry) · verified: `tools_of_a_disabled_capability_are_not_registered`, capability defaults test (2026-09-23)
- [x] **CAP-02** · M1 · A request that needs a disabled capability gets a plain answer ("Screen awareness is off. Turn it on?") with a one-tap link; toggles never turn on automatically (§2) → done: a request needing a disabled capability gets "<Capability> is off. Turn it on?" on the Island with a Turn on button (the overlay may only switch on that one capability); nothing turns on by itself · verified: `the_island_may_only_turn_on_the_capability_the_request_needed`, the Island turn tests, policy `CapabilityOff` test (2026-09-23)
- [x] **CAP-03** · M1 · Every toggle change is written to the audit log (§2) → done: `capabilities.set` writes an audit row (`capabilities.set`, "<Capability> = on/off") in the hash chain · verified: `capabilities_are_listed_toggled_and_audited` (2026-09-23)
- [x] **CAP-04** · M4 · Permissions → Capabilities page: each row with toggle, description, "used … ago" and Local / Cloud / Costly / Sensitive badges, plus the per-capability controls from §1 (§1) → done: Permissions → Capabilities: every capability with its toggle, description, "used … ago" (from the audit log), badges, and options for Apps (blocked apps), Files (allowed and private folders), Clipboard (read/write), Shell (read-only), UI Automation, Screen awareness (cloud vision, apps), Computer use (apps) and Browser pages (extension status and install, sites) · verified: `Permissions.test.tsx` (2026-09-23)
- [x] **CAP-05** · M4 · Presets Minimal / Balanced (default) / Power user / Custom (§2) → done: Minimal / Balanced / Power user over the everyday capabilities (consent-driven ones untouched), Custom when toggles differ; each change audited (`capabilities.preset`) · verified: `presets_set_their_toggles_and_are_recognized`, `Permissions.test.tsx` (2026-09-23)
- [x] **CAP-06** · M4 · Active-use indicators in the tray and the Island: screen (eye), input control (hand), shell (terminal) (§2) → done: while screen awareness, input control or the shell is in use, the tray badge and tooltip ("Using the screen, shell commands") and the Island's eye / hand / terminal icons show it (`StateSnapshot.in_use`) · verified: tray tests (`icon_shown`, tooltip), `the_emergency_stop_reaches_commands` (shell indicator on, then cleared), Island indicator test (2026-09-23) · note: computer use's banner and screen frame are CAP-12 (M8)
- [x] **CAP-07** · M4 · Per-app allow and block lists for UIA, screen awareness and computer use; the default block list covers password managers, banking apps and Windows Security (§2) → done: per-capability allow and block lists for UI Automation, screen awareness and computer use (`tools.*-apps`), matched by program, name or `*pattern*`; the default block list covers password managers, `*bank*` apps and Windows Security · verified: `app_scopes_block_password_managers_banking_and_windows_security_by_default`, `Permissions.test.tsx` (2026-09-23)
- [x] **CAP-08** · M4 · Screen awareness on request: capture once → UIA excerpt + local OCR first → vision brain only if needed and allowed, with "Sent a screenshot of … to …"; screenshots held in memory only (§3) → done: `screen.read` captures once, reads the UIA excerpt and local OCR, keeps nothing; `screen.look` sends one screenshot only to a vision brain allowed by the cloud-vision option and privacy mode, and the card and Activity say "Sent a screenshot of … to …" · verified: `reading_uses_the_tree_and_local_ocr_and_keeps_nothing`, `screenshots_go_only_to_a_brain_allowed_to_see` (2026-09-23)
- [x] **CAP-09** · M8 · Computer use: `ComputerUseProvider` trait (Anthropic, OpenAI, Gemini); used only when no semantic method exists, the capability is on and the app is allowed; estimated cost shown before starting (§4) → done: `ComputerUseProvider` (Anthropic computer use, OpenAI computer-use-preview, Gemini computer use); `computer.use` is the last rung — offered only when asked for in so many words, with the capability on, cloud vision allowed and the app allowed — and its High-risk card shows the most it may cost · verified: provider contract tests on mock servers, `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (the card shows $0.25 for 25 steps)
- [x] **CAP-10** · M8 · Computer-use loop screenshot → proposed action → `authorize()` → execute → verify, with max 25 steps, max $0.50, 2-minute timeout (all configurable) (§4) → done: the loop — screenshot of the app's window → the model's one action → KIVO's input tool in screen pixels → the permission engine under the task's grant (that window only) → run → next screenshot; stops at 25 steps, $0.50 or 2 minutes (all settable), on Stop, or when the user takes the mouse (pause-on-mouse) · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (steps on the fake input, the step limit), engine unit tests (coordinates, estimate, watch)
- [x] **CAP-11** · M8 · Watch mode (default for the first 10 tasks): overlay marker at the target, approval by click, Enter, or voice for Low risk (§4) → done: watch mode for the first 10 tasks (or always / never): each step waits for Allow or Skip on the Island, by click, Enter or voice, with the target highlighted; a skipped step doesn't run and the task goes on · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (Allow, Skip, Allow)
- [x] **CAP-12** · M8 · Computer-use experience: Island controller live activity (step, cost, Pause, Stop), pulsing target highlight, black callout with Allow / Skip in watch mode, optional KIVO cursor, persistent "KIVO is controlling your screen — Stop (Ctrl+Alt+Shift+Esc)" banner and screen frame Off / Subtle / Full (§2, §4.1) → done: the Island's controller live activity (step, cost, Pause, Stop), the pulsing target highlight, the black watch card with Allow / Skip, the optional KIVO cursor and the "KIVO is controlling" banner with the chosen frame (glow window) · verified: `computer_use_watches_each_step_then_runs_under_the_tasks_grant` (controller and highlight shown, gone after), Island tests, glow page test
- [x] **CAP-13** · M8 · Computer-use options page (Visibility, Control incl. "Pause when I use the mouse" and speed, Limits, Apps) (§4.1) → done: Capabilities → Computer use → Options: Visibility (frame, KIVO cursor, highlight, Island controller), Control (approve each step, pause when I use the mouse, speed), Limits (steps, cost, time) and Apps (allow and block lists) · verified: `sets computer use's visibility, control and limits (CAP-13)`
