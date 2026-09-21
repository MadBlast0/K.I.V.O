# Integrations, plugins and remote access

Status: Draft v1, 2026-09-21. Research: [features-and-extensions/REPORT.md](../research/features-and-extensions/REPORT.md).
Implements plan §121, §149–152, and the owner's integration priorities (Google, Microsoft,
Spotify/media, dev tools, then other major apps).

## 0. Owner rule: no keys for users (2026-09-21)

**Users never paste API keys or register their own developer apps.** Connecting works the way
Claude Desktop's **Settings → Connectors** does: a directory of connectors, a **Connect** button, a
browser sign-in, done. This supersedes the "bring-your-own client ID" idea below. How it is
achieved:

| Mechanism | Used for | Why no user keys are needed |
|---|---|---|
| **Remote MCP connectors with MCP authorization** (OAuth 2.1 + PKCE; the client registers itself through **Dynamic Client Registration** or a **Client ID Metadata Document**) | Services that host an official remote MCP server (GitHub, Notion, Linear, Atlassian, Asana, and more) | The client registers itself with the server automatically. KIVO publishes a CIMD URL identifying "KIVO". ([MCP authorization spec](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization), [client registration](https://blog.modelcontextprotocol.io/posts/client_registration/)) |
| **KIVO-owned OAuth apps** (one client ID per provider, registered by the KIVO project and embedded as a public PKCE client) | Google Workspace (its official remote MCP servers or the APIs), Microsoft Graph | The project registers once and users just sign in. **Cost to the project:** Google verification and, for restricted scopes (Gmail read, full Drive), an **annual CASA assessment**. Start with non-restricted scopes (Calendar, `drive.file`) and add restricted scopes when funded ([Google Workspace MCP](https://developers.google.com/workspace/guides/configure-mcp-servers)) |
| **Local means, no account at all** | Media (Spotify playback through Windows media controls), desktop apps (UIA), web apps (browser extension on the user's logged-in session), CLIs (`gh`, `code`) | Nothing to connect. These work out of the box |
| **Services that can't be offered publicly** | Spotify Web API (5-user dev mode, extended quota only for organizations with 250k MAU) | Use local means only (media controls, `spotify:` URIs, UIA). There is no Web API connector until the platform allows it |

**Connectors page** (Control Center → Extensions → Connectors), same idea as Claude's:

- **Directory:** a curated list with each connector's logo, a description, the tools it provides,
  the data it can access, and badges (Local / Cloud / Sensitive).
- **Connecting:** **Connect** opens the system browser for sign-in. The token is stored in
  Credential Manager. Afterwards the page shows the connected account, the tools with per-tool
  toggles and risk levels, **Disconnect**, and a last-used time.
- **Custom connector (advanced):** paste a remote MCP URL. DCR or CIMD handles registration, so a
  key is still not needed when the server supports it.
- **Capabilities:** all tools are subject to [CAPABILITIES.md](CAPABILITIES.md) and the
  permission engine.

## 1. Integration strategy: the cheapest path that works

For each app, this order applies:

1. **Local means (no account needed):**
   - Windows media controls (Spotify, YouTube Music, any player);
   - app CLIs (`gh`, `code`, `git`, `wt`);
   - UIA on the desktop app (Outlook, Teams);
   - the browser extension on the web app.
2. **A vendor's official MCP server**, if one exists (zero maintenance for KIVO).
3. **A native KIVO connector** using the vendor's web API with OAuth, for deep features (search
   mail, create events) that local means can't do well.

**Connector shape:** a `Connector` provides ToolSpecs (namespaced `google.gmail.search`, …), an
auth flow, health checks, and a scope list shown to the user. It is covered by
[CAPABILITIES.md](CAPABILITIES.md) toggles and the permission engine.

## 2. Priority connectors

| Integration | Local path (M4) | Web API connector (M8+) | Constraints |
|---|---|---|---|
| **Google** (Gmail, Calendar, Drive) | Browser extension on the web apps | OAuth (PKCE, loopback redirect per RFC 8252), minimal scopes first (Calendar events, Drive file-level `drive.file`) | Gmail read and full Drive are **restricted scopes**: a public client ID needs verification plus an annual **CASA** assessment. Per §0 users never register their own apps: KIVO's own OAuth client ships non-restricted scopes first, and restricted scopes wait for verification funding |
| **Microsoft** (Outlook, Calendar, Teams, OneDrive) | UIA on desktop Outlook/Teams; web via extension | Microsoft Graph with MSAL-style PKCE for personal and work accounts | Admin consent may be required for work tenants; verify scopes per feature |
| **Spotify / media** | **Windows media transport controls** (play/pause/next, now playing) | None until Spotify allows public apps (§0) | Dev mode allows **5 users**, and the owner needs Premium; extended quota is only for organizations with 250k MAU. So per §0 there is no Web API connector for now: use the Spotify URI protocol (`spotify:search:…`) + UIA for "play X" without the API |
| **Dev tools** | `gh` CLI, `code` CLI, `git`, Windows Terminal (`wt`), project detection | GitHub REST/GraphQL via the user's `gh` auth; a VS Code extension later | — |
| Other major apps (Notion, Slack, Discord, WhatsApp Desktop, Zoom, Office) | UIA + browser extension + app CLIs + URI protocols | Official MCP servers where available; native connectors on demand | Each one gets a short research note before its implementation |

**OAuth rules:**

- **Desktop flow:** system browser + PKCE + loopback redirect. KIVO never uses an embedded
  webview for login.
- **Storage:** tokens live in Credential Manager.
- **Scopes:** each connector requests the **narrowest scopes** per feature, incrementally.
- Consumer session cookies and tokens are never extracted.

## 3. Plugins (plan §121)

**Two extension paths:**

| Path | For | Isolation |
|---|---|---|
| **MCP servers** (any language, out of process) | Tools and integrations | Separate process, per-server permissions ([TOOLS_AND_CONTROL.md §9](TOOLS_AND_CONTROL.md)) |
| **KIVO plugins** (WASM, wasmtime Component Model) | Tools, fast-path commands, routine steps, intent grammars, companion skins, voices (data only) | In-process sandbox. The **WIT interface** defines the host functions, and capabilities are granted by **linking only the imports the user approved** (no import, no access) |

**Plugin manifest (`kivo-plugin.toml`):**

- `id`, `version`, `publisher`, `kivo_api` range;
- `capabilities` (for example `net:["api.example.com"]`, `fs:["$DOCUMENTS/Recipes"]`,
  `tools:["apps.launch"]`);
- `tools` provided, with risk levels;
- the signature.

**Rules:**

- **Install:** the requested capabilities are shown before install (like mobile app
  permissions). Updates that add capabilities require re-consent.
- **Resource limits:** fuel and time limits per call, and a memory cap.
- **Tool calls:** plugin tools go through `authorize()`. Plugins can never call the permission
  engine to approve themselves.
- **Distribution:** local files at first. A signed plugin index is a post-v1 idea (plan §149).

**Milestone:** post-MVP (after M9). The ToolSpec and WIT interfaces are drafted in M4, so that
built-in tools already fit the plugin shape.

## 4. KIVO Remote (phone and other PCs, plan §151)

**Goal:** talk to or type to your PC's KIVO from your phone, see tasks, approve confirmations,
and get notifications. **The desktop runtime stays the only authority.**

**Architecture:**

```text
Phone app (PWA first; native later) ⇄ E2E-encrypted channel ⇄ [relay (ciphertext only) | direct P2P/LAN] ⇄ kivo-runtime (remote endpoint)
```

- **Pairing:** the Control Center shows a **QR code** containing the runtime's Ed25519 public key,
  a one-time secret and relay info. It is single-use and expires after 2 minutes.
- **Handshake and encryption:** X25519 key agreement (Noise protocol pattern), then an
  AEAD-encrypted channel with replay counters.
- **Transport:**
  - LAN direct (mDNS) when on the same network;
  - WebRTC P2P otherwise;
  - an untrusted relay as the fallback (self-hostable; KIVO's relay sees only ciphertext and
    metadata).
- **Permissions:**
  - A remote client is its own **permission principal**, with a default profile of *Remote
    Balanced*: no shell, no computer use, and all Medium actions confirmed.
  - High-risk actions need confirmation **on the phone with device biometrics**.
  - Paired devices can be listed and revoked in the Control Center, and every remote action is
    audited with its device id.
- **Voice from the phone:** audio can be transcribed on the phone (platform STT) or streamed to the
  desktop's STT. The privacy mode applies.

**Milestone:** post-MVP (after M9), in a "Remote" track. Nothing in M0–M9 may assume that the UI is
local-only. The IPC is already versioned and clients are already untrusted.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Connectors (§0–2)**

- [ ] **INT-01** · M4 · Local integration paths that need no account: Windows media controls for any player, app CLIs (`gh`, `code`, `git`, `wt`), UIA on desktop apps, the browser extension on web apps, URI protocols such as `spotify:` (§1, §2)
- [ ] **INT-02** · M6 · `Connector` shape: namespaced ToolSpecs, auth flow, health checks and a scope list shown to the user; covered by capability toggles and the permission engine (§1)
- [ ] **INT-03** · M6 · Extensions → Connectors page: directory (logo, description, tools, data access, badges), Connect → system-browser sign-in, connected account, per-tool toggles and risk, Disconnect, last used (§0)
- [ ] **INT-04** · M6 · Remote MCP connectors with MCP authorization (OAuth 2.1 + PKCE, DCR or a KIVO Client ID Metadata Document) and "Custom connector": paste a remote MCP URL (§0)
- [ ] **INT-05** · M8 · KIVO-owned OAuth apps (public PKCE clients, loopback redirect per RFC 8252, never an embedded webview): Google Calendar + `drive.file` first, then Microsoft Graph (§0, §2)
- [ ] **INT-06** · M8 · GitHub via the user's `gh` auth (REST/GraphQL) (§2)
- [ ] **INT-07** · M8 · Tokens in Credential Manager, narrowest scopes requested incrementally, consumer session cookies never extracted (§2)
- [ ] **INT-08** · Post · Restricted Google scopes (Gmail read, full Drive) once verification/CASA is funded; each other major app gets a research note before implementation (§0, §2)
- [ ] **INT-15** · Post · File Explorer context menu "Ask KIVO about this" / "Summarize with KIVO" (needs package identity on Windows 11, DIST-06) (mockup → System surfaces)

**Plugins (§3)**

- [ ] **INT-09** · M4 · ToolSpec and WIT interfaces drafted so built-in tools already fit the plugin shape (§3)
- [ ] **INT-10** · Post · WASM plugins (wasmtime Component Model): `kivo-plugin.toml` manifest, capabilities granted by linking only approved imports, consent at install and on capability-adding updates, fuel/time/memory limits, tools through `authorize()` (§3)

**KIVO Remote (§4)**

- [x] **INT-11** · M0 · Nothing assumes the UI is local-only: IPC is versioned and clients are untrusted (§4) → done: every IPC client is untrusted: token-authenticated, version-checked, schema-validated and size-limited; nothing assumes the UI is local-only beyond the transport · verified: kivo-ipc tests
- [ ] **INT-12** · Post · Pairing by single-use QR (Ed25519 key, one-time secret, 2 min expiry), Noise/X25519 handshake, AEAD channel with replay counters (§4)
- [ ] **INT-13** · Post · Transports LAN (mDNS), WebRTC P2P, self-hostable ciphertext-only relay (§4)
- [ ] **INT-14** · Post · Remote principal with the Remote Balanced profile (no shell, no computer use, Medium confirmed), High confirmed on the phone with biometrics, paired devices listed and revocable, remote actions audited with device id (§4)
