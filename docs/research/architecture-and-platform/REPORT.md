# Runtime, brains, computer control, security, packaging, memory

Research date: 2026-09-21. This round covers everything the voice and UI research did not. Its
findings feed the architecture documents in [docs/architecture/](../../architecture/).

## 1. Runtime and process model

- **Tauri status:** the latest stable release is **Tauri 2.11.6** (2026-09-19). A **3.0.0 alpha**
  line started on 2026-09-13, so KIVO should build on 2.11.x and watch 3.x before beta.
  ([crates.io/tauri](https://crates.io/crates/tauri))
- **Sidecars:** Tauri supports bundling and spawning external binaries as sidecars, including
  lifecycle management from Rust. ([Tauri: Embedding external binaries](https://v2.tauri.app/develop/sidecar/),
  [Evil Martians: Rust + Tauri + sidecar](https://evilmartians.com/chronicles/making-desktop-apps-with-revved-up-potential-rust-tauri-sidecar))
- **Tauri IPC:** Tauri's own IPC (commands and events) connects only the webview and its own Rust
  core. Talking to a separate runtime process needs an OS-level transport.
  ([Tauri IPC](https://v2.tauri.app/concept/inter-process-communication/))
- **Local IPC crate:** **`interprocess`** provides cross-platform local sockets (named pipes on
  Windows, Unix domain sockets elsewhere) with Tokio async support.
  ([interprocess](https://github.com/kotauskas/interprocess), [docs.rs](https://docs.rs/interprocess))
  Windows named pipes support a security descriptor and the `PIPE_REJECT_REMOTE_CLIENTS` flag. Both
  should be used so that only the current user's local processes can connect. The crate's DACL API
  is not yet verified; `windows-rs` is the fallback.
- **Why not a Windows service:** services run in session 0, which has no access to the interactive
  desktop, and they get no UI Automation or foreground-window context. The KIVO runtime must be a
  per-user process in the user's session. This is established Windows behavior, not newly
  researched.

## 2. Brains (AI providers)

- **Rust multi-provider crates:**
  - **`genai`** covers OpenAI, Anthropic, Gemini, Ollama, Groq, DeepSeek, xAI, OpenRouter,
    Bedrock, Vertex and more. It uses each provider's native protocol where one exists, and
    handles chat, vision and function calling.
    ([rust-genai](https://github.com/jeremychone/rust-genai))
  - **`rig-core`** supports 20+ providers and adds agent, RAG and vector-store abstractions.
    ([rig](https://docs.rs/rig-core/latest/rig_core/))
  - **`llm`** is a third multi-backend crate. ([llm](https://crates.io/crates/llm))
- **CLI agents:**
  - The **Agent Client Protocol (ACP)** is JSON-RPC 2.0 over stdio, created by Zed in August 2025.
    Gemini CLI supports it natively (`--acp`). Claude Code joins through the `claude-agent-acp`
    adapter and Codex through `codex-acp`. GitHub Copilot, Cursor, Goose, OpenCode, Qwen Code, Kimi
    CLI and many others are native. An ACP registry launched in January 2026.
    ([ACP agents list](https://agentclientprotocol.com/get-started/agents),
    [Marc Nuri: ACP intro](https://blog.marcnuri.com/agent-client-protocol-acp-introduction),
    [Zed external agents](https://zed.dev/docs/ai/external-agents))
  - **ACP carries permission requests from the agent to the client.** This is exactly the hook
    KIVO's permission engine needs.
  - **Codex App Server** is a bidirectional JSON-RPC (JSONL) interface that powers every Codex
    surface. Clients launch it as a long-running child process over stdio, or over WebSocket or a
    Unix socket. It rejects new requests under overload (-32001) and expects retry with backoff.
    ([OpenAI: Codex app server](https://openai.com/index/unlocking-the-codex-harness/),
    [Codex app-server guide](https://codex.danielvaughan.com/2026/04/15/codex-app-server-complete-guide/))
  - **Claude Code headless:** `claude -p` supports `--output-format text|json|stream-json`,
    stdin input, and `--allowedTools` / `--permission-mode`. The Claude Agent SDK
    (Python/TypeScript) wraps the same engine.
    ([Anthropic docs](https://docs.anthropic.com/en/docs/claude-code/sdk/sdk-headless))
- **MCP:** **`rmcp`** is the official Rust SDK. It implements the stable **2026-07-28**
  specification, stays compatible with 2025-11-25, and ships stdio, child-process and Streamable
  HTTP transports (no legacy SSE). It has more than 4.7M downloads.
  ([modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk))

**Implications:**

- KIVO should be an **ACP client** for CLI agents, with direct Codex App Server support as an
  optional richer adapter. It should also run an **MCP server** (via `rmcp`), so that CLI agents can
  call KIVO's own tools, and those calls still pass through KIVO's permission engine.
- For API brains, KIVO needs its own `BrainProvider` trait. `genai` can supply broad coverage
  behind it, with native adapters where KIVO needs provider-specific features. Every local server
  (Ollama, LM Studio, llama.cpp server) speaks the OpenAI-compatible API, so one adapter covers
  them.

## 3. Computer control

- **UI Automation from Rust:** the **`uiautomation`** crate (0.24.x, updated May 2026) wraps
  `IUIAutomation`, including events, processes, dialogs and clipboard, behind feature flags.
  `windows-rs` also exposes raw UIA bindings.
  ([uiautomation-rs](https://github.com/leexgone/uiautomation-rs), [lib.rs](https://lib.rs/crates/uiautomation))
  The crate's license and the COM threading model must be verified; UIA clients should run on a
  dedicated MTA thread.
- **Browser, the user's real Chrome:** since **Chrome 136**, `--remote-debugging-port` and
  `--remote-debugging-pipe` are **ignored for the default user-data directory**. CDP only works on
  a separate profile. This breaks "attach to my normal browser" for tools such as
  chrome-devtools-mcp and browser-use.
  ([Chrome blog](https://developer.chrome.com/blog/remote-debugging-port),
  [chrome-devtools-mcp #1830](https://github.com/ChromeDevTools/chrome-devtools-mcp/issues/1830),
  [browser-use #1520](https://github.com/browser-use/browser-use/issues/1520))
- **Browser, KIVO-driven automation:** **`chromiumoxide`** is an async, Tokio-based, complete CDP
  client that can launch Chrome or connect to a running one. `headless_chrome` is simpler and less
  maintained. ([chromiumoxide](https://github.com/mattsse/chromiumoxide),
  [comparison](https://dev.to/vhub_systems_ed5641f65d59/headless-browsers-in-rust-chromiumoxide-vs-headlesschrome-vs-the-python-alternative-25e5))

**Implications:**

- **User's real browser:** KIVO needs a **browser extension plus native messaging** (the pattern
  Claude in Chrome uses) to read and act on the tabs the user is logged into.
- **Autonomous or background tasks:** CDP on a **KIVO-managed profile** handles these.
- **Fallback:** UIA on the browser window.

## 4. Security

- **Prompt injection:** the Dual LLM pattern (a privileged planner plus a quarantined reader)
  still leaks through the quarantined model's outputs. **CaMeL** (Google DeepMind, 2025) separates
  control flow from data flow. The privileged LLM writes a plan in a restricted language, and an
  interpreter tracks capabilities and provenance on every value, enforcing policies at tool-call
  time. ([Simon Willison on CaMeL](https://simonwillison.net/2025/Apr/11/camel/),
  [arXiv 2503.18813](https://css.csail.mit.edu/6.5660/2026/readings/camel.pdf),
  [agentic AI attack/defense survey 2026](https://arxiv.org/pdf/2603.11088))
- **Implication:** KIVO should start with **provenance/taint tracking plus policy at the tool
  boundary**, a pragmatic "CaMeL-lite":
  - Once untrusted content enters a turn, risky and outbound actions require confirmation, or are
    blocked unless the user named the destination.
  - A full plan-interpreter design is a later hardening step.
- **Secrets:** the **`keyring`** ecosystem (keyring-rs, now built around `keyring-core` plus
  per-store crates) supports Windows Credential Manager, the macOS Keychain and Linux Secret
  Service. ([keyring-rs](https://github.com/open-source-cooperative/keyring-rs))
  Windows Credential Manager entries can be read by any process running as the user. That is
  acceptable, because it matches the OS threat model, but it is documented as a known limitation.

## 5. Packaging, signing, updates

- **MSIX:** Tauri ships MSI and NSIS installers. MSIX comes from community tools
  (`tauri-windows-bundle`, which produces Store-ready MSIX with extensions) or Microsoft's
  **winapp CLI**, which gives package identity for Windows APIs such as notifications, security and
  AI. ([Tauri: Microsoft Store](https://v2.tauri.app/distribute/microsoft-store/),
  [tauri-windows-bundle](https://github.com/Choochmeque/tauri-windows-bundle),
  [MS Learn: winapp CLI with Tauri](https://learn.microsoft.com/en-us/windows/apps/dev-tools/winapp-cli/guides/tauri))
  Package identity is required by the Windows AI Speech API; see the voice research.
- **Updater:** `tauri-plugin-updater` supports NSIS and MSI on Windows. For NSIS it runs a silent,
  in-place upgrade and relaunches (`/P /R /UPDATE`). Since 2.10 it is installer-aware, choosing the
  `windows-{arch}-{nsis|msi}` key. Updates are signed with a minisign key pair created with
  `tauri signer generate`. ([Tauri updater](https://v2.tauri.app/plugin/updater/),
  [changelog](https://v2.tauri.app/release/updater/all-versions/))
  Earlier releases had duplicate-install bugs, fixed by [tauri#12365](https://github.com/tauri-apps/tauri/pull/12365).
- **Code signing:**
  - **Azure Artifact Signing** (formerly Trusted Signing) costs **$9.99/month** for 5,000
    signatures. **Individual developers are limited to the USA and Canada**; organizations are
    eligible in more countries.
  - It does **not** give instant SmartScreen trust; reputation builds across consistently signed
    releases.
  - ([Azure pricing](https://azure.microsoft.com/en-us/pricing/details/artifact-signing/),
    [MS Learn: code signing options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options),
    [devclass](https://www.devclass.com/security/2026/01/14/code-signing-windows-apps-may-be-easier-and-more-secure-with-new-azure-artifact-service/4079554))

## 6. Memory

- The common local pattern is **SQLite + `sqlite-vec`** (a statically registered extension with
  `vec0` tables), plus **`fastembed-rs`** running a small embedding model (all-MiniLM-L6-v2,
  384-dim) on ONNX Runtime. Changing the embedding model means re-embedding everything.
  ([sqlite-vec in Rust](https://alexgarcia.xyz/sqlite-vec/rust.html),
  [example design](https://github.com/ACFHarbinger/Coding-Assistants/issues/261))
- **ONNX Runtime advisories:** `ort` has had RUSTSEC advisories, so dependency auditing
  (`cargo audit` / `cargo deny`) belongs in CI.

## 7. Gaps

- The `interprocess` crate's named-pipe security-descriptor API is unverified.
- The `uiautomation` crate's license is unverified.
- The Codex App Server's stability guarantees across versions are unverified.
- Subscription and managed-login terms: some providers restrict third-party use of consumer
  subscription credentials. KIVO should only use subscriptions through the providers' own
  CLIs/agents (ACP), never by extracting tokens.
