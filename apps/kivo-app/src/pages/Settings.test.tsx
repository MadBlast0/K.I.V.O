import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import { ThemeProvider } from "../lib/theme";
import type { SettingsTab } from "./Settings";

const calls: Array<{ method: string; params: unknown }> = [];
let config: Record<string, Record<string, unknown>> = {};

function defaults(): Record<string, Record<string, unknown>> {
  return {
    general: {
      "start-with-windows": false,
      "keep-running-on-close": true,
      "tray-icon": true,
      language: "en",
      languages: [],
      format: "windows",
    },
    sounds: { enabled: true, set: "soft", volume: 70, "thinking-cue": false, off: ["hangup"] },
    overlay: {
      position: "top-center",
      size: "standard",
      monitor: "active",
      "show-transcript": true,
      "show-undo": true,
      "voice-hints": true,
      "hide-after-seconds": 4,
      "in-fullscreen": "hide",
    },
    companion: { style: "pill" },
    accessibility: {
      announcements: true,
      captions: true,
      "large-island-text": false,
      "voice-only": false,
      "warn-silent": true,
    },
    automation: {
      "quiet-hours": "",
      sources: {},
      "catch-up-on-return": true,
      "live-activities": { media: true, timer: true, download: true, agent: true },
      speak: "when-free",
      toasts: true,
      "notification-sound": true,
      "follow-focus": true,
    },
    voice: { "push-to-talk": ["Ctrl", "Space"], "type-to-kivo": ["Ctrl", "Shift", "Space"], "pause-shortcut": [] },
    permissions: { "mode-shortcut": ["Ctrl", "Shift", "M"], "emergency-stop": ["Ctrl", "Alt", "Shift", "Esc"] },
    performance: { profile: "auto" },
    appearance: { theme: "light", accent: "blue", "text-size": "normal", motion: "system", transparency: true },
  };
}

function patchOf(params: unknown): Record<string, Record<string, unknown>> {
  const out: Record<string, Record<string, unknown>> = {};
  if (typeof params !== "object" || params === null) return out;
  for (const [group, value] of Object.entries(params)) {
    if (typeof value === "object" && value !== null && !Array.isArray(value)) out[group] = { ...value };
  }
  return out;
}

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.4.2", snapshot: null, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "settings.get":
      return Promise.resolve(config);
    case "settings.set":
      for (const [group, patch] of Object.entries(patchOf(params))) config[group] = { ...config[group], ...patch };
      return Promise.resolve(config);
    case "settings.export":
      return Promise.resolve({ file: "C:\\Users\\Sam\\Downloads\\KIVO settings 2026-09-24.json" });
    case "performance.status":
      return Promise.resolve({
        cpuPercent: 1.3,
        memoryMb: 132,
        processes: 5,
        gpu: false,
        latency: { wakeToChime: 142, speechToText: 260, commandDone: 180, firstWord: 1100 },
        models: [{ id: "moonshine-base", name: "Moonshine", kind: "stt", residency: "warm", diskBytes: 199229440 }],
      });
    case "diagnostics.run":
      return Promise.resolve([
        { id: "microphone", ok: true, title: "Microphone", detail: "USB Headset", fix: null },
        {
          id: "extension",
          ok: false,
          title: "Browser extension",
          detail: "Not installed or not connected",
          fix: "permissions",
        },
      ]);
    case "models.list":
      return Promise.resolve([
        { id: "kokoro", name: "Kokoro", license: "Apache-2.0", attribution: "hexgrad", source: "Hugging Face" },
      ]);
    default:
      return Promise.resolve(null);
  }
};

const { Settings } = await import("./Settings");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

function page(tab: SettingsTab, onNavigate?: (p: string) => void) {
  render(
    <ThemeProvider>
      <ToastProvider>
        <Settings initialTab={tab} onNavigate={onNavigate} />
      </ToastProvider>
    </ThemeProvider>,
  );
}

const set = (params: unknown) =>
  calls.some((c) => c.method === "settings.set" && JSON.stringify(c.params) === JSON.stringify(params));

beforeEach(() => {
  calls.length = 0;
  config = defaults();
  localStorage.clear();
});

describe("Settings tabs (UX-31)", () => {
  it("has the ten tabs of the mockup", async () => {
    page("general");
    await settle();
    const tabs = screen.getAllByRole("tab").map((t) => t.textContent);
    expect(tabs).toEqual([
      "General",
      "Appearance",
      "Island",
      "Sounds",
      "Notifications",
      "Accessibility",
      "Shortcuts",
      "Performance",
      "Diagnostics",
      "About",
    ]);
  });
});

describe("Settings → General", () => {
  it("changes startup, exports and resets after asking", async () => {
    page("general");
    await settle();
    fireEvent.click(screen.getByRole("switch", { name: "Open KIVO when Windows starts" }));
    await settle();
    expect(set({ general: { "start-with-windows": true } })).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await settle();
    expect(calls.some((c) => c.method === "settings.export")).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Reset" }));
    const dialog = await screen.findByRole("dialog", { name: "Reset all settings?" });
    expect(calls.some((c) => c.method === "settings.reset")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Reset" }));
    await settle();
    expect(calls.some((c) => c.method === "settings.reset")).toBe(true);
  });

  it("imports a settings file's contents", async () => {
    page("general");
    await settle();
    const input = screen.getByLabelText("Import settings");
    const file = new File(['{"kind":"kivo-settings","version":1}'], "KIVO settings.json", { type: "application/json" });
    fireEvent.change(input, { target: { files: [file] } });
    await settle();
    await settle();
    expect(calls).toContainEqual({
      method: "settings.import",
      params: { content: '{"kind":"kivo-settings","version":1}' },
    });
  });
});

describe("Settings → Appearance (DS-03)", () => {
  it("picks the theme and a custom accent, and turns transparency off", async () => {
    page("appearance");
    await settle();
    fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    fireEvent.click(screen.getByRole("switch", { name: "Transparency effects" }));
    expect(document.documentElement.dataset.transparency).toBe("off");
    expect(screen.getByRole("radio", { name: "Custom colour" })).toBeTruthy();
  });
});

describe("Settings → Island", () => {
  it("moves, sizes and hides the Island; Orb waits for a later update", async () => {
    page("island");
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Where I dragged it" }));
    await settle();
    expect(set({ overlay: { position: "remember-drag" } })).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Large" }));
    await settle();
    expect(set({ overlay: { size: "large" } })).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Never" }));
    await settle();
    expect(set({ overlay: { "hide-after-seconds": 0 } })).toBe(true);
    expect(screen.getByRole("radio", { name: /Orb/ })).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("radio", { name: /Hidden/ }));
    await settle();
    expect(set({ companion: { style: "hidden" } })).toBe(true);
  });

  it("warns when the Island is hidden and every sound is off (UX-54)", async () => {
    config.companion = { style: "hidden" };
    config.sounds = { ...config.sounds, enabled: false };
    page("island");
    await settle();
    expect(screen.getByText("KIVO won’t show or sound that it’s listening")).toBeTruthy();
  });
});

describe("Settings → Sounds (VOICE-27)", () => {
  it("shows every cue with its state and previews the set or one cue", async () => {
    page("sounds");
    await settle();
    expect(screen.getByRole("switch", { name: "Starts listening" })).toHaveProperty("ariaChecked", "true");
    expect(screen.getByRole("switch", { name: "Thinking" })).toHaveProperty("ariaChecked", "false");
    expect(screen.getByRole("switch", { name: "Conversation ends" })).toHaveProperty("ariaChecked", "false");
    fireEvent.click(screen.getByRole("button", { name: "Play Soft" }));
    fireEvent.click(screen.getByRole("button", { name: "Play Needs your answer" }));
    expect(calls).toContainEqual({ method: "sounds.preview", params: { set: "soft", cue: null } });
    expect(calls).toContainEqual({ method: "sounds.preview", params: { set: "soft", cue: "question" } });
  });

  it("turns cues off one by one, and the thinking cue through its own setting", async () => {
    page("sounds");
    await settle();
    fireEvent.click(screen.getByRole("switch", { name: "Done" }));
    await settle();
    expect(set({ sounds: { off: ["hangup", "done"] } })).toBe(true);
    fireEvent.click(screen.getByRole("switch", { name: "Thinking" }));
    await settle();
    expect(set({ sounds: { "thinking-cue": true } })).toBe(true);
    fireEvent.click(screen.getByRole("radio", { name: /Glass/ }));
    await settle();
    expect(set({ sounds: { set: "glass" } })).toBe(true);
  });
});

describe("Settings → Notifications (UX-40)", () => {
  it("says it out loud never, and turns Windows notifications off", async () => {
    page("notifications");
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Never" }));
    await settle();
    expect(set({ automation: { speak: "never" } })).toBe(true);
    fireEvent.click(screen.getByRole("switch", { name: "Windows notifications" }));
    await settle();
    expect(set({ automation: { toasts: false } })).toBe(true);
  });
});

describe("Settings → Accessibility", () => {
  it("turns voice-only mode on", async () => {
    page("accessibility");
    await settle();
    fireEvent.click(screen.getByRole("switch", { name: "Voice-only mode" }));
    await settle();
    expect(set({ accessibility: { "voice-only": true } })).toBe(true);
  });
});

describe("Settings → Shortcuts", () => {
  it("shows every shortcut and refuses a chord another one uses", async () => {
    page("shortcuts");
    await settle();
    expect(screen.getByText("Not set")).toBeTruthy();
    // Pause listening: record Ctrl+Space, which is Hold to talk.
    // Only Pause listening is unset, so its button says Set.
    fireEvent.click(screen.getByRole("button", { name: "Set" }));
    fireEvent.keyDown(window, { key: " ", ctrlKey: true });
    await settle();
    expect(screen.getByText("Already used by Hold to talk")).toBeTruthy();
    expect(calls.some((c) => c.method === "settings.set")).toBe(false);
  });
});

describe("Settings → Performance and Diagnostics", () => {
  it("shows what KIVO uses and how fast it answers", async () => {
    page("performance");
    await settle();
    expect(screen.getByText("132 MB")).toBeTruthy();
    expect(screen.getByText("Not used")).toBeTruthy();
    expect(screen.getByText("1.1 s")).toBeTruthy();
    expect(screen.getByText("Warm")).toBeTruthy();
  });

  it("runs the checks and sends a problem to the page that fixes it", async () => {
    const navigate = vi.fn<(p: string) => void>();
    page("diagnostics", navigate);
    await settle();
    expect(screen.getByText("USB Headset")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Fix" }));
    expect(navigate).toHaveBeenCalledWith("permissions");
  });
});

describe("Settings → About", () => {
  it("lists every library's and model's licence and opens the source", async () => {
    page("about");
    await settle();
    expect(screen.getByText(/Version 0\.4\.2/)).toBeTruthy();
    fireEvent.click(screen.getByText("Licences"));
    const dialog = await screen.findByRole("dialog", { name: "Licences" });
    fireEvent.change(within(dialog).getByRole("searchbox", { name: "Search libraries" }), {
      target: { value: "rusqlite" },
    });
    expect(within(dialog).getByText("rusqlite")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("tab", { name: "Models" }));
    await settle();
    expect(within(dialog).getByText("Kokoro")).toBeTruthy();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await settle();
    fireEvent.click(screen.getByText("Source code"));
    expect(calls).toContainEqual({ method: "system.openUrl", params: { url: "https://github.com/MadBlast0/K.I.V.O" } });
  });
});
