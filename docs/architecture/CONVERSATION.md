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

**Sharing memory with other AIs:**

- The KIVO MCP server exposes read-only `memory.search` / `memory.get`, and optionally
  `memory.add`, to agents KIVO launches, so Claude Code in a project gets the user's workspace
  notes.
- Each agent must be allowed first, and sensitive memories are never shared with cloud agents
  unless the user allows it.

**External memory services** (Mem0 hosted, a self-hosted Graphiti, Basic Memory) can be added as
**Connectors** by users who want them. They are not bundled.

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
