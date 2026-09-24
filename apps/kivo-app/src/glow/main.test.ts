import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

describe("the wake glow (UX-16)", () => {
  it("plays once per wake, restarting cleanly", async () => {
    const { play } = await import("./main");
    const el = document.createElement("div");
    play(el);
    expect(el.classList.contains("k-glow--on")).toBe(true);
    play(el);
    expect(el.classList.contains("k-glow--on")).toBe(true);
    play(null);
  });
});
