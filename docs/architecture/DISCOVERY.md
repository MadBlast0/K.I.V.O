# Discovery, installation and data freshness

Status: Draft v1, 2026-09-21. Owner requests:

- detect brains, agents and extensions that already exist on the PC, so the user just switches
  them on;
- offer to install terminal AIs;
- a Refresh button;
- a clear plan for how often each kind of data updates.

## 1. What KIVO detects

**Rule:** detection **suggests**, and the user **enables**. Nothing found is switched on
automatically, and nothing is sent anywhere during detection. Anything that brings new tools
(MCP servers, skills, plugins) shows its contents for review before it's enabled.

### 1.1 Brains and agents

| What | How it's detected | Shown as |
|---|---|---|
| CLI agents: `claude`, `codex`, `gemini`, `copilot`, `opencode`, `goose`, `qwen`, `kimi`… | Search `PATH` and known install dirs (npm global, `%LOCALAPPDATA%\Programs`, winget/Scoop shims). Run `--version`. Check sign-in where the CLI exposes it (a status command or a credential file that exists; its contents are never read). Cross-check against the ACP registry | "Found on this PC · signed in / needs sign-in" + **Use** |
| Local model servers: Ollama, LM Studio, llama.cpp server, any OpenAI-compatible localhost | Probe known ports (`11434` `/api/tags`, `1234` `/v1/models`, `8080` `/v1/models`) and list their models | "Running on this PC · N models" + **Use** |
| Desktop AI apps: Claude Desktop, ChatGPT, Microsoft Copilot | Installed-apps index (Start menu + AppX packages) | Agents page → Desktop AI apps |

**Install for the user (with consent):** a supported CLI that isn't found appears under **Not
installed**, with an **Install** button. The flow ([plan §19](../KIVO_Project_Plan.md)):

1. Explain what gets installed and the exact command (for example `npm install -g
   @anthropic-ai/claude-code`, or a winget ID).
2. Check dependencies. Node.js is installed via winget, with consent.
3. Run the install in a visible progress sheet, with its output collapsible.
4. Verify with `--version`.
5. Start the CLI's own sign-in in a terminal or browser.
6. Test it with a tiny prompt.

Exact package names and commands live in a versioned **catalog** (§3) and are never hard-coded,
because vendors change them.

### 1.2 Extensions

| Tab | Sources KIVO scans | Result |
|---|---|---|
| **Connectors** | Signed-in CLIs that already give access, e.g. **GitHub via `gh auth status`**. Installed apps that have built-in connectors (Spotify, VS Code, Outlook desktop, Teams). Browsers, plus whether the KIVO extension is installed | "Ready to use" + a switch (no sign-in needed) |
| **MCP servers** | MCP configs other apps already use: **Claude Desktop** `%APPDATA%\Claude\claude_desktop_config.json` (the Store build uses `%LOCALAPPDATA%\Packages\Claude_*\LocalCache\Roaming\Claude\`), Claude Code (`~/.claude.json`, project `.mcp.json`), Cursor (`~/.cursor/mcp.json`), VS Code (`.vscode/mcp.json` / user settings), Codex (`~/.codex/config.toml`), Gemini CLI (`~/.gemini/settings.json`). Exact paths are verified per app version at implementation time ([MCP docs](https://modelcontextprotocol.io/docs/2026-07-28/develop/connect-local-servers), [Claude Desktop on Windows](https://exchangepedia.com/2026/04/claudetools-claude-desktop-powershell-module.html)) | "Found in Claude Desktop (3)" + **Import**. The server is copied into KIVO's own config; the original is never modified. Env-var secrets are moved into Credential Manager |
| **Plugins** | KIVO's `plugins\` folder (a dropped file shows up), plus signed update notices | "New plugin found" + **Review** |
| **Skills** | `~/.claude/skills/`, the project's `.claude/skills/` in known workspaces, other agents' skill folders as they're verified, and KIVO's `skills\` folder ([Claude Code skills](https://code.claude.com/docs/en/skills)) | "Found in Claude Code (4)" + **Review** → enable. Skills are referenced in place (not copied) unless the user chooses Copy |

## 2. Refresh

- **Every discovery section** shows **"Checked 2 min ago · Refresh"**.
- **Refresh** re-runs only that section's detectors, with a small spinner and results that
  animate in. New items get a **New** label until they're viewed.
- **Automatic re-detection** (below) means that Refresh is rarely needed.

## 3. How data stays fresh

**Principle:** event-driven first, then check when a page opens, and poll only while something is
visible. **Zero background work while the Control Center is closed**, except what the runtime
needs anyway.

| Data | Where it comes from | How it updates | How often |
|---|---|---|---|
| Island state, Tasks, Activity, Agent progress | Runtime event bus → IPC | **Pushed** as events happen | Real time |
| Usage and cost | Each request's usage | Pushed after every request | Real time |
| Price table (cost estimates) | Bundled + refreshed from the price source | Background fetch, signed | Weekly (can be turned off) |
| Brain health (reachable, signed in, rate limited) | Adapter health checks | When Brains opens if older than 5 min; every 5 min while it's visible; immediately after any error; lazily before use | On demand |
| Detected CLIs and local servers | PATH scan, port probes | App start (low priority, after 30 s); PATH change events (`WM_SETTINGCHANGE`); page open if older than 10 min; Refresh | Mostly event-driven |
| Local model lists (Ollama etc.) | Server API | Page open, before use, Refresh | On demand |
| MCP configs in other apps, skills folders | Files on disk | **File watchers** (`notify` crate) on the known paths | Instant |
| MCP server tools | The server | On connect + MCP `tools/list_changed` notifications; description hashes compared (tool-poisoning check) | Event-driven |
| Connector sign-ins | OAuth tokens | Refreshed silently before expiry; health checked when used | As needed |
| Installed apps index | Start menu + AppX | App start + folder watchers | Event-driven |
| Memory vault index | Markdown files | File watcher → incremental re-index | Instant |
| Context layer sizes | Instructions, memory, skills | Recomputed when any layer changes | On change |
| Performance metrics | OS counters | Sampled every 2 s **only while Performance is open** | Visible only |
| Catalogs (connector directory, plugin index, model catalog, CLI install catalog) | KIVO catalog files, signed | Background fetch with an offline cache | Daily |
| App updates | Update manifest | Check | Every 6 h + on start |

**Implementation notes:**

- **Discovery runs in the runtime,** on a low-priority background task with EcoQoS, never on the
  UI.
- **Result cache:** results go into SQLite (`discovery` table) with `checked_at`, so the UI shows
  them instantly and then updates.
- **Detectors are plugins of the runtime:** one trait, `Detector { id, scope, run() → Vec<Found> }`.
  Adding support for a new CLI or app is a catalog entry plus a detector, not UI work.
- **Privacy:** detection reads file *existence* and config *structure* only. It never uploads what
  it finds, and it never reads credential contents.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Framework (§3 implementation notes)**

- [ ] **DISC-01** · M3 · `Detector { id, scope, run() → Vec<Found> }` trait; discovery runs in the runtime on a low-priority EcoQoS task, never in the UI (§3)
- [ ] **DISC-02** · M3 · Results cached in a `discovery` table with `checked_at`, shown instantly and then updated (§3)
- [ ] **DISC-03** · M3 · Detection suggests, the user enables: nothing is switched on automatically, nothing is sent anywhere, credential contents are never read (§1, §3)

**Brains and agents (§1.1)**

- [ ] **DISC-04** · M3 · CLI agent detector: PATH and known install dirs, `--version`, sign-in state where exposed, cross-checked with the ACP registry → "Found on this PC · signed in / needs sign-in" + Use (§1.1)
- [ ] **DISC-05** · M3 · Local model server detector: ports 11434, 1234, 8080 with model lists → "Running on this PC · N models" + Use (§1.1)
- [ ] **DISC-06** · M5 · Desktop AI apps detector (Claude Desktop, ChatGPT, Copilot) from the installed-apps index (§1.1)
- [ ] **DISC-07** · M8 · In-app CLI install: explain + exact command from the catalog, dependency check (Node via winget with consent), visible progress sheet, verify, start sign-in, test prompt (§1.1)

**Extensions (§1.2)**

- [ ] **DISC-08** · M6 · Connectors detector: signed-in CLIs (`gh auth status`), installed apps with built-in connectors, browsers and whether the KIVO extension is installed (§1.2)
- [ ] **DISC-09** · M6 · MCP config import from Claude Desktop (incl. the Store path), Claude Code, Cursor, VS Code, Codex and Gemini CLI: copied into KIVO's config, originals never modified, env-var secrets moved to Credential Manager (§1.2)
- [ ] **DISC-10** · M6 · Skills detector for `~/.claude/skills/`, project `.claude/skills/` and KIVO's `skills\`; referenced in place unless the user chooses Copy; review before enabling (§1.2)
- [ ] **DISC-11** · Post · Plugins folder watcher: a dropped plugin shows "New plugin found" + Review (§1.2)

**Refresh and freshness (§2–3)**

- [ ] **DISC-12** · M3 · Every discovery section shows "Checked … · Refresh"; Refresh re-runs only that section's detectors; new items get a New label until viewed (§2)
- [x] **DISC-13** · M1 · Live state (Island, Tasks, Activity, agent progress, usage) is pushed over IPC; nothing polls it (§3) → done: state, the live turn, mic and voice levels and Activity changes are pushed over IPC (`runtime://link`, `runtime://level`, `runtime://event`); the UI has no timers polling the runtime · verified: no `setInterval` in the UI, Activity refreshes on the pushed event (2026-09-23); tasks, agents and usage join when they exist (M3–M7)
- [ ] **DISC-14** · M3 · Brain health: on page open if older than 5 min, every 5 min while visible, immediately after errors, lazily before use (§3)
- [ ] **DISC-15** · M3 · CLI/local-server re-detection: app start (after 30 s, low priority), `WM_SETTINGCHANGE` PATH changes, page open if older than 10 min (§3)
- [ ] **DISC-16** · M6 · File watchers on other apps' MCP configs and skills folders; MCP `tools/list_changed` handling with description-hash comparison (§3)
- [ ] **DISC-17** · M8 · Signed catalogs (connector directory, plugin index, model catalog, CLI install catalog) fetched daily with an offline cache (§3)
- [ ] **DISC-18** · M8 · Performance metrics sampled every 2 s only while the Performance page is open (§3)
- [x] **DISC-19** · M1 · Zero background work while the Control Center is closed beyond what the runtime needs (§3) → done: with nothing to do the runtime sleeps: the detection thread blocks until a command arrives (it used to wake every 30 ms), the speech worker isn't running, and the speaker closes when quiet · verified: live runtime with the app running, 0 ms of CPU over 20 s idle, 67 MB working set (2026-09-23)
