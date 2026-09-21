/** How the runtime's state reads in the UI: one place for the words used by Home and the sidebar. */
import type { Link, SessionState } from "../ipc/generated";

export interface StateText {
  title: string;
  detail: string;
}

const SESSION: Record<SessionState, StateText> = {
  idle: { title: "KIVO is ready", detail: "It keeps running in the tray when you close this window." },
  listening: { title: "Listening…", detail: "Go ahead, KIVO is listening." },
  thinking: { title: "Thinking…", detail: "Working out what you asked for." },
  acting: { title: "Working on it…", detail: "KIVO is doing what you asked." },
  speaking: { title: "Speaking…", detail: "Say “stop” to interrupt." },
  followUp: { title: "Listening for a follow-up", detail: "Keep talking, no wake word needed." },
  interrupted: { title: "Stopping…", detail: "Cancelling what KIVO was doing." },
  paused: { title: "Listening is paused", detail: "KIVO won’t listen until you resume." },
  awaitingConfirmation: { title: "Waiting for your OK", detail: "KIVO needs your approval to continue." },
  error: { title: "Something went wrong", detail: "KIVO will be ready again in a moment." },
};

/** A short label for the state ("Ready", "Paused"), matching the tray tooltip. */
const SHORT: Record<SessionState, string> = {
  idle: "Ready",
  listening: "Listening",
  thinking: "Thinking",
  acting: "Working",
  speaking: "Speaking",
  followUp: "Listening for a follow-up",
  interrupted: "Stopping",
  paused: "Paused",
  awaitingConfirmation: "Waiting for your OK",
  error: "Something went wrong",
};

export type LinkTone = "ok" | "busy" | "off";

export interface LinkView extends StateText {
  /** For the sidebar: "Connected · Ready". */
  status: string;
  tone: LinkTone;
  session: SessionState | null;
}

/** What to show for the connection and state. `link` is null outside the KIVO app. */
export function viewLink(link: Link | null): LinkView {
  if (!link) {
    return {
      title: "Not connected",
      detail: "Open this in the KIVO app to see KIVO’s live status.",
      status: "Preview · not connected",
      tone: "off",
      session: null,
    };
  }
  switch (link.status) {
    case "connecting":
      return {
        title: "Starting KIVO…",
        detail: link.message ?? "This takes a moment the first time.",
        status: "Starting…",
        tone: "busy",
        session: null,
      };
    case "reconnecting":
      return {
        title: "KIVO isn’t running",
        detail: "It stopped unexpectedly. It reconnects as soon as KIVO is back.",
        status: "Reconnecting…",
        tone: "busy",
        session: null,
      };
    case "incompatible":
      return {
        title: "KIVO needs a restart",
        detail: link.message ?? "Parts of KIVO are from different versions. Quit KIVO and open it again.",
        status: "Restart needed",
        tone: "off",
        session: null,
      };
    case "connected": {
      const session = link.snapshot?.session ?? "idle";
      return { ...SESSION[session], status: `Connected · ${SHORT[session]}`, tone: "ok", session };
    }
  }
}
