import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { MotionConfig } from "motion/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { RecommendationItem, SpeechChoices } from "../ipc/generated";

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

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.0.0", snapshot: null, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

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
      return Promise.resolve({ voice: { "push-to-talk": ["Ctrl", "Space"] } });
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

async function mount(onFinish = vi.fn<() => void>()) {
  render(
    <MotionConfig reducedMotion="always">
      <ToastProvider>
        <Onboarding onFinish={onFinish} />
      </ToastProvider>
    </MotionConfig>,
  );
  await settle();
  return onFinish;
}

async function next() {
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  await settle();
}

describe("Onboarding (UX-33, UX-60, UX-34)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("walks the voice steps, then Finish marks setup done", async () => {
    const onFinish = await mount();
    expect(screen.getByText("Talk to your PC.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Get started" }));
    await settle();

    expect(await screen.findByText("Check your microphone")).toBeTruthy();
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
    expect(calls.some((c) => c.method === "brains.connect")).toBe(false);
    expect(await violations()).toEqual([]);
    fireEvent.click(screen.getAllByRole("button", { name: "Use" })[1]);
    await settle();
    expect(calls).toContainEqual({
      method: "brains.connect",
      params: { id: "ollama", baseUrl: "http://127.0.0.1:11434/v1" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Finish" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { general: { onboarded: true } } });
    expect(onFinish).toHaveBeenCalledOnce();
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
