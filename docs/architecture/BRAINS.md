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
| **API** | Anthropic, OpenAI, Google Gemini, OpenRouter, Groq, Mistral, DeepSeek, xAI, custom OpenAI-compatible | KIVO's own adapters behind the trait: native Anthropic and Gemini, and one OpenAI-compatible adapter for the rest (DECISIONS "Own adapters on `reqwest`") |
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
- [x] **BRAIN-03** · M3 · Semantic stage: a local embedding model + kNN over command exemplars, accepted only above a high threshold and with resolvable slots (§2) → done: `kivo-intent/src/semantic.rs` (kNN over `grammar/en/exemplars.toml`, threshold 0.72 with margin and consensus, slots resolved by the grammar) backed by all-MiniLM-L6-v2 run by `kivo-voice/src/embed.rs` (own WordPiece, `ort`), pinned in the model catalog and loaded when the user has it (DECISIONS "Semantic stage") · verified: tokenizer matches BERT's reference ids; real-model test: 8 unseen paraphrases resolve, 8 other requests (incl. two-action and ambiguous ones) don't; router tests (2026-09-23)
- [x] **BRAIN-04** · M3 · Hybrid requests go to the brain, with the fast-path tools exposed as tools (§2) → done: requests joining two actions skip the semantic stage and go to the brain, which gets the fast-path tools as tools (≤ 20, BRAIN-27) · verified: `the_brains_tool_calls_go_through_the_permission_engine` (mute + launch + close through tools), semantic tests reject "open chrome and search for cats" (2026-09-23)
- [x] **BRAIN-05** · M1 · Metrics `fast_path_ratio` and per-stage p95 latency (§2) → done: `IntentRouter::metrics` keeps the fast-path share and the grammar stage's p95 over the last requests; the engine logs them per turn and exposes `Engine::router_metrics` · verified: `the_fast_path_ratio_and_grammar_latency_are_tracked`, `p95_uses_the_nearest_rank`, and the typed end-to-end test asserts 1 of 1 routed without AI (2026-09-23)
- [x] **BRAIN-06** · M3 · A "That's not what I meant" misroute report in the card (§2) → done: "That's not what I meant" on the Island's answer and on each Chat reply records the turn, its route and note in `misroutes` and the router's metrics (`chat.misroute`) · verified: `the_control_centers_brain_requests`, `Chat.test.tsx`, Island tests (2026-09-23)
- [ ] **BRAIN-07** · M5 · Event tasks ("tell me when…", "watch…", "remind me…") classified by grammar first and by the brain when ambiguous, then turned into Tasks with watchers (§2)

**Provider contract and adapters (§3–4)**

- [x] **BRAIN-08** · M3 · `BrainProvider` trait (`info`, `health`, `models`, `chat` with cancellation), `BrainEvent` stream and `NormalizedError`, exactly as in §3 → done: `kivo-brain/src/provider.rs` + `types.rs`: `BrainProvider` (`info`, `health`, `models`, `chat` with a `CancellationToken`), `BrainEvent` stream, `NormalizedError` · verified: 10 adapter contract tests (text, tool calls, usage, errors, cancel < 100 ms) (2026-09-23)
- [x] **BRAIN-09** · M3 · Reasoning content is never shown or spoken; it is kept in the turn log only if the user enables that (§3) → done: reasoning deltas never reach the card or the voice; with `brains.keep-reasoning` on they are kept in the turn record's `reasoning` column only · verified: adapter tests (reasoning separate from text), engine code path (2026-09-23)
- [x] **BRAIN-10** · M3 · API adapters for Anthropic, OpenAI, Gemini and OpenRouter (native adapters where prompt caching, realtime or reasoning controls need them; `genai` for breadth) (§4) → done: native Anthropic (prompt caching) and Gemini (schema trimming) adapters, and one OpenAI-compatible adapter for OpenAI and OpenRouter (plus Groq, Mistral, DeepSeek, xAI, custom) on a shared streaming `reqwest` layer (DECISIONS "Own adapters on `reqwest`") · verified: `contract_tests.rs` against recorded SSE for each (2026-09-23)
- [ ] **BRAIN-11** · M8 · More API adapters: Groq, Mistral, DeepSeek, xAI and custom OpenAI-compatible endpoints (§4)
- [x] **BRAIN-12** · M3 · Local adapter: OpenAI-compatible (Ollama, LM Studio, llama.cpp server, any localhost URL) (§4) → done: the OpenAI-compatible adapter for local servers (Ollama, LM Studio, llama.cpp, any localhost URL), no key needed, free and private · verified: `local_servers_need_no_key_and_health_follows_the_model_list`, contract tests (2026-09-23)
- [x] **BRAIN-13** · M3 · `AgentSession` trait (prompt, cancel, streamed `AgentEvent`, `set_mode`) and an ACP client (JSON-RPC over stdio) (§3, §4) → done: `kivo-brain/src/acp.rs`: ACP client (newline JSON-RPC over stdio), `AcpSession` (prompt with streamed `AgentEvent`, cancel via `session/cancel`, `set_mode`, resume with `session/load`) · verified: 8 ACP tests incl. a dying agent process (2026-09-23)
- [~] **BRAIN-14** · M3 · CLI agents over ACP: Claude Code (`claude-agent-acp`), Gemini CLI (`--acp`) and Codex (`codex-acp`) (§4) → partial: KIVO runs CLI agents over ACP with the catalog's entry points (Claude Code `claude-agent-acp`, Gemini CLI `--acp`, Codex `codex-acp`, OpenCode `acp`); live handshakes with the installed Gemini CLI and OpenCode pass (`KIVO_TEST_REAL_AGENTS=1`), and the whole agent turn is tested with a scripted ACP agent · missing: a live run with Claude Code and Codex, whose ACP adapters aren't installed on this PC (owner: `npm install -g @zed-industries/claude-agent-acp @zed-industries/codex-acp`)
- [x] **BRAIN-15** · M3 · ACP `session/request_permission` is routed into KIVO's permission engine and confirmation UI; the agent's file and terminal operations show as activity (§4) → done: an agent's `session/request_permission` becomes a KIVO action (`agent.read` … `agent.execute`, DECISIONS "Agents' requests") decided by `authorize()`, shown on the card and answerable by click or voice; its plan, tool work and file changes show as steps · verified: `claude_code_asks_for_permission_on_the_card_and_follow_ups_reuse_its_session` (2026-09-23)
- [ ] **BRAIN-16** · M6 · The KIVO MCP server is passed in each ACP session's MCP server list (§4)
- [x] **BRAIN-17** · M3 · No-key sign-in first: CLI login flows launched from KIVO, OpenRouter OAuth PKCE ("Connect with OpenRouter"), local auto-detect; direct API keys only under "Advanced: use your own API key" (§4) → done: Brains and onboarding lead with no-key options: the CLI's own login in a terminal (`brains.signIn`), "Connect with OpenRouter" (OAuth PKCE, key straight into Credential Manager), local servers found on this PC; API keys under "Use your own API key" · verified: `openrouter_oauth_ends_with_a_stored_key_and_no_copying`, `a_cli_sign_in_opens_the_clis_own_login`, Brains and onboarding tests (2026-09-23)
- [x] **BRAIN-18** · M3 · API keys are write-only from the UI, stored in Credential Manager and tested by the runtime (§4, SECURITY §5) → done: keys go UI → runtime once (`brains.setKey`), are tested against the provider first, stored in Credential Manager as `secret://kivo/<provider>/api-key`, and never returned (views say only `hasKey`) · verified: `the_control_centers_brain_requests` (refused key not stored, key absent from list and config), `testing_a_key_does_not_save_it`, `Brains.test.tsx` (2026-09-23)
- [ ] **BRAIN-19** · Post · Codex App Server adapter; GitHub Copilot CLI, OpenCode and Goose over ACP (§4)

**Profiles and routing (§5)**

- [x] **BRAIN-20** · M3 · `Profile` model and the built-in profiles Default, Fast, Smart, Coding, Private, Offline and Cheap; users can create more (§5) → done: `Profile` with the seven built-ins; users can change a built-in (reset restores it) or add their own (Brains → Profiles, `brains.saveProfile`) · verified: `user_profiles_override_built_ins_and_add_their_own`, routing tests (2026-09-23)
- [x] **BRAIN-21** · M3 · Deterministic routing in the §5 rule order; each decision records a one-line reason that the card shows (§5) → done: `routing.rs` in the §5 order with a one-line reason ("Coding · Claude Code — because this looked like a coding task") in the card chip, Activity and the turn record · verified: 12 routing tests, `an_open_question_is_answered_by_the_default_brain_with_a_reason` (2026-09-23)
- [x] **BRAIN-22** · M3 · Failover on RateLimited / ProviderDown / Network only to a fallback in the same privacy class; otherwise ask the user (§5) → done: on RateLimited/ProviderDown/Network with nothing said yet, the next fallback of the same privacy class answers and the chip says so; otherwise a plain message · verified: `a_busy_brain_fails_over_within_its_privacy_class` (local brain untouched), routing failover tests (2026-09-23)
- [x] **BRAIN-23** · M3 · Health checks per provider, refreshed as in DISCOVERY §3 (§3) → done: health per provider from its model list, cached, refreshed as DISC-14 describes, re-checked after errors · verified: `brains::tests` (8), `a_failing_provider_is_reported_plainly_and_kivo_carries_on` (2026-09-23)

**Context and streaming (§6, §11)**

- [x] **BRAIN-24** · M3 · Always-on context block ≤ 300 tokens, updated with deltas (§6, MEMORY §4) → done: the live block (time, language, permission mode, active app, window title as untrusted) within 300 tokens in every request, with "since the last message" deltas; agents get the block once, then deltas (MEM-11) · verified: `an_open_question…` checks the live context in the request; context tests (2026-09-23)
- [x] **BRAIN-25** · M4 · On-demand context exposed as tools (`get_active_tab`, `read_selection`, `get_ui_tree`, `capture_screen`), not pushed (§6) → done: on-demand context tools: `browser.active_tab`, `context.selection` (the page's or the focused app's selection through UIA TextPattern), `uia.get_tree`, `screen.read` / `screen.look`; nothing is pushed · verified: `the_selection_is_read_on_demand_and_untrusted`, browser and screen tool tests (2026-09-23)
- [x] **BRAIN-26** · M3 · Every context item carries `Provenance { source, trust }` (§6) → done: `ContextItem { text, provenance: { source, trust } }`; untrusted items are fenced `<untrusted>` (window titles, attachments) · verified: context tests (2026-09-23)
- [x] **BRAIN-27** · M3 · Tool exposure by task class and capability tags, at most 20 tools per request (§6) → done: `select_tools` by task class, namespace hints and profile scope, at most 20, wire names `apps__launch` · verified: context tests, requests in the e2e tests carry ≤ 20 tools (2026-09-23)
- [x] **BRAIN-28** · M3 · Streaming LLM → phrase chunker → streaming TTS, so speech starts before the full answer (§1, §11) → done: streamed deltas → `speech::Chunker` (first phrase at a clause break) → phrases queued to the speech worker while the answer still streams · verified: `a_spoken_answer_starts_before_the_brain_finishes`, chunker tests (2026-09-23)
- [x] **BRAIN-29** · M3 · Voice style: short, speakable answers; the card shows rich text while TTS gets a speakable version; code blocks are replaced with "I've put the details on screen"; a text normalizer for numbers, URLs and units (§11) → done: voice turns ask for short speakable answers; the card shows the rich text; speech drops markdown, says "I've put the details on screen" for code, and normalizes numbers, money, units and addresses · verified: speech tests (2026-09-23)

**Agent path (§7)**

- [ ] **BRAIN-30** · M5 · Planner: multi-step requests produce `propose_plan`, which becomes a Task graph; independent steps run concurrently (§7)
- [ ] **BRAIN-31** · M5 · Validation: tasks declare `success_criteria`, and "done" is reported only after a verification step passes (§7)
- [ ] **BRAIN-32** · M5 · Coding tasks are delegated to the Coding profile's ACP agent, with progress, relayed permission prompts and a summary (§7)

**Realtime, usage and personas (§8–10)**

- [ ] **BRAIN-33** · M8 · `RealtimeProvider` (OpenAI realtime, Gemini Live): the fast path runs first; tool calls go through `authorize()`; audio is held while a confirmation is pending; session resumption hides provider limits; closes after 15 s of silence, "that's all" or the budget (§8)
- [x] **BRAIN-34** · M3 · Usage metering into a `usage` table for every brain, STT, TTS, realtime and computer-use call, with turn, task and routine ids (§9) → done: every brain and agent request writes a `usage` row with turn/task/routine ids (`Brains::meter`), pushed as `usageRecorded` · verified: `usage_is_metered_…`, e2e usage checks · note: STT/TTS/realtime/computer-use metering joins with cloud speech (M8), realtime (M8) and computer use (M7), which are local-only or not built yet
- [x] **BRAIN-35** · M3 · Cost = usage × a bundled price table (LiteLLM-derived, weekly refresh that can be turned off, per-model overrides, local = $0), always labelled as an estimate (§9) → done: cost = usage × the bundled LiteLLM-derived table (703 models, MIT), weekly refresh (can be turned off), per-model overrides, local = $0, labelled estimates; CSV export · verified: cost tests, `usage_is_metered_with_a_cost_estimate_and_limits_add_up`, `the_control_centers_brain_requests` (2026-09-23)
- [x] **BRAIN-36** · M3 · Limits with scope, period, amount, warning thresholds, at-limit action and per-task caps; the default is "track only" (§9) → done: limits by scope, period (local reset day), amount, warnings and at-limit action (ask on the card, cheaper profile, local only, block) plus task caps; default track only · verified: `a_spending_limit_can_block_cloud_requests`, limit tests (2026-09-23)
- [ ] **BRAIN-37** · M7 · Usage page: charts by day, provider, feature and routine; top expensive tasks; CSV export; a live "≈ $0.12" in the card when enabled (§9)
- [x] **BRAIN-38** · M3 · Personas Calm (default), Friendly, Witty and Custom; guardrails keep confirmations, errors and status reports neutral; set per user profile and overridable per brain profile (§10) → done: Calm (default), Friendly, Witty and Custom with guardrails after the style; chosen on the Voice page, overridable per profile · verified: persona tests, Voice page test (2026-09-23)
