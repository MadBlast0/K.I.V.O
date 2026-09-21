/**
 * The Island for the runtime's session state (UX §2). Only what the runtime actually reports is
 * shown: transcripts, steps and answers join as the voice pipeline and tools deliver them.
 */
import type { PermissionMode, SessionState } from "../../ipc/generated";
import { modeLabel } from "../../lib/session";
import { IslandChip, IslandDot, IslandSpin, type IslandModel } from "./Island";

/** States that show the Island. Idle and Paused show nothing (Paused only when summoned). */
export function islandForSession(state: SessionState): IslandModel | null {
  switch (state) {
    case "listening":
      return { state, width: 236, label: "Listening", wave: true };
    case "followUp":
      return { state, width: 300, label: "Listening", sub: "for a follow-up", wave: true };
    case "thinking":
      return { state, width: 196, label: "Thinking", trail: <IslandSpin /> };
    case "acting":
      return { state, width: 236, label: "Working", trail: <IslandSpin /> };
    case "speaking":
      return { state, width: 236, label: "KIVO", wave: true, voice: "kivo" };
    case "interrupted":
      return { state, width: 196, label: "Stopping", trail: <IslandSpin /> };
    case "awaitingConfirmation":
      return { state, width: 300, label: "Needs your OK", lead: <IslandDot color="#FFC857" /> };
    case "error":
      return { state, width: 300, label: "Something went wrong", lead: <IslandDot color="#FF5147" /> };
    case "idle":
    case "paused":
      return null;
  }
}

/** A short notice after the permission mode changes (Ctrl+Shift+M, the tray, Home). */
export function islandForMode(mode: PermissionMode): IslandModel {
  return {
    state: `mode-${mode}`,
    width: 280,
    label: modeLabel(mode),
    sub: "mode",
    trail: <IslandChip>{mode === "auto" ? "AUTO" : mode === "accept-edits" ? "EDITS" : mode.toUpperCase()}</IslandChip>,
  };
}
