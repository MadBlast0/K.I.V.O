import { act, fireEvent, render, screen, within } from "@testing-library/react";
import axe from "axe-core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { ModelItem, RecommendationItem, SpeechChoices, SpeechEngineItem } from "../ipc/generated";

const calls: Array<{ method: string; params: unknown }> = [];
const models: ModelItem[] = [
  {
    id: "moonshine-base-en",
    name: "Moonshine Base (English)",
    kind: "stt",
    license: "MIT",
    attribution: "Moonshine by Useful Sensors, MIT License.",
    source: "https://github.com/moonshine-ai/moonshine",
    languages: ["en"],
    size: 141_300_566,
    installed: true,
    diskBytes: 141_300_566,
    downloading: null,
    residency: null,
    state: "ready",
    inUse: true,
  },
  {
    id: "kokoro-82m",
    name: "Kokoro (English voices)",
    kind: "tts",
    license: "Apache-2.0",
    attribution: "Kokoro-82M by hexgrad, Apache License 2.0.",
    source: "https://huggingface.co/hexgrad/Kokoro-82M",
    languages: ["en"],
    size: 101_000_000,
    installed: false,
    diskBytes: 0,
    downloading: null,
    residency: null,
    state: "notInstalled",
    inUse: false,
  },
];

function engine(e: Partial<SpeechEngineItem> & Pick<SpeechEngineItem, "id" | "name" | "slot">): SpeechEngineItem {
  return {
    profiles: [],
    privacy: "local",
    license: "MIT",
    commercialUse: true,
    languages: ["en"],
    streaming: true,
    devices: ["cpu"],
    downloadMb: 100,
    ramMb: 300,
    model: e.id,
    ready: false,
    fitsLanguage: true,
    voices: [],
    measured: null,
    ...e,
  };
}

const choices: SpeechChoices = {
  language: "en",
  stt: "moonshine-base-en",
  tts: "system",
  ttsVoice: "",
  engines: [
    engine({
      id: "moonshine-base-en",
      name: "Moonshine Base",
      slot: "stt",
      ready: true,
      measured: { realTimeFactor: 0.05, latencyMs: 120, wordErrorRate: 0.042, measuredAt: 1 },
    }),
    engine({ id: "moonshine-tiny-en", name: "Moonshine Tiny", slot: "stt", downloadMb: 42 }),
    engine({ id: "system", name: "Windows voices", slot: "tts", model: null, ready: true, languages: ["*"] }),
    engine({
      id: "kokoro-82m",
      name: "Kokoro",
      slot: "tts",
      license: "Apache-2.0",
      voices: [{ id: "af_heart", name: "Heart (American, female)", style: "female", languages: ["en"] }],
    }),
    engine({ id: "supertonic-3", name: "Supertonic 3", slot: "tts", license: "OpenRAIL-M", languages: ["en", "es"] }),
    engine({ id: "deepgram-flux", name: "Deepgram Flux", slot: "stt", privacy: "cloud", model: null, ready: false }),
    engine({ id: "openai-tts", name: "OpenAI voices", slot: "tts", privacy: "cloud", model: null, ready: true }),
  ],
  profiles: [
    { slot: "stt", profile: "recommended", engine: "moonshine-base-en", otherLanguagesOnly: false },
    { slot: "stt", profile: "lightweight", engine: "moonshine-tiny-en", otherLanguagesOnly: false },
    { slot: "stt", profile: "highAccuracy", engine: null, otherLanguagesOnly: false },
    { slot: "stt", profile: "multilingual", engine: null, otherLanguagesOnly: true },
    { slot: "tts", profile: "natural", engine: "kokoro-82m", otherLanguagesOnly: false },
    { slot: "tts", profile: "lightweight", engine: "system", otherLanguagesOnly: false },
    { slot: "tts", profile: "multilingual", engine: "supertonic-3", otherLanguagesOnly: false },
    { slot: "tts", profile: "expressive", engine: null, otherLanguagesOnly: false },
  ],
};

const recommendation: RecommendationItem = {
  tier: "high",
  sttEngine: "moonshine-base-en",
  sttFallback: "moonshine-tiny-en",
  ttsEngine: "kokoro-82m",
  ttsFallback: "system",
  threads: 4,
  reason: "This is a fast PC.",
};

// Stable like the real provider's, so effects that depend on them don't rerun every render.
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
  if (method === "models.list") return Promise.resolve(models);
  if (method === "voice.engines") return Promise.resolve(choices);
  if (method === "voice.recommend") return Promise.resolve(recommendation);
  if (method === "wake.list") {
    return Promise.resolve({
      words: [
        {
          id: "hey-kivo",
          phrase: "Hey Kivo",
          phonetic: null,
          enabled: true,
          builtIn: true,
          sensitivity: 0.5,
          samples: [],
          quality: "good",
          falseAlarmTest: null,
        },
      ],
      modelInstalled: false,
      listening: false,
    });
  }
  if (method === "voiceId.status") {
    return Promise.resolve({ enrolled: false, prompts: ["Hey Kivo"], recorded: [false], embeddings: 0 });
  }
  if (method === "settings.get") {
    return Promise.resolve({
      voice: { "tts-engine": "system", "follow-up-seconds": 8, "speaker-mode": "off", "tts-speed": 100 },
      general: { language: "en" },
      performance: {},
    });
  }
  if (method === "brains.list") return Promise.resolve({ persona: "calm", customPersona: "" });
  if (method === "voice.vocabulary" || method === "voice.addWord") return Promise.resolve(["Kubernetes"]);
  if (method === "voice.advanced") {
    return Promise.resolve({
      stt: {
        engine: "moonshine-base-en",
        name: "Moonshine Base",
        model: "moonshine-base-en",
        path: "D:/models/moonshine-base-en",
        devices: ["cpu"],
        streaming: true,
        license: "MIT",
      },
      tts: {
        engine: "system",
        name: "Windows voices",
        model: null,
        path: null,
        devices: ["cpu"],
        streaming: true,
        license: "System",
      },
      sttFallback: "moonshine-tiny-en",
      fallbackOn: true,
      threads: 4,
      threadsSetting: 0,
      cores: 16,
      sttWarmMinutes: 10,
      ttsWarmMinutes: 10,
      timing: { endSilenceMs: 800, partialEveryMs: 500 },
      modelsFolder: "D:/models",
    });
  }
  if (method === "voice.benchmark") {
    return Promise.resolve({
      measured: {
        realTimeFactor: 0.08,
        latencyMs: 140,
        wordErrorRate: 0.05,
        noisyWordErrorRate: 0.25,
        cpuPercent: 4.2,
        memoryMb: 310,
        measuredAt: 2,
      },
      budget: { sttMs: 300, ttsMs: 300, realTimeFactor: 1, cancelMs: 100, wordErrorRate: 0.1, noisyWordErrorRate: 0.2 },
    });
  }
  if (method === "voice.devices") {
    return Promise.resolve({ inputs: [{ id: "mic-1", name: "USB Headset", isDefault: true }], outputs: [] });
  }
  return Promise.resolve(null);
};

const { Voice } = await import("./Voice");

async function violations() {
  const result = await axe.run(document.body, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

async function mount() {
  render(
    <ToastProvider>
      <Voice />
    </ToastProvider>,
  );
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
  // The engine choosers open from the summary's Change (UX-61).
  const change = screen.queryAllByRole("button", { name: "Change" })[0];
  if (change) {
    fireEvent.click(change);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 0));
    });
  }
}

const hearing = () => within(screen.getByRole("radiogroup", { name: "How KIVO hears you" }));
const speaking = () => within(screen.getByRole("radiogroup", { name: "How KIVO speaks" }));

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });

describe("Voice page", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("offers curated profiles with honest measurements (VOICE-43)", async () => {
    await mount();
    const base = hearing().getByRole("radio", { name: "Recommended" });
    expect(base.getAttribute("aria-checked")).toBe("true");
    expect(base.textContent).toContain("4.2% word errors");
    const tiny = hearing().getByRole("radio", { name: "Lightweight" });
    expect(tiny.textContent).toContain("Not benchmarked by KIVO");
    expect(screen.getAllByText("Not available yet").length).toBeGreaterThan(0);
    expect(screen.getByText("Recommended for your PC")).toBeTruthy();
  });

  it("benchmarks an installed engine on this PC against KIVO's budgets (BENCH-15)", async () => {
    await mount();
    const buttons = screen.getAllByRole("button", { name: "Benchmark this engine" });
    // Moonshine Base and the Windows voices are here and local; nothing else is ready.
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]);
    await settle();
    expect(calls).toContainEqual({ method: "voice.benchmark", params: { engine: "moonshine-base-en" } });
    const result = screen.getByText("Measured on this PC").parentElement!;
    expect(result.textContent).toContain("140 ms");
    expect(result.textContent).toContain("KIVO’s budget 300 ms");
    expect(within(result).getAllByText("Meets").length).toBe(3);
    // 25% with noise is over the 20% budget.
    expect(within(result).getByText("Over budget")).toBeTruthy();
  });

  it("shows the licence before a new engine downloads, then switches safely (VOICE-45)", async () => {
    await mount();
    fireEvent.click(speaking().getByRole("radio", { name: "Natural" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Apache-2.0")).toBeTruthy();
    expect(within(dialog).getByText("Kokoro-82M by hexgrad, Apache License 2.0.")).toBeTruthy();
    expect(calls.some((c) => c.method === "voice.switch")).toBe(false);
    fireEvent.click(within(dialog).getByRole("button", { name: "Download and use" }));
    await settle();
    expect(calls).toContainEqual({
      method: "voice.switch",
      params: { slot: "tts", engine: "kokoro-82m", voice: null },
    });
  });

  it("adds a cloud service's key, tested by the runtime, and uses a ready one (VOICE-10/11)", async () => {
    await mount();
    fireEvent.click(screen.getAllByText("Cloud services")[0]);
    fireEvent.click(screen.getByRole("button", { name: "Add key" }));
    const field = screen.getByLabelText("API key for Deepgram Flux");
    expect(field.getAttribute("type")).toBe("password");
    fireEvent.change(field, { target: { value: "dg-secret" } });
    fireEvent.click(screen.getByRole("button", { name: "Test and save" }));
    await settle();
    expect(calls).toContainEqual({
      method: "voice.setKey",
      params: { engine: "deepgram-flux", key: "dg-secret", region: null },
    });
    fireEvent.click(screen.getByRole("button", { name: "Use" }));
    await settle();
    expect(calls).toContainEqual({
      method: "voice.switch",
      params: { slot: "tts", engine: "openai-tts", voice: null },
    });
  });

  it("switches straight away to an engine that is already here", async () => {
    choices.tts = "kokoro-82m";
    await mount();
    // The Windows voices need no download: no licence dialog, straight to the test and switch.
    fireEvent.click(speaking().getByRole("radio", { name: "Lightweight" }));
    await settle();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls).toContainEqual({ method: "voice.switch", params: { slot: "tts", engine: "system", voice: null } });
    choices.tts = "system";
  });

  it("lists wake words, and turning one on downloads the listener first", async () => {
    await mount();
    expect(screen.getByText("“Hey Kivo”")).toBeTruthy();
    fireEvent.click(screen.getByRole("switch", { name: "Listen for “Hey Kivo”" }));
    await settle();
    const methods = calls.map((c) => c.method);
    expect(methods.indexOf("models.install")).toBeLessThan(methods.indexOf("wake.set"));
    expect(calls).toContainEqual({
      method: "capabilities.set",
      params: { capability: "mic-listening", on: true },
    });
  });

  it("has no ARIA or labelling problems (UX-52)", async () => {
    await mount();
    expect(await violations()).toEqual([]);
  });

  it("lists the models with their licence and size (DIST-13)", async () => {
    await mount();
    expect(screen.getByText("Moonshine Base (English)")).toBeTruthy();
    expect(screen.getByText(/MIT · 141 MB on this PC/)).toBeTruthy();
    expect(screen.getByText(/Apache-2.0 · 101 MB to download/)).toBeTruthy();
  });

  it("sets the speed, personality and words, manages models and shows the advanced view (UX-61, VOICE-49)", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Faster" }));
    await settle();
    expect(calls).toContainEqual({ method: "settings.set", params: { voice: { "tts-speed": 125 } } });

    fireEvent.click(screen.getByRole("radio", { name: "Witty" }));
    await settle();
    expect(calls).toContainEqual({ method: "brains.setDefault", params: { persona: "witty", customPersona: "" } });

    fireEvent.change(screen.getByLabelText("Add a word"), { target: { value: "Kubernetes" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await settle();
    expect(calls).toContainEqual({ method: "voice.addWord", params: { word: "Kubernetes" } });
    expect(screen.getByText("Kubernetes")).toBeTruthy();

    // The model in use says so; the other one downloads after its licence.
    expect(screen.getAllByText("In use").length).toBeGreaterThan(0);

    fireEvent.click(screen.getByRole("switch", { name: "Show advanced" }));
    await settle();
    expect(screen.getByText(/Speech uses 4 of 16 processor threads/)).toBeTruthy();
    // Devices by name, not the runtime's ids (VOICE-50).
    expect(screen.getAllByText(/runs on Processor/).length).toBeGreaterThan(0);
    expect(screen.getByText("moonshine-tiny-en")).toBeTruthy();
    expect(await violations()).toEqual([]);
  });
});
