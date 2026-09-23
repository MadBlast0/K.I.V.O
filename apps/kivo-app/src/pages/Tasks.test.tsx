import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { TaskView } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];
let active: TaskView[] = [];
let finished: TaskView[] = [];

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
  if (method === "tasks.list") {
    const finishedList = typeof params === "object" && params !== null && "finished" in params && params.finished;
    return Promise.resolve(finishedList ? finished : active);
  }
  return Promise.resolve(null);
};

const { Tasks, currentStep, elapsed, isActive } = await import("./Tasks");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function task(over: Partial<TaskView>): TaskView {
  return {
    id: "t1",
    title: "Fix failing tests in kivo-runtime",
    kind: "coding",
    owner: "you",
    status: "running",
    steps: [
      { id: "s1", title: "Run tests", status: "done", detail: "2 failing", startedAt: 1, finishedAt: 2, attempts: 1 },
      { id: "s2", title: "Change code", status: "running", detail: null, startedAt: 2, finishedAt: null, attempts: 1 },
      {
        id: "s3",
        title: "Run tests again",
        status: "pending",
        detail: null,
        startedAt: null,
        finishedAt: null,
        attempts: 0,
      },
    ],
    createdAt: Date.now() - 65_000,
    updatedAt: Date.now(),
    finishedAt: null,
    result: null,
    error: null,
    summary: null,
    usesAi: true,
    question: null,
    routineId: null,
    ...over,
  };
}

beforeEach(() => {
  calls.length = 0;
  active = [];
  finished = [];
});

describe("Tasks page (UX-24)", () => {
  it("shows running tasks with their current step, time and steps, and stops them", async () => {
    active = [task({})];
    render(
      <ToastProvider>
        <Tasks />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText("Fix failing tests in kivo-runtime")).toBeTruthy();
    expect(screen.getByText(/Step 2 of 3 · 1 min 0[56] s · Uses AI/)).toBeTruthy();
    expect(screen.getByText("Run tests")).toBeTruthy();
    expect(screen.getByText("2 failing")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Stop" }));
    await settle();
    expect(calls).toContainEqual({ method: "tasks.cancel", params: { id: "t1" } });
    fireEvent.click(screen.getByRole("button", { name: "Pause" }));
    await settle();
    expect(calls).toContainEqual({ method: "tasks.pause", params: { id: "t1" } });
  });

  it("answers a task's question, resumes a paused task, and clears finished ones", async () => {
    active = [
      task({
        status: "needsYou",
        question: { step: "s2", text: "“Change code” failed.", choices: ["retry", "skip", "stop"] },
      }),
      task({ id: "t2", title: "Watch dataset.zip", kind: "watch", status: "paused", usesAi: false, steps: [] }),
    ];
    finished = [
      task({
        id: "t3",
        title: "Remind me to stretch",
        kind: "reminder",
        status: "done",
        result: "Reminded you",
        finishedAt: Date.now(),
      }),
    ];
    render(
      <ToastProvider>
        <Tasks />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText("“Change code” failed.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await settle();
    expect(calls).toContainEqual({ method: "tasks.answer", params: { id: "t1", choice: "retry" } });
    fireEvent.click(screen.getByRole("button", { name: "Resume" }));
    await settle();
    expect(calls).toContainEqual({ method: "tasks.resume", params: { id: "t2" } });
    expect(screen.getByText(/Reminded you/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Delete “Remind me to stretch”" }));
    await settle();
    expect(calls).toContainEqual({ method: "tasks.delete", params: { id: "t3" } });
    fireEvent.click(screen.getByRole("button", { name: "Clear finished" }));
    await settle();
    expect(calls.some((c) => c.method === "tasks.clearFinished")).toBe(true);
  });

  it("stops every running task at once", async () => {
    active = [task({}), task({ id: "t2", status: "waiting" })];
    render(
      <ToastProvider>
        <Tasks />
      </ToastProvider>,
    );
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Stop all" }));
    await settle();
    const cancelled = calls.filter((c) => c.method === "tasks.cancel").map((c) => c.params);
    expect(cancelled).toEqual([{ id: "t1" }, { id: "t2" }]);
  });

  it("says when nothing is running", async () => {
    render(
      <ToastProvider>
        <Tasks />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText("Nothing running")).toBeTruthy();
  });
});

describe("task helpers", () => {
  it("formats elapsed time, finds the current step and tells running from finished", () => {
    expect(elapsed(4_000)).toBe("4 s");
    expect(elapsed(65_000)).toBe("1 min 05 s");
    expect(elapsed(3_780_000)).toBe("1 h 03 min");
    expect(currentStep(task({}))?.id).toBe("s2");
    expect(isActive(task({ status: "waiting" }))).toBe(true);
    expect(isActive(task({ status: "interrupted" }))).toBe(false);
  });
});
