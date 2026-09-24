import { describe, expect, it } from "vitest";
import { forceLayout } from "./forceLayout";

const dist = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);

describe("forceLayout (CONV-25)", () => {
  it("is deterministic, stays in the box, and pulls linked nodes together", () => {
    const edges = [
      [0, 1],
      [1, 2],
      [3, 4],
    ] as const;
    const box = { width: 800, height: 500 };
    const a = forceLayout(8, edges, box);
    expect(forceLayout(8, edges, box)).toEqual(a);
    for (const p of a) {
      expect(p.x).toBeGreaterThanOrEqual(24);
      expect(p.x).toBeLessThanOrEqual(776);
      expect(p.y).toBeGreaterThanOrEqual(24);
      expect(p.y).toBeLessThanOrEqual(476);
    }
    // Linked nodes end closer than the average pair.
    let sum = 0;
    let pairs = 0;
    for (let i = 0; i < a.length; i++)
      for (let j = i + 1; j < a.length; j++) {
        sum += dist(a[i], a[j]);
        pairs++;
      }
    const mean = sum / pairs;
    expect(dist(a[0], a[1])).toBeLessThan(mean);
    expect(dist(a[3], a[4])).toBeLessThan(mean);
    // No two nodes on top of each other.
    for (let i = 0; i < a.length; i++)
      for (let j = i + 1; j < a.length; j++) expect(dist(a[i], a[j])).toBeGreaterThan(10);
  });

  it("handles no nodes and one node", () => {
    expect(forceLayout(0, [], { width: 100, height: 100 })).toEqual([]);
    expect(forceLayout(1, [], { width: 100, height: 100 })).toHaveLength(1);
  });
});
