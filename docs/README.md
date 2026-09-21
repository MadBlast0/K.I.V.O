# How the KIVO docs work

This folder is the complete specification of KIVO. It is written so that a person or an AI can
build the product from it, milestone by milestone, and so that progress is recorded **in the
docs themselves**, next to the requirement it satisfies.

## 1. Document map

| Document | What it is | Authority |
|---|---|---|
| [KIVO_Project_Plan.md](KIVO_Project_Plan.md) | The original product blueprint: vision, principles, full feature list | The *why*. Where §167 lists an amendment, the named spec wins. §168 maps every plan section to the spec items that build it |
| [architecture/*.md](architecture/ARCHITECTURE.md) | Engineering specs, one per subsystem | **The *how*. Authoritative for their topic** |
| [design/DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) + [design/mockups/kivo-app.html](design/mockups/kivo-app.html) | Look, components, motion, and every screen | **Authoritative for UI** (the mockup is the visual reference; the spec holds the rules) |
| [DECISIONS.md](DECISIONS.md) | The owner's decisions, dated, newest first | Settled. Not reopened without the owner |
| [ROADMAP.md](ROADMAP.md) | Build order: milestones, which docs to read, which items to build, exit criteria | Decides *what is built next* |
| [research/](research/) | The research behind the decisions | Background only |
| `benchmarks/` (created in M0) | Measured results | Evidence for engine defaults |

**When two documents disagree:** DECISIONS.md → the topic's spec → DESIGN_SYSTEM/mockup (UI
only) → the plan. Fix the loser in the same change and note it in DECISIONS.md.

## 2. Requirement IDs

Every spec ends with a **`## Build checklist`**. Each line is one buildable, verifiable
requirement:

```text
- [ ] **VOICE-12** · M2 · Barge-in: duck TTS −12 dB within 50 ms, cancel after 300 ms of speech (§7)
       │            │    │                                                                   └ section that defines it
       │            │    └ what must exist
       │            └ milestone (M0–M9, D0, L1–L5, Post)
       └ ID: never renumbered or reused
```

| Prefix | Spec |
|---|---|
| `ARCH` | [architecture/ARCHITECTURE.md](architecture/ARCHITECTURE.md) |
| `VOICE` | [architecture/VOICE.md](architecture/VOICE.md) |
| `BRAIN` | [architecture/BRAINS.md](architecture/BRAINS.md) |
| `TOOL` | [architecture/TOOLS_AND_CONTROL.md](architecture/TOOLS_AND_CONTROL.md) |
| `SEC` | [architecture/SECURITY.md](architecture/SECURITY.md) |
| `UX` | [architecture/UX.md](architecture/UX.md) |
| `CONV` | [architecture/CONVERSATION.md](architecture/CONVERSATION.md) |
| `MEM` | [architecture/MEMORY.md](architecture/MEMORY.md) |
| `DISC` | [architecture/DISCOVERY.md](architecture/DISCOVERY.md) |
| `CAP` | [architecture/CAPABILITIES.md](architecture/CAPABILITIES.md) |
| `ROUT` | [architecture/ROUTINES.md](architecture/ROUTINES.md) |
| `INT` | [architecture/INTEGRATIONS_AND_PLUGINS.md](architecture/INTEGRATIONS_AND_PLUGINS.md) |
| `DIST` | [architecture/DISTRIBUTION.md](architecture/DISTRIBUTION.md) |
| `REL` | [architecture/RELEASE.md](architecture/RELEASE.md) |
| `BENCH` | [architecture/BENCHMARKS.md](architecture/BENCHMARKS.md) |
| `DS` | [design/DESIGN_SYSTEM.md](design/DESIGN_SYSTEM.md) |
| `PLAN` | [KIVO_Project_Plan.md §168](KIVO_Project_Plan.md) (requirements that exist only in the plan) |

New requirements get the next free number in their spec and must also be named in their
milestone's **Build order** in ROADMAP.md (`pnpm docs:sync` enforces this). A requirement that is dropped keeps its
ID and is marked `[-]`.

## 3. Status marks

| Mark | Meaning | Required note |
|---|---|---|
| `[ ]` | Not started | — |
| `[~]` | Partly built | `→ partial: <what exists> · missing: <what doesn't>` |
| `[x]` | Built **and verified** | `→ done: <main path(s)> · verified: <test name, command, or manual check>` |
| `[-]` | Dropped or deferred | `→ dropped: <reason>, see DECISIONS.md <date>` |

An item is `[x]` only when **all** of its text is true, its checks pass (tests, typecheck,
clippy, no warnings), and it has been exercised: by an automated test where one is practical,
otherwise by a described manual check. Mocked or stubbed behaviour is `[~]`, never `[x]`.

## 4. Build protocol

To build a milestone (the prompt can be as short as *"Build M1 per docs/ROADMAP.md"*):

1. **Read** the milestone in [ROADMAP.md](ROADMAP.md). Then read **in full** every document in
   its *Read first* list, plus DECISIONS.md.
2. **Collect** the milestone's items: every checklist line tagged with that milestone, in every
   spec (search for `· M1 ·`). The ROADMAP lists them too; if the two disagree, the spec tag wins
   and the ROADMAP is corrected.
3. **Report conflicts before building.** If a spec contradicts a decision or another spec, stop
   and resolve it (ask the owner if it is a product question).
4. **Build vertically:** backend, IPC and UI together, so each feature works end to end.
5. **Verify** each item, then **mark it** in its spec with the note from §3, and run
   `pnpm docs:sync`. That copies every item and its mark into ROADMAP.md and refreshes the progress
   table. It stops without writing if a box was ticked only in the ROADMAP copy, if an ID is
   duplicated, if a mark lacks its note, or if a milestone's **Build order** doesn't name exactly
   that milestone's items. Tick the ROADMAP exit criteria by hand as they pass.
6. **Keep docs true:** when the implementation has to differ from the spec, change the spec in
   the same commit and log the decision.
7. The milestone is complete only when every item tagged with it is `[x]` or `[-]` and every exit
   criterion is checked. Then update the Status section in the root `CLAUDE.md`.

Items tagged with a later milestone are not built early unless the owner asks, but code is
shaped so they fit (for example, `profile_id` scoping from M0).
