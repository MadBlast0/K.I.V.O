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
    sounds: { enabled: true, set: "soft", volume: 70, "thinking-cue": false, off: ["hangup"], custom: ["error"] },
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
        gpu: true,
        gpuPercent: 12,
        gpuMemoryMb: 640,
        vramMb: 6144,
        ramMb: 16384,
        ramFreeMb: 8192,
        npu: false,
        profile: "auto",
        effectiveProfile: "gaming",
        latency: {
          wakeToChime: 142,
          speechToText: 260,
          commandDone: 180,
          firstWord: 1100,
          stages: { stt: 260, brain: 540, tool: 85, tts: 310, total: 2400 },
        },
        models: [{ id: "moonshine-base", name: "Moonshine", kind: "stt", residency: "warm", diskBytes: 199229440 }],
        graphics: {
          options: ["auto", "cuda", "vulkan", "processor"],
          running: { backend: "vulkan", device: "NVIDIA GeForce RTX 3060 Laptop GPU" },
          cuda: { model: "cuda-runtime-13", installed: false, ready: false, driverTooOld: false },
        },
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
    case "diagnostics.bundle":
      return Promise.resolve({ kind: "kivo-diagnostics", kivo: "0.4.2", os: { build: 26200 } });
    case "diagnostics.save":
      return Promise.resolve({ file: "C:/Users/Sam/Downloads/KIVO diagnostics 2026-09-24.json" });
    case "models.list":
      return Promise.resolve([
        { id: "kokoro", name: "Kokoro", license: "Apache-2.0", attribution: "hexgrad", source: "Hugging Face" },
        {
          id: "cuda-runtime-13",
          name: "GPU acceleration for NVIDIA (CUDA)",
          kind: "gpuRuntime",
          license: "NVIDIA CUDA Toolkit EULA (redistributable components)",
          attribution: "cuBLAS is NVIDIA's redistributable CUDA library.",
          source: "https://developer.download.nvidia.com/compute/cuda/redist/",
          size: 423_620_712,
          installed: false,
          downloading: null,
        },
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

describe("Settings → General: product modes (PLAN-06)", () => {
  it("shows the mode in effect and switches through the runtime", async () => {
    config.general = { ...config.general, "product-mode": "gaming" };
    page("general");
    await settle();
    expect(screen.getByText("Stays off the GPU and out of the way over fullscreen games")).toBeTruthy();
    fireEvent.click(screen.getByRole("combobox", { name: "How KIVO behaves right now" }));
    await settle();
    const option = await screen.findByRole("option", { name: "Presentation" });
    // Base UI ignores a release right after opening (a press-drag-release pick).
    await act(async () => {
      await new Promise((r) => setTimeout(r, 450));
    });
    fireEvent.keyDown(option, { key: "Enter" });
    await settle();
    expect(calls).toContainEqual({ method: "mode.set", params: { mode: "presentation" } });
    expect(await screen.findByText("Presentation mode is on")).toBeTruthy();
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
  it("moves, sizes and hides the Island; picks the Orb or the Character", async () => {
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
    fireEvent.click(screen.getByRole("radio", { name: /Character/ }));
    await settle();
    expect(set({ companion: { style: "character" } })).toBe(true);
    fireEvent.click(screen.getByRole("radio", { name: /Orb/ }));
    await settle();
    expect(set({ companion: { style: "orb" } })).toBe(true);
    // What it shows (UX-17) and the wake glow (UX-16).
    fireEvent.click(screen.getByRole("button", { name: "Card" }));
    await settle();
    expect(set({ overlay: { style: "card-only" } })).toBe(true);
    fireEvent.click(screen.getByRole("switch", { name: "Wake glow" }));
    await settle();
    expect(set({ overlay: { "wake-glow": true } })).toBe(true);
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

describe("Settings → Sounds: your own sounds (VOICE-28)", () => {
  it("imports a file for a cue, goes back to the set's, and picks the notification sound", async () => {
    page("sounds");
    await settle();
    // Error already has the user's sound.
    expect(screen.getAllByText("Your sound").length).toBe(1);
    fireEvent.click(screen.getByRole("button", { name: "Use the set’s" }));
    await settle();
    expect(calls).toContainEqual({ method: "sounds.clear", params: { cue: "error" } });
    const input = screen.getByLabelText("Your sound for Done");
    const file = new File([new Uint8Array([82, 73, 70, 70])], "ding.wav", { type: "audio/wav" });
    fireEvent.change(input, { target: { files: [file] } });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 20));
    });
    expect(calls).toContainEqual({
      method: "sounds.import",
      params: { cue: "done", name: "ding.wav", data: "UklGRg==" },
    });
    expect(screen.getByRole("radio", { name: /Custom/ })).toBeTruthy();
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
    expect(screen.getByText("In use · 640 MB")).toBeTruthy();
    expect(screen.getByText("12%")).toBeTruthy();
    expect(screen.getByText("8 GB free of 16 GB")).toBeTruthy();
    // Auto says what it chose (PLAN-08).
    expect(screen.getByText("Auto is using Gaming right now")).toBeTruthy();
    expect(screen.getByText("1.1 s")).toBeTruthy();
    // Each stage on its own (PLAN-07).
    expect(screen.getByText("Brain, to its first word")).toBeTruthy();
    expect(screen.getByText("540 ms")).toBeTruthy();
    expect(screen.getByText("2.4 s")).toBeTruthy();
    expect(screen.getByText("Warm")).toBeTruthy();
    // Sampled again every 2 s while open (DISC-18).
    const before = calls.filter((c) => c.method === "performance.status").length;
    await act(async () => {
      await new Promise((r) => setTimeout(r, 2_100));
    });
    expect(calls.filter((c) => c.method === "performance.status").length).toBeGreaterThan(before);
  });

  it("runs speech recognition on the graphics card first, and it can be turned off", async () => {
    page("performance");
    await settle();
    const gpu = screen.getByRole("switch", { name: "Speech recognition on the graphics card" });
    expect(gpu.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(gpu);
    await settle();
    expect(set({ performance: { "gpu-speech": false } })).toBe(true);
  });

  it("offers only the graphics backends this PC has and says where recognition runs (VOICE-50)", async () => {
    page("performance");
    await settle();
    await settle();
    expect(screen.getByText(/Speech recognition runs on NVIDIA GeForce RTX 3060 Laptop GPU \(Vulkan\)/)).toBeTruthy();
    fireEvent.click(screen.getByRole("combobox", { name: "Graphics backend" }));
    await settle();
    const options = (await screen.findAllByRole("option")).map((o) => o.textContent);
    expect(options).toEqual(["Automatic", "CUDA (NVIDIA)", "Vulkan", "Processor only"]);
    const processor = screen.getByRole("option", { name: "Processor only" });
    await act(async () => {
      await new Promise((r) => setTimeout(r, 450));
    });
    fireEvent.keyDown(processor, { key: "Enter" });
    await settle();
    expect(set({ performance: { "graphics-backend": "processor" } })).toBe(true);
  });

  it("downloads NVIDIA's CUDA libraries only after showing their licence (VOICE-50)", async () => {
    page("performance");
    await settle();
    await settle();
    expect(screen.getByText(/NVIDIA’s CUDA libraries, about 424 MB/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    await settle();
    expect(calls.some((c) => c.method === "models.install")).toBe(false);
    expect(screen.getByText("NVIDIA CUDA Toolkit EULA (redistributable components)")).toBeTruthy();
    const dialog = screen.getByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "Download" }));
    await settle();
    expect(calls).toContainEqual({ method: "models.install", params: { id: "cuda-runtime-13" } });
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

describe("Settings → Diagnostics: the report (ARCH-40)", () => {
  it("shows the whole report before saving it", async () => {
    page("diagnostics");
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Create report" }));
    const dialog = await screen.findByRole("dialog", { name: "Diagnostics report" });
    expect(within(dialog).getByText(/"kind": "kivo-diagnostics"/)).toBeTruthy();
    expect(calls.some((c) => c.method === "diagnostics.save")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Save to Downloads" }));
    await settle();
    expect(calls.some((c) => c.method === "diagnostics.save")).toBe(true);
    expect(await screen.findByText(/Saved to .*KIVO diagnostics 2026-09-24\.json/)).toBeTruthy();
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
