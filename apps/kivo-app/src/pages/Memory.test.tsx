import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { MemoryNoteDetail, MemoryNoteView, MemoryOverview } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];

function note(over: Partial<MemoryNoteView>): MemoryNoteView {
  return {
    path: "notes/x.md",
    kind: "fact",
    title: "X",
    excerpt: "",
    tags: [],
    folder: "notes",
    workspace: null,
    sensitivity: "normal",
    shareCloud: false,
    updatedAt: Date.now() - 60_000,
    validUntil: null,
    useCount: 0,
    lastUsedAt: null,
    ...over,
  };
}

const notes = [
  note({ path: "about-me.md", kind: "about", title: "about-me", excerpt: "Call me Sam.", folder: "" }),
  note({ path: "people/maya.md", kind: "person", title: "Maya", excerpt: "Designer", folder: "people" }),
  note({
    path: "notes/design-reviews.md",
    title: "Design reviews",
    excerpt: "Design reviews with [[Maya]] are on Thursdays",
    tags: ["design"],
    useCount: 2,
    lastUsedAt: Date.now() - 3_600_000,
  }),
  note({ path: "notes/project.md", title: "Project folder", excerpt: "My project is in D:\\work", tags: ["projects"] }),
];

const overview: MemoryOverview = {
  root: "C:\\Users\\Sam\\AppData\\Roaming\\KIVO\\memory",
  notes,
  tags: [
    { tag: "design", count: 1 },
    { tag: "projects", count: 1 },
  ],
  folders: [
    { path: "notes", count: 2 },
    { path: "people", count: 1 },
  ],
  suggestions: [
    { id: 7, text: "Sam ships releases on Fridays", reason: "From a conversation", workspace: null, createdAt: 1 },
  ],
};

function detail(path: string): MemoryNoteDetail {
  const n = notes.find((x) => x.path === path) ?? note({});
  const markdown =
    path === "about-me.md"
      ? "Call me Sam. I work with [[Maya]] on `KIVO`.\n"
      : path === "people/maya.md"
        ? "---\ntype: person\n---\n# Maya\n- Designer\n"
        : `---\ntype: fact\ntags: [design]\n---\n# ${n.title}\n\n${n.excerpt}\n`;
  return {
    note: n,
    markdown,
    links: path === "notes/design-reviews.md" ? ["Maya"] : [],
    backlinks: path === "people/maya.md" ? ["notes/design-reviews.md"] : [],
    history: [],
    supersededBy: null,
  };
}

function pathOf(params: unknown): string {
  return typeof params === "object" && params !== null && "path" in params && typeof params.path === "string"
    ? params.path
    : "";
}

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.0.0", snapshot: { mode: "auto" }, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "memory.overview":
      return Promise.resolve(overview);
    case "memory.note":
      return Promise.resolve(detail(pathOf(params)));
    case "settings.get":
      return Promise.resolve({ memory: { capture: "suggest", "workspace-notes": true, detail: "standard" } });
    case "settings.set":
      return Promise.resolve({ memory: { capture: "only-when-asked" } });
    case "memory.forget":
      return Promise.resolve({ forgotten: 4 });
    default:
      return Promise.resolve(null);
  }
};

const { Memory } = await import("./Memory");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function page() {
  render(
    <ToastProvider>
      <Memory />
    </ToastProvider>,
  );
}

beforeEach(() => {
  calls.length = 0;
});

describe("Memory (UX-29, MEM-06)", () => {
  it("shows About me with its links, and a link opens the note", async () => {
    page();
    await settle();
    await settle();
    expect(calls).toContainEqual({ method: "memory.note", params: { path: "about-me.md" } });
    expect(screen.getByText("KIVO")).toBeTruthy();
    // The link in the note, not the tree's entry.
    fireEvent.click(within(screen.getByRole("article")).getByRole("button", { name: "Maya" }));
    await settle();
    expect(calls).toContainEqual({ method: "memory.note", params: { path: "people/maya.md" } });
    await settle();
    // Maya's note: one note links to her.
    fireEvent.click(screen.getByRole("button", { name: "1 linked note" }));
    expect(screen.getByText("Links here")).toBeTruthy();
  });

  it("filters by tag and search, and opens a note from the list", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: /#design/ }));
    expect(screen.getByText("Design reviews")).toBeTruthy();
    expect(screen.queryByText("Project folder")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /#design/ }));
    fireEvent.change(screen.getByRole("searchbox", { name: "Search notes" }), { target: { value: "D:\\work" } });
    expect(screen.getByText("Project folder")).toBeTruthy();
    fireEvent.click(screen.getByText("Project folder"));
    await settle();
    expect(calls).toContainEqual({ method: "memory.note", params: { path: "notes/project.md" } });
  });

  it("keeps a suggestion, switches Suggest off, and forgets everything after asking", async () => {
    page();
    await settle();
    expect(screen.getByText("Sam ships releases on Fridays")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Remember" }));
    await settle();
    expect(calls).toContainEqual({ method: "memory.suggestion", params: { id: 7, accept: true } });

    fireEvent.click(screen.getByRole("switch", { name: "Suggest things to remember" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { memory: { capture: "only-when-asked" } } });

    // The header's button opens the whole vault (the note's opens that note).
    fireEvent.click(screen.getAllByRole("button", { name: "Open in Obsidian" })[0]);
    expect(calls).toContainEqual({ method: "memory.open", params: { in: "obsidian" } });

    fireEvent.click(screen.getByRole("button", { name: "Forget" }));
    // (The toast from keeping the suggestion is a dialog too.)
    const dialog = await screen.findByRole("dialog", { name: "Forget everything?" });
    expect(within(dialog).getByText(/4 notes and every suggestion/)).toBeTruthy();
    expect(calls.some((c) => c.method === "memory.forget")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Forget" }));
    await settle();
    expect(calls.some((c) => c.method === "memory.forget")).toBe(true);
  });

  it("remembers something typed, with tags", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Remember something" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox", { name: "What to remember" }), {
      target: { value: "Standups are at 9:30" },
    });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Tags (optional)" }), {
      target: { value: "#work, team" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "Remember something" }));
    await settle();
    expect(calls).toContainEqual({
      method: "memory.remember",
      params: { text: "Standups are at 9:30", tags: ["work", "team"] },
    });
  });
});
