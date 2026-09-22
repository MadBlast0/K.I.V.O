import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import i18n from "../../i18n";
import type { PermissionMode, StateSnapshot } from "../../ipc/generated";
import { hasButtons, islandForTurn, type IslandHandlers } from "./turn";

function acting(mode: PermissionMode): StateSnapshot {
  return {
    session: "acting",
    mode,
    islandHidden: false,
    speech: { state: "ready" },
    revision: 1,
    turn: {
      id: "t1",
      source: "pushToTalk",
      transcript: "open chrome",
      transcriptFinal: true,
      steps: [{ id: "s1", title: "Open Google Chrome", status: "running", detail: null }],
      answer: null,
      error: null,
      confirm: null,
      targetApp: "Google Chrome",
      capabilityOff: null,
      quiet: null,
    },
  };
}

function handlers(): IslandHandlers {
  return {
    stop: vi.fn<IslandHandlers["stop"]>(),
    answer: vi.fn<IslandHandlers["answer"]>(),
    openControlCenter: vi.fn<IslandHandlers["openControlCenter"]>(),
    openMode: vi.fn<IslandHandlers["openMode"]>(),
    retry: vi.fn<IslandHandlers["retry"]>(),
    enable: vi.fn<IslandHandlers["enable"]>(),
  };
}

describe("Island while KIVO acts", () => {
  it("shows the permission mode, except in Auto (SEC-04)", () => {
    const on = handlers();
    const t = i18n.t.bind(i18n);
    const { rerender } = render(<>{islandForTurn(acting("plan"), t, on)?.trail}</>);
    const chip = screen.getByRole("button", { name: /Plan first/ });
    expect(chip.textContent).toBe("PLAN");
    fireEvent.click(chip);
    expect(on.openMode).toHaveBeenCalledOnce();

    rerender(<>{islandForTurn(acting("auto"), t, on)?.trail}</>);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("asks with the action, why and who asked, and never goes away by itself (SEC-10)", () => {
    const snapshot = acting("ask");
    snapshot.session = "awaitingConfirmation";
    snapshot.turn!.confirm = {
      callId: "c1",
      tool: "apps.close",
      action: "Close Google Chrome",
      target: "Google Chrome",
      why: "You asked KIVO to check with you before doing things.",
      provenance: "You asked",
      risk: "medium",
      strength: "normal",
      allowAlways: true,
      plan: false,
    };
    const on = handlers();
    const model = islandForTurn(snapshot, i18n.t.bind(i18n), on);
    render(<>{model?.body}</>);
    expect(screen.getByText("Close Google Chrome")).toBeTruthy();
    expect(screen.getByText(/You asked KIVO to check/).textContent).toContain("You asked");
    fireEvent.click(screen.getByRole("button", { name: "Always allow for Google Chrome" }));
    expect(on.answer).toHaveBeenCalledWith("c1", true, true);
    expect(hasButtons(model)).toBe(true);
  });

  it("takes clicks while it shows Stop", () => {
    const model = islandForTurn(acting("auto"), i18n.t.bind(i18n), handlers());
    expect(hasButtons(model)).toBe(true);
  });
});
