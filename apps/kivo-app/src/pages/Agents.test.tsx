import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { AgentsOverview, WorkspaceItem } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];
let overview: AgentsOverview = { cli: [], desktop: [], sessions: [] };
let workspaces: WorkspaceItem[] = [];
const instructions: Record<string, string> = {};

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
  const scope =
    typeof params === "object" && params !== null && "scope" in params && typeof params.scope === "string"
      ? params.scope
      : "";
  switch (method) {
    case "agents.overview":
      return Promise.resolve(overview);
    case "workspaces.list":
      return Promise.resolve({ workspaces, current: workspaces[0]?.id ?? null });
    case "instructions.get":
      return Promise.resolve({ scope, text: instructions[scope] ?? "" });
    case "agents.openInTerminal":
      return Promise.resolve({ title: "Claude Code · kivo" });
    case "workspaces.exportAgentsMd":
      return Promise.resolve({ file: "D:\\kivo\\AGENTS.md" });
    default:
      return Promise.resolve(null);
  }
};

const { Agents, initials } = await import("./Agents");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

const kivo: WorkspaceItem = {
  id: "w1",
  path: "D:\\kivo",
  name: "kivo",
  instructions: "",
  agentFiles: ["CLAUDE.md"],
  preferredAgent: null,
  lastUsed: Date.now(),
};

beforeEach(() => {
  calls.length = 0;
  overview = {
    cli: [
      { id: "claude-code", name: "Claude Code", installed: true, signedIn: true, version: "2.1.0", terminal: true },
      { id: "codex", name: "Codex", installed: false, signedIn: null, version: null, terminal: true },
    ],
    desktop: [{ id: "claude-desktop", name: "Claude Desktop", appId: "Claude" }],
    sessions: [
      {
        id: "s1",
        agent: "claude-code",
        workspace: "D:\\kivo",
        kind: "acp",
        running: false,
        resumable: true,
        lastUsed: 1,
      },
    ],
  };
  workspaces = [kivo];
  instructions["global"] = "Call me Sam.";
});

function page(tab?: "agents" | "workspaces") {
  render(
    <ToastProvider>
      <Agents initialTab={tab} />
    </ToastProvider>,
  );
}

describe("Agents page (UX-25)", () => {
  it("lists sessions, agents found and desktop AI apps", async () => {
    page();
    await settle();
    expect(screen.getByText("claude-code · D:\\kivo")).toBeTruthy();
    expect(screen.getByText("Claude Code")).toBeTruthy();
    expect(screen.getByText("2.1.0 · Signed in")).toBeTruthy();
    expect(screen.getByText("Not installed: Codex.")).toBeTruthy();
    expect(screen.getByText("Claude Desktop")).toBeTruthy();
  });

  it("hands a background session to a terminal (CONV-13)", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Open in terminal" }));
    await settle();
    expect(calls).toContainEqual({ method: "agents.openInTerminal", params: { session: "s1" } });
  });

  it("starts an agent in a folder and a mode as a request of its own (CONV-14)", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Bypass" }));
    expect(within(dialog).getByText(/KIVO confirms first/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Start" }));
    await settle();
    expect(calls).toContainEqual({
      method: "agents.start",
      params: { agent: "claude-code", folder: "D:\\kivo", mode: "bypass" },
    });
  });
});

describe("Workspaces and instructions (CONV-09/10/11)", () => {
  it("edits About me and a workspace's instructions, exports AGENTS.md and forgets", async () => {
    page("workspaces");
    await settle();
    const about = screen.getByRole("textbox", { name: /What every brain and agent should know/ });
    expect(about).toHaveProperty("value", "Call me Sam.");
    fireEvent.change(about, { target: { value: "Call me Sam. Answer briefly." } });
    fireEvent.blur(about);
    await settle();
    expect(calls).toContainEqual({
      method: "instructions.set",
      params: { scope: "global", text: "Call me Sam. Answer briefly." },
    });
    const notes = screen.getByRole("textbox", { name: /Notes for work in kivo/ });
    fireEvent.change(notes, { target: { value: "Use pnpm." } });
    fireEvent.blur(notes);
    await settle();
    expect(calls).toContainEqual({ method: "instructions.set", params: { scope: "workspace:w1", text: "Use pnpm." } });
    expect(screen.getByText("CLAUDE.md")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Export as AGENTS.md" }));
    await settle();
    expect(calls).toContainEqual({ method: "workspaces.exportAgentsMd", params: { id: "w1" } });
    fireEvent.click(screen.getByRole("button", { name: "Forget" }));
    await settle();
    expect(calls).toContainEqual({ method: "workspaces.forget", params: { id: "w1" } });
  });
});

describe("initials", () => {
  it("takes two letters", () => {
    expect(initials("Claude Code")).toBe("CC");
    expect(initials("gemini-cli")).toBe("GC");
    expect(initials("Codex")).toBe("CO");
  });
});
