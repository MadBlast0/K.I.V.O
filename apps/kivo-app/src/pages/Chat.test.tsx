import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider, TooltipProvider } from "../components/ui";
import type { Conversation, StoredMessage } from "../ipc/brains";
import type { StateSnapshot } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];

const thread: Conversation = {
  id: "c-1",
  kind: "chat",
  title: "Why is my build failing?",
  summary: "",
  summarized: 0,
  brain: "Claude Code",
  pinned: false,
  createdAt: 1_700_000_000_000,
  updatedAt: 1_700_000_000_000,
};

const messages: StoredMessage[] = [
  { id: 1, conversationId: "c-1", ts: 1, role: "user", text: "Why is my build failing?", brain: null, turnId: "t1" },
  {
    id: 2,
    conversationId: "c-1",
    ts: 2,
    role: "assistant",
    text: "Config::load returns a Result; add ?.",
    brain: "Claude Code",
    turnId: "t1",
  },
];

const snapshot: StateSnapshot = {
  session: "idle",
  mode: "ask",
  islandHidden: false,
  turn: null,
  speech: { state: "ready" },
  revision: 1,
};

const runtime = vi.hoisted(() => ({
  link: {
    status: "connected",
    runtimeVersion: "0.0.0",
    snapshot: null as StateSnapshot | null,
    message: null,
  },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));
runtime.link.snapshot = snapshot;

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "chat.threads":
      return Promise.resolve([thread]);
    case "chat.thread":
      return Promise.resolve({ thread, messages });
    case "brains.list":
      return Promise.resolve({
        connected: [],
        profiles: [{ id: "coding", name: "Coding" }],
        defaultProfile: "default",
      });
    case "chat.search":
      return Promise.resolve([messages[1]]);
    case "brains.context":
      return Promise.resolve({
        brain: "Claude Code",
        budget: { voice: 12000, chat: 32000 },
        layers: [
          { id: "system", tokens: 600, on: true },
          { id: "skills", tokens: 900, on: false },
          { id: "turns", tokens: 7400, on: true },
        ],
      });
    case "chat.branch":
      return Promise.resolve({ ...thread, id: "c-2", title: "Why is my build failing? (branch)" });
    case "chat.export":
      return Promise.resolve({ text: "# Why", file: "C:/Users/me/Downloads/KIVO - Why.md" });
    default:
      return Promise.resolve(null);
  }
};

const { Chat } = await import("./Chat");

async function violations() {
  const result = await axe.run(document.body, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

async function mount() {
  render(
    <TooltipProvider>
      <ToastProvider>
        <Chat onOpenPermissions={() => {}} />
      </ToastProvider>
    </TooltipProvider>,
  );
  await settle();
  await settle();
}

describe("Chat (UX-21)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("shows the threads and the conversation, and sends in the open thread", async () => {
    await mount();
    expect(screen.getAllByText("Why is my build failing?").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Config::load returns a Result; add ?.")).toBeTruthy();
    expect(await violations()).toEqual([]);
    fireEvent.change(screen.getByLabelText("Message KIVO"), { target: { value: "Fix it and re-run" } });
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await settle();
    expect(calls).toContainEqual({
      method: "chat.send",
      params: { thread: "c-1", text: "Fix it and re-run", profile: null, attachments: [] },
    });
  });

  it("reports a misroute and searches past conversations", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "That’s not what I meant" }));
    await settle();
    expect(calls).toContainEqual({ method: "chat.misroute", params: { turnId: "t1", note: "" } });
    fireEvent.change(screen.getByLabelText("Search conversations"), { target: { value: "Result" } });
    await settle();
    expect(calls).toContainEqual({ method: "chat.search", params: { query: "Result" } });
  });

  it("has the power features: context meter, continue, branch and export (CONV-03)", async () => {
    await mount();
    // Between turns the meter shows what the next message starts with: 8,000 of 32,000.
    expect(screen.getByText("25% of context")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Branch from here" }));
    await settle();
    expect(calls).toContainEqual({ method: "chat.branch", params: { id: "c-1", message: 2 } });

    fireEvent.click(screen.getByRole("button", { name: "More" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Continue by voice" }));
    await settle();
    expect(calls).toContainEqual({ method: "chat.continue", params: { id: "c-1" } });

    fireEvent.click(screen.getByRole("button", { name: "More" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Export as Markdown" }));
    await settle();
    expect(calls).toContainEqual({ method: "chat.export", params: { id: "c-1", json: false, save: true } });
    expect(await screen.findByText(/Saved to .*KIVO - Why\.md/)).toBeTruthy();
  });
});
