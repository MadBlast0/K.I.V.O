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
      guest: false,
      answering: false,
      waiting: false,
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
    edit: vi.fn<IslandHandlers["edit"]>(),
    talk: vi.fn<IslandHandlers["talk"]>(),
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

  it("lets the user fix what KIVO heard, type or talk again from the card (UX-09)", () => {
    const on = handlers();
    const model = islandForTurn(acting("auto"), i18n.t.bind(i18n), on);
    render(<>{model?.body}</>);
    fireEvent.click(screen.getByRole("button", { name: "open chrome" }));
    expect(on.edit).toHaveBeenCalledWith("open chrome");
    fireEvent.click(screen.getByRole("button", { name: i18n.t("island.typePlaceholder") }));
    expect(on.edit).toHaveBeenLastCalledWith("");
    expect(screen.getByRole("button", { name: i18n.t("island.talk") })).toHaveProperty("disabled", true);
  });

  it("takes clicks while it shows Stop", () => {
    const model = islandForTurn(acting("auto"), i18n.t.bind(i18n), handlers());
    expect(hasButtons(model)).toBe(true);
  });
});

describe("Island M2 states (UX-08, UX-45, CONV-26)", () => {
  const t = i18n.t.bind(i18n);
  const decision = (risk: "medium" | "high") => {
    const snapshot = acting("ask");
    snapshot.session = "awaitingConfirmation";
    snapshot.turn!.confirm = {
      callId: "c1",
      tool: "apps.close",
      action: "Close Google Chrome",
      target: "Google Chrome",
      why: "It closes an app.",
      provenance: "You asked",
      risk,
      strength: "normal",
      allowAlways: false,
      plan: false,
    };
    return snapshot;
  };

  it("marks a guest's turn with a Guest chip", () => {
    const snapshot = acting("auto");
    snapshot.turn!.guest = true;
    render(<>{islandForTurn(snapshot, t, handlers())?.trail}</>);
    expect(screen.getByText("GUEST")).toBeTruthy();
  });

  it("counts down the follow-up window with a ring", () => {
    const snapshot = acting("auto");
    snapshot.session = "followUp";
    snapshot.turn!.followUp = 8;
    const model = islandForTurn(snapshot, t, handlers());
    render(<>{model?.trail}</>);
    const ring = screen.getByRole("img", { name: /8 seconds to follow up/ });
    expect(ring.getAttribute("style")).toContain("--dur: 8s");
    expect(model?.sub).toBe(t("island.followUpSub"));
  });

  it("shows the words KIVO listens for while it waits for a spoken answer", () => {
    const snapshot = decision("medium");
    snapshot.turn!.answering = true;
    render(<>{islandForTurn(snapshot, t, handlers())?.body}</>);
    const hint = screen.getByText(/^Say/).textContent ?? "";
    expect(hint).toContain("allow");
    expect(hint).toContain("deny");
    expect(hint).toContain("wait");
  });

  it("gives no voice hint when only a click can approve", () => {
    const snapshot = decision("high");
    snapshot.turn!.answering = true;
    render(<>{islandForTurn(snapshot, t, handlers())?.body}</>);
    expect(screen.queryByText(/^Say/)).toBeNull();
  });

  it("says Waiting for you after the user said wait, keeping the buttons", () => {
    const snapshot = decision("medium");
    snapshot.turn!.waiting = true;
    const model = islandForTurn(snapshot, t, handlers());
    expect(model?.label).toBe("Waiting for you");
    expect(hasButtons(model)).toBe(true);
  });
});
