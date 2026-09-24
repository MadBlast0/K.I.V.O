/**
 * The memory graph (CONV-25): notes as dots, `[[links]]` between them as lines, and their tags as
 * small rings joined to the notes that carry them — the vault's shape at a glance, as Obsidian
 * shows it. Hovering or focusing a node brings out its neighbours; a note opens on click or Enter,
 * a tag filters the notes. The layout is computed once, so nothing moves while it's open.
 */
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { MemoryLinkView, MemoryNoteView } from "../../ipc/generated";
import { forceLayout } from "../../lib/forceLayout";

/** More than this and the busiest notes are shown (most used, then newest). */
const MAX_NOTES = 120;
const WIDTH = 800;
const HEIGHT = 480;

interface Node {
  id: string;
  label: string;
  kind: "note" | "tag";
  noteKind?: string;
  degree: number;
}

/** A dot's size: tags small, notes by how connected they are. */
function radius(n: Node): number {
  return n.kind === "tag" ? 4 : Math.min(4 + Math.sqrt(n.degree) * 2.2, 11);
}

export function MemoryGraph({
  notes,
  links,
  onOpen,
  onTag,
}: {
  notes: MemoryNoteView[];
  links: MemoryLinkView[];
  onOpen: (path: string) => void;
  onTag: (tag: string) => void;
}) {
  const { t } = useTranslation();
  const [active, setActive] = useState<number | null>(null);

  const graph = useMemo(() => {
    const shown = notes.toSorted((a, b) => b.useCount - a.useCount || b.updatedAt - a.updatedAt).slice(0, MAX_NOTES);
    const nodes: Node[] = shown.map((n) => ({
      id: n.path,
      label: n.kind === "about" ? t("memory.folders.about") : n.title,
      kind: "note",
      noteKind: n.kind,
      degree: 0,
    }));
    const index = new Map(nodes.map((n, i) => [n.id, i]));
    const edges: [number, number][] = [];
    for (const l of links) {
      const a = index.get(l.from);
      const b = index.get(l.to);
      if (a !== undefined && b !== undefined) edges.push([a, b]);
    }
    for (const n of shown) {
      for (const tag of n.tags) {
        const id = `#${tag}`;
        let at = index.get(id);
        if (at === undefined) {
          at = nodes.length;
          nodes.push({ id, label: id, kind: "tag", degree: 0 });
          index.set(id, at);
        }
        const from = index.get(n.path);
        if (from !== undefined) edges.push([from, at]);
      }
    }
    for (const [a, b] of edges) {
      nodes[a].degree++;
      nodes[b].degree++;
    }
    const points = forceLayout(nodes.length, edges, { width: WIDTH, height: HEIGHT });
    return { nodes, edges, points, hidden: notes.length - shown.length };
  }, [notes, links, t]);

  const near = useMemo(() => {
    if (active === null) return null;
    const set = new Set([active]);
    for (const [a, b] of graph.edges) {
      if (a === active) set.add(b);
      if (b === active) set.add(a);
    }
    return set;
  }, [active, graph.edges]);

  const choose = (i: number) => {
    const n = graph.nodes[i];
    if (n.kind === "tag") onTag(n.id.slice(1));
    else onOpen(n.id);
  };

  return (
    <figure className="k-graph">
      <svg
        viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
        role="group"
        aria-label={t("memory.graphLabel", { notes: graph.nodes.filter((n) => n.kind === "note").length })}
      >
        <g className="k-graph__edges" aria-hidden>
          {graph.edges.map(([a, b]) => {
            const on = near?.has(a) && near.has(b) && (a === active || b === active);
            const tagEdge = graph.nodes[b].kind === "tag";
            return (
              <line
                key={`${a}-${b}`}
                x1={graph.points[a].x}
                y1={graph.points[a].y}
                x2={graph.points[b].x}
                y2={graph.points[b].y}
                className={`${tagEdge ? "is-tag" : ""} ${on ? "is-on" : ""} ${near && !on ? "is-dim" : ""}`}
              />
            );
          })}
        </g>
        {graph.nodes.map((n, i) => {
          const p = graph.points[i];
          const dim = near !== null && !near.has(i);
          const r = radius(n);
          return (
            <g
              key={n.id}
              className={`k-graph__node k-graph__node--${n.kind} ${n.noteKind ? `k-graph__node--${n.noteKind}` : ""} ${dim ? "is-dim" : ""} ${active === i ? "is-on" : ""}`}
              transform={`translate(${p.x} ${p.y})`}
              role="button"
              tabIndex={0}
              aria-label={
                n.kind === "tag"
                  ? t("memory.graphTag", { tag: n.label, count: n.degree })
                  : t("memory.graphNote", { title: n.label, count: n.degree })
              }
              onMouseEnter={() => setActive(i)}
              onMouseLeave={() => setActive(null)}
              onFocus={() => setActive(i)}
              onBlur={() => setActive(null)}
              onClick={() => choose(i)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  choose(i);
                }
              }}
            >
              <circle r={r + 6} className="k-graph__hit" />
              <circle r={r} className="k-graph__dot" />
              {(n.kind === "note" || active === i) && (
                <text y={r + 13} textAnchor="middle">
                  {n.label.length > 24 ? `${n.label.slice(0, 23)}…` : n.label}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      {graph.hidden > 0 && <figcaption>{t("memory.graphHidden", { count: graph.hidden })}</figcaption>}
    </figure>
  );
}
