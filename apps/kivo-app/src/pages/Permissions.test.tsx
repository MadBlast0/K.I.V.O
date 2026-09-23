import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { CapabilityItem, GrantItem } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];

let settings: Record<string, Record<string, unknown>> = {};
let capabilities: CapabilityItem[] = [];
let grants: GrantItem[] = [];

const runtime = vi.hoisted(() => ({
  link: {
    status: "connected",
    runtimeVersion: "0.0.0",
    snapshot: { mode: "auto" },
    message: null,
  },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

function item(
  capability: CapabilityItem["capability"],
  enabled: boolean,
  lastUsed: number | null = null,
): CapabilityItem {
  return { capability, label: capability, enabled, default: enabled, badges: ["local"], lastUsed };
}

function merge(base: Record<string, Record<string, unknown>>, patch: Record<string, Record<string, unknown>>) {
  const out = { ...base };
  for (const [k, v] of Object.entries(patch)) out[k] = { ...out[k], ...v };
  return out;
}

function isPatch(v: unknown): v is Record<string, Record<string, unknown>> {
  return typeof v === "object" && v !== null;
}

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "settings.get":
      return Promise.resolve(settings);
    case "settings.set":
      if (isPatch(params)) settings = merge(settings, params);
      return Promise.resolve(settings);
    case "capabilities.get":
      return Promise.resolve(capabilities);
    case "capabilities.set":
    case "capabilities.preset":
      return Promise.resolve(capabilities);
    case "permissions.grants":
      return Promise.resolve(grants);
    case "browser.status":
      return Promise.resolve({ connected: false, extensionId: "abc", folder: "C:\\KIVO\\extensions\\browser" });
    default:
      return Promise.resolve(null);
  }
};

const { Permissions } = await import("./Permissions");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function page(tab?: "mode" | "capabilities" | "privacy") {
  render(
    <ToastProvider>
      <Permissions initialTab={tab} />
    </ToastProvider>,
  );
}

describe("Permissions (UX-28)", () => {
  beforeEach(() => {
    calls.length = 0;
    settings = {
      tools: {
        preset: "balanced",
        "cloud-vision": false,
        "shell-read-only": true,
        "private-folders": [],
        "ui-automation-apps": { allow: [], block: ["KeePassXC"] },
      },
      privacy: { mode: "cloud", "retention-days": 30, "debug-transcripts": false },
      permissions: { "blocked-apps": [] },
    };
    capabilities = [
      item("apps-and-windows", true, Date.now() - 3 * 60_000),
      item("shell", false),
      item("ui-automation", true),
      item("screen-awareness", false),
    ];
    grants = [
      {
        id: 7,
        tool: "files.move",
        scope: null,
        createdAt: 0,
        expiresAt: null,
        pattern: "C:\\Work\\*",
        sessionOnly: false,
      },
      { id: 8, tool: "shell.run", scope: null, createdAt: 0, expiresAt: null, pattern: null, sessionOnly: true },
    ];
  });

  it("switches the mode, shows the hard limits and revokes a grant", async () => {
    page("mode");
    await settle();
    fireEvent.click(screen.getByRole("radio", { name: "Plan first" }));
    expect(calls).toContainEqual({ method: "permissions.setMode", params: { mode: "plan" } });
    expect(screen.getByText("Password fields are never typed into")).toBeTruthy();
    expect(screen.getByText("C:\\Work\\* · always")).toBeTruthy();
    expect(screen.getByText("until KIVO restarts")).toBeTruthy();
    fireEvent.click(screen.getAllByRole("button", { name: "Revoke" })[0]);
    await settle();
    expect(calls).toContainEqual({ method: "permissions.revoke", params: { id: 7 } });
  });

  it("lists capabilities with when they were used, toggles them and applies presets", async () => {
    page("capabilities");
    await settle();
    expect(screen.getByText(/used 3 minutes ago/)).toBeTruthy();
    fireEvent.click(screen.getByRole("switch", { name: "shell" }));
    await settle();
    expect(calls).toContainEqual({ method: "capabilities.set", params: { capability: "shell", on: true } });
    fireEvent.click(screen.getByRole("button", { name: "Power user" }));
    await settle();
    expect(calls).toContainEqual({ method: "capabilities.preset", params: { preset: "power-user" } });
  });

  it("edits a capability's own options: per-app lists and cloud vision", async () => {
    page("capabilities");
    await settle();
    const rows = screen.getAllByRole("button", { name: "Options" });
    // UI Automation's options (apps, shell, then UI Automation): the block list is shown and an
    // app is added.
    fireEvent.click(rows[2]);
    await settle();
    const sheet = screen.getByRole("dialog");
    expect(within(sheet).getByText("KeePassXC")).toBeTruthy();
    const fields = within(sheet).getAllByRole("textbox");
    fireEvent.change(fields[0], { target: { value: "MyBank" } });
    fireEvent.click(within(sheet).getAllByRole("button", { name: "Add" })[0]);
    await settle();
    expect(calls).toContainEqual({
      method: "settings.set",
      params: { tools: { "ui-automation-apps": { allow: [], block: ["KeePassXC", "MyBank"] } } },
    });
  });

  it("sets where requests go and asks before keeping nothing", async () => {
    page("privacy");
    await settle();
    fireEvent.click(screen.getByRole("radio", { name: "Strict private" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { privacy: { mode: "strict-private" } } });
    fireEvent.click(screen.getByRole("switch", { name: "Save transcripts in logs" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { privacy: { "debug-transcripts": true } } });
  });
});
