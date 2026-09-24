/**
 * A small, deterministic force layout for the memory graph (CONV-25): Fruchterman–Reingold with
 * cooling, from a golden-angle spiral, so the same notes always land in the same places and no
 * animation is needed. Linked nodes pull together, every pair pushes apart, and nothing leaves the
 * box.
 */

export interface Point {
  x: number;
  y: number;
}

export function forceLayout(
  count: number,
  edges: ReadonlyArray<readonly [number, number]>,
  {
    width,
    height,
    iterations = 300,
    margin = 24,
  }: { width: number; height: number; iterations?: number; margin?: number },
): Point[] {
  if (count === 0) return [];
  const cx = width / 2;
  const cy = height / 2;
  // The ideal distance between nodes for the area they share.
  const k = Math.sqrt(((width - 2 * margin) * (height - 2 * margin)) / count) * 0.75;
  const golden = Math.PI * (3 - Math.sqrt(5));
  const spread = Math.min(width, height) / 2 - margin;
  const pos: Point[] = Array.from({ length: count }, (_, i) => {
    const r = spread * Math.sqrt((i + 0.5) / count);
    return { x: cx + r * Math.cos(i * golden), y: cy + r * Math.sin(i * golden) };
  });
  if (count === 1) return pos;
  let temperature = spread / 4;
  const cooling = temperature / (iterations + 1);
  for (let step = 0; step < iterations; step++) {
    const move = pos.map(() => ({ x: 0, y: 0 }));
    for (let i = 0; i < count; i++) {
      for (let j = i + 1; j < count; j++) {
        const dx = pos[i].x - pos[j].x;
        const dy = pos[i].y - pos[j].y;
        const d = Math.max(Math.hypot(dx, dy), 0.01);
        const push = (k * k) / d;
        move[i].x += (dx / d) * push;
        move[i].y += (dy / d) * push;
        move[j].x -= (dx / d) * push;
        move[j].y -= (dy / d) * push;
      }
    }
    for (const [a, b] of edges) {
      if (a === b || a >= count || b >= count) continue;
      const dx = pos[a].x - pos[b].x;
      const dy = pos[a].y - pos[b].y;
      const d = Math.max(Math.hypot(dx, dy), 0.01);
      const pull = (d * d) / k;
      move[a].x -= (dx / d) * pull;
      move[a].y -= (dy / d) * pull;
      move[b].x += (dx / d) * pull;
      move[b].y += (dy / d) * pull;
    }
    for (let i = 0; i < count; i++) {
      // A little gravity keeps unlinked nodes from drifting to the edges.
      move[i].x += (cx - pos[i].x) * 0.02 * k;
      move[i].y += (cy - pos[i].y) * 0.02 * k;
      const d = Math.max(Math.hypot(move[i].x, move[i].y), 0.01);
      const stride = Math.min(d, temperature);
      pos[i].x = Math.min(width - margin, Math.max(margin, pos[i].x + (move[i].x / d) * stride));
      pos[i].y = Math.min(height - margin, Math.max(margin, pos[i].y + (move[i].y / d) * stride));
    }
    temperature -= cooling;
  }
  return pos;
}
