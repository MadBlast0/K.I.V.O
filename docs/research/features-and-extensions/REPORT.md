# Features and extensions: realtime voice, computer use, costs, integrations, plugins, remote, companion, name

Research date: 2026-09-21. The specs based on this research are in [docs/architecture/](../../architecture/ARCHITECTURE.md).

## 1. Realtime speech-to-speech models

**OpenAI Realtime:**

- `gpt-realtime-2.1` costs **$32 / 1M audio-input tokens** and **$64 / 1M audio-output
  tokens**. The mini model costs $10 / $20.
- Audio is about 600 tokens per user-minute and 1,200 tokens per assistant-minute.
- Typical cost is **about $0.06–0.11/min** (flagship) and **$0.02–0.05/min** (mini) with prompt
  caching. With caching broken it can reach $0.18–0.46/min.
- Sources: [layer3labs](https://www.layer3labs.io/guides/openai-realtime-api-pricing),
  [HackerNoon: 4,000 sessions](https://hackernoon.com/openai-realtime-api-pricing-in-2026-real-world-data-from-4000-measured-sessions),
  [Fora Soft](https://www.forasoft.com/blog/article/openai-realtime-api-pricing).

**Gemini Live API (native audio):**

- About **$0.005/min input and $0.018/min output**, at 25 tokens per second of audio.
- Supports function calling, code execution and Search.
- **Limits:** audio-only sessions last 15 min without context-window compression (unlimited
  with it). A WebSocket connection lasts about 10 min, so the client must resume sessions.
- Sources: [Gemini pricing](https://ai.google.dev/gemini-api/docs/pricing),
  [Live API session management](https://ai.google.dev/gemini-api/docs/live-session),
  [capabilities](https://ai.google.dev/gemini-api/docs/live-api/capabilities),
  [Google blog: Gemini Live](https://blog.google/innovation-and-ai/technology/developers-tools/build-real-time-voice-applications-gemini-audio/).

**Implications:**

- A **Realtime conversation mode** is feasible as an *optional brain type*. The model hears and
  speaks directly, which gives the most natural turn-taking and tone.
- Trade-offs:
  - It costs more per minute than STT + LLM + TTS for short commands.
  - Audio goes to the cloud continuously during the session.
  - Tool calls must still pass KIVO's permission engine.
  - The fast path is skipped.
- **Design:** KIVO's local wake word, VAD and fast path stay in front. The realtime session opens
  only for a conversation ("let's talk"), or when a profile selects it. It closes after silence.

## 2. Computer use (AI sees the screen and operates it like a person)

- **Claude computer use** gives the model a portable screenshot + mouse/keyboard tool.
- **OpenAI** shipped *Codex Background Computer Use* on 2026-04-16 (macOS-first).
- **Gemini Computer Use**, which grew out of Project Mariner, is browser-optimized.
- Source: [digitalapplied: computer-use agents 2026](https://www.digitalapplied.com/blog/computer-use-agents-2026-claude-openai-gemini-matrix).
- **Cost:** agent loops multiply token use 50–500× per task. Every step sends a screenshot, so
  vision loops are the most expensive and slowest way to operate a PC.
  ([kunalganglani](https://www.kunalganglani.com/blog/ai-agent-cost-per-task-2026),
  [Ivern benchmark](https://ivern.ai/blog/ai-agent-cost-benchmark-report-2026))

**Implications:**

- Computer use is the **top of the capability ladder only when nothing semantic works**, and the
  user must opt in to it separately (a capability toggle, off by default).
- It needs per-app allow and block lists, a visible "AI is controlling your screen" indicator, an
  instant stop, a per-task step and cost cap, and screenshots that never persist unless debugging
  is on.

## 3. Cost tracking data

- LiteLLM's **`model_prices_and_context_window.json`** is a community-maintained, MIT-licensed
  database of per-token prices and context windows for 100+ providers. It is fetched from GitHub
  and used for cost calculation.
  ([file](https://github.com/BerriAI/litellm/blob/main/model_prices_and_context_window.json),
  [docs](https://docs.litellm.ai/docs/proxy/custom_model_cost_map))
- **Implication:** KIVO computes cost locally from the usage each response returns, times a
  bundled and periodically refreshed price table. The price table is overridable per model.
  Provider billing APIs need admin keys, so they are not required; they could be an optional
  reconciliation.

## 4. Integrations: platform constraints

- **Google:**
  - Gmail read, full Drive and similar scopes are **restricted**. Restricted-scope verification
    plus an annual **CASA** security assessment (Tier 2/3, hundreds to thousands of dollars a
    year) is required when data is stored on or passes through servers. It is required in
    general for public apps using restricted scopes.
  - Unverified apps are limited to 100 test users and show warning screens.
  - Sources: [Google: restricted scope verification](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification),
    [DeepStrike CASA guide](https://deepstrike.io/blog/google-casa-security-assessment-2025),
    [example OSS issue](https://github.com/anasakkari3/maybesitter/issues/516).
- **Spotify:**
  - Development mode allows **5 authenticated users**, and the app owner must have Premium.
  - Extended quota has been **organizations only, with 250k+ MAU** since 2025-05-15. As of July
    2026, up to 25 client IDs per developer are allowed.
  - Sources: [Spotify quota update 2026-07](https://developer.spotify.com/blog/2026-07-23-web-api-quota-updates),
    [criteria](https://developer.spotify.com/blog/2025-04-15-updating-the-criteria-for-web-api-extended-access),
    [quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes).

**Implications:**

- **Bring-your-own-client-ID** (the user registers their own OAuth app) is the standard workaround
  for open-source desktop apps. KIVO should support it for Google and Spotify. The onboarding
  guide must make this painless.
- Anything that works without web APIs should use local means first:
  - Windows media controls for Spotify playback;
  - desktop Outlook/Teams through UIA;
  - the browser extension for web apps.
- Microsoft Graph (Outlook, Calendar, OneDrive, Teams) and GitHub are more permissive. They are
  still to be verified per scope at implementation time.
- MCP servers from vendors, where they exist, are the zero-maintenance path.

## 5. Plugins

- **Extism** is a cross-language WASM plugin framework on wasmtime, with bytes-in/bytes-out
  calls, host functions, and runtime-checked capabilities.
- The **wasmtime Component Model** gives typed WIT interfaces. Capabilities are granted by linking
  only the allowed imports, which limits it to wasmtime.
- WASI gives sandboxed filesystem access only to preopened directories.
- Sources: [Extism](https://github.com/extism/extism),
  [uni-plugin-extism notes](https://docs.rs/uni-plugin-extism/latest/uni_plugin_extism/),
  [wasmtime](https://docs.wasmtime.dev/api/wasmtime/).
- **Implication:** KIVO uses the **Component Model (wasmtime) with WIT interfaces**. It gives
  typed contracts and structural capability gating, which fits a permission-centric design. MCP
  servers remain the "any language, out of process" extension path.

## 6. Remote / phone access

- Recent agent-remote apps (Codex remote/Remodex, Herdr relay, Paseo, OpenChamber) converge on
  one pattern:
  - **QR pairing** carries the desktop's public key and a one-time secret;
  - an **X25519/Ed25519 handshake** sets up an end-to-end encrypted channel (AES-GCM or
    XSalsa20-Poly1305) with replay counters;
  - an **untrusted relay** routes ciphertext only;
  - direct P2P (WebRTC) is preferred when reachable;
  - QR codes are single-use and expire.
- Sources: [Paseo security](https://github.com/getpaseo/paseo/blob/main/public-docs/security.md),
  [Codex mobile pairing](https://codex.danielvaughan.com/2026/06/06/codex-cli-remote-control-mobile-pairing-app-server-v2-remodex/),
  [herdr-mobile-relay](https://github.com/0cv/herdr-mobile-relay).
- **Implication:** KIVO Remote is a post-MVP feature built on that pattern. The phone is a
  *client* of the runtime, like the Control Center, with its own permission profile. High-risk
  actions require confirmation on the phone plus biometrics.

## 7. Companion

- **Rive** has React bindings (`useRive`, `useStateMachineInput`). Its state machines suit
  characters that react to inputs.
  ([Rive state machines](https://help.rive.app/runtimes/state-machines),
  [mascot engineering](https://dev.to/uianimation/engineering-interactive-mascots-with-rives-state-machine-and-runtime-architecture-4e2h))
- Several open-source **Tauri 2 + React desktop pets** show that transparent always-on-top
  characters work, with click-through for transparent pixels as a known problem.
  ([deskpet](https://github.com/Scyyyy4/deskpet),
  [CaYa-Desktop-Pet](https://github.com/CaYatur/CaYa-Desktop-Pet),
  [desktop-mascot](https://github.com/jingluoguo/desktop-mascot),
  [click-through PR](https://github.com/Robben-Ge/desktop-pet/pull/2))
- **Implication:** there are three companion styles the user can switch between: **Pill**
  (default), **Orb** (shader or Rive), and **Character** (Rive). All three are driven by the same
  `SessionState` and audio levels. Each has its own window only when it is shown, and each has an
  idle frame budget of zero.

## 8. Name / trademark check ("KIVO")

This is **not legal advice**. A web search finds existing uses in overlapping markets:

- **"KIVO", an AI companion app** on Google Play ("an AI companion you can talk to"), in the
  *same category* as this project.
  ([Play Store](https://play.google.com/store/apps/details?id=com.kivoapp&hl=en-US))
- **Kivo.ai** (W3villa), AI agents for HR/CRM/business automation. Its site says its trademarks
  are protected. ([kivo.ai](https://www.kivo.ai/), [terms](https://www.kivo.ai/terms-of-use))
- **Kivo** (kivo.io), life-sciences regulatory and document-management software.
  ([kivo.io](https://kivo.io/))

**Implication:** there is a real risk of confusion or conflict for a public "KIVO" AI assistant,
especially with the AI-companion app. Options:

1. Keep "K.I.V.O." as the internal codename and pick a distinct public name before the first
   public release.
2. Keep KIVO after checking actual registrations (USPTO TESS, EUIPO, WIPO Global Brand Database,
   and India's IP office) in the relevant classes (9 and 42), ideally with legal advice.

Renaming is cheap now and expensive later. The code uses `kivo` identifiers, which is fine for a
codename.
