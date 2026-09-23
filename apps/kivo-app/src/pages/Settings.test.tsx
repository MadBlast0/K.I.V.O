import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";

const calls: Array<{ method: string; params: unknown }> = [];
const sounds = { enabled: true, set: "soft", volume: 70, "thinking-cue": false, off: ["hangup"] };

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
  if (method === "settings.get") return Promise.resolve({ sounds });
  if (method === "settings.set") {
    const patch = typeof params === "object" && params !== null && "sounds" in params ? params.sounds : {};
    return Promise.resolve({ sounds: Object.assign({}, sounds, patch) });
  }
  return Promise.resolve(null);
};

const { Settings } = await import("./Settings");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe("Settings → Sounds (VOICE-27)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("shows every cue with its state and previews the set or one cue", async () => {
    render(
      <ToastProvider>
        <Settings />
      </ToastProvider>,
    );
    await settle();
    expect(screen.getByRole("switch", { name: "Starts listening" })).toHaveProperty("ariaChecked", "true");
    expect(screen.getByRole("switch", { name: "Thinking" })).toHaveProperty("ariaChecked", "false");
    expect(screen.getByRole("switch", { name: "Conversation ends" })).toHaveProperty("ariaChecked", "false");

    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    fireEvent.click(screen.getByRole("button", { name: "Play Needs your answer" }));
    expect(calls).toContainEqual({ method: "sounds.preview", params: { set: "soft", cue: null } });
    expect(calls).toContainEqual({ method: "sounds.preview", params: { set: "soft", cue: "question" } });
  });

  it("turns cues off one by one, and the thinking cue through its own setting", async () => {
    render(
      <ToastProvider>
        <Settings />
      </ToastProvider>,
    );
    await settle();
    fireEvent.click(screen.getByRole("switch", { name: "Done" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { sounds: { off: ["hangup", "done"] } } });
    fireEvent.click(screen.getByRole("switch", { name: "Thinking" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { sounds: { "thinking-cue": true } } });
  });
});

describe("Settings → Island (UX-13)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("shows where the Island appears and changes it", async () => {
    const link = runtime.link as { snapshot: unknown };
    link.snapshot = { island: { position: "top-center", spots: [] } };
    try {
      render(
        <ToastProvider>
          <Settings />
        </ToastProvider>,
      );
      await settle();
      const trigger = screen.getByRole("combobox", { name: "Where the Island appears" });
      expect(trigger.textContent).toContain("Top center");
      fireEvent.click(trigger);
      const option = await screen.findByRole("option", { name: "Where I dragged it" });
      // Base UI selects on the pointer's release, as a real click does.
      fireEvent.pointerDown(option, { pointerType: "mouse" });
      fireEvent.mouseDown(option);
      fireEvent.pointerUp(option, { pointerType: "mouse" });
      fireEvent.mouseUp(option);
      fireEvent.click(option);
      await settle();
      expect(calls).toContainEqual({ method: "settings.set", params: { overlay: { position: "remember-drag" } } });
    } finally {
      link.snapshot = null;
    }
  });

  it("stays hidden until the runtime says where the Island is", async () => {
    render(
      <ToastProvider>
        <Settings />
      </ToastProvider>,
    );
    await settle();
    expect(screen.queryByRole("combobox", { name: "Where the Island appears" })).toBeNull();
  });
});
