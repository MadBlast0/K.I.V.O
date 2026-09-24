import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Island } from "./Island";
import { orbColor } from "./OrbGl";

describe("the Orb companion (UX-38)", () => {
  it("floats above the card, which keeps the words; without WebGL the still orb stands in", () => {
    render(
      <Island
        companion="orb"
        model={{ state: "speaking", width: 460, label: "KIVO", voice: "kivo", body: <p>Chrome is open.</p> }}
        aria-label="KIVO"
      />,
    );
    const island = screen.getByRole("status", { name: "KIVO" });
    expect(island.className).toContain("k-island-orb");
    expect(island.querySelector(".k-island-orb__orb")).toBeTruthy();
    expect(screen.getByText("Chrome is open.")).toBeTruthy();
    // jsdom has no WebGL: the static orb is drawn instead of the canvas.
    expect(island.querySelector("canvas")).toBeNull();
  });

  it("takes the state's colour", () => {
    expect(orbColor("listening")).toBe("#FFFFFF");
    expect(orbColor("thinking")).toBe("#B69CFF");
    expect(orbColor("speaking", "kivo")).toBe("#9AD3FF");
    expect(orbColor("confirm-3")).toBe("#FFC857");
    expect(orbColor("error-9")).toBe("#FF5147");
    expect(orbColor("done-1")).toBe("#8AB8FF");
  });
});
