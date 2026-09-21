# Routines and custom commands

Status: Draft v1, 2026-09-21. **This is a new feature beyond the master plan.** It overlaps plan §148
(workflow builder) and reuses the same Tool and Task systems (§148: "rather than introducing a
second automation engine").

Prior art: Siri Shortcuts, Alexa and Google Home routines, and Home Assistant automations.

## 1. Concepts

| Concept | What it is | Example |
|---|---|---|
| **Custom command** | A user-defined phrase that runs one fast-path action without AI | "Kivo, **cinema**" → volume 30%, media play |
| **Routine** | A named, ordered or parallel list of steps, triggered by voice, hotkey, schedule or event | "**Work mode**" → open VS Code at `D:\work\kivo`, open Slack, focus VS Code, set Focus mode on, say "Ready" |
| **Step** | A tool call with parameters, an optional condition, delay, error policy and optional AI step | `apps.launch{app: "Slack"}`; `brain.ask{prompt: "Summarize my unread mail", profile: Fast}` |
| **Trigger** | What starts a routine | phrase (with variants), hotkey, schedule (cron-like), event (app launched, USB device, Wi-Fi network, time of day, idle/return, battery low), routine-from-routine |
| **Variables** | Inputs captured from the phrase or earlier steps | "Kivo, **focus for {minutes}**" → timer {minutes} |

## 2. Why this matters

- It is **instant and free**: phrase and hotkey triggers resolve in the intent router's grammar
  stage, before any brain.
- It is **offline**, and **predictable**, because the routine does exactly what the user wrote.
- It makes AI optional for power users while still allowing AI steps where they are needed.

## 3. Execution model

- A routine compiles to a **Task graph** (see [BRAINS.md §7](BRAINS.md)): sequential by default,
  with parallel groups optional.
- Each step goes through `authorize()` like any tool call.
  - **Pre-authorization:** when a routine is saved, KIVO shows the full list of permissions it
    needs, and the user grants them as a *routine grant* (scoped to that routine and its exact
    arguments).
  - Unattended triggers (schedule or event) **never** run High-risk steps without a confirmation
    on screen.
- **Error policy per step:** `stop` (default), `continue`, `retry(n)`, or `ask`.
- **AI steps** (`brain.ask`, `brain.decide`) are allowed. They use a chosen profile and count
  toward cost limits. A routine that contains AI steps is marked with a badge.
- Cancellation works like any other task: "Kivo stop" ends it.

## 4. Creating routines

| Method | Flow |
|---|---|
| **Builder UI** (Control Center → Routines) | Trigger picker → step list (drag to reorder) → each step picks a tool from a searchable catalog with a generated form from the JSON Schema → test-run button → save |
| **By voice / chat** | "Kivo, create a routine called work mode that opens VS Code and Slack". The brain drafts the routine as structured data, KIVO shows it in the builder **for review**, and it is saved only after the user confirms |
| **From history** | "Save what you just did as a routine": the last turn's tool calls become the draft |
| **Import / export** | `.kivo-routine.json` files, signed by nobody and treated as untrusted. The permissions are shown before import |

## 5. Data model

```text
Routine { id, name, description, enabled, triggers: [Trigger], steps: [Step], variables: [VarDef],
          grants: [GrantRef], created_at, updated_at, last_run, run_history: [RunSummary], contains_ai: bool }
Trigger = Phrase{ phrases: [String], lang } | Hotkey{ chord } | Schedule{ cron, tz } | Event{ kind, filter } | Manual
Step    = { id, tool_id, args (may reference ${vars}), when?: Condition, delay_ms?, on_error, parallel_group? }
```

- Storage is the SQLite `routines` table plus a JSON body. The schema is versioned.
- Phrase triggers are **validated for collisions** with the built-in fast-path grammar, other
  routines, and wake words, using the same confusability checker as custom wake words.

## 6. Built-in starter routines (all disabled until the user turns them on)

- **Work mode:** opens chosen apps and a project, turns Focus on.
- **Break:** pauses media, locks the PC after a confirmation.
- **Meeting:** mutes notifications, sets the mic to the headset, opens the calendar link.
- **Goodnight:** closes chosen apps, sleeps the PC (confirmation).
- **Focus for {minutes}:** a timer with Focus mode and an earcon when it ends.

## 7. Milestones

- **M5 (Agents & tasks):** the routine engine, phrase and hotkey triggers, and the builder MVP.
- **M8:** schedule and event triggers, creating routines by voice or from history, and
  import/export.
