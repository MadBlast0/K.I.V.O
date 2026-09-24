import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider, TooltipProvider } from "../components/ui";
import type { ActivityItem, AuditItem } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];
const now = Date.now();

const activity: ActivityItem[] = [
  {
    id: 2,
    ts: now - 1000,
    turnId: "t1",
    kind: "tool",
    title: "Mute",
    detail: null,
    status: "done",
  },
  {
    id: 1,
    ts: now - 2000,
    turnId: "t1",
    kind: "transcript",
    title: "mute",
    detail: null,
    status: "done",
  },
];

const audit: AuditItem[] = [
  {
    ts: now - 1000,
    turnId: "t1",
    tool: "audio.mute",
    argsSummary: "",
    risk: "safe",
    decision: "allow",
    confirmedBy: null,
    result: "ok",
    error: null,
  },
  {
    ts: now - 500,
    turnId: "t2",
    tool: "files.delete",
    argsSummary: "C:\\Work\\old.txt",
    risk: "high",
    decision: "deny",
    confirmedBy: "user",
    result: null,
    error: "You said no",
  },
];

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.0.0", snapshot: null, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "activity.list":
      return Promise.resolve(activity);
    case "audit.list":
      return Promise.resolve(audit);
    case "activity.export":
      return Promise.resolve({ file: "C:/Users/me/Downloads/KIVO activity 2026-09-24.csv" });
    default:
      return Promise.resolve(null);
  }
};

const { Activity } = await import("./Activity");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe("Activity (UX-20, SEC-24)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("shows the audit log: every permission decision, allowed or refused, with why", async () => {
    render(
      <TooltipProvider>
        <ToastProvider>
          <Activity />
        </ToastProvider>
      </TooltipProvider>,
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Audit" }));
    await settle();
    expect(screen.getByText("audio.mute")).toBeTruthy();
    expect(screen.getByText("files.delete · C:\\Work\\old.txt")).toBeTruthy();
    expect(screen.getByText(/deny · user — You said no/)).toBeTruthy();
    const result = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
    });
    expect(result.violations.map((v) => v.id)).toEqual([]);
  });

  it("filters to what a brain answered and exports the timeline", async () => {
    render(
      <TooltipProvider>
        <ToastProvider>
          <Activity />
        </ToastProvider>
      </TooltipProvider>,
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "AI" }));
    await settle();
    // The fixtures are a command and a tool: nothing a brain answered.
    expect(screen.getByText("Nothing here yet.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await settle();
    expect(calls.some((c) => c.method === "activity.export")).toBe(true);
    expect(screen.getByText(/Saved to .*KIVO activity 2026-09-24\.csv/)).toBeTruthy();
  });
});
