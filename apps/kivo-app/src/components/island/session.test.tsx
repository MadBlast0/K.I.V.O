import { describe, expect, it } from "vitest";
import type { SessionState } from "../../ipc/generated";
import { islandForSession } from "./session";

const ALL: SessionState[] = [
  "idle",
  "listening",
  "thinking",
  "acting",
  "speaking",
  "followUp",
  "interrupted",
  "paused",
  "awaitingConfirmation",
  "error",
];

describe("islandForSession", () => {
  it("shows nothing at rest or when paused", () => {
    expect(islandForSession("idle")).toBeNull();
    expect(islandForSession("paused")).toBeNull();
  });

  it("shows every active state, keyed by the state so content cross-fades on change", () => {
    for (const state of ALL.filter((s) => s !== "idle" && s !== "paused")) {
      const model = islandForSession(state);
      expect(model?.state).toBe(state);
      expect(typeof model?.label).toBe("string");
    }
  });

  it("animates the waveform only while someone is talking", () => {
    const waving = ALL.filter((s) => islandForSession(s)?.wave);
    expect(waving).toEqual(["listening", "speaking", "followUp"]);
    expect(islandForSession("speaking")?.voice).toBe("kivo");
  });
});
