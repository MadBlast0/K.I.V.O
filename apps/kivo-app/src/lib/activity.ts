/**
 * The Activity timeline as people read it (UX-20, plan §83): one entry per request — what was
 * heard, what KIVO did and what it said — plus the entries that aren't requests (a capability
 * switched, the emergency stop). The runtime stores the pieces; this puts them back together.
 */
import type { ActivityItem } from "../ipc/generated";

export type EntryKind = "voice" | "tool" | "setting";
export type Outcome = "done" | "failed" | "cancelled" | "denied" | "unhandled";

export interface Entry {
  key: string;
  ts: number;
  kind: EntryKind;
  /** What the user asked for, or what happened. */
  title: string;
  /** What KIVO did and said. */
  detail: string | null;
  outcome: Outcome;
  /** The request's turn, when it was one (for "Why did you say that?"). */
  turn: string | null;
  /** KIVO answered (so its answer may have used memories). */
  answered: boolean;
  /** A brain answered it (the "AI" filter). */
  ai: boolean;
}

const OUTCOMES: ReadonlyArray<Outcome> = ["done", "failed", "cancelled", "denied", "unhandled"];

function outcomeOf(status: string): Outcome {
  return OUTCOMES.find((o) => o === status) ?? "done";
}

/** Groups Activity rows (newest first) into entries (newest first). */
export function entries(items: ActivityItem[]): Entry[] {
  const byTurn = new Map<string, ActivityItem[]>();
  const out: Entry[] = [];
  for (const item of items) {
    if (!item.turnId) {
      out.push({
        key: `a${item.id}`,
        ts: item.ts,
        kind: "setting",
        title: item.title,
        detail: item.detail,
        outcome: outcomeOf(item.status),
        turn: null,
        answered: false,
        ai: false,
      });
      continue;
    }
    const rows = byTurn.get(item.turnId);
    if (rows) rows.push(item);
    else {
      byTurn.set(item.turnId, [item]);
      // A placeholder keeps the turn in time order; it is filled in below.
      out.push({
        key: item.turnId,
        ts: item.ts,
        kind: "voice",
        title: "",
        detail: null,
        outcome: "done",
        turn: item.turnId,
        answered: false,
        ai: false,
      });
    }
  }
  for (const entry of out) {
    const rows = byTurn.get(entry.key);
    if (!rows) continue;
    const heard = rows.find((r) => r.kind === "transcript");
    const tools = rows.filter((r) => r.kind === "tool");
    const reply = rows.find((r) => r.kind === "reply");
    const failed = tools.find((r) => r.status === "failed" || r.status === "denied");
    entry.ts = Math.min(...rows.map((r) => r.ts));
    entry.kind = tools.length > 0 ? "tool" : "voice";
    entry.title = heard?.title ?? reply?.title ?? tools[0]?.title ?? "";
    entry.detail =
      [tools.map((t) => t.detail ?? t.title).join(" · "), reply?.title].filter(Boolean).join(" — ") || null;
    entry.outcome = outcomeOf(failed?.status ?? reply?.status ?? tools[0]?.status ?? "done");
    entry.answered = reply !== undefined;
    entry.ai = rows.some((r) => r.kind === "brain");
  }
  return out;
}
