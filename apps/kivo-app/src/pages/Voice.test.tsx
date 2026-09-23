import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { ModelItem } from "../ipc/generated";

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
  },
];

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => ({
    link: { status: "connected", runtimeVersion: "0.0.0", snapshot: null, message: null },
    request: (method: string, params?: unknown) => {
      calls.push({ method, params });
      if (method === "models.list") return Promise.resolve(models);
      if (method === "settings.get") return Promise.resolve({ voice: { "tts-engine": "system" } });
      return Promise.resolve(null);
    },
  }),
  useRuntimeEvents: () => {},
}));

const { Voice } = await import("./Voice");

async function mount() {
  render(
    <ToastProvider>
      <Voice />
    </ToastProvider>,
  );
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

describe("Voice page (DIST-13)", () => {
  beforeEach(() => {
    calls.length = 0;
  });

  it("lists the models with their licence and size, and offers Remove or Download", async () => {
    await mount();
    expect(screen.getByText("Moonshine Base (English)")).toBeTruthy();
    expect(screen.getByText(/MIT · 141 MB on this PC/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Remove Moonshine Base (English)" })).toBeTruthy();
    expect(screen.getByText(/Apache-2.0 · 101 MB to download/)).toBeTruthy();
  });

  it("shows the licence before anything downloads", async () => {
    await mount();
    fireEvent.click(screen.getByRole("button", { name: "Download" }));
    expect(await screen.findByText("Kokoro-82M by hexgrad, Apache License 2.0.")).toBeTruthy();
    expect(calls.some((c) => c.method === "models.install")).toBe(false);
    const confirm = screen.getAllByRole("button", { name: "Download" }).at(-1)!;
    await act(async () => {
      fireEvent.click(confirm);
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(calls).toContainEqual({ method: "models.install", params: { id: "kokoro-82m" } });
  });
});
