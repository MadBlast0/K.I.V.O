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
| CLI agents: `claude`, `codex`, `gemini`, `copilot`, `opencode`, `goose`, `qwen`, `kimi`… | Search `PATH` and known install dirs (npm global, `%LOCALAPPDATA%\Programs`, winget/Scoop shims). Run `--version`. Check sign-in where the CLI exposes it (a status command or a credential file that exists; its contents are never read). Cross-check against KIVO's catalog of ACP agents, which mirrors the ACP registry (DECISIONS "CLI agents found") | "Found on this PC · signed in / needs sign-in" + **Use** |
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

- [x] **DISC-01** · M3 · `Detector { id, scope, run() → Vec<Found> }` trait; discovery runs in the runtime on a low-priority EcoQoS task, never in the UI (§3) → done: `discovery.rs`: `Detector { id, scope, run() }`, run by the runtime in the background, never the UI · verified: discovery tests (2026-09-23)
- [x] **DISC-02** · M3 · Results cached in a `discovery` table with `checked_at`, shown instantly and then updated (§3) → done: results in the `discovery` table with `checked_at`; the page shows the cache at once, then updates · verified: discovery tests (2026-09-23)
- [x] **DISC-03** · M3 · Detection suggests, the user enables: nothing is switched on automatically, nothing is sent anywhere, credential contents are never read (§1, §3) → done: found items only suggest (Use / Sign in); nothing is connected or sent; sign-in files are checked for existence, never read · verified: the CLI detector test (a credentials file that isn't JSON is never read), onboarding test (nothing connects unasked) (2026-09-23)

**Brains and agents (§1.1)**

- [x] **DISC-04** · M3 · CLI agent detector: PATH and known install dirs, `--version`, sign-in state where exposed, cross-checked with the ACP registry → "Found on this PC · signed in / needs sign-in" + Use (§1.1) → done: PATH (as Windows has it now) plus npm, WinGet, pnpm, Scoop and Bun folders; `--version`; signed-in files; ACP adapter presence from KIVO's catalog (DECISIONS "CLI agents found") · verified: `cli_agents_are_found_with_version_and_sign_in_state_without_reading_credentials` (2026-09-23)
- [x] **DISC-05** · M3 · Local model server detector: ports 11434, 1234, 8080 with model lists → "Running on this PC · N models" + Use (§1.1) → done: probes 11434, 1234, 8080 `/v1/models` with a short timeout, listing models · verified: `local_servers_are_found_with_their_models` (2026-09-23)
- [x] **DISC-06** · M5 · Desktop AI apps detector (Claude Desktop, ChatGPT, Copilot) from the installed-apps index (§1.1) → done: Claude Desktop, ChatGPT and Copilot from the installed-apps index via the app registry's `desktop_ai` entries; listed on the Agents page · verified: `the_agents_page_requests`, `Agents.test.tsx` (2026-09-23)
- [x] **DISC-07** · M8 · In-app CLI install: explain + exact command from the catalog, dependency check (Node via winget with consent), visible progress sheet, verify, start sign-in, test prompt (§1.1) → done: Agents → "Not installed" → Install: the sheet explains, shows the exact commands from the catalog (a newer signed catalog's when there is one), checks dependencies (Node via winget, asked first), runs with visible progress, verifies with `--version`, then offers the CLI's own sign-in and a test · verified: `installing_a_cli_agent_explains_asks_runs_and_verifies`, installer unit tests, Agents UI test

**Extensions (§1.2)**

- [x] **DISC-08** · M6 · Connectors detector: signed-in CLIs (`gh auth status`), installed apps with built-in connectors, browsers and whether the KIVO extension is installed (§1.2) → done: `apps/kivo-runtime/src/connectors.rs` detects `gh auth status`, installed apps with connectors (VS Code, Spotify) and the KIVO browser extension, shown under Ready to use · verified: `m6_extensions::connectors_sign_in_and_local_ones_are_found` with a fake `gh`, `Extensions.test.tsx`
- [x] **DISC-09** · M6 · MCP config import from Claude Desktop (incl. the Store path), Claude Code, Cursor, VS Code, Codex and Gemini CLI: copied into KIVO's config, originals never modified, env-var secrets moved to Credential Manager (§1.2) → done: `crates/kivo-mcp/src/imports.rs` reads Claude Desktop (incl. the Store path), Claude Code (incl. projects), Cursor, VS Code (JSONC), Codex (TOML) and Gemini CLI; copied into KIVO, secrets to Credential Manager · verified: imports unit tests and `m6_extensions::an_imported_server_is_reviewed_then_used_by_a_brain`, which checks the original file is byte-for-byte unchanged (M6-X2)
- [x] **DISC-10** · M6 · Skills detector for `~/.claude/skills/`, project `.claude/skills/` and KIVO's `skills\`; referenced in place unless the user chooses Copy; review before enabling (§1.2) → done: skills detector for `~/.claude/skills/`, project `.claude/skills/` and KIVO's folder, referenced in place, Copy on import; review before enabling · verified: `m6_extensions::skills_are_found_reviewed_and_offered_to_brains`, `Extensions.test.tsx`
- [ ] **DISC-11** · Post · Plugins folder watcher: a dropped plugin shows "New plugin found" + Review (§1.2)

**Refresh and freshness (§2–3)**

- [x] **DISC-12** · M3 · Every discovery section shows "Checked … · Refresh"; Refresh re-runs only that section's detectors; new items get a New label until viewed (§2) → done: "Checked … · Refresh" per section; Refresh re-runs that section only; New until viewed · verified: discovery and Brains tests (2026-09-23)
- [x] **DISC-13** · M1 · Live state (Island, Tasks, Activity, agent progress, usage) is pushed over IPC; nothing polls it (§3) → done: state, the live turn, mic and voice levels and Activity changes are pushed over IPC (`runtime://link`, `runtime://level`, `runtime://event`); the UI has no timers polling the runtime · verified: no `setInterval` in the UI, Activity refreshes on the pushed event (2026-09-23); tasks, agents and usage join when they exist (M3–M7)
- [x] **DISC-14** · M3 · Brain health: on page open if older than 5 min, every 5 min while visible, immediately after errors, lazily before use (§3) → done: page open (older than 5 min), every 5 min while visible, at once after errors, lazily before use (background) (DECISIONS "Health checked around use") · verified: brains tests, `a_failing_provider…` (2026-09-23)
- [x] **DISC-15** · M3 · CLI/local-server re-detection: app start (after 30 s, low priority), `WM_SETTINGCHANGE` PATH changes, page open if older than 10 min (§3) → done: 30 s after start at low priority; `WM_SETTINGCHANGE` "Environment" re-reads PATH from the registry and re-detects CLIs; page open if older than 10 min · verified: `the_current_path_comes_from_the_registry`, `a_watch_starts_and_stops`, discovery tests (2026-09-23)
- [x] **DISC-16** · M6 · File watchers on other apps' MCP configs and skills folders; MCP `tools/list_changed` handling with description-hash comparison (§3) → done: `notify` watchers on the folders of other apps' MCP config files (so a setup made later is seen) and the skills folders (`Mcp::watch`, `Skills::watch`) raise `DiscoveryChanged`; `tools/list_changed` re-lists and compares description hashes · verified: `m6_extensions::a_description_changed_after_approval_is_switched_off` (rug pull on the hostile fixture) and `m6_extensions::other_apps_setups_and_skills_are_watched`
- [~] **DISC-17** · M8 · Signed catalogs (connector directory, plugin index, model catalog, CLI install catalog) fetched daily with an offline cache (§3) → partial: signed catalogs (connectors, plugins, models, CLI installs): fetched daily, ed25519 signatures checked against `catalogs/keys.txt`, an offline cache, the bundled copy until a newer signed one arrives; `pnpm catalog:sign` signs; off in Strictly private and with "Check for catalog updates" · verified: catalogs unit tests (a tampered or unsigned catalog is refused), Permissions test · missing: the owner's signing key in `keys.txt` and a published catalog location (until then only the bundled catalogs are used)
- [x] **DISC-18** · M8 · Performance metrics sampled every 2 s only while the Performance page is open (§3) → done: Settings → Performance asks the runtime every 2 s only while that tab is open; nothing is sampled otherwise · verified: `Settings.test.tsx`, the tab's effect cleanup (2026-09-24)
- [x] **DISC-19** · M1 · Zero background work while the Control Center is closed beyond what the runtime needs (§3) → done: with nothing to do the runtime sleeps: the detection thread blocks until a command arrives (it used to wake every 30 ms), the speech worker isn't running, and the speaker closes when quiet · verified: live runtime with the app running, 0 ms of CPU over 20 s idle, 67 MB working set (2026-09-23)
