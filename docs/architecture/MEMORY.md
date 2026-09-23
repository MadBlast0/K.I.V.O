# Memory and context storage

> **Superseded in part (2026-09-21):** the knowledge-graph memory, the Markdown mirror, capture
> modes (Only when I ask / **Suggest** / Workspace notes), workspaces, instructions, and the
> context budgets per model class are specified in [CONVERSATION.md](CONVERSATION.md) §2, §4 and
> §6. This file still defines the base tables and retention.

Status: Draft v1, 2026-09-21. Implements plan §62–65. Research:
[architecture-and-platform/REPORT.md §6](../research/architecture-and-platform/REPORT.md).

## 1. Stores

| Kind | What | Table(s) | Retention |
|---|---|---|---|
| Conversation history | Turns: text, speaker, brain, tools used | `conversations`, `messages` | Setting (30 days default; "never keep" option) |
| Task state | Tasks, steps, grants, results | `tasks`, `task_steps` | Until the user deletes it; completed tasks are summarized after 90 days |
| Preferences | Explicit settings and stated preferences ("call me Sam", "use metric") | `preferences` | Until changed |
| Long-term memory | Facts the user asked KIVO to remember, or accepted when KIVO proposed them | `memories` + `memories_fts` (FTS5) + `memories_vec` (sqlite-vec, later) | Until deleted |
| Runtime state | Active window, devices, network | In memory only | Process lifetime |

## 2. Rules (plan §63)

- **Explicit capture only in v1.** A memory is created when the user says "remember …", clicks
  "Remember this" in the card, or accepts a *proposed* memory. Proposals appear inline:
  "Want me to remember that your project lives in D:\\work?"
- **Record shape:** each memory records `text, scope (global | app:<id> | project:<path>),
  sensitivity (data class), source (turn id), created_at, last_used_at, use_count`.
- **Control Center:** Memory lists, searches, edits and deletes memories, can "forget everything",
  and can export to JSON.
- **Cloud brains:** `sensitive` and above never leave the device, unless the privacy mode and the
  per-memory flag both allow it.
- **Guest sessions:** no memory reads and no writes.

## 3. Retrieval

1. **v1:** FTS5 keyword search plus scope filtering. The top-5 memories are injected into the
   task-level context as `System` provenance, marked "user-provided memory".
2. **v1.1:** hybrid search, adding embeddings from a local small model (MiniLM-class via
   `fastembed`/`ort`, 384-dim, the same model the intent router uses) in `sqlite-vec`.
   - The embedding model id is stored with each vector, and a model change triggers a background
     re-embed.
3. **Budget:** at most 400 tokens of memory per request. The Activity view can show which memories
   were used ("Why did you say that?").

## 4. Context deltas (plan §65)

- The runtime keeps a `ContextSnapshot` per session and sends `ContextDelta`s (for example
  "active app changed: VS Code, main.rs") in the always-available block, instead of the full
  state.
- The snapshot is rebuilt for a new brain session or after a provider failover.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md). Memory v2 items (graph,
vault, capture modes) are in [CONVERSATION.md](CONVERSATION.md) as CONV-17 to CONV-25.

- [x] **MEM-01** · M3 · `conversations` and `messages` tables with the retention setting (30 days default, "never keep" option) (§1) → done: `conversations` and `messages` (FTS5) with the retention setting; retention 0 keeps nothing on disk (the session is kept in memory only) · verified: store tests, e2e thread checks (2026-09-23)
- [x] **MEM-02** · M5 · `tasks` and `task_steps` kept until deleted; completed tasks summarized after 90 days (§1) → done: tasks and steps are kept until deleted; finished tasks older than 90 days keep a summary instead of their steps (checked at start) · verified: `old_finished_tasks_keep_a_summary_only` (2026-09-23)
- [x] **MEM-03** · M3 · `preferences` table for explicit settings and stated preferences ("call me Sam", "use metric") (§1) → done: `preferences` table, edited in Brains → Context ("About me") and sent in every request · verified: `the_control_centers_brain_requests` ("Call me Sam" in the next request) (2026-09-23)
- [x] **MEM-04** · M7 · `memories` + `memories_fts` (FTS5) with `text, scope, sensitivity, source, created_at, last_used_at, use_count` (§1, §2) → done: migration 10 `memories` + `memories_fts` (FTS5, triggers) with text, scope, sensitivity, source, created_at, last_used_at, use_count, indexed from the vault · verified: `kivo-store` memory tests (search by bm25, use counts) (2026-09-24)
- [x] **MEM-05** · M7 · "Remember …" and the card's "Remember this" button create memories; inline proposals ("Want me to remember …?") (§2) → done: "Remember …" by voice or typed (grammar `memory.add`, exact words kept), the Island's "Remember this" on an answer (`memory.rememberTurn`) and inline proposals ("Want me to remember …?") · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, `turn.test.tsx` Remember this (2026-09-24)
- [x] **MEM-06** · M7 · Memory page: list, search, edit, delete, "forget everything", export to JSON (§2) → done: Memory page: the vault by tags and folders, search, edit (the Markdown), delete, "Forget everything" (confirmed), export to JSON or Markdown in Downloads · verified: `Memory.test.tsx` (4), `m7_memory::the_memory_page` (2026-09-24)
- [x] **MEM-07** · M7 · `sensitive` and above never leave the device unless the privacy mode and the per-memory flag both allow it; guest sessions read and write no memory (§2) → done: a memory marked sensitive (or found so by the detectors) is recalled for a cloud brain only when both the privacy mode and `memory.sensitive-to-cloud` allow it; credentials are never stored; guests get no memory tool at all (policy) and no recall · verified: runtime `memory` tests (recall for cloud vs local), `kivo-security` `guests_get_no_memory` (2026-09-24)
- [x] **MEM-08** · M7 · Retrieval: FTS5 + scope filter, top memories injected as `System` provenance marked "user-provided memory", ≤ 400 tokens per request (§3) → done: `Memory::recall`: FTS5 with scope (app, project, workspace) filtering, ≤ 150 tokens per item and 400 per request, injected as `System` provenance "user-provided memory" (MEMORY_TOKENS = 400) · verified: runtime `memory` recall tests, `m7_memory::remembering_by_voice_then_answers_use_it` (the request carries it) (2026-09-24)
- [ ] **MEM-09** · M8 · Embeddings (MiniLM-class via `fastembed`/`ort`, 384-dim, shared with the intent router) in `sqlite-vec`; model id stored per vector, background re-embed on change (§3)
- [x] **MEM-10** · M7 · "Why did you say that?" in Activity shows which memories were used (§3) → done: Activity's "Why?" on an answer lists the memories that turn used (`turn_memories`, `memory.why`), including ones since deleted · verified: `m7_memory::remembering_by_voice_then_answers_use_it`, Activity popover (2026-09-24)
- [x] **MEM-11** · M3 · `ContextSnapshot` per session with `ContextDelta`s ("active app changed: VS Code, main.rs"); rebuilt for a new brain session or after failover (§4) → done: a live snapshot per thread and per agent session with "what changed" deltas; rebuilt whole for a new session or brain · verified: context tests, agent e2e (2026-09-23)
