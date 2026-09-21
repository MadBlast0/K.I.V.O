import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ISLAND_LIVE, ISLAND_NOTICES, ISLAND_STATES, islandPreset, type IslandState } from "./presets";

const ALL = [...ISLAND_STATES, ...ISLAND_NOTICES, ...ISLAND_LIVE];

/** The button words and the spoken hint words of a preset's body, or null if it has no hint. */
function hintOf(id: IslandState): { buttons: string[]; spoken: string[] } | null {
  const { container, unmount } = render(<>{islandPreset(id)?.body}</>);
  const hint = container.querySelector(".k-island__hint:has(b)");
  const result = hint && {
    buttons: [...container.querySelectorAll(".k-island__buttons .k-island__btn")].map((b) =>
      (b.textContent ?? "").toLowerCase(),
    ),
    spoken: [...hint.querySelectorAll("b")].map((b) => (b.textContent ?? "").replace(/[“”]/g, "")),
  };
  unmount();
  return result;
}

function trailText(mode: "auto" | "plan" | "bypass") {
  const { container, unmount } = render(<>{islandPreset("acting", { mode })?.trail}</>);
  const text = container.textContent;
  unmount();
  return text;
}

describe("Island presets", () => {
  it("returns a model keyed by its own state for every visible state", () => {
    const visible = ALL.filter((s) => s.id !== "idle");
    expect(visible.map((s) => islandPreset(s.id)?.state)).toEqual(visible.map((s) => s.id));
  });

  it("hides the Island when idle", () => {
    expect(islandPreset("idle")).toBeNull();
  });

  it("uses unique state ids", () => {
    expect(new Set(ALL.map((s) => s.id)).size).toBe(ALL.length);
  });

  // DESIGN_SYSTEM §4: the voice hint lists the words on the buttons, in the same order.
  it("lists exactly the button words, in order, in every voice hint", () => {
    const hints = ALL.map((s) => hintOf(s.id)).filter((h) => h !== null);
    expect(hints.length).toBeGreaterThanOrEqual(6);
    expect(hints.map((h) => h.spoken)).toEqual(hints.map((h) => h.buttons));
  });

  it("shows the live partial transcript while listening", () => {
    const { container } = render(<>{islandPreset("listening", { partial: "open chrome" })?.body}</>);
    expect(container.textContent).toBe("open chrome");
  });

  it("shows the permission-mode chip only when the mode is not Auto", () => {
    expect(trailText("auto")).toBe("");
    expect(trailText("plan")).toBe("PLAN");
    expect(trailText("bypass")).toBe("BYPASS");
  });
});
