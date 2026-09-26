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
      voices: [{ id: "zira", name: "Microsoft Zira", style: "", languages: ["en-US"], character: "" }],
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

// "Hey Kivo" as the runtime has it, and push-to-talk (tests change them).
const heyKivo = {
  id: "hey-kivo",
  phrase: "Hey Kivo",
  phonetic: null,
  enabled: false,
  builtIn: true,
  sensitivity: 0.8,
  samples: [],
  quality: "good",
  falseAlarmTest: null,
};
let wake = { words: [heyKivo], modelInstalled: false, listening: false };
let ptt = true;

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
    case "voice.test":
      return Promise.resolve({ said: "What time is it", heard: "What time is it", passed: true });
    case "voice.micCheck":
      return Promise.resolve({ heard: true, peakDb: -12, seconds: 1.4 });
    case "settings.get":
      return Promise.resolve({
        voice: { "push-to-talk": ["Ctrl", "Space"] },
        general: { "keep-running-on-close": true },
        overlay: { position: "top-center" },
        sounds: { enabled: true, set: "soft" },
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
      return Promise.resolve(wake);
    case "voice.devices":
      return Promise.resolve({
        inputs: [{ id: "mic-1", name: "USB Microphone", isDefault: true }],
        outputs: [{ id: "spk-1", name: "Speakers", isDefault: true }],
      });
    case "capabilities.get":
      return Promise.resolve([
        { capability: "push-to-talk", label: "Push-to-talk", enabled: ptt, default: true, badges: [], lastUsed: null },
      ]);
    case "capabilities.set":
      ptt = typeof params === "object" && params !== null && "on" in params && params.on === true;
      return Promise.resolve([]);
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
    wake = { words: [{ ...heyKivo }], modelInstalled: false, listening: false };
    ptt = true;
  });

  it("walks every step, then Finish marks setup done", async () => {
    const onFinish = await mount();
    expect(screen.getByText("Talk to your PC.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await settle();

    // First the microphone and speaker: the system's by default, each tested.
    expect(await screen.findByText("Your microphone and speaker")).toBeTruthy();
    expect(screen.getByText("Voice · 1 of 4")).toBeTruthy();
    expect(screen.getAllByText("System default · USB Microphone").length).toBeGreaterThan(0);
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getByRole("button", { name: "Test" }));
    await settle();
    expect(screen.getByText("Sounds good")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Play a sound" }));
    await settle();
    expect(calls).toContainEqual({ method: "sounds.preview", params: { set: "soft" } });
    await next();

    // Then how KIVO hears and speaks: four presets, the one for this PC in use already.
    expect(await screen.findByText("Choose how KIVO hears and speaks")).toBeTruthy();
    // Listening and Speaking side by side, each with its levels; both already in use here.
    expect(screen.getByRole("heading", { name: "Listening" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Speaking" })).toBeTruthy();
    for (const name of ["Recommended", "High", "Medium", "Low"]) {
      expect(screen.getAllByRole("button", { name }).length).toBe(2);
    }
    expect(await screen.findByText("This is a mid-range PC.")).toBeTruthy();
    expect(await screen.findAllByText("Tested and working")).toHaveLength(2);
    expect(screen.getByText("Microsoft Zira")).toBeTruthy();
    expect(await violations()).toEqual([]);
    await next();

    // Then how to call KIVO, with a first try.
    expect(await screen.findByText("How do you call KIVO?")).toBeTruthy();
    expect(await screen.findByRole("switch", { name: "Say “Hey Kivo”" })).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Push-to-talk" })).toBeTruthy();
    expect(await violations()).toEqual([]);
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

  it("shows how calling KIVO looks, keeps one way on and follows a live try (owner, 2026-09-26)", async () => {
    wake = { words: [{ ...heyKivo, enabled: true }], modelInstalled: true, listening: true };
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
    await next();
    expect(await screen.findByText("How do you call KIVO?")).toBeTruthy();
    // "Hey Kivo" comes before push-to-talk, and both are on.
    const heyKivoSwitch = await screen.findByRole("switch", { name: "Say “Hey Kivo”" });
    const pttSwitch = screen.getByRole("switch", { name: "Push-to-talk" });
    expect(heyKivoSwitch.compareDocumentPosition(pttSwitch) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("At least one stays on, so you can always reach KIVO.")).toBeNull();
    // Push-to-talk off: "Hey Kivo" is now the only way, so it can't be switched off.
    fireEvent.click(pttSwitch);
    await settle();
    expect(calls).toContainEqual({ method: "capabilities.set", params: { capability: "push-to-talk", on: false } });
    expect(screen.getByText("At least one stays on, so you can always reach KIVO.")).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Say “Hey Kivo”" }).hasAttribute("data-disabled")).toBe(true);
    // The try only mentions "Hey Kivo" now.
    expect(screen.getByText(/^Say “Hey Kivo, what time is it\?”/)).toBeTruthy();
    // The Island plays a request, and the try follows KIVO listening.
    expect(document.querySelector(".k-onboarding__demo .k-island")).toBeTruthy();
    expect(screen.getByText("Try it now")).toBeTruthy();
    expect(screen.getByText("Listening… say “What time is it?”")).toBeTruthy();
    runtime.link = { status: "connected", runtimeVersion: "0.0.0", snapshot: { mode: "auto" }, message: null };
  });

  it("walks a model through download, load and test, then use, before Continue", async () => {
    const stt = choices.engines[0];
    stt.ready = false;
    // KIVO listens with something else now, so the recommended one has to be chosen.
    choices.stt = "moonshine-tiny-en";
    try {
      await mount();
      fireEvent.click(screen.getByRole("button", { name: "Get started" }));
      await settle();
      await next();
      expect(await screen.findByText("Choose how KIVO hears and speaks")).toBeTruthy();
      // Not ready yet: Continue waits.
      expect(screen.getByRole("button", { name: "Continue" })).toHaveProperty("disabled", true);
      // Step 1: size and licence up front; nothing downloads until the button.
      expect(await screen.findByText("135 MB · MIT")).toBeTruthy();
      expect(calls.some((c) => c.method === "models.install")).toBe(false);
      expect(screen.queryByRole("button", { name: "Load and test" })).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Download" }));
      await settle();
      expect(calls).toContainEqual({ method: "models.install", params: { id: "moonshine-base-en" } });
      // Downloaded (the runtime reports it; another level re-reads the list here).
      stt.ready = true;
      fireEvent.click(screen.getAllByRole("button", { name: "High" })[0]);
      await settle();
      // Step 2: loaded and tested in a separate worker; what it heard is shown.
      fireEvent.click(screen.getByRole("button", { name: "Load and test" }));
      await settle();
      expect(calls).toContainEqual({
        method: "voice.test",
        params: { slot: "stt", engine: "moonshine-base-en", voice: null },
      });
      expect(screen.getByText("Heard: “What time is it”")).toBeTruthy();
      expect(calls.some((c) => c.method === "voice.switch")).toBe(false);
      // Step 3: only now is it chosen.
      fireEvent.click(screen.getByRole("button", { name: "Use this" }));
      await settle();
      expect(calls).toContainEqual({
        method: "voice.switch",
        params: { slot: "stt", engine: "moonshine-base-en", voice: null },
      });
    } finally {
      stt.ready = true;
      choices.stt = "moonshine-base-en";
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
    // Through the three steps before "Your voice", one at a time.
    await screen.findByText("Your microphone and speaker");
    await next();
    await screen.findByText("Choose how KIVO hears and speaks");
    await next();
    await screen.findByText("How do you call KIVO?");
    await next();
    const start = await screen.findByRole("button", { name: "Start" });
    expect(start).toHaveProperty("disabled", true);
    fireEvent.click(screen.getByRole("checkbox"));
    expect(start).toHaveProperty("disabled", false);
  });
});
