# Brains, intent routing and agents

Status: Draft v1, 2026-09-21. Research: [architecture-and-platform/REPORT.md §2](../research/architecture-and-platform/REPORT.md).
This spec implements plan §11–22 and §37–43.

## 1. Request flow

```text
final transcript / typed text
  → IntentRouter
      ├─ FastPath (deterministic grammar)        → Tool call(s) directly, no brain
      ├─ EventTask (watch/notify patterns)       → Task with watcher
      └─ Brain path
           → BrainRouter picks provider+model from profile, privacy, capability, health
           → context assembly (always / on-demand / task, as deltas)
           → tool exposure (minimal set)
           → stream: text → TTS chunker; tool calls → permission engine → tools
           → validation step for tasks that claim success
```

## 2. Intent router

It works in three stages, and the first confident stage wins:

1. **Grammar match** (Rust, microseconds). Commands are declared with slots, for example
   `open {app}`, `volume {up|down|to N}`, `mute`, `pause|play|next`, `take a screenshot`,
   `minimize|maximize|close {window?}`, `lock`. The slots resolve against live indexes such as
   installed apps and windows, and phrasing variants and synonyms come from data files, so they
   can be localized. Destructive commands (shutdown, restart) still go through permissions.
2. **Semantic match**. The utterance is embedded with the small local embedding model and
   compared (kNN) against command exemplars. It is accepted only above a high threshold and when
   the slots are resolvable. This catches paraphrases such as "turn the sound off".
3. **Everything else goes to the brain.**

- **Hybrid requests** such as "Open Chrome and search for cats" go to the brain, which receives the
  fast-path tools as tools.
- **Metrics:** `fast_path_ratio`, misroute reports (a "that's not what I meant" button in the
  card), and the p95 latency of each stage.
- **Event tasks:** patterns like "tell me when …" / "watch …" / "remind me …" are classified by
  grammar first and by the brain when ambiguous. The result is a Task with a watcher
  ([TOOLS_AND_CONTROL.md §6](TOOLS_AND_CONTROL.md)).

## 3. Provider contract (plan §13)

```text
trait BrainProvider {
  fn info(&self) -> ProviderInfo;            // id, kind: Api|Cli|Local|ManagedLogin, privacy class, cost meta
  async fn health(&self) -> Health;
  async fn models(&self) -> Vec<ModelInfo>;  // context window, tool support, vision, streaming, price
  async fn chat(&self, req: ChatRequest, cancel: CancellationToken) -> BrainStream;
}
enum BrainEvent { TextDelta, ToolCall{ id, name, args }, ToolCallDelta, Reasoning(hidden), Usage, Done, Error(NormalizedError) }
enum NormalizedError { Auth, RateLimited{retry_after}, Quota, ContextTooLong, ContentFiltered, Network, ProviderDown, Cancelled, Other }
```

- **Reasoning** content is never shown or spoken (plan §145). It may be kept in the turn log for
  debugging only if the user enables that.
- **Agent brains** (CLI) implement a richer trait: `AgentSession` with `prompt`, `cancel`, streamed
  `AgentEvent` (message, plan, tool activity, file diff, permission request), and `set_mode`.

## 4. Adapters

| Kind | v1 adapters | Implementation |
|---|---|---|
| **API** | Anthropic, OpenAI, Google Gemini, OpenRouter, Groq, Mistral, DeepSeek, xAI, custom OpenAI-compatible | `genai` behind KIVO's trait, with native adapters for Anthropic and OpenAI where needed (prompt caching, realtime, reasoning controls) |
| **Local** | Ollama, LM Studio, llama.cpp server, any OpenAI-compatible localhost URL | The OpenAI-compatible adapter plus discovery (probe known ports, `ollama list`) |
| **CLI agent** | Claude Code (`claude-agent-acp`), Gemini CLI (`--acp`), Codex (`codex-acp`, or Codex App Server natively), GitHub Copilot CLI, OpenCode, Goose, … | **KIVO is an ACP client** (JSON-RPC over stdio). Codex App Server is an optional richer adapter |
| **Managed login** | Via the provider's own CLI/agent authentication (ACP agents), or OAuth where a provider officially offers it to third-party apps | Never extract or reuse consumer session tokens |

**Signing in without API keys (owner preference, 2026-09-21).** Onboarding and the Brains page
lead with options that need no key:

| Option | How the user connects |
|---|---|
| **CLI agents** (Claude Code, Codex, Gemini CLI) | They sign in with their own account or subscription in the CLI's own login flow. KIVO can launch the login flow |
| **OpenRouter** | **OAuth PKCE "Connect with OpenRouter"**: the user signs in, and OpenRouter returns a user-controlled key to KIVO automatically. There is nothing to copy ([OpenRouter OAuth PKCE](https://openrouter.ai/docs/guides/overview/auth/oauth)) |
| **Local models** (Ollama, LM Studio) | Auto-detected; no account |
| Direct Anthropic / OpenAI / Gemini APIs | Their public APIs require an API key. This stays available under **"Advanced: use your own API key"** and is never required |

**ACP specifics:**

- The agent's permission requests (`session/request_permission`) are routed into KIVO's
  permission engine and confirmation UI, so one consistent permission UX covers everything.
- The agent's file and terminal operations run in the agent's own process; KIVO shows them as
  activity.
- KIVO gives agents its own tools through the **KIVO MCP server** (rmcp, stdio), passed in the ACP
  session's MCP server list. Those tools then go through KIVO's permissions.
- Adapters that need Node.js trigger the Dependency Manager flow (plan §19–20).

**CLI discovery** (plan §18) covers:

- the ACP registry;
- known executable names on PATH and in common install dirs;
- `--version` probes;
- auth state checks where a CLI exposes them.

## 5. Brain profiles and routing (plan §15–16)

```text
Profile { id, name, primary: ProviderModelRef, fallbacks: [ProviderModelRef],
          privacy: Cloud|LocalPreferred|StrictPrivate, cost_pref, latency_pref, max_context,
          allowed_tools: ToolScope, system_prompt_addendum }
```

- **Built-in profiles:** Default, Fast, Smart, Coding, Private, Offline, Cheap.
- **Routing is deterministic and explainable.** It evaluates rules in order:
  1. the explicit user choice ("use Claude for this");
  2. the task class (coding → Coding profile);
  3. data classification (sensitive → Private);
  4. offline state;
  5. availability and health;
  6. latency and cost preference.

  Each decision records a one-line reason that the card can show (plan §145).
- **Failover** (plan §73): on `RateLimited` / `ProviderDown` / `Network`, KIVO moves to the next
  fallback **only if** that fallback satisfies the same privacy class. Otherwise it asks the user.

## 6. Context assembly (plan §64–65)

| Level | Contents | Delivery |
|---|---|---|
| Always | Session state, active app/window title, user preferences (short), permission mode, time/locale | A compact system block of ≤ 300 tokens, updated with deltas |
| On demand | Browser tab/URL/selection, clipboard, UI tree excerpt, file contents, screenshot | Exposed as tools (`get_active_tab`, `read_selection`, `get_ui_tree`, `capture_screen`), not pushed |
| Task | Project info, selected docs, relevant memories, prior task steps | Retrieved per task (FTS/embeddings) |

- **Provenance:** every context item carries a `Provenance { source, trust: User|System|Untrusted }`,
  which feeds taint tracking ([SECURITY.md §4](SECURITY.md)).
- **Tool exposure** (plan §55 and §123): tools are chosen by task class plus the capability tags of
  the resolved intent. The default cap is 20 tools per request. MCP tools are exposed only when
  relevant, by description embedding match.

## 7. Agent path (plan §39–43)

- **Planning:** only for multi-step requests. The brain produces a plan as a structured tool call
  (`propose_plan`), which becomes a Task graph. Independent steps run concurrently.
- **Validation:** tasks declare `success_criteria`. Before KIVO reports success, a verification
  step runs, such as re-running the tests, checking that the window exists, or checking that the
  file exists. "Done" is only said after validation.
- **Coding tasks** are delegated to the Coding profile's CLI agent via ACP. KIVO supervises, shows
  progress, relays permission prompts, and summarizes.

## 8. Realtime conversation mode (optional)

Research: [features-and-extensions/REPORT.md §1](../research/features-and-extensions/REPORT.md).

- **What it is:** a `RealtimeProvider` brain type that uses speech-to-speech models (OpenAI
  `gpt-realtime` family, Gemini Live native audio). The model hears and speaks directly, which gives
  the most natural turn-taking, tone and interruptions.
- **What stays local:** the wake word, VAD, the fast path and the permission engine. **The fast
  path always runs first**, so "mute" never opens a realtime session.
- **When a session opens:**
  - a conversation-type request ("let's talk about…", "practice my interview");
  - a profile set to Realtime;
  - the user toggling the mode in the card.
- **When it closes:** after N seconds of silence (15 s by default), when the user says "that's
  all", or at the budget limit.
- **Tools:** tool calls from the realtime model go through `authorize()`. While a confirmation is
  pending, the model's audio is held back.
- **Session limits:** Gemini connections last about 10 min, so the adapter implements session
  resumption and context compression. The adapter hides these limits from the user.
- **Transcripts** from the provider are shown in the card, and history is saved as usual.
- **Cost:** about $0.02–0.11/min (OpenAI) and about $0.02/min (Gemini) at research time. It is
  metered live; see §9.
- **Capability:** off by default; toggled in [CAPABILITIES.md](CAPABILITIES.md).

## 9. Usage, cost tracking and limits

**Metering:**

- Every brain, STT, TTS, realtime and computer-use call records `Usage { provider, model,
  input_tokens, output_tokens, cached_tokens, audio_seconds, images, requests }` into a `usage`
  table, with turn, task and routine ids.
- **Cost** = usage × price table.
  - The price table is bundled, derived from LiteLLM's `model_prices_and_context_window.json`
    (MIT), and refreshed from GitHub weekly (the user can turn this off).
  - Users can override the price per model, for example for negotiated rates or free tiers.
  - Local models cost $0.
  - The UI labels estimates clearly as *estimates*.

**Limits (all user-chosen; the default is "track only, no limit"):**

| Setting | Options |
|---|---|
| Budget scope | Overall · per provider · per profile · per feature (realtime, computer use, agents) |
| Period | Daily · weekly · monthly (reset day configurable) |
| Limit | None (default) · amount in the user's currency |
| Warnings | User-chosen thresholds, e.g. 50% / 80% / 95%, as a toast + card note + optional spoken warning |
| At limit | Ask each time (default when a limit is set) · switch to a cheaper profile · switch to local only · block |
| Per-task caps | Computer use (default $0.50/task), agents (default $2/task), realtime (default 30 min/session); each can be changed or set to none |

**Where it shows:**

- **Control Center → Usage:** charts by day, provider, feature and routine; the top expensive
  tasks; CSV export.
- **The card:** long tasks show a live "≈ $0.12" when the user has the cost display on.

## 10. Personas (personality and voice)

- **What a persona is:** `Persona { id, name, style_prompt, verbosity: Brief|Normal|Detailed,
  humor: Off|Light|Playful, formality, default_voice_per_lang, earcon_set }`.
- **Built-in personas** (the user picks one in onboarding and can switch any time):

  | Persona | Style |
  |---|---|
  | **Calm** (default) | Concise, neutral, precise; the plan's §144 style |
  | **Friendly** | Warm, a little conversational, still brief |
  | **Witty** | Light humour and banter, never in confirmations, errors or high-risk flows |
  | **Custom** | The user writes the style, within guardrails |

- **Guardrails:** personas can never change safety behavior, permission prompts or the honesty of
  status reports. Confirmations and errors always use neutral wording.
- **Scope:** persona is per user profile, and can be overridden per brain profile. The voice is
  chosen separately, by language.

## 11. Voice response style (plan §144)

- The system prompt for voice turns asks for short, speakable answers. There is no markdown in
  speech; the card shows the rich text while TTS gets a speakable version.
- The TTS chunker strips code blocks and says "I've put the details on screen".

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Intent router (§1–2)**

- [x] **BRAIN-01** · M1 · Grammar stage: slot-based commands (`open {app}`, volume, `mute`, media, screenshot, window commands, `lock`) resolved against live app and window indexes, with phrasing loaded from per-language data files (§2) → done: `kivo-intent` grammar from `grammar/en/commands.toml` (per-language data: fillers, patterns, slots `{app}`, `{window}`, `{number}`, `{url}`, `{text}`), normalized transcripts, slots resolved against the live app index (Start menu + AppsFolder, fuzzy with aliases) and window index; covers open/close app, window commands, volume, mute, mic, media, screenshot, lock, sleep, restart, shutdown, open URL and web search · verified: grammar/index/normalize unit tests (`the_m1_journeys_match`, `misheard_names_still_match_but_nonsense_does_not` …) and both end-to-end tests (2026-09-23)
- [x] **BRAIN-02** · M1 · Destructive commands (shutdown, restart) still go through the permission engine (§2) → done: shutdown and restart are High-risk tools; the grammar only produces a call, which goes through `authorize` like any other and always asks (except in Bypass) · verified: `destructive_commands_are_recognized_like_any_other`, policy tests (High must confirm) (2026-09-23)
- [ ] **BRAIN-03** · M3 · Semantic stage: a local embedding model + kNN over command exemplars, accepted only above a high threshold and with resolvable slots (§2)
- [ ] **BRAIN-04** · M3 · Hybrid requests go to the brain, with the fast-path tools exposed as tools (§2)
- [x] **BRAIN-05** · M1 · Metrics `fast_path_ratio` and per-stage p95 latency (§2) → done: `IntentRouter::metrics` keeps the fast-path share and the grammar stage's p95 over the last requests; the engine logs them per turn and exposes `Engine::router_metrics` · verified: `the_fast_path_ratio_and_grammar_latency_are_tracked`, `p95_uses_the_nearest_rank`, and the typed end-to-end test asserts 1 of 1 routed without AI (2026-09-23)
- [ ] **BRAIN-06** · M3 · A "That's not what I meant" misroute report in the card (§2)
- [ ] **BRAIN-07** · M5 · Event tasks ("tell me when…", "watch…", "remind me…") classified by grammar first and by the brain when ambiguous, then turned into Tasks with watchers (§2)

**Provider contract and adapters (§3–4)**

- [ ] **BRAIN-08** · M3 · `BrainProvider` trait (`info`, `health`, `models`, `chat` with cancellation), `BrainEvent` stream and `NormalizedError`, exactly as in §3
- [ ] **BRAIN-09** · M3 · Reasoning content is never shown or spoken; it is kept in the turn log only if the user enables that (§3)
- [ ] **BRAIN-10** · M3 · API adapters for Anthropic, OpenAI, Gemini and OpenRouter (native adapters where prompt caching, realtime or reasoning controls need them; `genai` for breadth) (§4)
- [ ] **BRAIN-11** · M8 · More API adapters: Groq, Mistral, DeepSeek, xAI and custom OpenAI-compatible endpoints (§4)
- [ ] **BRAIN-12** · M3 · Local adapter: OpenAI-compatible (Ollama, LM Studio, llama.cpp server, any localhost URL) (§4)
- [ ] **BRAIN-13** · M3 · `AgentSession` trait (prompt, cancel, streamed `AgentEvent`, `set_mode`) and an ACP client (JSON-RPC over stdio) (§3, §4)
- [ ] **BRAIN-14** · M3 · CLI agents over ACP: Claude Code (`claude-agent-acp`), Gemini CLI (`--acp`) and Codex (`codex-acp`) (§4)
- [ ] **BRAIN-15** · M3 · ACP `session/request_permission` is routed into KIVO's permission engine and confirmation UI; the agent's file and terminal operations show as activity (§4)
- [ ] **BRAIN-16** · M6 · The KIVO MCP server is passed in each ACP session's MCP server list (§4)
- [ ] **BRAIN-17** · M3 · No-key sign-in first: CLI login flows launched from KIVO, OpenRouter OAuth PKCE ("Connect with OpenRouter"), local auto-detect; direct API keys only under "Advanced: use your own API key" (§4)
- [ ] **BRAIN-18** · M3 · API keys are write-only from the UI, stored in Credential Manager and tested by the runtime (§4, SECURITY §5)
- [ ] **BRAIN-19** · Post · Codex App Server adapter; GitHub Copilot CLI, OpenCode and Goose over ACP (§4)

**Profiles and routing (§5)**

- [ ] **BRAIN-20** · M3 · `Profile` model and the built-in profiles Default, Fast, Smart, Coding, Private, Offline and Cheap; users can create more (§5)
- [ ] **BRAIN-21** · M3 · Deterministic routing in the §5 rule order; each decision records a one-line reason that the card shows (§5)
- [ ] **BRAIN-22** · M3 · Failover on RateLimited / ProviderDown / Network only to a fallback in the same privacy class; otherwise ask the user (§5)
- [ ] **BRAIN-23** · M3 · Health checks per provider, refreshed as in DISCOVERY §3 (§3)

**Context and streaming (§6, §11)**

- [ ] **BRAIN-24** · M3 · Always-on context block ≤ 300 tokens, updated with deltas (§6, MEMORY §4)
- [ ] **BRAIN-25** · M4 · On-demand context exposed as tools (`get_active_tab`, `read_selection`, `get_ui_tree`, `capture_screen`), not pushed (§6)
- [ ] **BRAIN-26** · M3 · Every context item carries `Provenance { source, trust }` (§6)
- [ ] **BRAIN-27** · M3 · Tool exposure by task class and capability tags, at most 20 tools per request (§6)
- [ ] **BRAIN-28** · M3 · Streaming LLM → phrase chunker → streaming TTS, so speech starts before the full answer (§1, §11)
- [ ] **BRAIN-29** · M3 · Voice style: short, speakable answers; the card shows rich text while TTS gets a speakable version; code blocks are replaced with "I've put the details on screen"; a text normalizer for numbers, URLs and units (§11)

**Agent path (§7)**

- [ ] **BRAIN-30** · M5 · Planner: multi-step requests produce `propose_plan`, which becomes a Task graph; independent steps run concurrently (§7)
- [ ] **BRAIN-31** · M5 · Validation: tasks declare `success_criteria`, and "done" is reported only after a verification step passes (§7)
- [ ] **BRAIN-32** · M5 · Coding tasks are delegated to the Coding profile's ACP agent, with progress, relayed permission prompts and a summary (§7)

**Realtime, usage and personas (§8–10)**

- [ ] **BRAIN-33** · M8 · `RealtimeProvider` (OpenAI realtime, Gemini Live): the fast path runs first; tool calls go through `authorize()`; audio is held while a confirmation is pending; session resumption hides provider limits; closes after 15 s of silence, "that's all" or the budget (§8)
- [ ] **BRAIN-34** · M3 · Usage metering into a `usage` table for every brain, STT, TTS, realtime and computer-use call, with turn, task and routine ids (§9)
- [ ] **BRAIN-35** · M3 · Cost = usage × a bundled price table (LiteLLM-derived, weekly refresh that can be turned off, per-model overrides, local = $0), always labelled as an estimate (§9)
- [ ] **BRAIN-36** · M3 · Limits with scope, period, amount, warning thresholds, at-limit action and per-task caps; the default is "track only" (§9)
- [ ] **BRAIN-37** · M7 · Usage page: charts by day, provider, feature and routine; top expensive tasks; CSV export; a live "≈ $0.12" in the card when enabled (§9)
- [ ] **BRAIN-38** · M3 · Personas Calm (default), Friendly, Witty and Custom; guardrails keep confirmations, errors and status reports neutral; set per user profile and overridable per brain profile (§10)
