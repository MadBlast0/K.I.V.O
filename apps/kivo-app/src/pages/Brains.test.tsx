import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider, TooltipProvider } from "../components/ui";
import type { BrainsList, Catalog, ContextPreview, DiscoverySection } from "../ipc/brains";

const calls: Array<{ method: string; params: unknown }> = [];

const list: BrainsList = {
  connected: [
    {
      id: "ollama",
      name: "Ollama",
      kind: "local",
      privacy: "local",
      free: "Free and private: runs on this PC",
      enabled: true,
      health: { state: "ready" },
      hasKey: false,
      models: ["llama3.2"],
      checkedAgo: 3,
    },
    {
      id: "anthropic",
      name: "Anthropic",
      kind: "api",
      privacy: "cloud",
      free: null,
      enabled: true,
      health: { state: "needsSignIn" },
      hasKey: false,
      models: [],
      checkedAgo: null,
    },
  ],
  profiles: [
    {
      id: "default",
      name: "Default",
      tier: "default",
      primary: null,
      fallbacks: [],
      privacy: "cloud",
      maxContext: null,
      allowedTools: { kind: "all" },
      systemPromptAddendum: "",
      persona: null,
      builtIn: true,
    },
    {
      id: "private",
      name: "Private",
      tier: "default",
      primary: null,
      fallbacks: [],
      privacy: "strictPrivate",
      maxContext: null,
      allowedTools: { kind: "all" },
      systemPromptAddendum: "",
      persona: null,
      builtIn: true,
    },
  ],
  defaultProfile: "default",
  active: null,
  persona: "calm",
  customPersona: "",
  cliAgentsOn: false,
  cloudOn: true,
  workspace: "C:\\Users\\me",
};

const catalog: Catalog = {
  brains: [
    {
      id: "gemini-cli",
      name: "Gemini CLI",
      kind: "cli",
      privacy: "cloud",
      signIn: "cliLogin",
      free: "Free with a Google account",
      baseUrl: "",
      names: [],
    },
    {
      id: "codex",
      name: "Codex",
      kind: "cli",
      privacy: "cloud",
      signIn: "cliLogin",
      free: "Included with ChatGPT",
      baseUrl: "",
      names: [],
    },
    {
      id: "openrouter",
      name: "OpenRouter",
      kind: "api",
      privacy: "cloud",
      signIn: "oAuth",
      free: "Free models",
      baseUrl: "",
      names: [],
    },
    {
      id: "anthropic",
      name: "Anthropic",
      kind: "api",
      privacy: "cloud",
      signIn: "apiKey",
      free: null,
      baseUrl: "",
      names: [],
    },
  ],
  cli: [
    {
      id: "gemini-cli",
      commands: ["gemini"],
      adapters: [],
      install: "npm install -g @google/gemini-cli",
      adapterInstall: "",
    },
    {
      id: "codex",
      commands: ["codex"],
      adapters: ["codex-acp"],
      install: "npm install -g @openai/codex",
      adapterInstall: "npm install -g @zed-industries/codex-acp",
    },
  ],
};

const cli: DiscoverySection = {
  section: "cli",
  checkedAt: Date.now() - 120_000,
  items: [
    {
      id: "gemini-cli",
      new: true,
      data: {
        name: "Gemini CLI",
        signedIn: true,
        version: "0.9.1",
        free: "Free with a Google account",
        program: "gemini",
      },
    },
  ],
};

const preview: ContextPreview = {
  brain: "Ollama",
  budget: { voice: 4096, chat: 4096 },
  layers: [
    { id: "system", text: "You are KIVO…", tokens: 320, max: 800, on: true },
    { id: "instructions", text: "- name: Call me Sam", tokens: 12, max: 400, on: true },
    { id: "workspace", text: "", tokens: 0, max: 800, on: true },
    { id: "live", text: "Current context:", tokens: 40, max: 300, on: true },
    { id: "skills", text: "", tokens: 0, max: 1000, on: false },
    { id: "memories", tokens: 0, max: 600, on: true },
    { id: "summary", text: "", tokens: 0, max: 1500, on: true },
    { id: "turns", tokens: 0, on: true },
    { id: "tools", tokens: 900, count: 18, max: 20, on: true },
  ],
  sessionCost: 0,
  preview: "[system, cached]\nYou are KIVO…\n\n[user]\nhello [redacted]",
  previewTokens: 1272,
};

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.0.0", snapshot: null, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

const sectionOf = (params: unknown) =>
  typeof params === "object" && params !== null && "section" in params ? String(params.section) : "";

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "brains.list":
      return Promise.resolve(list);
    case "brains.catalog":
      return Promise.resolve(catalog);
    case "brains.discovery":
    case "brains.refresh":
      return Promise.resolve(sectionOf(params) === "cli" ? cli : { section: "local", items: [], checkedAt: null });
    case "brains.context":
      return Promise.resolve(preview);
    case "memory.preferences":
      return Promise.resolve([{ key: "name", value: "Call me Sam" }]);
    case "settings.get":
    case "settings.set":
      return Promise.resolve({ context: { "live-fields": ["local time", "language"], "compact-at": 100 } });
    case "brains.setKey":
      return Promise.resolve({ health: { state: "ready" } });
    case "brains.setActive":
      return Promise.resolve({
        ...list,
        active: { provider: "ollama", model: "llama3.2", ...(hasReasoning(params) ? { reasoning: "low" } : {}) },
      });
    case "brains.reasoning":
      return Promise.resolve(["off", "low", "medium", "high"]);
    default:
      return Promise.resolve(null);
  }
};

const hasReasoning = (params: unknown) =>
  typeof params === "object" && params !== null && "reasoning" in params && params.reasoning !== null;

const { Brains } = await import("./Brains");

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
        <Brains />
      </ToastProvider>
    </TooltipProvider>,
  );
  await settle();
}

describe("Brains (UX-22)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("lists connected brains with health, what's on this PC, and free options", async () => {
    await mount();
    expect(await screen.findByText("Ollama")).toBeTruthy();
    expect(screen.getByText("Needs sign-in")).toBeTruthy();
    // Found on this PC, labelled New and Free, with Use; Codex isn't installed: its command.
    expect(screen.getByText("Gemini CLI")).toBeTruthy();
    expect(screen.getAllByText("Free").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("npm install -g @openai/codex")).toBeTruthy();
    expect(screen.getByText(/Checked/)).toBeTruthy();
    expect(await violations()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Use" }));
    await settle();
    expect(calls).toContainEqual({ method: "brains.connect", params: { id: "gemini-cli" } });

    fireEvent.click(screen.getAllByRole("button", { name: /Refresh/ })[0]);
    await settle();
    expect(calls).toContainEqual({ method: "brains.refresh", params: { section: "cli" } });
  });

  it("stores an API key write-only: tested by the runtime, never shown back", async () => {
    await mount();
    fireEvent.click(await screen.findByRole("button", { name: /Anthropic/ }));
    await settle();
    const field = screen.getByLabelText("API key");
    expect(field.getAttribute("type")).toBe("password");
    fireEvent.change(field, { target: { value: "sk-ant-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Test and save" }));
    await settle();
    expect(calls).toContainEqual({ method: "brains.setKey", params: { id: "anthropic", key: "sk-ant-secret" } });
    expect(screen.getByLabelText<HTMLInputElement>("API key").value).toBe("");
  });

  it("chooses the brain KIVO uses, its model and how hard it thinks (owner, 2026-09-26)", async () => {
    await mount();
    expect(await screen.findByText("KIVO thinks with")).toBeTruthy();
    fireEvent.click(screen.getByRole("combobox", { name: "Brain" }));
    await settle();
    fireEvent.keyDown(screen.getByRole("option", { name: "Ollama" }), { key: "Enter" });
    await settle();
    expect(calls).toContainEqual({
      method: "brains.setActive",
      params: { provider: "ollama", model: "llama3.2", reasoning: null },
    });
    // The model takes reasoning levels: they show, and one is chosen.
    fireEvent.click(await screen.findByRole("button", { name: "Low" }));
    await settle();
    expect(calls).toContainEqual({
      method: "brains.setActive",
      params: { provider: "ollama", model: "llama3.2", reasoning: "low" },
    });
  });

  it("switches the default profile", async () => {
    await mount();
    fireEvent.click(await screen.findByRole("radio", { name: "Private" }));
    await settle();
    expect(calls).toContainEqual({ method: "brains.setDefault", params: { profile: "private" } });
  });

  it("shows what a conversation starts with (Context)", async () => {
    await mount();
    fireEvent.click(screen.getByRole("tab", { name: "Context" }));
    await settle();
    expect(await screen.findByText(/tokens to start a conversation/)).toBeTruthy();
    expect(screen.getByText("Call me Sam")).toBeTruthy();
    expect(screen.getByText("Free: this brain costs nothing")).toBeTruthy();
    expect(await violations()).toEqual([]);

    // Settings → Context (CONV-31): a layer turned off, live fields, compaction, fresh start.
    expect(screen.getByRole("switch", { name: "Skills index" }).getAttribute("aria-checked")).toBe("false");
    fireEvent.click(screen.getByRole("switch", { name: "About me" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { context: { "about-me": false } } });
    fireEvent.click(screen.getByRole("checkbox", { name: "Active app" }));
    await settle();
    expect(calls).toContainEqual({
      method: "settings.set",
      params: { context: { "live-fields": ["local time", "language", "active app"] } },
    });
    fireEvent.click(screen.getByRole("switch", { name: "Start each conversation fresh" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { context: { "fresh-start": true } } });

    // The whole request, secrets redacted by the runtime.
    fireEvent.click(screen.getByRole("button", { name: "Preview what the AI sees" }));
    await settle();
    expect(screen.getByText(/hello \[redacted\]/)).toBeTruthy();
  });
});
