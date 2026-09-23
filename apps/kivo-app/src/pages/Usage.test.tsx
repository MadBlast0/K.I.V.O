import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { UsageSummary } from "./Usage";

const calls: Array<{ method: string; params: unknown }> = [];
let summary: UsageSummary;

function base(): UsageSummary {
  const day = Date.UTC(2026, 8, 20);
  return {
    total: 1.84,
    today: 0.06,
    requests: 40,
    unpriced: 1,
    turns: 50,
    freeTurns: 34,
    byDay: [
      { day, cost: 0.5, ai: 0.4, speech: 0.1 },
      { day: day + 86_400_000, cost: 1.34, ai: 1.34, speech: 0 },
    ],
    byProvider: [
      { provider: "OpenRouter", cost: 0.58, tokens: 1000 },
      { provider: "Claude Code", cost: 1.12, tokens: 0 },
    ],
    byFeature: [{ feature: "brain", cost: 1.7 }],
    byRoutine: [{ routine: "r1", name: "Morning brief", cost: 0.14 }],
    top: [{ kind: "task", id: "t1", title: "Fix the failing test", cost: 0.9 }],
    limits: [],
    caps: { computerUse: 0.5, agents: 2, realtimeMinutes: 30 },
  };
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
    case "usage.summary":
      return Promise.resolve(summary);
    case "usage.setLimits":
      summary = {
        ...summary,
        limits: [
          {
            limit: {
              scope: { kind: "overall" },
              period: { monthly: { reset_day: 1 } },
              amount: 10,
              warnings: [80],
              atLimit: "ask",
            },
            state: { spent: 1.84, amount: 10, warning: null, reached: false },
          },
        ],
      };
      return Promise.resolve(summary);
    case "usage.setCaps":
      return Promise.resolve(summary);
    case "usage.export":
      return Promise.resolve({ csv: "time\n", file: "C:\\Users\\Sam\\Downloads\\KIVO usage 2026-09-23.csv" });
    case "settings.get":
      return Promise.resolve({ brains: { "show-cost": false } });
    default:
      return Promise.resolve(null);
  }
};

const { Usage } = await import("./Usage");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function page() {
  render(
    <ToastProvider>
      <Usage />
    </ToastProvider>,
  );
}

beforeEach(() => {
  calls.length = 0;
  summary = base();
});

describe("Usage (UX-30, BRAIN-37)", () => {
  it("shows totals, the free share, providers, routines and the most expensive work", async () => {
    page();
    await settle();
    expect(calls).toContainEqual({ method: "usage.summary", params: { days: 30 } });
    expect(screen.getByText("$1.84")).toBeTruthy();
    expect(screen.getByText("68%")).toBeTruthy();
    expect(screen.getByText("No limit")).toBeTruthy();
    expect(screen.getByRole("img", { name: /spend per day/ })).toBeTruthy();
    expect(screen.getByText("Morning brief")).toBeTruthy();
    expect(screen.getByText("Fix the failing test")).toBeTruthy();
    expect(screen.getByText(/1 request used a model with no known price/)).toBeTruthy();
    // The costliest provider first.
    const providers = screen.getAllByRole("progressbar").map((p) => p.getAttribute("aria-label"));
    expect(providers[0]).toBe("Share spent on Claude Code");

    fireEvent.click(screen.getByRole("button", { name: "Week" }));
    await settle();
    expect(calls).toContainEqual({ method: "usage.summary", params: { days: 7 } });
  });

  it("turns on a monthly budget and chooses what happens at the limit", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("switch", { name: "Monthly budget" }));
    await settle();
    expect(calls).toContainEqual({
      method: "usage.setLimits",
      params: {
        limits: [
          {
            scope: { kind: "overall" },
            period: { monthly: { reset_day: 1 } },
            amount: 10,
            warnings: [80],
            atLimit: "ask",
          },
        ],
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "Local only" }));
    await settle();
    const last = calls.findLast((c) => c.method === "usage.setLimits");
    expect(JSON.stringify(last?.params)).toContain('"atLimit":"localOnly"');
  });

  it("exports a CSV and edits the per-task limits", async () => {
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "CSV" }));
    await settle();
    expect(calls).toContainEqual({ method: "usage.export", params: { days: 30, save: true } });

    fireEvent.click(screen.getByText("Per-task limits"));
    const dialog = await screen.findByRole("dialog", { name: "Per-task limits" });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Agents, per task" }), { target: { value: "" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await settle();
    expect(calls).toContainEqual({
      method: "usage.setCaps",
      params: { computerUse: 0.5, agents: null, realtimeMinutes: 30 },
    });
  });
});
