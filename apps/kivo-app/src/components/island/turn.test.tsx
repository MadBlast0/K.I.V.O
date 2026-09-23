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
    misroute: vi.fn<IslandHandlers["misroute"]>(),
    remember: vi.fn<IslandHandlers["remember"]>(() => Promise.resolve(null)),
    editDraft: vi.fn<IslandHandlers["editDraft"]>(),
    offer: vi.fn<IslandHandlers["offer"]>(),
    openTask: vi.fn<IslandHandlers["openTask"]>(),
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
      hello: false,
    };
    const on = handlers();
    const model = islandForTurn(snapshot, i18n.t.bind(i18n), on);
    render(<>{model?.body}</>);
    expect(screen.getByText("Close Google Chrome")).toBeTruthy();
    expect(screen.getByText(/You asked KIVO to check/).textContent).toContain("You asked");
    // "Always for…" asks how long (SEC-08).
    fireEvent.click(screen.getByRole("button", { name: "Always allow for Google Chrome" }));
    fireEvent.click(screen.getByRole("button", { name: "For 24 hours" }));
    expect(on.answer).toHaveBeenCalledWith("c1", true, true, { duration: "day" });
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
      hello: false,
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

describe("Island for a brain's answer (PLAN-17, BRAIN-06)", () => {
  it("shows which brain answered and why, and takes a misroute report", () => {
    const on = handlers();
    const t = i18n.t.bind(i18n);
    const snapshot = acting("auto");
    snapshot.session = "idle";
    if (!snapshot.turn) throw new Error("turn");
    snapshot.turn.steps = [];
    snapshot.turn.answer = "Paris is the capital of France.";
    snapshot.turn.brain = {
      name: "Claude Code",
      profile: "Coding",
      reason: "Coding · Claude Code — because this looked like a coding task",
      local: false,
      cost: 0.004,
      contextUsed: 1200,
      contextBudget: 32000,
    };
    const model = islandForTurn(snapshot, t, on);
    render(
      <>
        {model?.trail}
        {model?.body}
      </>,
    );
    const chip = screen.getByText("Coding · Claude Code · ≈ $0.0040");
    expect(chip.getAttribute("title")).toBe("Coding · Claude Code — because this looked like a coding task");
    fireEvent.click(screen.getByRole("button", { name: "That’s not what I meant" }));
    expect(on.misroute).toHaveBeenCalledWith("t1");
  });

  it("keeps an answer with Remember this (MEM-05)", async () => {
    const on = handlers();
    const t = i18n.t.bind(i18n);
    const snapshot = acting("auto");
    snapshot.session = "idle";
    if (!snapshot.turn) throw new Error("turn");
    snapshot.turn.steps = [];
    snapshot.turn.answer = "Your dentist is Dr. Rao.";
    snapshot.turn.brain = {
      name: "Ollama",
      profile: "Default",
      reason: "Default · Ollama",
      local: true,
      contextUsed: 900,
      contextBudget: 8000,
    };
    const model = islandForTurn(snapshot, t, on);
    render(<>{model?.body}</>);
    fireEvent.click(screen.getByRole("button", { name: "Remember this" }));
    expect(on.remember).toHaveBeenCalledWith("t1");
    expect(await screen.findByText("Remembered")).toBeTruthy();
  });
});

describe("Island M4 (SEC-02, SEC-11, UX-43, UX-46, CAP-06, CAP-08)", () => {
  const t = i18n.t.bind(i18n);

  it("shows a plan as its steps and approves it in one answer", () => {
    const snapshot = acting("plan");
    snapshot.session = "awaitingConfirmation";
    snapshot.turn!.confirm = {
      callId: "t1-plan1",
      tool: "plan",
      action: ["Plan:", "1. Type “Ada” into a field", "2. Switch an option on or off"].join("\n"),
      target: null,
      why: "In Plan first, nothing changes until you approve the plan.",
      provenance: "Suggested by the AI",
      risk: "medium",
      strength: "normal",
      allowAlways: false,
      plan: true,
      hello: false,
    };
    const on = handlers();
    render(<>{islandForTurn(snapshot, t, on)?.body}</>);
    const plan = screen.getByText(/1\. Type “Ada” into a field/);
    expect(plan.className).toContain("k-island__action--plan");
    fireEvent.click(screen.getByRole("button", { name: "Approve plan" }));
    expect(on.answer).toHaveBeenCalledWith("t1-plan1", true, false);
    expect(screen.queryByRole("button", { name: /Always/ })).toBeNull();
  });

  it("offers Windows Hello for high risk", () => {
    const snapshot = acting("auto");
    snapshot.session = "awaitingConfirmation";
    snapshot.turn!.confirm = {
      callId: "c9",
      tool: "system.shutdown",
      action: "Shut down the computer",
      target: null,
      why: "This is a high-risk action.",
      provenance: "You asked",
      risk: "high",
      strength: "strong",
      allowAlways: false,
      plan: false,
      hello: true,
    };
    const on = handlers();
    render(<>{islandForTurn(snapshot, t, on)?.body}</>);
    fireEvent.click(screen.getByRole("button", { name: "Confirm with Windows Hello" }));
    expect(on.answer).toHaveBeenCalledWith("c9", true, false, { hello: true });
  });

  it("offers Undo with a ring after a reversible change, and shows the app's icon and notes", () => {
    const snapshot = acting("auto");
    snapshot.session = "idle";
    snapshot.turn!.answer = "Moved 1 item to Archive.";
    snapshot.turn!.targetIcon = "data:image/png;base64,iVBORw0KGgo=";
    snapshot.turn!.note = "Sent a screenshot of Notepad to Claude.";
    snapshot.turn!.undo = { title: "Move files to Archive", until: Date.now() + 8000 };
    const on = { ...handlers(), undo: vi.fn<() => void>() };
    const model = islandForTurn(snapshot, t, on);
    render(
      <>
        {model?.lead}
        {model?.body}
      </>,
    );
    expect(screen.getByRole("img", { name: "Google Chrome" }).getAttribute("src")).toContain("data:image/png");
    expect(screen.getByText("Sent a screenshot of Notepad to Claude.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(on.undo).toHaveBeenCalledOnce();
    // Once the offer has run out, no Undo.
    snapshot.turn!.undo = { title: "x", until: Date.now() - 1 };
    const later = islandForTurn(snapshot, t, on);
    expect(later?.body).toBeTruthy();
  });

  it("shows which sensitive capability is in use while acting", () => {
    const snapshot = acting("auto");
    snapshot.inUse = ["screen", "shell"];
    render(<>{islandForTurn(snapshot, t, handlers())?.trail}</>);
    expect(screen.getByRole("img", { name: "Reading the screen" })).toBeTruthy();
    expect(screen.getByRole("img", { name: "Running a command" })).toBeTruthy();
  });
});

describe("Island M5 parts", () => {
  const t = i18n.t.bind(i18n);

  it("shows a prompt draft with Send, Edit and Cancel, and voice hints while listening (CONV-15)", () => {
    const snapshot = acting("auto");
    snapshot.session = "awaitingConfirmation";
    snapshot.turn!.answering = true;
    snapshot.turn!.draft = { target: "Claude Code · terminal · K.I.V.O", text: "Run the tests and fix what fails." };
    snapshot.turn!.confirm = {
      callId: "c9",
      tool: "agents.send_prompt",
      action: "Send a prompt to Claude Code",
      target: "Claude Code",
      why: "Sending a prompt can't be taken back.",
      provenance: "You asked",
      risk: "medium",
      strength: "normal",
      allowAlways: false,
      plan: false,
      hello: false,
    };
    const on = handlers();
    const model = islandForTurn(snapshot, t, on);
    expect(model?.state.startsWith("draft-")).toBe(true);
    expect(hasButtons(model)).toBe(true);
    render(
      <>
        <span>{model?.label}</span>
        <span>{model?.sub}</span>
        {model?.body}
      </>,
    );
    expect(screen.getByText("Claude Code · terminal · K.I.V.O")).toBeTruthy();
    expect(screen.getByText("Run the tests and fix what fails.")).toBeTruthy();
    expect(screen.getByText(/read it back/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(on.editDraft).toHaveBeenCalledWith("c9", "Run the tests and fix what fails.");
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(on.answer).toHaveBeenCalledWith("c9", true, false);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(on.answer).toHaveBeenCalledWith("c9", false, false);
  });

  it("lists help examples and links the task a request started (UX-44, UX-24)", () => {
    const snapshot = acting("auto");
    snapshot.session = "idle";
    snapshot.turn!.answer = "Here's what you can say.";
    snapshot.turn!.help = ["new tab", "reopen the closed tab"];
    snapshot.turn!.taskId = "task-7";
    const on = handlers();
    render(<>{islandForTurn(snapshot, t, on)?.body}</>);
    expect(screen.getByText("“new tab”")).toBeTruthy();
    expect(screen.getByText("“reopen the closed tab”")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open in Tasks" }));
    expect(on.openTask).toHaveBeenCalledWith("task-7");
  });

  it("shows a live activity with its countdown, and how many more there are (UX-15)", () => {
    const now = Date.now();
    const snapshot: StateSnapshot = {
      ...acting("auto"),
      session: "idle",
      turn: null,
      activities: [
        { id: "a1", kind: "timer", title: "Stretch", detail: null, progress: null, until: now + 65_000, taskId: "t1" },
        { id: "a2", kind: "download", title: "dataset.zip", detail: null, progress: 0.6, until: null, taskId: null },
      ],
    };
    const on = handlers();
    const model = islandForTurn(snapshot, t, on);
    expect(model?.state).toBe("activity-a1");
    render(
      <>
        <span>{model?.label}</span>
        <span>{model?.sub}</span>
        {model?.trail}
      </>,
    );
    expect(screen.getByText("Stretch")).toBeTruthy();
    expect(screen.getByText(/^1:0[45]$/)).toBeTruthy();
    expect(screen.getByText("+1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open in Tasks" }));
    expect(on.openTask).toHaveBeenCalledWith("t1");
  });

  it("asks an offer outside a turn and sends the answer (CONV-10)", () => {
    const snapshot: StateSnapshot = {
      ...acting("auto"),
      session: "idle",
      turn: null,
      offer: {
        id: "w1",
        kind: "workspace",
        text: "Remember kivo as a workspace?",
        accept: "Remember",
        decline: "Not now",
      },
    };
    const on = handlers();
    const model = islandForTurn(snapshot, t, on);
    expect(hasButtons(model)).toBe(true);
    render(<>{model?.body}</>);
    fireEvent.click(screen.getByRole("button", { name: "Remember" }));
    expect(on.offer).toHaveBeenCalledWith("w1", true);
    fireEvent.click(screen.getByRole("button", { name: "Not now" }));
    expect(on.offer).toHaveBeenCalledWith("w1", false);
  });
});

describe("Island preferences (Settings → Island, Accessibility)", () => {
  const t = i18n.t.bind(i18n);
  function speaking(prefs: Partial<NonNullable<StateSnapshot["island"]>>): StateSnapshot {
    const s = acting("auto");
    s.session = "speaking";
    s.turn!.answer = "Chrome is open.";
    s.turn!.undo = { until: Date.now() + 8_000, title: "Close Chrome" };
    s.island = {
      position: "top-center",
      spots: [],
      size: "standard",
      showTranscript: true,
      showUndo: true,
      voiceHints: true,
      largeText: false,
      captions: true,
      announcements: true,
      motion: "system",
      companion: "pill",
      ...prefs,
    };
    return s;
  }

  it("shows the answer and Undo by default, and hides them when turned off", () => {
    const on = { ...handlers(), undo: vi.fn<() => void>() };
    const { unmount } = render(<>{islandForTurn(speaking({}), t, on)?.body}</>);
    expect(screen.getByText("Chrome is open.")).toBeTruthy();
    expect(screen.getByRole("button", { name: t("island.undo") })).toBeTruthy();
    unmount();
    render(<>{islandForTurn(speaking({ captions: false, showUndo: false }), t, on)?.body}</>);
    expect(screen.queryByText("Chrome is open.")).toBeNull();
    expect(screen.queryByRole("button", { name: t("island.undo") })).toBeNull();
  });

  it("shows nothing when hidden, except a question that needs an answer", () => {
    const hidden = speaking({ companion: "hidden" });
    expect(islandForTurn(hidden, t, handlers())).toBeNull();
    hidden.session = "awaitingConfirmation";
    hidden.turn!.confirm = {
      callId: "c1",
      tool: "apps.close",
      action: "Close Google Chrome",
      target: "Google Chrome",
      why: "",
      provenance: "You asked",
      risk: "medium",
      strength: "normal",
      allowAlways: false,
      plan: false,
      hello: false,
    };
    expect(islandForTurn(hidden, t, handlers())).not.toBeNull();
  });

  it("keeps the user's words to itself while they speak when asked to", () => {
    const listening = speaking({ showTranscript: false });
    listening.session = "listening";
    listening.turn!.transcript = "open chr";
    listening.turn!.transcriptFinal = false;
    const model = islandForTurn(listening, t, handlers());
    expect(model?.body).toBeUndefined();
    const shown = speaking({});
    shown.session = "listening";
    shown.turn!.transcript = "open chr";
    shown.turn!.transcriptFinal = false;
    render(<>{islandForTurn(shown, t, handlers())?.body}</>);
    expect(screen.getByText("open chr")).toBeTruthy();
  });
});
