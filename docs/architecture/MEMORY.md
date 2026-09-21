# Memory and context storage

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
