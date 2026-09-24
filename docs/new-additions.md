# New additions — ideas worth adding to KIVO

Status: backlog, 2026-09-24. Nothing here is planned or built yet. These are features the owner
approved as *worth adding later*. When one is picked up, it moves into the right spec as checklist
items (with IDs and a milestone tag), gets a DECISIONS.md entry, and is removed from this file.

Sources: an Instagram post on "Jarvis OS 2.0" (Claude Code + Obsidian + Whisper/Kokoro + a
dashboard), web research on what users want from assistants in 2026, and a full review of
**HeyClicky** (formerly Clicky; YC Spring 2026, Mac only, cloud only, $20–100/month) including its
changelog from v1.0 to v1.0.51. Links are at the end.

Every idea keeps the architecture invariants (CLAUDE.md): the runtime stays authoritative, AI never
bypasses the permission engine, deterministic requests never need a model, cloud stays optional,
and KIVO never passively records the user.

---

## How to read this file

- **ID:** `NA-xx`, used for referring to an item before it gets a real spec ID.
- **Priority:** **P1** high value, do first · **P2** strong · **P3** nice to have.
- **Size:** S (days) · M (a week or two) · L (several weeks).
- **Lands in:** the spec(s) that would own it.
- **Beats HeyClicky:** where the idea comes from them, how KIVO does it better.

---

## Progress tracker

Status values: **Not started** · **Incomplete** (work begun, not usable yet) · **Partial**
(usable, but some of the item is missing) · **Complete** (built, verified, and moved into its
spec). Update the status and the note when work happens; the spec checklist remains the source of
truth once an item moves there.

| ID | Item | Priority | Status | Note |
|---|---|---|---|---|
| NA-01 | Today: daily notes | P1 | Not started | |
| NA-02 | Commitments and follow-ups | P1 | Not started | |
| NA-03 | Information starter routines | P1 | Not started | |
| NA-04 | Quick actions on Home, Ctrl+K and the Island | P1 | Not started | |
| NA-05 | Reports and results land in the vault | P1 | Not started | |
| NA-06 | Vault structure for Obsidian users | P2 | Not started | |
| NA-07 | Proactive morning check-in, with manners | P1 | Not started | |
| NA-08 | Dictation into any app | P1 | Not started | |
| NA-09 | Reply in my voice | P2 | Not started | |
| NA-10 | Audio resilience | P1 | Not started | |
| NA-11 | Voice picker with spoken previews | P2 | Not started | |
| NA-12 | Live translate | P2 | Not started | |
| NA-13 | Guided walkthroughs ("teach me") | P1 | Not started | |
| NA-14 | Circle to ask (spatial context) | P1 | Not started | |
| NA-15 | Screenshot → ask / copy text | P2 | Not started | |
| NA-16 | Whole-document context | P1 | Not started | |
| NA-17 | Instant local file search | P1 | Not started | |
| NA-18 | Clipboard history | P2 | Not started | |
| NA-19 | Text snippets | P2 | Not started | |
| NA-20 | Window layouts | P2 | Not started | |
| NA-21 | Game / fullscreen mode | P1 | Not started | |
| NA-22 | Named assistants | P2 | Not started | |
| NA-23 | "Why did you do that?" | P1 | Not started | |
| NA-24 | Agent start delay and retry | P2 | Not started | |
| NA-25 | Suggestions with manners | P2 | Not started | |
| NA-26 | Connector health, verified | P2 | Not started | |
| NA-27 | Hands-on "try it" tour | P1 | Not started | |
| NA-28 | Chat-app remote (Telegram / Discord) | P2 | Not started | |
| NA-29 | Multi-device handoff | P3 | Not started | |
| NA-30 | Weekly "KIVO saved you" recap | P3 | Not started | |
| NA-31 | Routine and skill gallery | P2 | Not started | |
| NA-32 | Accessibility-first profile | P2 | Not started | |
| NA-33 | Opt-in meeting notes | P2 | Not started | |
| NA-34 | Drafts, pins and archive in Chat | P3 | Not started | |
| NA-35 | Cursor and overlay customisation | P3 | Not started | |
| NA-36 | Says "I'm not sure" | P1 | Not started | |
| NA-37 | Check before claiming done | P1 | Not started | |
| NA-38 | AI-free mode | P2 | Not started | |
| NA-39 | Home Assistant | P2 | Not started | |
| NA-40 | Calendar write and scheduling | P1 | Not started | |
| NA-41 | Email send and follow-up | P2 | Not started | |
| NA-42 | Health nudges | P3 | Not started | |
| NA-43 | Focus sessions | P2 | Not started | |
| NA-44 | Time tracking by project | P3 | Not started | |
| NA-45 | Quick capture anywhere | P1 | Not started | |
| NA-46 | Read it to me: human-sounding narrator | P1 | Not started | |
| NA-47 | Follow-up without the wake word | P1 | Not started | |
| NA-48 | Whisper mode | P2 | Not started | |
| NA-49 | Voice profiles and cloning, local only | P2 | Not started | |
| NA-50 | Duck other apps' audio | P1 | Not started | |
| NA-51 | Character system | P2 | Not started | |
| NA-52 | Character Studio page | P2 | Not started | |
| NA-53 | A more expressive Orb | P2 | Not started | |
| NA-54 | Offline knowledge packs | P3 | Not started | |
| NA-55 | Local model manager for chat LLMs | P2 | Not started | |
| NA-56 | Portable mode | P3 | Not started | |
| NA-57 | Public plugin SDK and docs | P2 | Not started | |
| NA-58 | Build and test watcher by voice | P2 | Not started | |
| NA-59 | Voice git | P3 | Not started | |
| NA-60 | Explain this error | P2 | Not started | |
| NA-61 | PC health doctor | P2 | Not started | |
| NA-62 | Downloads and desktop tidy | P3 | Not started | |
| NA-63 | Smart notifications digest | P2 | Not started | |
| NA-64 | Battery and power coach | P3 | Not started | |
| NA-65 | Language practice partner | P3 | Not started | |
| NA-66 | Companion phone app | P2 | Not started | |
| NA-67 | Themes and accent picker | P3 | Not started | |
| NA-68 | Customizable Home widgets | P2 | Not started | |
| NA-69 | Mini mode | P2 | Not started | |
| NA-70 | "Remember where I put…" | P2 | Not started | |
| NA-71 | People notes | P2 | Not started | |
| NA-72 | Web clipper | P2 | Not started | |
| NA-73 | Ask my vault (and chosen folders) | P1 | Not started | |
| NA-74 | Watch and repeat (record a routine) | P2 | Not started | |
| NA-75 | More event triggers | P2 | Not started | |
| NA-76 | Web forms autopilot | P3 | Not started | |
| NA-77 | Page and price watcher | P3 | Not started | |
| NA-78 | Time-machine undo | P2 | Not started | |
| NA-79 | Dry-run for routines | P2 | Not started | |
| NA-80 | Panic phrase | P2 | Not started | |
| NA-81 | Per-brain data view | P2 | Not started | |
| NA-82 | Photo and screenshot memory | P3 | Not started | |
| NA-83 | Memory timeline | P2 | Not started | |
| NA-84 | Taskbar and Start presence | P2 | Not started | |
| NA-85 | Right-click "Ask KIVO" | P2 | Not started | |
| NA-86 | Share target | P3 | Not started | |
| NA-87 | Presenter mode | P2 | Not started | |
| NA-88 | Streaming and recording safe | P1 | Not started | |
| NA-89 | Travel / offline mode | P2 | Not started | |
| NA-90 | Low-vision mode | P2 | Not started | |
| NA-91 | Learns your phrasing | P2 | Not started | |
| NA-92 | Routine suggestions from habits | P2 | Not started | |
| NA-93 | Personal vocabulary | P2 | Not started | |
| NA-94 | Live captions for any audio | P2 | Not started | |
| NA-95 | Summarise this video | P2 | Not started | |
| NA-96 | Deep media control | P2 | Not started | |
| NA-97 | Podcast and audio notes | P3 | Not started | |
| NA-98 | Weather and commute | P2 | Not started | |
| NA-99 | News by topic | P3 | Not started | |
| NA-100 | Packages and deliveries | P3 | Not started | |
| NA-101 | Stocks and crypto glance | P3 | Not started | |
| NA-102 | Share a result | P3 | Not started | |
| NA-103 | Meeting follow-up drafts | P3 | Not started | |
| NA-104 | Storage manager | P1 | Not started | |
| NA-105 | Feature switchboard | P1 | Not started | |
| NA-106 | Starter packs | P2 | Not started | |
| NA-107 | Settings by voice | P2 | Not started | |
| NA-108 | Settings backup and sync | P2 | Not started | |
| NA-109 | "That was wrong" and bug reports | P1 | Not started | |
| NA-110 | Hands-on What's new | P3 | Not started | |
| NA-111 | Help by voice, offline | P2 | Not started | |
| NA-112 | Health self-test | P2 | Not started | |
| NA-113 | Image generation | P3 | Not started | |
| NA-114 | Document writer | P2 | Not started | |
| NA-115 | Slides from notes | P3 | Not started | |
| NA-116 | Social post drafts | P3 | Not started | |
| NA-117 | Game companion | P2 | Not started | |
| NA-118 | Game guide lookups | P3 | Not started | |
| NA-119 | Game launcher and library | P3 | Not started | |
| NA-120 | Tutor mode | P3 | Not started | |
| NA-121 | Agents and sub-agents off by default | P1 | Not started | |
| NA-122 | Background agent desktop | P2 | Not started | |
| NA-123 | Mission control | P2 | Not started | |
| NA-124 | Folder scopes for agents | P1 | Not started | |

---

## Competitive edge vs HeyClicky

HeyClicky is the closest product to KIVO: a screen-aware voice companion that points, draws,
dictates and runs agents. Where KIVO must stay ahead:

1. **Windows first and polished.** HeyClicky is Mac only; its Windows users are on a waitlist and
   community ports. Ship a polished Windows experience before theirs exists.
2. **Private by default.** HeyClicky sends screen, audio and prompts to Anthropic, OpenAI,
   Deepgram and Cerebras, keeps "AI-generated screen analysis", and has no SOC 2. KIVO keeps
   speech, wake word and screen reading local, and says exactly what leaves the PC and when.
3. **Accurate, not guessed.** Their pointing and walkthroughs guess from screenshots, and reviewers
   say it points at the wrong control. KIVO points using **UI Automation elements** (exact bounds,
   names, states) and only falls back to vision.
4. **Safe without being annoying.** Their answer to approval fatigue is an "Always approve"
   switch, which reviewers flag. KIVO has scoped, timed grants, taint, undo and a visible stop.
5. **No subscription, bring your own brain.** Local models, the user's own keys, or the CLIs'
   own logins. No message quotas.
6. **Hands-free.** HeyClicky is hotkey-first, with always-on only with headphones. KIVO has a
   wake word, echo cancellation and barge-in on speakers.
7. **Measured.** Publish latency and resource numbers from `kivo-bench` (they publish none except
   an idle CPU fix from 36–43 % to 3–5 %). KIVO's idle budget must beat that with room to spare.

---

## Phase 1 — Daily usefulness (P1)

The features that make KIVO worth opening every day. They reuse existing parts (vault, routines,
connectors, the grammar), so they are cheap relative to their value.

### NA-01 · Today: daily notes · P1 · M
**Lands in:** CONVERSATION (vault format), MEMORY, BRAINS (grammar)

- A `daily/YYYY-MM-DD.md` note in the vault with front-matter and sections **Top 3**,
  **Schedule**, **Notes**, **Done**, **Carried over**.
- Written by voice ("my top three today are…", "add *call the bank* to today", "done with the
  report"), by the Plan today routine (NA-03), and by the user in Obsidian. User edits always win.
- **Grammar fast path, no model:** "what are my priorities?", "what's on today?", "what did I do
  yesterday?", "what's left?" are answered straight from the file (invariant 3, instant).
- Unfinished items move to the next day's **Carried over**, and the move is noted on both days.
- Tidy: daily notes older than a setting (default 30 days) roll up into `weekly/YYYY-Www.md`.
  The dailies are archived, never silently deleted.
- The Island and Home show the Top 3. Each can be ticked off by click or voice.

### NA-02 · Commitments and follow-ups · P1 · M
**Lands in:** MEMORY, ROUTINES, TOOLS (scheduler)

- "Remind me to reply to Maya on Friday" becomes a tracked **commitment**, not only a timer.
  It appears in that day's note and in "what's left?".
- In chat and voice, when *the user* says "I'll send it tomorrow", KIVO offers (Suggest mode)
  to track it. It never tracks without asking.
- Commitments have states (open, done, dropped, snoozed) and link to their thread or task.
- "What do I owe people?" lists open commitments by person (`people/*.md` links).

### NA-03 · Information starter routines · P1 · M
**Lands in:** ROUTINES §6, INTEGRATIONS (connectors)

All off until turned on, and all editable in the builder:

| Starter | What it does |
|---|---|
| **Plan today** | Reads the calendar (connector), yesterday's carried-over items and open commitments, and asks for or proposes a Top 3. Writes today's note |
| **Morning brief** | Speaks or shows today's note, the next meetings, and optionally unread-mail counts and important senders (connector). Short by default |
| **Inbox brief** | Mail triaged into *needs reply*, *FYI* and *ignore*, with drafts offered for the first group (NA-18). Mail content is untrusted (taint) |
| **Intel brief** | Topics the user picks (feeds or web search), summarised into `reports/` |
| **Weekly review** | Rolls up the week's daily notes: what got done, what slipped, open commitments, time saved (NA-30). Writes `weekly/YYYY-Www.md` |
| **End of day** | "What's left?", then moves open items to tomorrow and says goodnight |
| **Research to vault** | Delegates a research task to a brain or agent and saves the report as a linked note (NA-05) |

Schedules use the M8 schedule triggers. Missed runs after sleep catch up once, never repeat, and
the retry limit is 3 failures, then pause and report (what HeyClicky learned the hard way).

### NA-04 · Quick actions on Home, Ctrl+K and the Island · P1 · S
**Lands in:** UX (Home, palette, Island)

- A **Quick actions** row on Home: the user pins up to 8 routines or commands, and can reorder
  them by dragging.
- The same pinned list appears in Ctrl+K and in the Island's menu, so there's one list everywhere.
- Each button shows its last result (a tick, an unread dot for a new report, or an error) and runs
  with one click. A long press opens its grants.
- Suggested pins after onboarding, based on the connectors and apps the user has.

### NA-05 · Reports and results land in the vault · P1 · S
**Lands in:** CONVERSATION, MEMORY, UX (Chat)

- Long brain or agent outputs (research, briefs, summaries) get a **Save as note** action. Starter
  routines save automatically to `reports/YYYY-MM-DD-<slug>.md` with a link back to the thread and
  forward from today's note.
- Report notes carry front-matter (`source: routine|chat|agent`, `brain`, `cost`) so the vault
  stays searchable and the graph view connects work to days.
- Results arrive **silently** in chat with an unread dot (no chime), and can be marked unread
  again.

### NA-06 · Vault structure for Obsidian users · P2 · S
**Lands in:** CONVERSATION (vault format)

- Optional standard folders created on first use only: `daily/`, `weekly/`, `reports/`,
  `inbox/` (quick captures: "Kivo, note that…"), `projects/` next to the existing `workspaces/`.
- "Kivo, note that…" goes to `inbox/`. The tidy job proposes where inbox items belong.
- **Harvest:** "wrap up this project" turns a finished workspace into a condensed wiki note
  (overview, decisions, lessons), user-triggered, next to the automatic workspace notes.

### NA-07 · Proactive morning check-in, with manners · P1 · S
**Lands in:** UX §7 (proactive speech), CAPABILITIES

- On the first unlock of the day (after a user-set hour, default 7:00), KIVO shows **one** morning
  card. It speaks only if the user allowed it: "Morning. Want your brief, or should I ask for your
  top three?"
- A quiet chime, and a card 30 % shorter than a full brief. Dismissed with one click or "not now".
- **Backs off by itself:** dismissed three days running → rests for five days → asks once whether
  to keep it. It follows the existing quiet rules (calls, fullscreen, quiet hours).

---

## Phase 2 — Voice everywhere

### NA-08 · Dictation into any app · P1 · L
**Lands in:** VOICE, TOOLS (text insertion), UX, SECURITY

Beats HeyClicky: local STT (no cloud round trip or quota), Windows first, and no dictation limits.

- **Hold to dictate** (default hotkey, configurable), or **double-tap for hands-free**. The
  voice "dictate" command also works. The Island shows a live transcript.
- **Two stages:** a streaming local STT, then a small cleanup pass (punctuation, capitals, filler
  removal, lists and paragraphs when spoken as such). Cleanup runs locally where a model exists.
  Otherwise it's rules-only, or the user chooses a cloud brain. **Faithful mode** skips cleanup.
- **Insertion:** UIA `ValuePattern` / `TextPattern` first, then typing through the input path, then
  the clipboard as the last resort (restoring the old clipboard). Works in Electron, Chromium and
  UWP apps. Any keyboard layout.
- **Safety:** never press Enter in a terminal or command field (the risk parser's field
  detection). Never dictate into password fields. Dictation text is user content, not a command.
- **Personal dictionary:** names, jargon and casing ("KIVO", "Tauri"). The last insertion can be
  corrected for 20 seconds with "no, I said…", and the correction feeds the dictionary.
- **Long sessions:** 10+ minutes saved continuously to a local backup. Interruptions (calls, mic
  change) keep what was already said.
- **Languages:** auto-detect among the user's enabled languages; CJK spacing handled.
- **Voice commands inside dictation:** "new line", "new paragraph", "delete that", "scratch that",
  "bullet".

### NA-09 · Reply in my voice · P2 · M
**Lands in:** BRAINS, TOOLS (screen reading), MEMORY

- "Draft a reply" reads the focused email or chat (UIA text, then OCR, marked untrusted) and drafts
  a reply in the user's style. The draft is inserted as text, **never sent** (sending is a
  separate, confirmed action).
- **Style profile:** learned only from samples the user explicitly approves (a few sent emails or
  messages pasted into Memory → Writing style), stored as a vault note the user can edit.
- Works mid-dictation: "reply saying yes, but Thursday" → a full reply in their voice.

### NA-10 · Audio resilience · P1 · S
**Lands in:** VOICE (devices), UX

- **Mic failover:** if the chosen mic disappears or errors, switch to the next best (built-in),
  say so once, and switch back when it returns.
- **Muted or no speakers:** don't speak into silence. Show the answer in the Island, copy it to the
  clipboard if the user allowed that, and offer "unmute and read it".
- **Bluetooth:** delay the headset-profile switch until after the reply's silence, so audio doesn't
  cut out mid-sentence. Detect hands-free profile quality drops and prefer the laptop mic.
- **Pro interfaces:** match sample format and rate. Never crash on an exotic device.
- **Sleep/wake and device changes** never freeze audio (a regression test on the device-change
  path).

### NA-11 · Voice picker with spoken previews · P2 · S
**Lands in:** VOICE, UX (Voice page)

- Every voice has a Play preview sentence, in the user's language, using their name.
- Speed from 0.5× to 1.5× next to the preview (the setting exists; it moves next to the picker).
- Kid-friendly and character voices grouped separately.

### NA-12 · Live translate · P2 · M
**Lands in:** VOICE, TOOLS (OCR), BRAINS

- "Translate this": the selection, or a screen region (OCR), shown and optionally spoken.
- **Conversation mode:** KIVO interprets between the user and someone else, turn by turn, in two
  chosen languages (local model where available, else the chosen brain; cloud disclosed).
- Translated text from the screen stays untrusted content.

---

## Phase 3 — Screen help that beats HeyClicky

### NA-13 · Guided walkthroughs ("teach me") · P1 · L
**Lands in:** CAPABILITIES (screen awareness, pointing), TOOLS (UIA), UX (overlay)

HeyClicky's headline feature, done more accurately.

- "Teach me how to add a column in Excel" or "show me where the export settings are" gives
  **step-by-step coaching** in the real app.
- Each step: a **hand-drawn ring or arrow** (sketchy, Excalidraw-like style) around the target, a
  short spoken instruction, and a caption near the cursor.
- **Targets come from UI Automation** (exact element bounds), with vision only as a fallback, and
  that fallback is labelled as a guess.
- **Waits for the user:** detects the click or the UIA state change (menu opened, dialog shown),
  then moves to the next step. "I'm stuck" re-explains. "Do it for me" hands the step to computer
  use under the normal permission engine.
- Survives pauses: the user can do something else and come back. The walkthrough resumes where it
  was (step budget 25, above HeyClicky's 15).
- **Saved walkthroughs:** a successful walkthrough can be saved as a replayable guide, or turned
  into a routine or a skill.
- **App knowledge packs:** per-app hints (menus, common tasks) shipped as signed catalog entries
  and in `apps/*.toml`, the way HeyClicky ships "teaching skills" for 89 apps.

### NA-14 · Circle to ask (spatial context) · P1 · M
**Lands in:** CAPABILITIES, UX (overlay), TOOLS (OCR/UIA)

- Hold a hotkey (or say "look at this") and **draw on the screen**: circle, underline, arrow or
  scribble. Then ask: "what is this?", "why is this red?", "copy this table".
- Only the marked region plus its UIA element is captured, one-shot. Never a background capture.
  The in-use indicator shows while it's captured.
- Drawings fade after the answer, or stay pinned until dismissed (a setting).
- Works with voice or typed questions, and with every brain (vision where the brain supports it,
  else OCR + UIA text).

### NA-15 · Screenshot → ask / copy text · P2 · S
**Lands in:** TOOLS (OCR), UX

- A hotkey snips a region, then: ask about it, copy its text (local OCR), translate (NA-12), or
  save to the vault. One-shot and explicit.

### NA-16 · Whole-document context · P1 · M
**Lands in:** CONVERSATION (context budgets), TOOLS (files)

- **Drag a file** (PDF, DOCX, text, image) onto the Island, the Orb or Chat to start a
  conversation about it. "Read this page" uses the browser extension's DOM text.
- Long documents are chunked and retrieved (hybrid search already exists), with a document preview
  panel in Chat that highlights the passage the answer used.
- All document content is untrusted and fenced (existing taint rules).

### NA-17 · Instant local file search · P1 · M
**Lands in:** TOOLS (files), UX (Ctrl+K, Island)

- "Find the PDF I downloaded last week about taxes", "open the spreadsheet I edited yesterday".
- Uses the **Windows Search index** (no crawler of our own) plus the vault index. Filters by
  kind, date, folder and app. Results appear in the Island and Ctrl+K with Open, Reveal and Attach.
- Deterministic filters come from the grammar. A brain is only used to interpret fuzzy phrases.

---

## Phase 4 — Everyday power tools

### NA-18 · Clipboard history · P2 · M
**Lands in:** TOOLS (clipboard), SECURITY (privacy)

- Local, capped (count and age), encrypted at rest. **Never** records from password fields or
  apps that mark clipboard data as sensitive (Windows' `ExcludeClipboardContentFromMonitorProcessing`
  / `CanIncludeInClipboardHistory` formats), and pauses in private browser windows.
- "Paste what I copied before that", "paste the link from earlier". Search from Ctrl+K.
- Off by default; switching it on is a capability toggle with a clear explanation.

### NA-19 · Text snippets · P2 · S
**Lands in:** ROUTINES (variables), TOOLS (text insertion)

- Saved text with `{variables}` (date, clipboard, a prompted value): addresses, replies, code
  blocks. Inserted by voice ("insert my address") or by name in Ctrl+K.
- Stored as Markdown in the vault (`snippets/`), so they sync and edit like everything else.

### NA-20 · Window layouts · P2 · M
**Lands in:** TOOLS (display/windows), ROUTINES

- "Save this layout as coding", then "set up my coding layout" restores apps, monitors, positions
  and snap zones, launching any missing apps.
- Usable as a routine step (Work mode can restore a layout).

### NA-21 · Game / fullscreen mode · P1 · S
**Lands in:** UX §7, VOICE, ARCHITECTURE (performance profiles)

- Automatically detects fullscreen games and presentations (the quiet rule already mutes speech
  there). It **also drops to the lowest performance profile**: unloads optional models, keeps only
  the wake word or push-to-talk, and hides the Island unless called.
- A per-app list: always / never treat as game mode.

---

## Phase 5 — Assistants, agents and trust

### NA-22 · Named assistants · P2 · L
**Lands in:** CONVERSATION (workspaces, threads), BRAINS (personas), MEMORY, UX

Beats HeyClicky's "Clickys": they share KIVO's one permission engine and one audit, instead of
being separate agents with their own approvals.

- The user can create several **named assistants** ("Research buddy", "Work", "Home"), each with
  its **own persona, memory scope, threads, files folder, default brain and pinned actions**.
- Voice requests are **routed to the right one** by name ("Research, how's it going?") or by topic.
  "How's it going?" goes to the one with running work.
- Each has a profile: its routines (pause, resume, run now), recent results, and its memory note.
- Created by voice ("make me an assistant for my thesis") or from a template. Up to about 8.

### NA-23 · "Why did you do that?" · P1 · S
**Lands in:** SECURITY (audit), BRAINS (route reasons), UX

- By voice or from any Activity entry: KIVO says or shows **what it did, why it chose that route**
  (grammar / semantic / which brain and why), **which grant or permission allowed it**, and **how to
  undo it**. All of this data already exists in the audit and route reasons.
- "Why didn't you do X?" explains a refusal or a missing permission and offers to fix it.

### NA-24 · Agent start delay and retry · P2 · S
**Lands in:** CONVERSATION (tasks), UX (Island)

- Before a long agent task starts, a **5-second cancel window** in the Island ("Starting research
  in 5… Cancel"), skipped for tasks the user already approved in a Plan.
- A **Retry** button on failed task cards. Failed messages stay where they happened in the thread.
- Follow-ups sent during a running task are queued in order, never dropped.

### NA-25 · Suggestions with manners · P2 · M
**Lands in:** UX §7, CONVERSATION (memory suggestions)

- Proactive suggestions (a routine to automate a repeated command, a connector for an app the user
  uses, a memory to keep) appear **one card at a time** in a small carousel.
- Accept, skip or adjust **by voice**. At most 3 a day. Rest periods after repeated dismissals
  (NA-07).
- Based **only on KIVO's own history** (commands, tasks, routines). Never on passive activity
  tracking, which HeyClicky removed after privacy pushback.

### NA-26 · Connector health, verified · P2 · S
**Lands in:** INTEGRATIONS, DISCOVERY

- Custom connector tokens and MCP servers are **checked with a real, read-only request** at save
  and at start. Status shows *Checking*, *Connected*, *Sign-in needed* or *Token rejected*, with the
  fix one click away.

---

## Phase 6 — Onboarding, delight and reach

### NA-27 · Hands-on "try it" tour · P1 · M
**Lands in:** UX (onboarding)

- After onboarding, an optional 5-minute tour. It is **replayable from Settings** and can be
  **skipped by voice**.
- The user tries real, undoable commands: say hello, open an app and undo it, set a timer, circle
  something on screen (NA-14), dictate a sentence (NA-08), pin a quick action (NA-04).
- Each step shows what happened in Activity, teaching that everything is visible and undoable.

### NA-28 · Chat-app remote (Telegram / Discord) · P2 · M
**Lands in:** INTEGRATIONS (phone remote)

- Message KIVO from Telegram or Discord on the phone, through a bot the user creates. It is paired
  with a one-time code, and only the paired account is accepted.
- Everything goes through the permission engine. High-risk actions require confirmation on the PC
  or with Windows Hello. Results come back as text, and files only if the user allows it.
- Messages are untrusted input for taint purposes, the same as other remote channels.

### NA-29 · Multi-device handoff · P3 · L
**Lands in:** INTEGRATIONS (phone remote), CONVERSATION

- Start a conversation or task on the PC and continue it on the phone (the phone remote or chat
  app), and back. The thread and the running task follow the user.
- The PC stays the only authority. The phone is a client (invariant 2).

### NA-30 · Weekly "KIVO saved you" recap · P3 · S
**Lands in:** UX (Usage), ROUTINES

- Local stats only: commands run, routines run and estimated time saved, tasks done, brain cost,
  and how much ran offline. Shown on Usage and in the Weekly review (NA-03). An optional spoken
  one-liner.

### NA-31 · Routine and skill gallery · P2 · L
**Lands in:** ROUTINES (import/export), CONVERSATION (skills), DISTRIBUTION (signed catalogs)

- Export and import routines and skills as **signed files**, and browse a **curated in-app
  gallery** (signed catalog). Every item shows its **grants before install** and is reviewed.
- Filters: *My routines*, by app, by capability. "Create from gallery item" to customise.
- Team or family sharing later, as private collections.

### NA-32 · Accessibility-first profile · P2 · L
**Lands in:** UX, CAPABILITIES, VOICE

- A hands-free profile for users with motor impairments:
  - **"Show numbers":** numbered badges on every clickable UIA element, then "click 12", "scroll
    down", "double-click 4".
  - **"Show grid":** a numbered mouse grid for elements UIA can't see.
  - Dictation everywhere (NA-08), a slower speech pace, longer end-of-speech timeouts, and
    confirmations by voice.
  - Everything in the app reachable by voice. It builds on the existing "every Island button by
    voice".
- A good test that KIVO's UIA layer is complete.

### NA-33 · Opt-in meeting notes · P2 · M
**Lands in:** VOICE, MEMORY, SECURITY (privacy)

- **Only when the user starts it** ("Kivo, take notes for this call"), with a visible recording
  indicator for the whole call. Never automatic (KIVO never passively records).
- Local transcription of the mic plus system audio (loopback), then a summary, decisions and
  action items saved as a vault note. Action items become commitments (NA-02).
- Reminds the user to tell the other participants; setting for which jurisdictions require consent.
- A live notepad: the user's own jotted notes are enhanced with the transcript afterwards.

### NA-34 · Drafts, pins and archive in Chat · P3 · S
**Lands in:** UX (Chat)

- Half-typed messages and attachments persist across app switches and restarts.
- Pin, archive and search conversations. "Mark as unread" keeps a result for later.
- Scroll position is remembered per thread. Long threads stay fast (virtualised list, target:
  a 300-turn thread opens in under 200 ms).

### NA-35 · Cursor and overlay customisation · P3 · S
**Lands in:** UX (Island, pointing), DESIGN_SYSTEM

- Colour and size of KIVO's pointer, rings and captions. Hand-drawn vs clean style for drawings.
  Respects Windows contrast themes and reduced motion.

---

## Phase 7 — Trust and honesty

Research: people's top complaints about AI in 2026 are confident made-up answers, AI pushed into
everything, and surveillance (Microsoft scaled back Copilot and reworked Recall after backlash; its
best-liked feature, Click to Do, is explicit and on-demand). KIVO should be the opposite.

### NA-36 · Says "I'm not sure" · P1 · M
**Lands in:** BRAINS (persona guardrails), CONVERSATION, UX (Chat, Island)

- Brains are instructed, and the persona guardrails enforce, to state uncertainty plainly. Spoken
  answers say "I think…" or "I couldn't check that" instead of guessing.
- Facts from the web, files or memory carry a **Sources** chip (web page, file, vault note), and a
  spoken "want the source?". Answers without a source are marked as the model's own knowledge.
- When a tool fails or data is missing, KIVO says so. It never fills the gap with a plausible
  answer.

### NA-37 · Check before claiming done · P1 · M
**Lands in:** TOOLS (tool schema: success checks), CONVERSATION (tasks)

- Every tool gets an optional **post-condition** (window exists, file written, volume changed,
  setting applied), checked before KIVO says "done". This extends the M5 rule that coding work is
  only called fixed after KIVO re-runs the tests.
- If the check fails: "I tried, but Chrome didn't open. Want me to try another way?"
- Activity shows *done and verified* vs *done, not verified*.

### NA-38 · AI-free mode · P2 · S
**Lands in:** CAPABILITIES (presets), BRAINS (router)

- One switch: only the grammar, routines, snippets and local tools. **No model of any kind**,
  local or cloud (speech engines stay). Requests the grammar can't handle get "I can only do
  commands in AI-free mode" plus the closest matching commands.
- For people who want a fast voice remote, not an AI. It also makes a clean fallback when every
  brain is down.

---

## Phase 8 — Life and home integrations

### NA-39 · Home Assistant · P2 · M
**Lands in:** INTEGRATIONS (connectors)

- Connects to a local Home Assistant (local URL + long-lived token in Credential Manager). It is
  found automatically on the LAN (mDNS) when present.
- Lights, plugs, climate, media and **scenes**: "Kivo, movie time" dims the lights and pauses
  notifications. Entities are exposed as tools with risk levels (locks and alarms are High and
  always confirmed).
- Deterministic phrases ("turn off the desk lamp") go through the grammar using the entity names,
  so no model is needed. Works offline on the LAN.
- HA automations can trigger KIVO routines, and the other way round (webhook, local only).

### NA-40 · Calendar write and scheduling · P1 · M
**Lands in:** INTEGRATIONS (Google / Outlook calendar connectors)

- "Find 30 minutes with Maya next week", "move my 3 pm to tomorrow", "block two hours for the
  report". KIVO proposes slots, then books after confirmation. Invitations to others are Medium
  risk and always shown before sending.
- Knows the user's working hours and focus blocks (NA-43). Feeds Plan today (NA-03).

### NA-41 · Email send and follow-up · P2 · M
**Lands in:** INTEGRATIONS (mail connectors), ROUTINES

- Send, reply, **schedule send**, and "**nudge me if Maya hasn't replied in 3 days**" (a
  commitment, NA-02). Sending is always confirmed, and the draft is shown in full.
- Mail content is untrusted. Replies drafted from mail use the reduced tool set after taint.

### NA-42 · Health nudges · P3 · S
**Lands in:** UX §7 (proactive), ROUTINES (starters)

- Optional break, water, eye (20-20-20) and posture reminders, based on **active time from KIVO's
  own idle detection**, never on activity tracking. Quiet in calls, games and focus sessions, and
  they back off like NA-07.

---

## Phase 9 — Focus, reading and capture

### NA-43 · Focus sessions · P2 · M
**Lands in:** ROUTINES (the Focus starter), TOOLS (browser extension), UX

- "Focus for 50 minutes on the report": a spoken start and end, a timer in the Island, optional
  **distracting-site blocking** through the browser extension (the user's list), notifications held
  and read on "what did I miss?".
- Pomodoro cycles with breaks. Each session is logged to today's note (NA-01) and to time tracking
  (NA-44).

### NA-44 · Time tracking by project · P3 · M
**Lands in:** CONVERSATION (workspaces), MEMORY, UX (Usage)

- Built only from **explicit** signals: "start/stop working on X", focus sessions, and the
  workspaces KIVO acted in. Never passive app tracking.
- "How long did I spend on KIVO this week?" is answered from the log. Totals appear in Weekly
  review.

### NA-45 · Quick capture anywhere · P1 · S
**Lands in:** UX (hotkeys), MEMORY (inbox)

- One hotkey + speech (or "Kivo, note…") sends an idea, task or link straight to `inbox/` in the
  vault, without opening any window. A soft earcon confirms it.
- The selected text or the current browser URL can be attached with a modifier key.

### NA-46 · Read it to me: human-sounding narrator · P1 · L
**Lands in:** VOICE (TTS front-end, new player), TOOLS (files), UX (a Reader page or panel)

"Kivo, read this story to me" / "read these files" / "continue my book."

- **Formats:** PDF, EPUB, DOCX, TXT, Markdown, HTML, web pages (extension DOM text), and the
  selection. PDF text uses its reading order; headers, footers and page numbers are dropped.
- **Text normalizer (a shared TTS front-end, used by every KIVO voice, not only the reader).**
  This is the step called *text normalization / verbalization*:
  - Links: "a link to github.com" or skipped (a setting). Never "h t t p s colon slash slash".
    File paths and code are summarised or spelled only on request.
  - Numbers, dates, times, currencies, units, ordinals, Roman numerals in chapter titles
    ("Chapter IV" → "Chapter four"), abbreviations (Dr., St., e.g.), all per language.
  - Punctuation becomes **prosody, never words**: commas and full stops become pauses of
    different lengths, em dashes and ellipses become dramatic beats, parentheses get a lowered
    aside, quotes and dialogue get a tone shift, headings get a pause before and after.
    Apostrophes are never read.
  - Emoji and symbols: named briefly or skipped (a setting). Footnote markers are skipped.
  - Chunked into paragraph-sized pieces for stable long-form prosody.
- **Character voices in dialogue:** a narrator voice plus a distinct voice per speaking character,
  with speakers detected locally (quote attribution: "…," said Maya) and editable per book.
- **Player:** play/pause/skip by voice or media keys, speed 0.5×–2×, a sleep timer, chapter
  navigation, **bookmarks and resume** ("continue my book" picks up at the last sentence), and
  **export to MP3/M4B** as an audiobook with chapters.
- **Pre-render ahead:** synthesises a few paragraphs ahead at low priority, so reading never
  stutters on slower PCs. Stops at once on pause or barge-in.
- Books are listed in a small Library (the files stay where they are; KIVO keeps positions).

---

## Phase 10 — Voice upgrades

### NA-47 · Follow-up without the wake word · P1 · S
**Lands in:** VOICE (wake/VAD), UX (Island)

- After KIVO answers, it keeps listening for about 5 seconds (a visible ring in the Island counts
  down), so "and tomorrow?" works without "Hey Kivo". Stops at once if someone else talks (speaker
  modes) or on silence. Length configurable, can be switched off.

### NA-48 · Whisper mode · P2 · S
**Lands in:** VOICE (TTS), BRAINS (verbosity)

- If the user whispers (a low-energy, breathy speech detector), or during set late hours, KIVO
  answers **quieter and shorter**, and may switch to text only in the Island.

### NA-49 · Voice profiles and cloning, local only · P2 · M
**Lands in:** VOICE (TTS engines), UX (Voice page)

- Record a voice (owner's own or anyone's; owner decision: the user is responsible for whose voice
  they record), then create a **local TTS voice profile** with an engine that supports cloning.
- **Profile manager** in the Voice page: create, rename, preview, export or import as a file, and
  delete. Audio samples and profiles are **never uploaded** and live in the KIVO data folder.
- Minimum safeguards that don't block the user: a one-time notice about misuse and local laws,
  profiles are labelled as cloned in the UI, and KIVO never uses a cloned voice to call, message or
  speak *as* someone to another person (it only speaks to the user).

### NA-50 · Duck other apps' audio · P1 · S
**Lands in:** VOICE (audio session control), TOOLS (audio)

- While listening and while speaking, lower other apps' audio sessions (Spotify, browsers,
  players) by a set amount through Windows' per-app audio sessions, then restore the exact
  previous levels. Per-app exceptions (never duck a call app).
- Today KIVO only ducks its **own** TTS during barge-in; other apps keep playing over it.

---

## Phase 11 — Characters and the Orb

People ask OpenAI for animated companions, but the ChatGPT app's characters make PCs lag. KIVO's
characters must be *light first*.

### NA-51 · Character system · P2 · L
**Lands in:** UX (Island, Orb), DESIGN_SYSTEM, BENCHMARKS (budget)

**What kinds (the owner left the choice to Claude):**

| Tier | Format | Cost | Default |
|---|---|---|---|
| **1 · Procedural 2D (SVG)** | `.avatar.json`, the approach of the avatar lab (see Sources): parametric geometry, expressions stored as offsets from a neutral face, timelines of expressions, automatic blinking, ambient motion | Tiny: no textures, no shaders, crisp at any size | **Yes** |
| **2 · Sprite / pixel art** | Sprite sheets (PNG/WebP atlas + JSON frames), pixelated or smooth | Very low; fixed frames | Optional |
| **3 · 3D (VRM)** | VRM models through a small WebGL renderer, loaded only when chosen | Highest; opt-in, with a warning and its own budget | Optional |

- **Size and placement:** **face or bust** by default (in the Island or Orb). An optional
  **full-body desktop buddy** can sit on the taskbar or window edges, walk along them, wave, and
  point at things with its hands during walkthroughs (NA-13). It is click-through except on the
  character, and it hides in fullscreen and games (NA-21).
- **States:** idle, listening (leans in, ears or eyes react to voice level), thinking, speaking,
  success, error, sleeping. These are the same states as the Orb, so any character works with any
  feature.
- **Lip-sync:** mouth shapes from the TTS output level KIVO already streams (`levels`), plus
  phoneme/viseme timing when the TTS engine provides it. No extra model.
- **Frame rate and resolution:** the user can choose 15 / 30 / 60 fps. **Default 30, 15 on
  battery**, and **0 when hidden, minimised or covered**. Rendering resolution follows the
  display's DPI.
- **Hard resource budget** (measured in `kivo-bench`, part of the release gate): Tier 1 and 2
  **≤ 2 % CPU and ≤ 30 MB RAM while animating**, ≈ 0 when idle and hidden. Tier 3 gets its own
  published budget. Exceeding the budget fails the release. Animation runs outside React's render
  cycle (a Motion-style value loop).
- VTube Studio / Live2D: not bundled (the Live2D SDK's licence and cost). Could come later as a
  plugin.

### NA-52 · Character Studio page · P2 · L
**Lands in:** UX (a new sidebar page), DESIGN_SYSTEM

- A page in the Control Center's left sidebar (not a separate app), with tabs:
  **Characters** (library, choose, import, export), **Look** (shapes, colours, proportions,
  accessories for Tier 1; sheets for Tier 2), **Expressions** (edit each state's expression
  against the neutral face; changes are copy-on-write per character), **Animations** (timelines,
  step duration, transitions, loop/once/ping-pong, blinking), **Voice** (the character's TTS voice
  or cloned profile, NA-49), **Behaviour** (placement, desktop-buddy on or off, how chatty it is),
  **Performance** (fps, a live CPU/RAM meter against the budget).
- Characters are shared as files and through the gallery (NA-31).

### NA-53 · A more expressive Orb · P2 · M
**Lands in:** UX (Orb), DESIGN_SYSTEM (motion)

- The Orb keeps its role as the default. It gains expression: reacting to voice energy,
  per-state motion curves, a subtle mood colour (success, warning, thinking), blinks and "looks"
  towards the pointer during walkthroughs, using the same expression and timeline model as NA-51
  so both share one engine and one budget.

---

## Phase 12 — Platform and reach

### NA-54 · Offline knowledge packs · P3 · M
**Lands in:** DISTRIBUTION (downloads), BRAINS (tools)

- Optional downloadable packs: a Wikipedia digest (a Kiwix ZIM file), a dictionary and thesaurus,
  a unit and currency converter (rates cached), world clock and holidays. They're exposed as local
  tools, so "what's the capital of…" and "define…" work offline (invariant 12).

### NA-55 · Local model manager for chat LLMs · P2 · M
**Lands in:** DISCOVERY (models), BRAINS (local), UX (Brains page)

- The speech-engine recommender, done for chat models too: "**which local model fits my PC?**"
  from RAM, VRAM and the GPU policy, then a one-click download (Ollama / LM Studio / KIVO's own
  runtime), a quick local benchmark (tokens/s, first-token time), and switching.
- Shows the disk used, and deletes models the user no longer uses.

### NA-56 · Portable mode · P3 · M
**Lands in:** DISTRIBUTION

- Runs from a folder or USB drive with **no install and no admin**. Data and models are stored
  next to the executable, and nothing is written to the registry except what Windows needs for the
  session.
- Features that need install-time registration (native messaging host, jump list, autostart) are
  shown as unavailable with a "Install KIVO for this" button.

### NA-57 · Public plugin SDK and docs · P2 · L
**Lands in:** INTEGRATIONS_AND_PLUGINS (WASM plugins), ROUTINES

- A developer portal: plugin (WASM) and routine templates, a local test harness (the `testenv/`
  fixtures), the manifest and permissions reference, and signing and submission to the gallery
  (NA-31).
- A `kivo-plugin` CLI to scaffold, run against a dev runtime, and package.

---

## Phase 13 — Developer helpers

### NA-58 · Build and test watcher by voice · P2 · S
**Lands in:** TOOLS (watchers), UX

- Extends the M5 build-exit watcher: "tell me when the build finishes **and read the first
  error**", then "open it" jumps to the file and line in VS Code (`code -g file:line`).
- Summarises test results ("3 failed, all in the parser"), and offers to delegate the fix to the
  Coding agent.

### NA-59 · Voice git · P3 · M
**Lands in:** TOOLS (shell, risk parser), CONVERSATION (workspaces)

- "What changed today?", "commit this with a message about the parser", "which branch am I on?",
  "stash my changes". Read commands are Low risk. Commit is Medium, shown before running. Push,
  reset and force operations are High and always confirmed. It uses the repo's own git
  configuration and never changes identity.

### NA-60 · Explain this error · P2 · S
**Lands in:** CAPABILITIES (circle to ask), BRAINS

- Select or circle (NA-14) an error in any terminal, IDE or browser console, and KIVO explains it
  and proposes a fix. It uses a local model when one is available. The error text is untrusted
  content.

---

## Phase 14 — PC smarts

### NA-61 · PC health doctor · P2 · M
**Lands in:** TOOLS (system info), UX

- "Why is my PC slow?" is answered from local counters: top processes by CPU, RAM, disk and GPU,
  disk space, startup apps, temperatures where available, pending updates. No brain is needed for
  the numbers; a brain only explains them if asked.
- Offers **safe fixes, each confirmed:** close a runaway app, disable a startup app (reversible),
  clear temp files (moved to the Recycle Bin), switch the power plan.

### NA-62 · Downloads and desktop tidy · P3 · S
**Lands in:** TOOLS (files), ROUTINES

- "Tidy my downloads" proposes a sort (by type, date or project) with a **preview**, then moves
  the files. It **never deletes**, and one Undo puts everything back. It can run as a scheduled
  routine.

### NA-63 · Smart notifications digest · P2 · M
**Lands in:** TOOLS (Windows `UserNotificationListener`), UX §7, SECURITY

- With **per-app opt-in**, KIVO reads Windows notifications and summarises them in "what did I
  miss?" and the Morning brief. Grouped by app and person, with duplicates merged.
- Notification text is untrusted content. Apps not opted in are never read.

### NA-64 · Battery and power coach · P3 · S
**Lands in:** TOOLS (power, display), ARCHITECTURE (performance profiles)

- "How long will my battery last?", "save battery" (power plan, brightness, KIVO's own
  low-power profile), "what's draining my battery?" (per-app energy where Windows reports it).

---

## Phase 15 — Personal

### NA-65 · Language practice partner · P3 · M
**Lands in:** VOICE (languages), BRAINS (persona), MEMORY

- "Let's practise Spanish for 10 minutes": a spoken conversation at the user's level, gentle
  corrections after each turn (or at the end, a setting), new words collected into a vault note
  `languages/spanish.md` for review.
- Uses local STT and TTS for the language when available; the brain is the user's choice.

---

## Phase 16 — Phone and Control Center look

### NA-66 · Companion phone app · P2 · L
**Lands in:** INTEGRATIONS (phone remote), DISTRIBUTION

- A light Android and iOS client for **your PC's KIVO** (not a separate assistant): talk or type
  to it, see results and running tasks, get its notifications, **approve actions** (with the
  phone's biometrics for High risk), read the daily note, and trigger quick actions (NA-04).
- Pairs with the PC over a QR code. Works on the LAN, and through an optional end-to-end
  encrypted relay when away. The PC stays the only authority (invariant 2).
- Carries the handoff (NA-29) and can replace the chat-app remote (NA-28) for users who install
  it.

### NA-67 · Themes and accent picker · P3 · S
**Lands in:** DESIGN_SYSTEM, UX (Settings → Appearance)

- Accent colour picker, "match Windows accent", "match wallpaper", compact or comfortable
  density, and a few curated themes. All built from the existing tokens, so contrast checks and
  Windows contrast themes keep working.

### NA-68 · Customizable Home widgets · P2 · M
**Lands in:** UX (Home)

- Home becomes a grid of widgets the user arranges and resizes: Today (NA-01), Quick actions
  (NA-04), running tasks, recent activity, weather, calendar, usage and cost, reading progress
  (NA-46), character (NA-51). A sensible default layout, and "reset layout".
- Widgets update from pushed events only (no polling), and widgets that aren't visible do no work.

### NA-69 · Mini mode · P2 · S
**Lands in:** UX (a window mode)

- A small always-on-top compact window (talk, the last answer, quick actions, a text box) instead
  of the full Control Center. It snaps to a screen corner and remembers its place. One click
  expands it to the full app.

---

## Phase 17 — Memory and knowledge

### NA-70 · "Remember where I put…" · P2 · S
**Lands in:** MEMORY, BRAINS (grammar)

- "The spare key is in the blue drawer" / "I parked on level 3" is saved as a small fact note
  (`places/` or `facts/`). "Where's the spare key?" is answered instantly from the grammar, with
  no model. A newer fact replaces the old one (`valid_until`).

### NA-71 · People notes · P2 · M
**Lands in:** MEMORY (`people/`), CONVERSATION (suggestions)

- Birthdays, family names, preferences and "last talked about", **only from what the user tells
  KIVO** (Suggest mode proposes, the user accepts). "What's Maya's kid's name?", "whose birthday is
  this week?" (feeds the Morning brief). Marked `sensitivity: personal`, and never sent to a cloud
  brain unless the privacy settings allow it.

### NA-72 · Web clipper · P2 · S
**Lands in:** TOOLS (browser extension), MEMORY

- "Save this page": the extension sends the readable article, and KIVO saves clean Markdown to
  `clips/` with the source URL, date, a short summary and tags. Images are optional. Clipped
  content is untrusted.

### NA-73 · Ask my vault (and chosen folders) · P1 · M
**Lands in:** MEMORY (hybrid search), CONVERSATION

- Questions across the vault **and folders the user adds** (PDFs, docs): answered from retrieved
  passages with **citations** to the notes and pages (NA-36). Local embeddings. The brain gets only
  the passages, subject to privacy labels.
- Folders are indexed on events (file watchers), never by rescanning.

---

## Phase 18 — Automation power-ups

### NA-74 · Watch and repeat (record a routine) · P2 · L
**Lands in:** ROUTINES (builder), TOOLS (UIA events)

- "Watch me do this": an explicit, time-boxed recording with a visible indicator. KIVO records the
  **UIA-level steps** (which app, which control, what value), not pixels or keystrokes, and turns
  them into an editable routine with variables suggested for values that change.
- Passwords and secure fields are never recorded. The recording is shown step by step before it's
  saved.

### NA-75 · More event triggers · P2 · M
**Lands in:** ROUTINES (triggers), TOOLS (watchers)

- USB device plugged in, Wi-Fi network joined or left, app opened or closed, headset or Bluetooth
  device connected, monitor attached or detached, docked or undocked, on battery or plugged in,
  session locked or unlocked. All are OS events, never polling.
- Example: "When I connect to the office Wi-Fi and dock, restore my work layout" (NA-20).

### NA-76 · Web forms autopilot · P3 · M
**Lands in:** TOOLS (browser extension), MEMORY, SECURITY

- Fills repetitive web forms from vault profile notes (name, address, company, standard answers).
  Every field is shown for **review before submit**, and submitting is a separate, confirmed
  action.
- **Never** fills passwords, card numbers, government IDs or bank details (those fields are
  detected and skipped). Destination binding applies.

### NA-77 · Page and price watcher · P3 · M
**Lands in:** TOOLS (watchers, browser/CDP), ROUTINES

- "Tell me when this page changes" or "when the price drops below ₹20,000": KIVO watches a chosen
  element on the page, with checks spaced out (default hourly, never faster than every 15
  minutes), run by KIVO's own browser in the background at low priority. Watched content is
  untrusted; alerts only, never automatic purchases.

---

## Phase 19 — Safety and recovery

### NA-78 · Time-machine undo · P2 · M
**Lands in:** SECURITY (audit), TOOLS (undo), UX (Activity)

- A timeline of everything KIVO changed today, with each step's undo state. "**Undo back to
  3 pm**" undoes, in reverse order, every step that can be undone, lists the ones that can't (a
  sent email), and asks before starting.

### NA-79 · Dry-run for routines · P2 · S
**Lands in:** ROUTINES (builder)

- "Test this routine" walks through each step and shows **what it would do** (which app, which
  file, which message), resolving variables, **without doing anything**. Steps that need live data
  show what they'd read.

### NA-80 · Panic phrase · P2 · S
**Lands in:** SECURITY (emergency stop), VOICE (grammar)

- A custom secret phrase (set by the user, checked for collisions like wake words) that **stops
  everything and locks the PC** at once. It works even in AI-free mode and with no brain loaded,
  and needs the owner's voice when enrollment is on.

### NA-81 · Per-brain data view · P2 · M
**Lands in:** SECURITY (privacy, audit), UX (Brains / Usage)

- "**What has Claude seen today?**" lists everything sent to each cloud brain: prompts, tool
  results, files, screenshots, with their privacy labels, filterable by day and thread, and
  exportable. Items can be deleted from KIVO's records (the provider's own retention is explained,
  with a link).

---

## Phase 20 — More memory

### NA-82 · Photo and screenshot memory · P3 · M
**Lands in:** MEMORY, TOOLS (OCR)

- "Remember this" on an image, screenshot or snipped region (NA-15): saved to the vault
  (`images/`) with local OCR text, a short description and tags, so "find that receipt I saved" or
  "what was the Wi-Fi password on the router sticker?" works later. Explicit only; no background
  capture.

### NA-83 · Memory timeline · P2 · M
**Lands in:** UX (Memory page), MEMORY

- A visual timeline of **what KIVO learned and when**: facts, preferences, people notes,
  workspace notes, each with its source (which conversation or task) and the option to edit,
  mark outdated or forget. It answers the top 2026 memory complaint, "what does it know about me,
  and why?".

---

## Phase 21 — Built into Windows

Context: Google shipped a native Gemini app for Windows on 2026-09-10, whose only system hook is
Alt+Space. Microsoft Copilot owns the taskbar and the Copilot key. KIVO should feel native without
fighting either.

### NA-84 · Taskbar and Start presence · P2 · M
**Lands in:** UX (lifecycle), DISTRIBUTION

- Beyond the tray: a Windows 11 **Widgets board** widget (today, quick actions, talk), jump-list
  and taskbar thumbnail buttons (talk, pause, stop), and KIVO actions in Start search results
  where Windows allows it.

### NA-85 · Right-click "Ask KIVO" · P2 · M
**Lands in:** TOOLS (files), DISTRIBUTION (shell extension registration)

- An Explorer context menu on files and folders (the Windows 11 modern menu): **summarise**,
  **rename smartly** (with preview and undo), **convert** (image, PDF, audio formats), **read it to
  me** (NA-46), **ask about this** (opens a chat with the file attached, NA-16).
- File content is untrusted.

### NA-86 · Share target · P3 · S
**Lands in:** DISTRIBUTION (package identity), UX

- KIVO appears in the Windows Share sheet to receive text, links and files from any app: save to
  the vault, ask about it, read it aloud, or add to today.

---

## Phase 22 — Special situations

### NA-87 · Presenter mode · P2 · S
**Lands in:** UX §7, TOOLS (UIA for PowerPoint and browsers)

- Detected automatically (PowerPoint or Google Slides in presentation, a share in progress) or
  switched on by voice. "Next slide", "go back", "show the timer", "how long have I been going?".
- **Silent-only:** nothing pops up on the presenting screen, and answers go to the presenter's
  other monitor, to the phone (NA-66) or to a soft earcon only.

### NA-88 · Streaming and recording safe · P1 · S
**Lands in:** UX (overlay), SECURITY (privacy)

- Detects OBS, screen recorders and screen sharing, then **excludes KIVO's windows from capture**
  (`SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE`) and hides personal info and
  notifications from the Island. Personal content only appears on the user's own screen.
- A manual "streamer mode" switch too.

### NA-89 · Travel / offline mode · P2 · S
**Lands in:** CAPABILITIES (presets), BRAINS (privacy)

- One switch: everything local (local STT, TTS and brain if installed), sync and catalogs paused,
  metered-connection aware. KIVO **says what won't work** ("web search and Claude are off until
  you're back online") instead of failing later. Can turn on automatically on a metered or
  unknown network (a setting).

### NA-90 · Low-vision mode · P2 · M
**Lands in:** UX, CAPABILITIES (screen reading), VOICE

- "Read the screen" / "what's under my mouse?" / "describe this image", using UIA text first and
  local OCR or vision on request. A big-text, high-contrast Island, and spoken confirmation of
  every action. Works alongside Narrator and NVDA without fighting them (KIVO's own announcements
  already use the screen-reader channel).

---

## Phase 23 — Learning the user (locally)

### NA-91 · Learns your phrasing · P2 · M
**Lands in:** BRAINS (grammar, semantic stage), MEMORY

- When the user corrects a misunderstood command ("no, I meant open Spotify"), KIVO offers to add
  that phrasing as a variant of the right command. These variants are local, listed in Settings
  (review, delete), and never learned silently. They feed the grammar and the semantic stage
  exemplars (and Reflex's correction learning).

### NA-92 · Routine suggestions from habits · P2 · M
**Lands in:** ROUTINES, UX (suggestions, NA-25)

- "You open VS Code, Chrome and Spotify most mornings around 9. Make it a routine?" This is based
  **only on KIVO's own command and task history**, never on passive tracking. It follows the
  suggestion manners (NA-25), and the proposed routine opens in the builder with its grants shown.

### NA-93 · Personal vocabulary · P2 · S
**Lands in:** VOICE (STT personalization), MEMORY

- One list of the user's names, places, products and jargon (KIVO, Tauri, family names), fed to
  STT biasing, dictation (NA-08), the wake-word confusability checker and TTS pronunciation
  (with a "say it like this" recording or spelling). Built from corrections, people notes (NA-71)
  and manual entries.

---

## Phase 24 — Media and audio

### NA-94 · Live captions for any audio · P2 · M
**Lands in:** VOICE (loopback STT), UX (caption overlay)

- Local captions for whatever the PC plays (videos, calls, streams) in a small movable overlay,
  with optional live translation (NA-12). **Nothing is kept** except a short rolling buffer in
  memory (used by NA-97), which is cleared when captions stop.
- Excluded from screen capture when sharing (NA-88).

### NA-95 · Summarise this video · P2 · M
**Lands in:** TOOLS (browser extension, files), BRAINS

- For the YouTube video or local video playing: the transcript (the site's captions first, local
  STT otherwise) becomes a summary with timestamps, plus "**jump to where they talk about X**"
  (seeks the player). The transcript is untrusted content; it can be saved to the vault.

### NA-96 · Deep media control · P2 · M
**Lands in:** INTEGRATIONS (media), TOOLS (UIA, URIs)

- "Play my focus playlist", "skip the intro", "what's playing?", "add this to liked", "play
  something like this", "volume 30 in Spotify only". Through Windows media controls, `spotify:`
  URIs, UIA in Spotify, YouTube (extension) and VLC. No Web API connector needed (INTEGRATIONS §0).

### NA-97 · Podcast and audio notes · P3 · S
**Lands in:** MEMORY, VOICE

- "Save that last bit" keeps the last 30–60 s of the live caption buffer (NA-94) as a note, with
  the show, episode and timestamp when known. Only works while captions are on; nothing is buffered
  otherwise.

---

## Phase 25 — Information at a glance

### NA-98 · Weather and commute · P2 · S
**Lands in:** INTEGRATIONS (keyless weather source), ROUTINES (Morning brief)

- Local weather without an account (an open, keyless forecast API, city set by the user, never
  precise location unless allowed). "Do I need an umbrella?", "will it rain at 6?". In the Morning
  brief and a Home widget (NA-68). Commute time only if the user adds a maps connector.

### NA-99 · News by topic · P3 · S
**Lands in:** INTEGRATIONS (feeds), ROUTINES (Intel brief)

- RSS and Atom feeds the user chooses (no algorithmic feed), grouped by topic, summarised in the
  Intel brief (NA-03), shown or spoken. "Any news about Tauri?". Feed content is untrusted.

### NA-100 · Packages and deliveries · P3 · M
**Lands in:** INTEGRATIONS (mail connectors), MEMORY

- Tracking numbers found in shipping emails (with the mail connector, opt-in) become a list:
  "where's my package?", "what's arriving today?". Status from the carrier's public tracking page
  where allowed. In the Morning brief.

### NA-101 · Stocks and crypto glance · P3 · S
**Lands in:** INTEGRATIONS (price source)

- **Price lookups only:** "what's Apple at?", a watchlist widget. No trading, no transfers, and no
  advice (KIVO says it isn't a financial adviser if asked what to buy).

---

## Phase 26 — Sharing

### NA-102 · Share a result · P3 · S
**Lands in:** UX (Chat, Memory, Routines), INTEGRATIONS

- Send a report, note or routine as a file (Markdown, PDF, `.kivo-routine`) or to a chat app
  connector. Always shown and confirmed before sending, and the recipient comes from the user, never
  from content.

### NA-103 · Meeting follow-up drafts · P3 · S
**Lands in:** INTEGRATIONS (mail), MEMORY (meeting notes)

- After opt-in meeting notes (NA-33): "draft the follow-up" writes an email with the summary,
  decisions and action items with owners, in the user's style (NA-09). Sending is confirmed
  (NA-41).

---

## Phase 27 — Files KIVO makes: storage and retention

With all these features KIVO creates many files: notes, reports, clips, audio, caches, models,
exports, logs. The user decides what's kept, for how long, and what's deleted.

### NA-104 · Storage manager · P1 · M
**Lands in:** UX (Settings → Storage, a new tab), ARCHITECTURE (storage), MEMORY, DISTRIBUTION

- **One page shows everything KIVO stores**, grouped by kind, with its size, location and oldest
  item:

  | Kind | Examples | Default |
  |---|---|---|
  | **Permanent** | Vault notes, people, instructions, routines, skills, voice profiles, characters | Keep forever |
  | **Results** | Reports, clips, video summaries, audiobook exports | Keep, ask after 90 days |
  | **Temporary** | Caption buffers, TTS pre-render (NA-46), screenshots for one-shot questions, dictation backups, downloads in progress, thumbnails | Delete after 24 h–7 days |
  | **Caches** | Search index, embeddings, catalog caches, OCR caches | Rebuilt when needed; trim on size cap |
  | **Models** | STT, TTS, wake, local LLMs, knowledge packs | Keep; offer to remove unused after 60 days |
  | **Logs and audit** | Activity, audit, diagnostics | Audit per SECURITY rules; logs 30 days |

- **Per-kind rules the user sets:** keep forever, delete after N days, keep the newest N, a size
  cap. **Per-item overrides:** pin to keep forever, or "delete this now".
- **Deletes go to a KIVO trash first** (restorable for 7 days, a setting), then are removed.
  The vault's own Markdown files are only ever moved to the trash, never hard-deleted by a rule the
  user didn't set.
- **Cleanup runs in the background** at idle and low priority, on events (a size cap reached, a
  day passing), never by rescanning the disk. A summary appears in Activity ("Cleared 1.2 GB of
  temporary files").
- "Kivo, how much space are you using?" / "clean up your temporary files" work by voice.
- Settings can move the data, vault and models folders to another drive (with a safe move and a
  rollback).

---

## Phase 28 — Staying in control of many features

### NA-105 · Feature switchboard · P1 · M
**Lands in:** CAPABILITIES, UX (Settings)

- One searchable page listing **every feature** (built and NA) with an on/off switch, what it
  costs (idle and active CPU/RAM, disk, from `kivo-bench`), whether it's **local or cloud**, what
  it needs (a connector, a model download), and a link to its settings. Filters: on, off, cloud,
  heavy.
- It extends the existing capability toggles rather than replacing them. Features that are off use
  no resources.

### NA-106 · Starter packs · P2 · S
**Lands in:** UX (onboarding, setup recommendations)

- At onboarding and later in the switchboard: **Productivity**, **Developer**, **Reader**,
  **Home**, **Accessibility**, **Creator**, **Minimal** (voice remote only, NA-38). Each turns on a
  related set of features and starter routines, with a preview of what's turned on and what it
  downloads. Packs can be mixed and undone.

### NA-107 · Settings by voice · P2 · S
**Lands in:** BRAINS (grammar), UX (Settings)

- "Turn off follow-up listening", "make your voice slower", "where's the dictation setting?"
  (opens the page and highlights the setting). Deterministic settings changes go through the grammar
  from the settings schema, with no model. Security settings can't be lowered by voice; they open
  the page instead.

### NA-108 · Settings backup and sync · P2 · M
**Lands in:** ARCHITECTURE (storage), DISTRIBUTION

- Export or import all settings, routines, snippets, characters and voice profiles as one file.
  Optional **sync between the user's PCs through a folder they choose** (OneDrive, a NAS, a USB
  drive), with no KIVO cloud. Secrets are never included (Credential Manager stays per PC), and
  conflicts are shown, never silently merged.

---

## Phase 29 — Quality and support

### NA-109 · "That was wrong" and bug reports · P1 · S
**Lands in:** UX, DISTRIBUTION (diagnostics bundle)

- "Kivo, that was wrong" (or a thumbs-down on any turn) saves the turn locally: what was heard,
  the route, the answer, and an optional note. These feed NA-91 (learning phrasing) and a local
  "misunderstandings" list.
- **Bug report:** attaches the diagnostics bundle with a full **preview of what's included**, and
  secrets and personal data are redacted. Nothing is sent without the user seeing and confirming
  it.

### NA-110 · Hands-on What's new · P3 · S
**Lands in:** UX (What's new, onboarding tour NA-27)

- After an update, the existing What's new becomes a short, skippable interactive tour of the new
  features ("try saying…"), with each item linking to its switch in the switchboard (NA-105).

### NA-111 · Help by voice, offline · P2 · M
**Lands in:** DISTRIBUTION (bundled docs), BRAINS

- "How do I make a routine?", "what can you do with Spotify?" are answered from **KIVO's own
  bundled help** (versioned with the app, searchable locally), with "show me" opening the page or
  starting a walkthrough (NA-13). Works offline, and in AI-free mode as search results.
- **This is how non-technical users are supported (owner decision):** no separate "simple mode"
  and nothing hidden. Every feature stays visible, and the user just asks KIVO: "what's a
  brain?", "what does this switch do?", "help me set up Gmail". KIVO explains it in plain words
  and offers to do it or walk them through it.

### NA-112 · Health self-test · P2 · S
**Lands in:** DISTRIBUTION (diagnostics), UX

- "Kivo, check yourself": mic (level and noise), speakers, wake word (say it once), echo path,
  STT/TTS engines, each brain and connector (a read-only request), the browser extension, hotkeys,
  disk space and models. Every failure comes with a one-click or spoken fix. Also runs after
  updates and on first start.

---

## Phase 30 — Creating things

### NA-113 · Image generation · P3 · M
**Lands in:** BRAINS (provider trait: image capability), DISCOVERY (local ComfyUI)

- "Make a thumbnail of…", "an icon for my app, flat style". Through a user-chosen provider
  (OpenAI, Gemini, others with the user's key) or a **local** model (ComfyUI / Stable Diffusion
  found on the PC). Results go to `results/images/` under the storage rules (NA-104), with the
  prompt saved alongside.
- Local generation runs only when asked, at a chosen priority, and never while gaming (NA-21).

### NA-114 · Document writer · P2 · M
**Lands in:** BRAINS, TOOLS (files)

- "Write a cover letter for this job", "turn this report into a proper document": DOCX, PDF or
  Markdown from **templates** (bundled and user-made), in the user's style (NA-09), with facts
  from the vault cited. It opens in the user's editor for the final say.

### NA-115 · Slides from notes · P3 · M
**Lands in:** TOOLS (files)

- A vault note or report becomes a simple PPTX deck (title, sections as slides, speaker notes),
  using a chosen template. It opens in PowerPoint for editing, and pairs with presenter mode
  (NA-87).

### NA-116 · Social post drafts · P3 · S
**Lands in:** BRAINS, UX

- A note, report or video summary becomes post drafts per platform (length and tone per
  platform), shown side by side for copying. **Never posted automatically.** Posting, if ever
  added, would be a confirmed action through a connector.

---

## Phase 31 — Gaming and learning

### NA-117 · Game companion · P2 · M
**Lands in:** TOOLS (Xbox Game Bar, Discord, display), UX (game mode NA-21)

- In game mode, a small set of voice commands that never break the game: "**clip that**" (Game
  Bar capture shortcut), "start/stop recording", "what's my FPS?" (from Windows performance
  counters where available), "mute Discord", "volume down in game only". Answers are spoken
  briefly or shown as a tiny overlay that's safe with fullscreen and anti-cheat (no injection into
  game processes, ever).
- Stays inside the game-mode resource budget (wake word or push-to-talk only).

### NA-118 · Game guide lookups · P3 · S
**Lands in:** BRAINS, TOOLS (web search)

- "How do I beat this boss?", "where's the blacksmith?": KIVO knows the game in focus, searches
  wikis and guides, and answers **by voice** without alt-tabbing. Sources are cited (NA-36), and
  spoilers get a warning first (a setting).

### NA-119 · Game launcher and library · P3 · M
**Lands in:** TOOLS (app registry), DISCOVERY

- "Play Elden Ring" finds the game across Steam, Epic, GOG, Xbox and Battle.net (from their local
  library files and URIs, no accounts), launches it, and can switch on game mode at the same
  time. "What games haven't I played lately?".

### NA-120 · Tutor mode · P3 · M
**Lands in:** BRAINS (persona), MEMORY

- "Teach me how compound interest works": explains step by step, checks understanding with short
  questions, adapts to the answers, and saves a summary to the vault. By voice or in Chat, and it
  pairs with walkthroughs (NA-13) for learning software.

---

## Phase 32 — Agents, kept off until the user wants them

**Owner rule:** agents and sub-agents (KIVO's own, and those inside products like Claude Code or
Codex) are **off by default**, to keep cost low. They run only when the user enables them, either
globally or per request, and the cost estimate is shown before they start.

### NA-121 · Agents and sub-agents off by default · P1 · S
**Lands in:** BRAINS (routing), CONVERSATION (agents), CAPABILITIES

- KIVO never routes to an agent, or spawns parallel or sub-agents, unless the user has switched
  agents on (Settings, the switchboard NA-105, or "use an agent for this").
- When KIVO launches a CLI agent, it passes that product's own settings to **disable its
  sub-agents or parallel tasks** where the product supports it (from the app registry), unless the
  user allowed them.
- A request that would need an agent says so and gives the estimated cost: "This needs an agent
  (about $0.40). Start one?"

### NA-122 · Background agent desktop · P2 · L
**Lands in:** CAPABILITIES (computer use), TOOLS (input), SECURITY

- When computer use is enabled, it can run in a **separate, invisible desktop session**
  (Windows' agent workspace where available, otherwise a separate desktop or a Windows Sandbox or
  VM), so it **doesn't take over the user's mouse and keyboard**. The user can **peek** at it live
  in a small window, take over, or stop it.
- Falls back to the visible desktop, with the existing watch mode, only when the app can't run in
  the separate session. It says so first.

### NA-123 · Mission control · P2 · M
**Lands in:** UX (Tasks and Agents pages), CONVERSATION (tasks)

- One view of every running agent and task across brains: progress, current step, cost so far,
  **files changed and diffs**, approve/merge or discard, pause, cancel. Finished work waits in a
  review queue instead of being applied silently.

### NA-124 · Folder scopes for agents · P1 · M
**Lands in:** SECURITY (grants), CAPABILITIES

- Agents can touch only **Documents, Downloads, Desktop and the current workspace** by default
  (matching Windows' agent model). Anything else needs an explicit grant, shown with the path,
  scoped and timed like other grants. Scopes are enforced by KIVO's file tools and, for CLI agents,
  by their launch settings and working directory.

---

## Cross-cutting rule — smooth on every device

Owner's requirement for **everything in this file and everything already built**: it must run
smoothly and use the least resources possible **without losing speed, accuracy or smoothness**, on
every supported PC, from low-end laptops to high-end desktops.

For every NA item, when it moves into a spec:

1. **A budget is declared up front** (idle CPU, RAM, disk, first-response latency) and measured in
   `kivo-bench` on the reference tiers, including a low-end tier. It joins the release regression
   gate.
2. **Nothing runs when unused:** no timers or polling (events only), models load on first use and
   unload after idle, hidden UI renders nothing (0 fps), background work runs at low priority and
   pauses on battery saver, in games (NA-21) and in fullscreen.
3. **Tiered by hardware:** the performance profiles pick lighter engines, lower fps and smaller
   models automatically on weak PCs, and say so. A feature that can't meet its budget on a tier is
   offered as "slower on this PC" rather than silently degrading everything.
4. **Measured, not assumed:** each feature's T0–T10 latency points and resource use are
   instrumented from day one, and shown on a performance page (existing PLAN-07).
5. **The UI stays smooth:** 60 fps interactions on the reference hardware, virtualised long
   lists, animations off the main render path, and reduced motion respected.

---

## Suggested order

| Phase | Items | Why first |
|---|---|---|
| 1 | NA-01, 02, 03, 04, 05, 07 (06 when convenient) | Daily value from parts that already exist |
| 2 | NA-08, 10 first, then 09, 11, 12 | Dictation is the most-used feature in this category |
| 3 | NA-13, 14, 16, 17, then 15 | Beats HeyClicky at its own game with UIA accuracy |
| 4 | NA-21 first, then 18, 19, 20 | Small power tools, big "feels native" effect |
| 5 | NA-23 first, then 24, 25, 26, 22 | Trust and the multi-assistant model |
| 6 | NA-27 first, then 28, 31, 32, 33, 30, 34, 35, 29 | Onboarding, reach and delight |
| 7 | NA-37, 36, then 38 | Trust is the top complaint about AI in 2026 |
| 8 | NA-40, then 39, 41, 42 | Life integrations |
| 9 | NA-46, 45, then 43, 44 | The reader is a standout feature |
| 10 | NA-50, 47, then 48, 49 | Small voice wins with a big effect |
| 11 | NA-51 Tier 1 + NA-53 first, then 52, then Tiers 2–3 | Light first, measured |
| 12 | NA-55, then 57, 54, 56 | Platform and reach |
| 13 | NA-58, 60, then 59 | Developer helpers |
| 14 | NA-61, 63, then 64, 62 | PC smarts from local data |
| 15 | NA-65 | Personal |
| 16 | NA-69, 68, then 66, 67 | Look and reach |
| 17 | NA-73, 70, then 71, 72 | Memory that answers |
| 18 | NA-75, then 74, 77, 76 | Automation |
| 19 | NA-80, 79, then 78, 81 | Safety and recovery |
| 20 | NA-83, then 82 | Memory you can see |
| 21 | NA-85, then 84, 86 | Native to Windows |
| 22 | NA-88, 89, then 87, 90 | Situations |
| 23 | NA-93, 91, then 92 | Learning, locally |
| 24 | NA-96, 94, then 95, 97 | Media |
| 25 | NA-98, then 99, 100, 101 | At a glance |
| 26 | NA-102, 103 | Sharing |
| 27 | NA-104 early, alongside the first features that create files | Everything else writes files |
| 28 | NA-105 early, then 106, 107, 108 | Many features need one place to control them |
| 29 | NA-109, 112, then 111, 110 | Quality and support |
| 30 | NA-114, then 113, 115, 116 | Creating things |
| 31 | NA-117, 119, then 118, 120 | Gaming and learning |
| 32 | NA-121, 124 early, then 123, 122 | Cost control and agent safety |

---

## Sources

- HeyClicky changelog: https://www.heyclicky.com/changelog
- HeyClicky review (features, pricing, privacy): https://www.therundown.ai/tools/clicky
- Original Clicky source (MIT): https://github.com/farzaa/clicky
- XDA review: https://www.xda-developers.com/someone-built-tiny-ai-that-lives-next-to-your-cursor-the-most-useful-thing-ive-tried-this-year/
- Reddit's view of AI assistants in 2026: https://www.vellum.ai/blog/best-ai-assistant-reddit
- AI memory as a UX challenge: https://fintechasia.net/2026/08/20/why-ai-memory-is-becoming-the-next-big-ux-challenge-according-to-2026-research/
- Voice UI design guide 2026: https://fuselabcreative.com/voice-user-interface-design-guide-2026/
- Wispr Flow review: https://efficient.app/apps/wispr-flow
- Raycast review: https://efficient.app/apps/raycast
- Screen assistants 2026: https://screenpipe.com/blog/screen-assistant-ai-2026
- Avatar lab (SVG procedural characters): https://github.com/smontlouis/bible-strong-avatar-lab
- Microsoft scaling back Copilot, reworking Recall: https://www.windowscentral.com/microsoft/windows-11/microsoft-is-reevaluating-its-ai-efforts-on-windows-11-plans-to-reduce-copilot-integrations-and-evolve-recall
- 90 % of people don't trust AI with their data: https://www.malwarebytes.com/blog/privacy/2026/03/90-of-people-dont-trust-ai-with-their-data
- Why people are frustrated with AI: https://www.pcworld.com/article/3032937/i-love-ai-but-the-more-i-use-it-the-more-i-hate-it.html
- Requests for animated companions in ChatGPT: https://community.openai.com/t/avatar-and-embodiment-desired-features/1397907
- Long-form TTS, normalization and prosody: https://fish.audio/blog/best-text-to-speech-for-audiobooks-2026/ · https://medium.com/data-science-collective/high-quality-long-form-tts-with-qwen3-open-weight-models-cdd6e3d00df0
- Jev (evaluated and rejected, see docs/reflex/REFLEX.md): the "Jarvis OS 2.0" Instagram post
