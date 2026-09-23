import { act, render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const calls: Array<{ method: string; params: unknown }> = [];
let appearance: Record<string, unknown> = {};

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
  if (method === "settings.get" || method === "settings.set") return Promise.resolve({ appearance });
  return Promise.resolve(null);
};

const { ThemeProvider } = await import("./theme");
const { ThemeSync } = await import("./ThemeSync");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
const root = document.documentElement;

function mount() {
  render(
    <ThemeProvider>
      <ThemeSync />
    </ThemeProvider>,
  );
}

beforeEach(() => {
  calls.length = 0;
  localStorage.clear();
});

describe("ThemeSync (UX-37)", () => {
  it("applies the appearance the runtime keeps", async () => {
    localStorage.setItem("kivo.appearance.moved", "1");
    appearance = { theme: "dark", accent: "violet", "text-size": "large", motion: "full", transparency: false };
    mount();
    await settle();
    expect(root.dataset.theme).toBe("dark");
    expect(root.dataset.accent).toBe("violet");
    expect(root.dataset.textsize).toBe("large");
    expect(root.dataset.transparency).toBe("off");
    expect(calls.some((c) => c.method === "settings.set")).toBe(false);
  });

  it("moves an appearance chosen before M7 into the runtime once, instead of losing it", async () => {
    localStorage.setItem("kivo.appearance", JSON.stringify({ theme: "dark", accent: "teal" }));
    appearance = { theme: "light", accent: "blue", "text-size": "normal", motion: "system", transparency: true };
    mount();
    await settle();
    expect(calls).toContainEqual({
      method: "settings.set",
      params: {
        appearance: { theme: "dark", accent: "teal", "text-size": "normal", motion: "system", transparency: true },
      },
    });
    expect(localStorage.getItem("kivo.appearance.moved")).toBe("1");
  });
});
