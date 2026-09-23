/**
 * What a screen reader should hear as the Island changes (UX §10, UX-53): each new state once, the
 * final transcript, KIVO's answer and any error. The overlay raises them as UI Automation
 * notifications (`announce` in the app), since the Island never takes focus.
 */
import type { TFunction } from "i18next";
import type { SessionState, StateSnapshot } from "../../ipc/generated";

export interface Heard {
  session: SessionState | null;
  /** The final transcript ("" until it is final). */
  transcript: string;
  answer: string;
  error: string;
}

export const NOTHING: Heard = { session: null, transcript: "", answer: "", error: "" };

/** Announces only states worth saying ("interrupted" and "idle" aren't). */
const STATES: ReadonlySet<SessionState> = new Set<SessionState>([
  "listening",
  "followUp",
  "thinking",
  "acting",
  "speaking",
  "awaitingConfirmation",
  "error",
  "paused",
]);

export function heardFrom(snapshot: StateSnapshot | null): Heard {
  if (!snapshot) return NOTHING;
  const turn = snapshot.turn;
  return {
    session: snapshot.session,
    transcript: turn?.transcriptFinal ? turn.transcript : "",
    answer: turn?.answer ?? "",
    error: turn?.error ?? "",
  };
}

/** The announcements for going from `before` to `now`, in the order they happened. */
export function announcements(before: Heard, now: Heard, t: TFunction): string[] {
  const out: string[] = [];
  if (now.session !== before.session && now.session !== null && STATES.has(now.session)) {
    // A spoken error or answer says more than the state's name.
    if (!(now.session === "error" && now.error)) out.push(t(`island.announce.${now.session}`));
  }
  if (now.transcript && now.transcript !== before.transcript) {
    out.push(t("island.announce.heard", { text: now.transcript }));
  }
  if (now.error && now.error !== before.error) out.push(now.error);
  if (now.answer && now.answer !== before.answer) out.push(t("island.announce.answer", { text: now.answer }));
  return out;
}
