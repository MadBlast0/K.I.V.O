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

- [ ] **CAP-01** · M1 · Capability model in the runtime: every §1 capability with its default; a disabled capability's tools are **not registered** for any brain, routine, agent, MCP exposure or remote client (§1, §2)
- [ ] **CAP-02** · M1 · A request that needs a disabled capability gets a plain answer ("Screen awareness is off. Turn it on?") with a one-tap link; toggles never turn on automatically (§2)
- [ ] **CAP-03** · M1 · Every toggle change is written to the audit log (§2)
- [ ] **CAP-04** · M4 · Permissions → Capabilities page: each row with toggle, description, "used … ago" and Local / Cloud / Costly / Sensitive badges, plus the per-capability controls from §1 (§1)
- [ ] **CAP-05** · M4 · Presets Minimal / Balanced (default) / Power user / Custom (§2)
- [ ] **CAP-06** · M4 · Active-use indicators in the tray and the Island: screen (eye), input control (hand), shell (terminal) (§2)
- [ ] **CAP-07** · M4 · Per-app allow and block lists for UIA, screen awareness and computer use; the default block list covers password managers, banking apps and Windows Security (§2)
- [ ] **CAP-08** · M4 · Screen awareness on request: capture once → UIA excerpt + local OCR first → vision brain only if needed and allowed, with "Sent a screenshot of … to …"; screenshots held in memory only (§3)
- [ ] **CAP-09** · M8 · Computer use: `ComputerUseProvider` trait (Anthropic, OpenAI, Gemini); used only when no semantic method exists, the capability is on and the app is allowed; estimated cost shown before starting (§4)
- [ ] **CAP-10** · M8 · Computer-use loop screenshot → proposed action → `authorize()` → execute → verify, with max 25 steps, max $0.50, 2-minute timeout (all configurable) (§4)
- [ ] **CAP-11** · M8 · Watch mode (default for the first 10 tasks): overlay marker at the target, approval by click, Enter, or voice for Low risk (§4)
- [ ] **CAP-12** · M8 · Computer-use experience: Island controller live activity (step, cost, Pause, Stop), pulsing target highlight, black callout with Allow / Skip in watch mode, optional KIVO cursor, persistent "KIVO is controlling your screen — Stop (Ctrl+Alt+Shift+Esc)" banner and screen frame Off / Subtle / Full (§2, §4.1)
- [ ] **CAP-13** · M8 · Computer-use options page (Visibility, Control incl. "Pause when I use the mouse" and speed, Limits, Apps) (§4.1)
