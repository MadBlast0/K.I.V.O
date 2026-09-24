# Conversation, context, memory and other AIs

Status: Draft v1, 2026-09-21. Owner requests:

- voice-first confirmations;
- ChatGPT-like open conversation;
- how much context to keep for each kind of model;
- free options;
- whether KIVO needs `CLAUDE.md`/`AGENTS.md`-style files;
- memory graphs and memory MCPs;
- KIVO driving *other* AIs (terminal agents, Claude Desktop, ChatGPT).

Related specs: [BRAINS.md](BRAINS.md), [MEMORY.md](MEMORY.md), [VOICE.md](VOICE.md),
[SECURITY.md](SECURITY.md), [TOOLS_AND_CONTROL.md](TOOLS_AND_CONTROL.md).

---

## 0. What the user experiences: no session management required

**Answer to "sessions or no sessions?"** KIVO **uses sessions internally, and the user never has
to manage them.**

- **Talk anytime:** KIVO continues the current conversation automatically, or starts a fresh one
  when enough time has passed or the topic clearly changed.
- **Context is automatic:** the right context (§8) is loaded without the user doing anything.
- **Old conversations are never lost:** they're summarized, and searchable in Chat.
- **Power users get full control:** they can open Chat to rename, pin, continue, branch, export or
  delete conversations; see the context meter; press "Compact now"; and edit exactly what context
  the AI starts with (Settings → Context).

Commands like "mute" or "open Chrome" **don't use a session or any AI at all**; they cost nothing.
A session only matters when a brain is involved.

## 1. Sessions and threads

| Unit | What it is | Lifetime |
|---|---|---|
| **Turn** | One request → response | Seconds |
| **Voice session** | The Island conversation, including follow-ups without the wake word | Ends after 2 min of silence (setting) |
| **Thread** | A conversation history, like a ChatGPT chat. Voice sessions within 30 min, on the same topic, join the same thread. "Kivo, new topic" starts a new one. Typed chats pick a thread explicitly | Until the retention period (30 days default) or pinned |
| **Agent session** | A CLI agent's own session (Claude Code, Codex, Gemini CLI) that KIVO started or attached to | As long as the agent keeps it; KIVO stores the session id so it can resume it |

**Chat** in the Control Center gives the open-ended, ChatGPT-style experience. It can be used by
voice ("Kivo, let's just talk") or by typing, it has unlimited history, and it can switch brains
at any point.

## 2. Context: how much to keep, per model

**KIVO never sends the whole history.** Each request is assembled within a **context budget**
that depends on the model's context window, its cost, and whether this is a voice turn:

| Model class | Examples | Budget per request (default) |
|---|---|---|
| Small local | 3–8B via Ollama, 8–32k window | 50% of window, capped at 8k |
| Large local | 14–70B, 32–128k | 40% of window, capped at 24k |
| Cloud API | 200k–1M+ windows | 32k for chat, **12k for voice turns** (speed and cost). Raised on demand for documents and code |
| CLI agents | Claude Code, Codex, Gemini CLI | The agent manages its own context. KIVO sends only a handoff (§5) |
| Realtime voice | gpt-realtime, Gemini Live | The provider's session context plus compression; KIVO injects a summary when a session starts |

Budgets are user-adjustable in Brains → Profile → Context.

**Assembly order** (the first items get guaranteed space):

1. Persona + safety system block (about 800 tokens).
2. **Instructions**: global plus active workspace (≤ 1.5k; §4).
3. **Always-on context** as deltas: active app, time, permission mode (≤ 500).
4. **Relevant memories**, retrieved (≤ 1k; §6).
5. **Running summary** of the thread (≤ 1.5k).
6. **Recent turns**, verbatim (fills the rest; always at least the last 4 turns).
7. **Recalled older messages** from this thread, by semantic search, only if they're relevant.
8. **Tool schemas**, minimal and dynamic (BRAINS §6).

**Compaction:**

- **When:** the verbatim tail goes over its share of the budget.
- **How:** the oldest turns are summarized into the thread's **running summary**. The summary is
  made by the cheapest suitable model (local if available). Summaries are stored, so nothing is
  lost: the full history stays in SQLite and can be searched and recalled.
- **Memory proposals:** at compaction, KIVO also extracts **memory candidates** (§6).
- **Prompt caching:** API adapters mark the stable prefix (items 1–2) as cacheable where providers
  support it (Anthropic, OpenAI, Gemini). This makes long conversations much cheaper.

## 3. Free and low-cost ways to talk to KIVO

KIVO never pays for users' AI; users bring their own accounts. The onboarding and Brains page
**label free options clearly**:

| Option | Cost | Notes |
|---|---|---|
| **Local models** (Ollama, LM Studio) | Free, unlimited | Needs a decent PC; fully private |
| **Gemini CLI**, signed in with a personal Google account | **Free**: about 1,000 requests/day, 60/min | The best free cloud option. It uses Gemini 3 models with a 1M context. An unpaid API key gets less ([Gemini CLI quotas](https://geminicli.com/docs/resources/quota-and-pricing/), [guide](https://inventivehq.com/blog/gemini-cli-free-tier-guide)) |
| **Codex**, signed in with ChatGPT | Included in ChatGPT **Free** (small allowance), Go, Plus, Pro | Token-based limits shared with Codex web/IDE since 2026-04 ([OpenAI help](https://help.openai.com/en/articles/11369540-using-codex-with-your-chatgpt-plan), [pricing guide](https://www.morphllm.com/codex-pricing)) |
| **OpenRouter** free models | Free, rate-limited | Sign in with OpenRouter OAuth; paid models are optional |
| Claude Code | Needs a paid Claude plan or API | — |
| Direct APIs | Pay per use | Advanced option |

**ChatGPT's regular chat** (chatgpt.com) has no API for third-party apps. The sanctioned routes
are Codex sign-in (above), or **driving the ChatGPT desktop app** as an app integration (§5.3),
which works but is less reliable.

## 4. Instructions: KIVO's equivalent of `CLAUDE.md` / `AGENTS.md`

**Does KIVO need such files?** KIVO needs the *concept*, not files in users' projects.

| Layer | What | Where it lives |
|---|---|---|
| **Global instructions** ("About me") | Name, language, tone, standing preferences ("use metric", "I'm on Windows 11, VS Code, PowerShell") | KIVO data folder, edited in Memory → Instructions |
| **Workspace instructions** | Notes for a project/folder KIVO works in ("tests: cargo nextest", "never touch /vendor") | KIVO data folder, per workspace |
| **Project files the agents already use** | `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` in the repo | Owned by the project. **KIVO reads them** when working in that folder, but doesn't write them unless the user asks ("Kivo, add this to CLAUDE.md") |

**Storage:**

- Everything is stored in SQLite and mirrored as **readable Markdown**:
  - `%APPDATA%\KIVO\instructions\global.md`
  - `…\workspaces\<name>\instructions.md`
- The user can edit the files directly; KIVO watches them and re-imports on change.
- One-click **"Export as AGENTS.md to project"** is offered but never automatic.

**Workspaces** are detected, not configured:

- **How:** the foreground VS Code folder, a terminal's working directory, a git repo KIVO acted
  in, or "in this project".
- **What a workspace record holds:** path, name, instructions, memories, recent threads and
  agent sessions, preferred agent and mode.
- **Confirmation:** KIVO asks once, "Remember *kivo-runtime* as a workspace?"

## 5. KIVO driving other AIs

**Goal:** the user can say, for example, "Hey Kivo, open a terminal in K.I.V.O and start Claude
with bypass permissions", then "tell that Claude to also add tests", or "in Claude Desktop,
prompt this". KIVO acts as a **voice front-end and conductor** for other AIs.

### 5.1 KIVO-run agent sessions (preferred)

- **How:** KIVO starts the agent itself over **ACP** (headless), in the chosen workspace and mode.
- **Talking to it:** progress shows in the Island, Tasks and Chat. "Tell Claude …" sends a
  follow-up prompt into **the same session**.
- **Permissions:** the agent's permission requests come back to KIVO and can be answered by voice
  (§7).
- **Open in terminal:** hands the session to a visible terminal where the agent supports resuming
  (for example `claude --resume <session-id>`, `codex resume`), so the user can take over.

### 5.2 Visible terminal agents

"Open a terminal in K.I.V.O and start Claude with bypass permissions":

1. **Launch:** KIVO runs Windows Terminal with the working directory and the agent command.
   - Mode flags are mapped per agent, for example Claude Code `--permission-mode` or
     `--dangerously-skip-permissions`, and Codex approval/sandbox flags. Exact flags are kept in
     the app registry and verified per version.
   - **Launching any agent in a bypass/"yolo" mode is High risk**, so it always confirms (except in
     KIVO's own Bypass mode).
2. **Tracking:** KIVO tracks the new window/process as a **terminal agent session** with a name
   ("Claude · K.I.V.O").
3. **Sending prompts:**
   - KIVO focuses that window and pastes the prompt through UIA/clipboard.
   - It **waits for the user's "send"** (§5.4) before pressing Enter.
   - It reads the response through UIA TextPattern, so it can summarize or speak it ("Claude says
     the tests pass").
4. **Limit:** typing into a terminal is less reliable than ACP. Where both are possible, KIVO
   suggests §5.1 ("Want me to run it in the background so I can follow along?").

### 5.3 Desktop AI apps (Claude Desktop, ChatGPT, Copilot)

- **Integration:** app-registry entries with UIA hints: locate the composer, the model/project
  selector, the send button and the latest reply.
- **Flow:** "In Claude Desktop, in the K.I.V.O project, prompt: …"
  1. Open or focus the app.
  2. Select the project if the app has one.
  3. Paste the prompt.
  4. **Draft review** (§5.4).
  5. Press send.
  6. Optionally read the reply.
- **Reliability:** UI changes can break it, so each entry is versioned and falls back to "I've put
  the prompt in; press Enter when ready".

### 5.4 Prompt drafting, by voice

For anything KIVO will type into another AI, the Island shows a **Draft** card:

- the target, for example "Claude · terminal · K.I.V.O";
- the prompt text;
- buttons **Send · Edit · Cancel**;
- the voice hints "Say *send*, *add …*, *read it back*, or *cancel*".

Voice edits apply live ("add: also run the tests", "remove the last sentence"). Sending is
Medium risk, so it follows the permission mode, and in Ask mode "send" is the confirmation.

## 6. Memory v2: KIVO as the user's memory hub

**Built-in, local, no bundled third-party memory server.** Popular memory MCPs don't fit
bundling:

- Mem0 is now hosted-only; its local OpenMemory was removed in 2026-07.
- Basic Memory is AGPL-3.0.
- Graphiti needs a graph database.
- The official reference server is minimal.

Sources: [comparison](https://hjarni.com/blog/best-mcp-memory-server),
[ChatForest](https://chatforest.com/guides/best-memory-mcp-servers/),
[Basic Memory](https://github.com/basicmachines-co/basic-memory),
[Graphiti vs Mem0](https://renezander.com/guides/graphiti-vs-mem0/).

KIVO borrows the good ideas, with its own implementation:

| Idea | From | KIVO implementation |
|---|---|---|
| Entities + relations + observations (knowledge graph) | Official MCP memory server (MIT) | SQLite tables `entities`, `relations`, `observations` |
| Facts with validity windows ("was true until…") | Graphiti | `valid_from` / `valid_to` on observations; newer facts supersede older ones, and history is kept |
| Plain Markdown the user can read and edit | Basic Memory (concept only; no AGPL code) | Markdown mirror in `%APPDATA%\KIVO\memory\` (`people/`, `workspaces/`, `topics/`), re-imported on edit |
| Hybrid search | Common practice | FTS5 + `sqlite-vec` embeddings |

**Capture modes** (Memory → settings):

| Mode | Behavior |
|---|---|
| Only when I ask | "Remember …" or the Remember button |
| **Suggest** (default) | KIVO proposes memories at compaction or after tasks. The user accepts, edits or dismisses them |
| **Workspace notes (automatic)** | KIVO automatically keeps notes **for the workspaces it works in**: what was done, decisions, commands that worked. Stored per workspace and shown in the Memory page |

**KIVO never passively records everything the user does on the PC.** Memory only comes from
conversations and tasks KIVO took part in.

**Owner decision (2026-09-21): Suggest *and* Workspace notes are both on by default**, and both
can be switched off. Notes must be *smart*, meaning small, organized and non-duplicating:

**Vault format (Obsidian-compatible):**

```text
%APPDATA%\KIVO\memory\            ← a plain Markdown "vault"; open it in Obsidian for graph view
├─ about-me.md
├─ people/maya.md
├─ workspaces/k-i-v-o/
│   ├─ overview.md                ← condensed, always-current summary of the project
│   ├─ decisions.md               ← dated decisions
│   └─ log/2026-09-21.md          ← session notes (auto-condensed later)
├─ topics/rust-testing.md
└─ .kivo/                         ← index, embeddings, graph cache (not for editing)
```

- **Format:** each note is plain Markdown with YAML front-matter (`type`, `tags`, `workspace`,
  `sensitivity`, `created`, `updated`, `valid_until`) and `[[wikilinks]]` between notes. Obsidian
  (or any Markdown tool) shows the **graph, tags and backlinks** with no plugin.
- **The Memory page** navigates the vault by **tags and folders**, and has **Open folder** and
  **Open in Obsidian** (the `obsidian://open?path=…` URI) buttons.
- **SQLite:** stays the index (FTS, embeddings, graph edges) and is rebuilt from the files, so
  the Markdown files are the source of truth and user edits always win.

**Note detail level** (Memory settings): **Brief** · **Standard** (default) · **Detailed**.

- **Session log entries:** Brief is 1–3 bullets, Standard ≤ 10 bullets, Detailed a full summary
  with commands and outcomes.

**Staying small** (a background "tidy" job, local model preferred, shown in Activity):

| Rule | Default |
|---|---|
| Merge duplicates and near-duplicates (embedding similarity + same entity) | On |
| Condense daily logs older than 14 days into `overview.md` / `decisions.md`, then archive the logs | On |
| Mark superseded facts (`valid_until`) instead of deleting them | On |
| Per-workspace size cap: after 50 KB of notes, condense harder | On |
| Never keep secrets, credentials or content from password fields; flag personal data as `sensitivity: personal` | Always |
| Ask before creating a new workspace or person note | On (Suggest) |

**Built-in memory MCP:** KIVO's MCP server exposes `memory.search`, `memory.read`,
`memory.write_note` (permission-gated) and `memory.tags`. That makes KIVO's vault the memory
backend for Claude Code, Codex or any MCP client the user allows, so a separate memory server
isn't needed.

**Sharing memory with other AIs:**

- The KIVO MCP server exposes read-only `memory.search` / `memory.get`, and optionally
  `memory.add`, to agents KIVO launches, so Claude Code in a project gets the user's workspace
  notes.
- Each agent must be allowed first, and sensitive memories are never shared with cloud agents
  unless the user allows it.

**External memory services** (Mem0 hosted, a self-hosted Graphiti, Basic Memory) can be added as
**Connectors** by users who want them. They are not bundled.

## 8. Context layers: what the AI starts with, and what it costs

Every AI app gives the model some starting context (like Claude Code reading `CLAUDE.md`). KIVO
makes each layer **explicit, small, cached and user-controllable**:

| # | Layer | Default size | Loaded when | Setting |
|---|---|---|---|---|
| 1 | **System prompt**: KIVO core, safety rules, output style for voice | ~600 tokens | Always (brain turns) | View only; the style is editable via Persona |
| 2 | **Personal instructions** ("About me") | ≤ 400 | Always | Edit, turn off |
| 3 | **Workspace instructions** + the project's `CLAUDE.md`/`AGENTS.md` summary | ≤ 800 | Only when in a workspace | Per workspace |
| 4 | **Live context** (active app, time, permission mode), sent as deltas | ~100 | Always | Choose fields |
| 5 | **Memory**: top relevant items | ≤ 600 | Retrieved per request | Capture mode, max items |
| 6 | **Skills index**: name and one-line description per enabled skill | ~40 each | Always, but the **full skill loads only when used** | Enable/disable per skill |
| 7 | **Tools**: only the tools relevant to the request | 0–2k | Per request (BRAINS §6) | Enable/disable tools, MCP servers, connectors |
| 8 | **Conversation**: running summary + recent turns | Rest of budget | Brain turns | Budget per profile |

**How start-up context stays cheap:**

1. **Tiny by default.** A new session costs roughly **1.5k tokens** before the user's message,
   without a workspace or skills. Heavy items (tools, full skills, files) are **lazy-loaded**,
   only when the request needs them. This follows the Agent Skills "progressive disclosure"
   pattern.
2. **Prompt caching.** Layers 1, 2, 3 and 6 form a stable prefix that is marked cacheable.
   Cached reads cost about **10% of normal input** at Anthropic, OpenAI (GPT-5.x) and Gemini 2.5+.
   So after the first message, the start-up context is almost free.
   ([Eden AI](https://www.edenai.co/post/prompt-caching-claude-vs-gpt-vs-gemini-cost-playbook),
   [LeanLM](https://leanlm.ai/blog/prompt-caching))
3. **Zero tokens for the fast path**, and local models cost nothing anyway.
4. **CLI agents** (Claude Code, Codex, Gemini CLI) load their *own* project files. KIVO adds only
   a short handoff (the request + ≤ 300 tokens of relevant memory), so context isn't duplicated.
5. **Transparency.** Settings → Context shows each layer's size, an estimated cost per new
   session for the current brain, and a **"Preview what the AI sees"** button that renders the
   exact assembled prompt, with secrets redacted.

**Settings → Context** (advanced; defaults just work; built as the Context tab of the Brains page, DECISIONS "M7 build"):

- toggles and edit links per layer;
- the per-profile budget;
- auto-compaction on/off and its threshold;
- "Start each conversation fresh" (no summary carry-over);
- the Preview.

## 9. Skills

- **Format:** KIVO supports the **Agent Skills open standard** (`SKILL.md` folders), adopted by
  Claude, Codex, Gemini CLI, Copilot, Cursor and more.
  ([agentskills overview](https://inference.sh/blog/skills/agent-skills-overview),
  [cross-tool guide](https://codex.danielvaughan.com/2026/05/05/agent-skills-open-standard-portable-skills-codex-cli-cross-agent/))
- **Where skills come from:** `%APPDATA%\KIVO\skills\`, imported from a folder or zip, created
  from a routine, or shared by the agents KIVO launches (with permission).
- **Loading:** only the name and description stay in context; the body loads when the brain
  decides to use the skill.
- **Scripts:** scripts inside skills run through KIVO's shell tool, under the permission engine.
  Skills from the internet are untrusted, so KIVO shows what they contain before enabling them.
- **UI:** the Skills page lists, enables/disables, views and deletes skills, with a source badge
  for each.

## 7. Conversational confirmations (voice)

When the Island asks for a decision (confirmation, plan approval, prompt draft, bypass prompt):

1. **A distinct "question" earcon** plays (a rising 2-note motif).
2. KIVO **listens without the wake word** for 10 s (extended while the user is talking). A mic
   ring shows on the Island.
3. The Island shows voice hints, for example: *Say "approve", "cancel", or "wait"*.
4. **A small local grammar**, so no AI call and it's instant, understands:

| Intent | Examples (EN / HI) |
|---|---|
| Approve | "yes", "approve", "go ahead", "proceed", "do it", "send it" · "haan", "karo", "theek hai" |
| Approve with scope | "always for this project", "allow for this session" |
| Deny | "no", "cancel", "don't", "deny" · "nahi", "mat karo" |
| Defer | "wait", "hold on", "let me think" · "ruko", "ek minute". The card stays and the Island shows **Waiting for you**, with no timeout |
| Edit | "change it to …", "edit", "add …" (goes to the brain) |
| Explain | "why?", "what will it do?" · KIVO explains the action and its risk, then asks again |

**Security** ([SECURITY.md](SECURITY.md)):

- **Medium risk:** voice approval is accepted when speaker recognition says it's the owner, or
  when recognition is off and the user is signed in on the device.
- **High risk:** "approve" by voice triggers **Windows Hello**. Face recognition is hands-free, so
  the flow stays conversational. A click works too. Voice alone is never enough for High, because
  replayed or cloned voices are possible.
- Guest voices can't approve.
- **Speech during TTS:** voice approvals heard while KIVO's own TTS is playing are ignored, so it
  can't approve itself.

**Audio feedback:**

- after an approval: a soft "approved" tick;
- after a denial or cancel: a "cancelled" tone;
- after Defer: no sound;
- all belong to the chosen sound set (VOICE.md §6).

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

**Sessions and threads (§0–1)**

- [x] **CONV-01** · M3 · Voice sessions end after 2 min of silence (setting); threads join voice sessions within 30 min on the same topic; "Kivo, new topic" starts a new thread; typed chats choose a thread (§1) → done: voice sessions end after `brains.voice-session-minutes` (2) of silence; a new session joins a thread from the last 30 min on the same topic; "Kivo, new topic" starts one; Chat picks its thread · verified: `follow_ups_continue_the_thread_and_new_topic_starts_another` (2026-09-23)
- [x] **CONV-02** · M3 · Agent sessions: the CLI agent's session id is stored so KIVO can resume it (§1) → done: the agent's session id is stored per agent and workspace and resumed with `session/load` when the agent restarts · verified: the Claude Code e2e test checks the stored id; `a_stored_session_is_resumed_when_the_agent_can` (2026-09-23)
- [x] **CONV-03** · M7 · Chat power features: rename, pin, continue, branch, export, delete, context meter, Compact now (§0) → done: the thread menu has rename, pin, Continue by voice (`chat.continue`: the next voice request joins the thread), Branch and "Branch from here" on an answer (`chat.branch`, the summary carried when every message it covers is), Export as Markdown or JSON to Downloads (`chat.export`), Compact now and delete; the context meter shows between turns from `brains.context` · verified: `Chat.test.tsx` power features, `brain_turns::follow_ups_continue_the_thread_and_new_topic_starts_another` (continue), `the_control_centers_brain_requests` (branch, export, continue), `kivo-store` `a_branch_copies_up_to_a_message_and_keeps_the_summary` (2026-09-24)

**Context budgets and compaction (§2)**

- [x] **CONV-04** · M3 · Per-request budget by model class (small local, large local, cloud chat 32k / voice 12k, CLI handoff, realtime), user-adjustable per profile (§2) → done: `budget()` per model class (small/large local, cloud 32k chat / 12k voice, agent handoff), overridable per profile (max context) · verified: budget tests (2026-09-23)
- [x] **CONV-05** · M3 · Assembly order 1–8 with guaranteed space for the first items and at least the last 4 turns verbatim (§2) → done: `assemble()` in the 1–8 order with guaranteed first layers and at least the last four turns verbatim · verified: context tests, compaction e2e (older turn dropped, last four kept) (2026-09-23)
- [x] **CONV-06** · M3 · Compaction into a stored running summary by the cheapest suitable model; full history kept in SQLite and recallable by semantic search (§2) → done: older turns are summarized into the thread's stored running summary by the cheapest suitable brain, automatically and by "Compact now"; the full history stays in SQLite; earlier messages of the thread are recalled into a request by words (FTS5) and by meaning — MiniLM vectors of every kept message in sqlite-vec (`message_vectors`, profile-scoped, embedded in the background with the notes) — fused by rank (`Memory::find_messages`) · verified: `earlier_messages_are_recalled_by_meaning_within_their_thread` (real MiniLM: "when does my plane depart" finds "Our flight to Lisbon leaves on the third" with no word in common; nothing from another thread or still in the request), `message_vectors_are_searched_and_go_with_their_conversation`, `follow_ups_continue_the_thread_and_new_topic_starts_another` (2026-09-24)
- [x] **CONV-07** · M3 · Prompt caching: the stable prefix (layers 1, 2, 3, 6) marked cacheable for Anthropic, OpenAI and Gemini (§2, §8) → done: the stable prefix (system, instructions, workspace, skills) is marked cacheable; Anthropic gets `cache_control`, OpenAI and Gemini cache the leading blocks themselves · verified: contract tests, e2e request check (2026-09-23)

**Free options (§3)**

- [x] **CONV-08** · M3 · Free and low-cost options labelled "Free" in onboarding and Brains (local models, Gemini CLI, Codex with ChatGPT, OpenRouter free models) (§3) → done: free options are labelled in the catalog and shown as "Free" in Brains and onboarding (local models, Gemini CLI, Codex with ChatGPT, OpenRouter free models) · verified: Brains and onboarding tests (2026-09-23)

**Instructions and workspaces (§4)**

- [x] **CONV-09** · M5 · Global instructions ("About me") and per-workspace instructions in SQLite, mirrored as Markdown (`instructions\global.md`, `workspaces\<name>\instructions.md`), watched and re-imported on edit (§4) → done: global ("About me") and per-workspace instructions in SQLite, mirrored to `instructions\global.md` and `workspaces\<name>\instructions.md`, watched with `notify` and read back when edited; edited on Agents → Workspaces · verified: `instructions_are_mirrored_and_edits_come_back`, `Agents.test.tsx` (2026-09-23)
- [x] **CONV-10** · M5 · Workspaces detected (VS Code folder, terminal cwd, git repo acted in), with a one-time "Remember … as a workspace?" (§4) → done: folders KIVO works in (a command's or CLI's folder, an agent's, the folder open in VS Code) are offered once: "Remember … as a workspace?" in the Island, answered by click or voice · verified: `a_new_project_is_offered_once_and_remembered`, `vs_codes_open_folder_is_found`, `the_islands_buttons_work_by_voice`, Island offer test (2026-09-23)
- [x] **CONV-11** · M5 · Project `CLAUDE.md` / `AGENTS.md` / `GEMINI.md` read when working in that folder, never written unless asked; "Export as AGENTS.md to project" on request (§4) → done: a workspace's CLAUDE.md / AGENTS.md / GEMINI.md are read into the context when working there, never written; "Export as AGENTS.md" writes a KIVO section and keeps the rest · verified: `exporting_agents_md_keeps_the_rest_of_the_file`, `Agents.test.tsx` (2026-09-23)

**Driving other AIs (§5)**

- [x] **CONV-12** · M3 · KIVO-run ACP sessions: "Tell Claude …" follow-ups into the same session, progress in the Island, Tasks and Chat (§5.1) → done: "Tell Claude Code …" goes into the same live ACP session; progress shows in the Island and Chat · verified: the Claude Code e2e test (second prompt in the same session) (2026-09-23) · note: the Tasks view joins with Tasks (M5)
- [x] **CONV-13** · M5 · "Open in terminal": hand the session to a visible terminal where the agent supports resuming (§5.1) → done: "Open in terminal" on the Agents page starts the agent's resume command for a KIVO-run session in Windows Terminal (`agents.openInTerminal`) · verified: `the_agents_page_requests`, `Agents.test.tsx` (2026-09-23)
- [x] **CONV-14** · M5 · Visible terminal agents: launch Windows Terminal with cwd + agent command (mode flags from the app registry; bypass/yolo launch is High risk), track the session, paste prompts via UIA/clipboard, wait for "send", read replies via TextPattern (§5.2) → done: `agents.open_terminal` launches Windows Terminal in a folder with the agent and its mode flags from the app registry (checked against `--help`; bypass is High), tracks the session; `agents.send_prompt` pastes by clipboard into its window; `agents.read_reply` reads the window's text as untrusted; "Start" on the Agents page · verified: `opening_an_agent_checks_its_flags_and_bypass_is_high`, `prompts_go_into_the_sessions_window_and_replies_are_read_back`, `terminal_agents_start_with_a_yes_and_prompts_are_drafts_first` (2026-09-23)
- [x] **CONV-15** · M5 · Prompt Draft card: target, text, Send · Edit · Cancel, voice edits live ("add …", "remove the last sentence", "read it back") (§5.4) → done: the Draft card (target, text, Send · Edit · Cancel) for every prompt to another AI; voice edits ("add …", "remove the last sentence", "read it back", "send") without AI; the Island's Edit field · verified: `terminal_agents_start_with_a_yes_and_prompts_are_drafts_first`, `draft_edits_are_read_from_speech`, Island draft test (2026-09-23)

**Memory v2 (§6)**

- [x] **CONV-17** · M7 · Knowledge graph tables `entities`, `relations`, `observations` with `valid_from` / `valid_to` (§6) → done: migration 10 adds `entities`, `relations` and `observations` (with `valid_from` / `valid_to`), filled from each note's people, `[[links]]` and facts when the vault is indexed (`kivo-memory` graph, `kivo_store::memory`); a superseded fact gets its `valid_to` · verified: `kivo-store` memory tests, `kivo-memory` graph tests, `m7_memory::the_memory_page` backlinks (2026-09-24)
- [x] **CONV-18** · M7 · Obsidian-compatible Markdown vault in `%APPDATA%\KIVO\memory\` (front-matter, wikilinks, `people/`, `workspaces/`, `topics/`, `.kivo/` index); SQLite is the index rebuilt from the files, and user edits win (§6) → done: `crates/kivo-memory`: Markdown notes with front-matter (unknown keys kept), wikilinks and tags in `%APPDATA%\KIVO\memory\` (`people/`, `workspaces/`, `topics/`, `.kivo/` for temp files); SQLite is only the index, rebuilt from the files by hash, and a note edited outside KIVO wins (watched) · verified: 19 `kivo-memory` tests, `m7_memory` (3), runtime `memory` tests (2026-09-24)
- [x] **CONV-19** · M7 · Capture modes Only when I ask / Suggest (on) / Workspace notes (on), each switchable; memory only from conversations and tasks KIVO took part in (§6) → done: `memory.capture` Only when I ask / Suggest (default) and `memory.workspace-notes` (on), each a switch on the Memory page; memories come only from turns and tasks KIVO ran (`remember`, compaction, task end), never from other apps' content · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, `compaction_suggests_and_the_island_asks`, runtime `memory` tests (2026-09-24)
- [x] **CONV-20** · M7 · Suggest: memory candidates at compaction or after tasks, accepted, edited or dismissed by the user (§2, §6) → done: compaction's `REMEMBER:` lines and finished tasks become suggestions (`memory_suggestions`), offered on the Island ("Want me to remember …?") and on the Memory page to accept, edit or dismiss · verified: `m7_memory::compaction_suggests_and_the_island_asks`, `Memory.test.tsx` (2026-09-24)
- [x] **CONV-21** · M8 · Workspace notes (automatic) with detail level Brief / Standard / Detailed (§6) → done: agent sessions KIVO runs write a workspace log entry by themselves (`engine/agent.rs` → `memory.workspace_log`) at the chosen detail, Brief ≤ 3 bullets, Standard ≤ 10, Detailed all, with an overview note per workspace; secrets are never kept · verified: `workspace_notes_and_suggestions`, `a_new_workspace_is_asked_about_first`
- [x] **CONV-22** · M8 · Tidy job: merge duplicates, condense logs older than 14 days, mark superseded facts, 50 KB per-workspace cap, never keep secrets, ask before new workspace/person notes (§6) → done: the tidy job merges duplicates (`tidy::same_fact`: the specifics — names, numbers, paths — must match, then MiniLM cosine ≥ 0.9, word overlap when no model), condenses logs older than 14 days, marks superseded facts, keeps each workspace under 50 KB, never keeps secrets, and asks before a new workspace's or person's notes (`memory.ask-new-notes`, on by default; "never for this one" is remembered) · verified: kivo-memory tidy tests with measured MiniLM cosines (paraphrase 0.93 merged; "jasmine tea" vs "coffee" 0.74 and 10 am vs 11 am kept apart by their specifics), runtime tidy tests, `a_new_workspace_is_asked_about_first`
- [x] **CONV-23** · M8 · Hybrid search FTS5 + `sqlite-vec` (§6, MEMORY §3) → done: hybrid search — FTS5 and sqlite-vec nearest neighbours fused by reciprocal rank (`memory.find`, `kivo_memory::query::fuse`), used by `memory.search` and recall; vector hits beyond cosine distance 0.65 are dropped · verified: `notes_are_recalled_by_meaning_with_the_local_model` (real MiniLM: a note found by meaning with no shared words), store vector tests
- [x] **CONV-24** · M6 · Memory MCP tools `memory.search`, `memory.read`/`memory.get`, `memory.write_note`/`memory.add` (permission-gated), `memory.tags`; each agent allowed first; sensitive memories never shared with cloud agents unless allowed (§6) → done: `memory.search`, `memory.get`, `memory.add` (asks, Medium) and `memory.tags` (`apps/kivo-runtime/src/memory_tools.rs`) over preferences, notes and conversations (the M7 vault replaces the store behind them); shared per agent from Agents → Memory; sensitive items (personal, About me, conversations) filtered unless allowed · verified: `m6_extensions::kivos_mcp_server_shares_memory_only_with_allowed_agents` (search, add asks, sensitive filtered, unshared agent sees nothing) and memory_tools unit tests
- [x] **CONV-25** · M8 · Memory page graph view (tags, backlinks) (§6) → done: Memory page Notes / Graph toggle; `MemoryGraph` draws the notes with their tag and backlink edges (runtime `links()` resolves `[[links]]`), laid out by `forceLayout.ts`, click opens the note · verified: forceLayout tests, Memory page tests, runtime links test

**Conversational confirmations (§7)**

- [x] **CONV-26** · M2 · Decisions play the `question` earcon, listen without the wake word for 10 s (extended while the user talks), show a mic ring and voice hints matching the buttons (§7, DESIGN_SYSTEM voice-hint rule) → done: a decision plays `question`, listens 10 s (extended while the user talks), and the Island shows the mic ring and the buttons' words ("allow", "always allow", "deny", "wait") · verified: `decisions_are_answered_by_voice_but_high_risk_needs_a_click`, Island hint tests (2026-09-23)
- [x] **CONV-27** · M2 · Local confirmation grammar (EN + HI): approve, approve with scope, deny, defer ("Waiting for you", no timeout), edit (to the brain), explain ("why?") (§7) → done: `kivo_intent::answers` (EN + HI): approve, approve with scope, deny, defer ("Waiting for you", no timeout), explain ("why?"), and edit, which drops the waiting action and hands the corrected request to a brain · verified: answers tests, `changing_a_decision_by_voice_goes_to_a_brain` (2026-09-23)
- [x] **CONV-28** · M2 · Voice approval rules: Medium needs the owner's voice (or signed-in device with recognition off); guests can't approve; speech heard during KIVO's own TTS is ignored (§7) → done: Medium needs the owner's voice (or recognition off), guests can't approve, High needs a click; speech heard during KIVO's own TTS is ignored for answers · verified: `decisions_are_answered_by_voice_but_high_risk_needs_a_click`, voice-ID tests (2026-09-23)
- [x] **CONV-29** · M4 · High risk: voice "approve" triggers Windows Hello; a click also works; voice alone never suffices (§7) → done: a spoken "approve" on a High-risk card starts Windows Hello, which must confirm; a click also works; voice alone never approves (without Hello set up, KIVO asks for a click) · verified: `windows_hello_confirms_high_risk`, `a_spoken_yes_is_not_enough_for_high_risk` (2026-09-23)

**Context layers (§8)**

- [x] **CONV-30** · M3 · Context layers 1–8 with the default sizes, lazy-loading tools and skills (~1.5k tokens at start), CLI handoff = request + ≤ 300 tokens of memory (§8) → done: layers 1–8 with the default sizes; tools are chosen per request, the agent handoff is the request plus ≤ 300 tokens of saved context; Brains → Context shows each layer · verified: context tests, `Brains.test.tsx` · note: skills load with M6 (2026-09-23)
- [x] **CONV-31** · M7 · Settings → Context: per-layer toggles and edit links, per-profile budget, auto-compaction threshold, "Start each conversation fresh", estimated cost per session, "Preview what the AI sees" with secrets redacted (§8) → done: Brains → Context (`components/brains/ContextSettings.tsx`): each layer's size with a switch and an Edit link (About me, workspace, memories, skills; `[context]` in `kivo.toml`), the live fields sent, auto-compaction and its threshold (60–100 %), "Start each conversation fresh", the budget, an estimated cost of a new conversation, and "Preview what the AI sees" (the next request built without side effects, secrets redacted) · verified: `brain_turns::the_control_centers_brain_requests` (preview, toggles, live fields), `follow_ups…` (fresh start), `engine::brain` `compaction_follows_the_context_settings`, `Brains.test.tsx` Context (2026-09-24)

**Skills (§9)**

- [x] **CONV-32** · M6 · Agent Skills (`SKILL.md` folders) from `%APPDATA%\KIVO\skills\`, import from folder/zip; only name + description in context, body loaded on use; scripts run through the shell tool under the permission engine; external skills reviewed before enabling (§9) → done: `apps/kivo-runtime/src/skills.rs`: SKILL.md folders in `%APPDATA%\KIVO\skills\`, import from folder or zip (zip-slip checked), only name + description in the prompt, `skills.load` loads the body, scripts run through `shell.run`; external skills off until reviewed · verified: `m6_extensions::skills_are_found_reviewed_and_offered_to_brains`, skills unit tests, `Extensions.test.tsx`
- [x] **CONV-33** · M8 · Skills created from a routine, or shared by agents KIVO launches (with permission) (§9) → done: "Make a skill" on a routine (`routines.toSkill` writes a SKILL.md with a from-routine marker) and skills that agents KIVO launches keep in `.claude`, `.codex`, `.gemini` or `.agents` folders, shared only with permission · verified: `routines_become_skills_and_agents_share_theirs_with_permission`
