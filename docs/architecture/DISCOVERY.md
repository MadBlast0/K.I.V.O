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
