/**
 * The Island's short notice after the permission mode changes (Ctrl+Shift+M, the tray, Home).
 * What KIVO is doing is shown by `turn.tsx`.
 */
import type { PermissionMode } from "../../ipc/generated";
import { modeLabel } from "../../lib/session";
import { IslandChip, type IslandModel } from "./Island";

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
