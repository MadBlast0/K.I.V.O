import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Link, StateSnapshot } from "../ipc/generated";

// The overlay talks to the app through Tauri; these stand in for it.
const handlers = new Map<string, (e: { payload: unknown }) => void>();
const invoked: Array<{ cmd: string; args: unknown }> = [];
let boot: Link;

vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: (cmd: string, args?: unknown) => {
    invoked.push({ cmd, args });
    return Promise.resolve(cmd === "ui_ready" ? { link: boot } : undefined);
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(event, handler);
    return Promise.resolve(() => handlers.delete(event));
  },
}));

const { Overlay } = await import("./Overlay");

function snapshot(confirm: boolean): StateSnapshot {
  return {
    session: confirm ? "awaitingConfirmation" : "thinking",
    mode: "ask",
    islandHidden: false,
    speech: { state: "ready" },
    revision: 1,
    turn: {
      id: "t1",
      source: "typed",
      transcript: "mute",
      transcriptFinal: true,
      steps: [],
      answer: null,
      error: null,
      targetApp: null,
      capabilityOff: null,
      quiet: null,
      guest: false,
      answering: false,
      waiting: false,
      confirm: confirm
        ? {
            callId: "c1",
            tool: "audio.mute",
            action: "Mute the sound",
            target: null,
            why: "You asked KIVO to check with you before doing things.",
            provenance: "You asked",
            risk: "low",
            strength: "normal",
            allowAlways: true,
            plan: false,
            hello: false,
          }
        : null,
    },
  };
}

function connected(s: StateSnapshot): Link {
  return { status: "connected", runtimeVersion: "0.0.0", snapshot: s, message: null };
}

async function mount(s: StateSnapshot) {
  boot = connected(s);
  render(<Overlay />);
  // Let the listeners register and `ui_ready` answer.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

describe("Overlay keyboard access (UX-52)", () => {
  beforeEach(() => {
    handlers.clear();
    invoked.length = 0;
  });

  it("puts focus on the Island's buttons when asked, and arrows move between them", async () => {
    await mount(snapshot(true));
    const buttons = screen.getAllByRole("button");
    expect(buttons.length).toBeGreaterThanOrEqual(2);
    expect(document.activeElement).not.toBe(buttons[0]);

    act(() => handlers.get("island://type")?.({ payload: null }));
    expect(document.activeElement).toBe(buttons[0]);

    fireEvent.keyDown(buttons[0], { key: "ArrowRight" });
    expect(document.activeElement).toBe(buttons[1]);
    fireEvent.keyDown(buttons[1], { key: "ArrowLeft" });
    expect(document.activeElement).toBe(buttons[0]);

    fireEvent.keyDown(buttons[0], { key: "Escape" });
    expect(invoked.some((c) => c.cmd === "overlay_typing_done")).toBe(true);
  });

  it("gives focus back once the buttons are gone", async () => {
    await mount(snapshot(true));
    act(() => handlers.get("island://type")?.({ payload: null }));
    invoked.length = 0;
    act(() => handlers.get("runtime://link")?.({ payload: connected(snapshot(false)) }));
    expect(invoked.some((c) => c.cmd === "overlay_typing_done")).toBe(true);
  });

  it("passes an ARIA audit with a confirmation showing", async () => {
    await mount(snapshot(true));
    const result = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
    });
    expect(result.violations.map((v) => v.id)).toEqual([]);
  });

  it("keeps a finished card while the pointer is over it (UX-10)", async () => {
    const done = snapshot(false);
    done.session = "idle";
    done.turn!.answer = "Muted.";
    await mount(done);
    expect(screen.getByText("Muted.")).toBeTruthy();
    const overlay = document.querySelector(".k-overlay")!;
    fireEvent.mouseEnter(overlay);
    expect(invoked.some((c) => c.cmd === "overlay_hover")).toBe(true);
    // The runtime clears the turn after 4 s; the card stays under the pointer.
    act(() => handlers.get("runtime://link")?.({ payload: connected({ ...done, turn: null }) }));
    expect(screen.getByText("Muted.")).toBeTruthy();
    fireEvent.mouseLeave(overlay);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 400));
    });
    expect(screen.queryByText("Muted.")).toBeNull();
  });

  it("opens the text field when the Island has no buttons", async () => {
    await mount(snapshot(false));
    act(() => handlers.get("island://type")?.({ payload: null }));
    expect(document.activeElement).toBe(screen.getByRole("textbox"));
  });
});
