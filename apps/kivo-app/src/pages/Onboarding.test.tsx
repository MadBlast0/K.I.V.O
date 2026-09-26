import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { MotionConfig } from "motion/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { ConnectorView, RecommendationItem, SetupAdvice, SpeechChoices } from "../ipc/generated";
import { ThemeProvider } from "../lib/theme";

const calls: Array<{ method: string; params: unknown }> = [];

const choices: SpeechChoices = {
  language: "en",
  stt: "moonshine-base-en",
  tts: "system",
  ttsVoice: "",
  engines: [
    {
      id: "moonshine-base-en",
      name: "Moonshine Base",
      slot: "stt",
      profiles: ["recommended"],
      privacy: "local",
      license: "MIT",
      commercialUse: true,
      languages: ["en"],
      streaming: true,
      devices: ["cpu"],
      downloadMb: 135,
      ramMb: 350,
      model: "moonshine-base-en",
      ready: true,
      fitsLanguage: true,
      voices: [],
      measured: null,
    },
    {
      id: "system",
      name: "Windows voices",
      slot: "tts",
      profiles: ["lightweight"],
      privacy: "local",
      license: "Windows",
      commercialUse: true,
      languages: ["*"],
      streaming: true,
      devices: ["cpu"],
      downloadMb: 0,
      ramMb: 0,
      model: null,
      ready: true,
      fitsLanguage: true,
      voices: [{ id: "zira", name: "Microsoft Zira", style: "", languages: ["en-US"] }],
      measured: null,
    },
  ],
  profiles: [
    { slot: "stt", profile: "recommended", engine: "moonshine-base-en", otherLanguagesOnly: false },
    { slot: "tts", profile: "lightweight", engine: "system", otherLanguagesOnly: false },
  ],
};

const recommendation: RecommendationItem = {
  tier: "mid",
  sttEngine: "moonshine-base-en",
  sttFallback: null,
  ttsEngine: "system",
  ttsFallback: null,
  threads: 4,
  reason: "This is a mid-range PC.",
};

const advice: SetupAdvice = {
  online: true,
  metered: false,
  ramMb: 16384,
  gpu: null,
  onBattery: false,
  mode: "auto",
  performance: "auto",
  privacy: "cloud",
  brain: "gemini-cli",
  brainUrl: null,
  downloadNow: true,
  reasons: ["Gemini CLI is installed and signed in."],
};

const connector = (id: string, name: string, kind: string, state: string): ConnectorView => ({
  id,
  name,
  description: "",
  access: `${name} things`,
  kind,
  state,
  detail: null,
  server: null,
  tools: 0,
  badges: [],
  lastUsed: null,
});

const runtime = vi.hoisted(() => {
  // The runtime's state as tests set it (a session, a turn).
  const snapshot: Record<string, unknown> = { mode: "auto" };
  return {
    link: { status: "connected", runtimeVersion: "0.0.0", snapshot, message: null },
    request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
  };
});

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

const sectionOf = (params: unknown) =>
  typeof params === "object" && params !== null && "section" in params ? String(params.section) : "";

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "voice.engines":
      return Promise.resolve(choices);
    case "voice.recommend":
      return Promise.resolve(recommendation);
    case "models.list":
      return Promise.resolve([]);
    case "voice.micCheck":
      return Promise.resolve({ heard: true, peakDb: -12, seconds: 1.4 });
    case "settings.get":
      return Promise.resolve({
        voice: { "push-to-talk": ["Ctrl", "Space"] },
        general: { "keep-running-on-close": true },
        overlay: { position: "top-center" },
        sounds: { enabled: true },
        performance: { profile: "auto" },
        privacy: { mode: "cloud" },
      });
    case "setup.recommend":
      return Promise.resolve(advice);
    case "connectors.list":
      return Promise.resolve([
        connector("git", "Git", "local", "ready"),
        connector("github", "GitHub", "remote", "available"),
      ]);
    case "browser.status":
      return Promise.resolve({ connected: false, extensionId: "x", folder: "C:\\KIVO\\extension" });
    case "wake.list":
      return Promise.resolve({ words: [], modelInstalled: false, listening: false });
    case "voiceId.status":
      return Promise.resolve({ enrolled: false, prompts: ["Hey Kivo"], recorded: [false], embeddings: 0 });
    case "brains.list":
      return Promise.resolve({ connected: [], profiles: [], defaultProfile: "default" });
    case "brains.refresh":
      return Promise.resolve(
        sectionOf(params) === "cli"
          ? {
              section: "cli",
              checkedAt: 1,
              items: [
                {
                  id: "gemini-cli",
                  new: true,
                  data: { name: "Gemini CLI", signedIn: true, free: "Free with a Google account", program: "gemini" },
                },
              ],
            }
          : {
              section: "local",
              checkedAt: 1,
              items: [
                {
                  id: "ollama",
                  new: true,
                  data: { name: "Ollama", url: "http://127.0.0.1:11434/v1", models: ["llama3.2"] },
                },
              ],
            },
      );
    default:
      return Promise.resolve(null);
  }
};

const { Onboarding } = await import("./Onboarding");

async function violations() {
  const result = await axe.run(document.body, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

async function mount(onFinish = vi.fn<(then?: string) => void>()) {
  render(
    <MotionConfig reducedMotion="always">
      <ThemeProvider>
        <ToastProvider>
          <Onboarding onFinish={onFinish} />
        </ToastProvider>
      </ThemeProvider>
    </MotionConfig>,
  );
  await settle();
  return onFinish;
}

async function next() {
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  await settle();
}

describe("Onboarding (UX-33–36, UX-60)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("walks all eleven steps, then Finish marks setup done", async () => {
    const onFinish = await mount();
    expect(screen.getByText("Talk to your PC.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await settle();

    expect(await screen.findByText("Check your microphone")).toBeTruthy();
    expect(screen.getByText("Voice · 1 of 5")).toBeTruthy();
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await settle();
    expect(screen.getByText("Sounds good")).toBeTruthy();
    await next();

    expect(await screen.findByText("How do you call KIVO?")).toBeTruthy();
    await next();

    expect(await screen.findByText("How KIVO hears and speaks")).toBeTruthy();
    expect(await violations()).toEqual([]);
    expect(screen.getByText("Recommended for your PC")).toBeTruthy();
    expect(screen.getByText("This is a mid-range PC.")).toBeTruthy();
    expect(screen.getByText("Microsoft Zira")).toBeTruthy();
    await next();

    expect(await screen.findByText("Your setup")).toBeTruthy();
    expect(screen.getByText(/Moonshine Base · Local/)).toBeTruthy();
    expect(screen.getByText(/up to 4 cores/)).toBeTruthy();
    await next();

    expect(await screen.findByText("Teach KIVO your voice")).toBeTruthy();
    await next();

    // Step 6 (UX-34): what's on this PC, free options marked; nothing is connected unasked.
    expect(await screen.findByText("Connect a brain")).toBeTruthy();
    expect(await screen.findByText("Gemini CLI")).toBeTruthy();
    expect(screen.getByText("Ollama")).toBeTruthy();
    expect(screen.getAllByText("Free").length).toBeGreaterThanOrEqual(2);
    // The one setup recommends for this PC (UX-36), with why.
    expect(screen.getByText("Recommended")).toBeTruthy();
    expect(screen.getByText("Gemini CLI is installed and signed in.")).toBeTruthy();
    expect(calls.some((c) => c.method === "brains.connect")).toBe(false);
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getAllByRole("button", { name: "Use" })[1]);
    await settle();
    expect(calls).toContainEqual({
      method: "brains.connect",
      params: { id: "ollama", baseUrl: "http://127.0.0.1:11434/v1" },
    });
    // The recommended profile and privacy were preselected once.
    expect(calls).toContainEqual({
      method: "settings.set",
      params: { performance: { profile: "auto" }, privacy: { mode: "cloud" } },
    });
    await next();

    // Step 7: apps ready now, ones a sign-in away, and the advanced ones for after setup.
    expect(await screen.findByText("Connect your apps and tools")).toBeTruthy();
    expect(await screen.findByText("Git")).toBeTruthy();
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    await settle();
    expect(calls).toContainEqual({ method: "connectors.connect", params: { id: "github" } });
    fireEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(screen.getByText(/C:\\KIVO\\extension/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(screen.getByText("Opens after setup")).toBeTruthy();
    await next();

    // Step 8: the permission mode, Auto recommended and chosen; no Bypass.
    expect(await screen.findByText("How much can KIVO do on its own?")).toBeTruthy();
    expect(screen.getByText("Control · 1 of 3")).toBeTruthy();
    expect(screen.queryByText(/Bypass permissions$/)).toBeNull();
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("radio", { name: "Plan first" }));
    await settle();
    expect(calls).toContainEqual({ method: "permissions.setMode", params: { mode: "plan" } });
    await next();

    // Step 9: look & feel on a live Island.
    expect(await screen.findByText("Make it yours")).toBeTruthy();
    expect(screen.getByRole("img", { name: "The Island on your desktop" })).toBeTruthy();
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Bottom" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { overlay: { position: "bottom-center" } } });
    fireEvent.click(screen.getByRole("switch", { name: "Chimes" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { sounds: { enabled: false } } });
    await next();

    // Step 10: startup, and what suits this PC.
    expect(await screen.findByText("Keep KIVO ready")).toBeTruthy();
    expect(screen.getByText("Recommended for this PC")).toBeTruthy();
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("switch", { name: "Open KIVO when Windows starts" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { general: { "start-with-windows": true } } });
    await next();

    // Step 11: Try it plays the demo on the Island.
    expect(await screen.findByText("You’re all set")).toBeTruthy();
    expect(screen.getAllByText("Ready").length).toBe(2);
    fireEvent.click(screen.getByRole("button", { name: "Show me" }));
    await settle();
    // The answer comes after listening and thinking, as in a real request.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 2500));
    });
    expect(screen.getByText(/^It’s /)).toBeTruthy();
    expect(screen.getByText("What time is it?")).toBeTruthy();
    expect(await violations()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Finish" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { general: { onboarded: true } } });
    // The MCP server the user wanted to add opens next.
    expect(onFinish).toHaveBeenCalledWith("extensions/mcp");
  }, 20_000);

  it("shows how calling KIVO looks, lists Hey Kivo first and follows a live try (owner, 2026-09-26)", async () => {
    runtime.link = {
      status: "connected",
      runtimeVersion: "0.0.0",
      snapshot: { mode: "auto", session: "listening" },
      message: null,
    };
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await settle();
    await next();
    expect(await screen.findByText("How do you call KIVO?")).toBeTruthy();
    // "Hey Kivo" comes before "Hold to talk".
    const wake = screen.getByText("“Hey Kivo”");
    const hold = screen.getByText("Hold to talk");
    expect(wake.compareDocumentPosition(hold) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // The Island plays a request, and the try follows KIVO listening.
    expect(document.querySelector(".k-onboarding__demo .k-island")).toBeTruthy();
    expect(screen.getByText("Try it now")).toBeTruthy();
    expect(screen.getByText("Listening… say “What time is it?”")).toBeTruthy();
    runtime.link = { status: "connected", runtimeVersion: "0.0.0", snapshot: { mode: "auto" }, message: null };
  });

  it("asks before downloading the recommended models that are missing", async () => {
    const stt = choices.engines[0];
    stt.ready = false;
    try {
      await mount();
      fireEvent.click(screen.getByRole("button", { name: "Get started" }));
      await settle();
      await next();
      await next();
      expect(await screen.findByText("How KIVO hears and speaks")).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Use recommended" }));
      await settle();
      expect(screen.getByText("Download what’s recommended?")).toBeTruthy();
      expect(screen.getByText(/135 MB · MIT/)).toBeTruthy();
      expect(calls.some((c) => c.method === "voice.switch")).toBe(false);
      fireEvent.click(screen.getByRole("button", { name: "Download and use" }));
      await settle();
      expect(calls.some((c) => c.method === "voice.switch")).toBe(true);
    } finally {
      stt.ready = true;
    }
  });

  it("can be skipped from the first screen", async () => {
    const onFinish = await mount();
    fireEvent.click(screen.getByRole("button", { name: "Skip setup" }));
    await settle();
    expect(onFinish).toHaveBeenCalledOnce();
  });

  it("asks for consent before recording a voiceprint", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await settle();
    // Through the four steps before "Your voice", one at a time.
    await screen.findByText("Check your microphone");
    await next();
    await screen.findByText("How do you call KIVO?");
    await next();
    await screen.findByText("How KIVO hears and speaks");
    await next();
    await screen.findByText("Your setup");
    await next();
    const start = await screen.findByRole("button", { name: "Start" });
    expect(start).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(start).toHaveProperty("disabled", false);
  });
});
