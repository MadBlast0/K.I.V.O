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

## 8. Voice response style (plan §144)

- The system prompt for voice turns asks for short, speakable answers. There is no markdown in
  speech; the card shows the rich text while TTS gets a speakable version.
- The TTS chunker strips code blocks and says "I've put the details on screen".
