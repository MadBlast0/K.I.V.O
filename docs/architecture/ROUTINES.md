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

- **Work mode:** opens VS Code and the browser together, then says it's ready.
- **Break:** pauses media, locks the PC after a confirmation.
- **Meeting:** unmutes the mic, opens the calendar link, says it's ready.
- **Goodnight:** says goodnight, then sleeps the PC (confirmation).
- **Focus for {minutes}:** pauses media and sets a reminder for the end, spoken with its earcon.

Windows has no documented API for turning on Focus / Do Not Disturb, muting other apps'
notifications or choosing the default microphone, and KIVO uses documented APIs only, so the
starters leave those out (DECISIONS "M5 build"). Users edit the starters' apps in the builder.

## 7. Milestones

- **M5 (Agents & tasks):** the routine engine, phrase and hotkey triggers, and the builder MVP.
- **M8:** schedule and event triggers, creating routines by voice or from history, and
  import/export.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [x] **ROUT-01** · M5 · `Routine`, `Trigger` and `Step` data model (§5) in a versioned `routines` table with a JSON body → done: `Routine`, `Trigger`, `RoutineStep` (`kivo-core/src/routine.rs`) stored as a versioned JSON body in `routines` · verified: `routine.rs` unit tests, `custom_commands_and_routines_run_from_their_phrases` (2026-09-23)
- [x] **ROUT-02** · M5 · A routine compiles to a Task graph (sequential, optional parallel groups); every step goes through `authorize()`; cancellation like any task (§3) → done: a routine compiles to a task graph (in order, parallel groups together); every step goes through `authorize()` with the routine's grants; cancellable like any task · verified: `custom_commands_and_routines_run_from_their_phrases`, `routines_run_only_what_was_granted` (2026-09-23)
- [x] **ROUT-03** · M5 · Routine grants: the full permission list is shown on save and granted scoped to the routine and its exact arguments (§3) → done: saving shows the full permission list (Routines → Save) and grants each tool with its exact arguments; anything else asks, and a task can't exceed them · verified: `routines_run_only_what_was_granted`, `Routines.test.tsx` builder test (2026-09-23)
- [x] **ROUT-04** · M5 · Error policy per step: stop (default), continue, retry(n), ask (§3) → done: stop (default), continue, retry(n), ask per step, set in the builder · verified: `error_policies_retry_continue_and_ask` (2026-09-23)
- [x] **ROUT-05** · M5 · Phrase triggers (with variants and `{variables}`) resolved in the intent router's grammar stage; hotkey triggers; manual run (§1, §2) → done: phrases with variants and `{variables}` matched before the grammar; hotkeys registered and re-registered on change; Run on the page and the jump list · verified: `custom_commands_and_routines_run_from_their_phrases`, `starters_are_added_once_and_hotkeys_run_routines`, `routine_hotkeys_replace_the_previous_ones` (2026-09-23)
- [x] **ROUT-06** · M5 · Custom commands: a phrase that runs one fast-path action without AI ("Kivo, cinema") (§1) → done: one phrase + one tool runs as a direct action without AI · verified: `custom_commands_and_routines_run_from_their_phrases` (2026-09-23)
- [x] **ROUT-07** · M5 · Phrase collision check against the fast-path grammar, other routines and wake words, using the wake-word confusability checker (§5) → done: phrases checked against the grammar, other routines and wake words (exact clash blocks, sound-alike warns, the wake-word confusability list), hotkeys against KIVO's and other routines' · verified: `clashing_phrases_and_hotkeys_are_caught`, `Routines.test.tsx` (2026-09-23)
- [x] **ROUT-08** · M5 · AI steps (`brain.ask`, `brain.decide`) with a chosen profile, counted toward cost limits; routines containing AI are badged (§3) → done: `ask` / `decide` steps call a brain with a chosen profile, metered toward cost limits; routines with AI are badged · verified: `ai_steps_ask_a_brain_and_count_toward_its_cost`, `Routines.test.tsx` (2026-09-23)
- [x] **ROUT-09** · M5 · Builder UI: trigger picker → done: `pages/Routines.tsx`: triggers (phrases, hotkey) → steps reordered by dragging or Move up / down → tool picker with forms generated from each tool's JSON Schema (`lib/schemaForm.ts`), say, wait, ask AI → error policy → save with the grant list → test run · verified: `Routines.test.tsx` (2026-09-23)
- [x] **ROUT-10** · M5 · Built-in starter routines, all disabled: Work mode, Break, Meeting, Goodnight, Focus for {minutes} (§6) → done: Work mode, Break, Meeting, Goodnight, Focus for {minutes}, all disabled, added once; without Focus / notification / default-mic APIs (DECISIONS "Starter routines without Focus", ROUTINES §6 updated) · verified: `starters_are_added_once_and_hotkeys_run_routines` (2026-09-23)
- [x] **ROUT-11** · M8 · Schedule (cron-like, time zone) and event triggers (app launched, USB device, Wi-Fi network, time of day, idle/return, battery low, routine-from-routine) (§1) → done: schedule triggers (cron with a time zone, `kivo_core::cron` on jiff, DST-safe) and event triggers — app launched, USB device, Wi-Fi network (NetworkListManager, SetupAPI), time of day, idle / return, battery low, after another routine — sampled every 5 s by `triggers.rs`; the builder's "Other triggers" dialog adds them · verified: cron and triggers unit tests (DST), `devices.rs` against this PC's networks and USB devices, `events_start_routines_unattended_and_high_risk_steps_ask_first`, Routines UI tests
- [x] **ROUT-12** · M8 · Unattended triggers never run High-risk steps without an on-screen confirmation (§3) → done: a routine started by a trigger runs unattended with every High-risk step set to confirm on screen first (`run_unattended`) · verified: `events_start_routines_unattended_and_high_risk_steps_ask_first`
- [x] **ROUT-13** · M8 · Create by voice or chat (brain drafts, builder reviews, save after confirm) and "Save what you just did as a routine" (§4) → done: "make a routine that…" by voice or chat — the brain drafts it with `routines.draft`, the builder opens with the draft to review, nothing is saved until the user saves; "save what you just did as a routine" drafts one from the last request's tool calls · verified: `routines_are_drafted_by_voice_and_chat_and_move_as_files`, Routines UI test
- [x] **ROUT-14** · M8 · Import/export `.kivo-routine.json`, treated as untrusted, permissions shown before import (§4) → done: Export / Import `.kivo-routine.json` (`routines.export`, `routines.import`): an imported file is untrusted — checked, unknown tools refused, and its steps and permissions shown in the builder before it is saved · verified: `routines_are_drafted_by_voice_and_chat_and_move_as_files`, Routines UI test
