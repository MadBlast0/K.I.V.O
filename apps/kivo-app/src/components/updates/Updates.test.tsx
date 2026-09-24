import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../ui";
import type { UpdateView } from "../../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];
let settings: Record<string, Record<string, unknown>> = {};
let view: UpdateView;

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.1.0", snapshot: {}, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "settings.get":
      return Promise.resolve(settings);
    case "settings.set":
      return Promise.resolve(settings);
    case "updates.status":
    case "updates.check":
      return Promise.resolve(view);
    default:
      return Promise.resolve(null);
  }
};

const { UpdatesSection, WhatsNewDialog, noteLines } = await import("./Updates");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe("Updates (DIST-07/08, UX-59)", () => {
  beforeEach(() => {
    calls.length = 0;
    settings = { updates: { check: true, install: "ask" } };
    view = { current: "0.1.0", channel: "beta", enabled: true, state: { kind: "upToDate", checkedAt: 1 } };
  });

  it("shows the version, checks now, and changes the channel and when to install", async () => {
    render(
      <ToastProvider>
        <UpdatesSection />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText("KIVO 0.1.0")).toBeTruthy();
    expect(screen.getByText("You’re up to date")).toBeTruthy();
    // The build's channel shows until one is chosen.
    expect(screen.getByRole("button", { name: "Beta" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Check now" }));
    await settle();
    expect(calls.map((c) => c.method)).toContain("updates.check");
    fireEvent.click(screen.getByRole("button", { name: "Stable" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { updates: { channel: "stable" } } });
    fireEvent.click(screen.getByRole("button", { name: "When idle" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { updates: { install: "when-idle" } } });
  });

  it("offers Install now when an update is ready, and says when a build can't update", async () => {
    view = {
      ...view,
      state: { kind: "ready", version: "0.2.0", notes: "- Faster wake word", whenIdle: false },
    };
    const { unmount } = render(
      <ToastProvider>
        <UpdatesSection />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText(/KIVO 0.2.0 is ready/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Install now" }));
    await settle();
    expect(calls).toContainEqual({ method: "updates.install", params: { whenIdle: false } });
    unmount();

    view = { current: "0.0.0", channel: "stable", enabled: false, state: { kind: "idle" } };
    render(
      <ToastProvider>
        <UpdatesSection />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByText(/can’t update itself/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Check now" }).hasAttribute("disabled")).toBe(true);
  });

  it("shows what's new once after an update, with release notes and Got it", async () => {
    view = {
      ...view,
      current: "0.2.0",
      whatsNew: { version: "0.2.0", from: "0.1.0", notes: "- Say “approve” when KIVO asks\n- New sound sets\n" },
    };
    render(<WhatsNewDialog />);
    await settle();
    expect(screen.getByText("What’s new in KIVO 0.2.0")).toBeTruthy();
    expect(screen.getByText("New sound sets")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Release notes" }));
    await settle();
    expect(calls).toContainEqual({
      method: "system.openUrl",
      params: { url: "https://github.com/MadBlast0/K.I.V.O/releases/tag/v0.2.0" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Got it" }));
    await settle();
    expect(calls.map((c) => c.method)).toContain("updates.seen");
    expect(screen.queryByText("What’s new in KIVO 0.2.0")).toBeNull();
  });

  it("turns release notes into lines", () => {
    expect(noteLines("- One\n* Two\n\nThree\r\n• Four")).toEqual(["One", "Two", "Three", "Four"]);
  });
});
