import { describe, expect, it } from "vitest";
import { islandForMode } from "./session";

describe("islandForMode", () => {
  it("names the new mode and keys the notice by it, so a second change cross-fades", () => {
    const plan = islandForMode("plan");
    expect(plan.state).toBe("mode-plan");
    expect(plan.label).toBe("Plan first");
    expect(islandForMode("auto").state).not.toBe(plan.state);
  });
});
