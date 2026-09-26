/**
 * The speech choosers' data (VOICE §11): the engine registry with its profiles and the current
 * choice (`voice.engines`), this PC's recommendation (`voice.recommend`), and the progress of a
 * switch, which the runtime reports as `engineSwitch` events. Everything refreshes on events.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  Method,
  type BenchmarkReply,
  type ModelItem,
  type RecommendationItem,
  type SpeechChoices,
} from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";

export type Slot = "stt" | "tts";

/** Where a switch is: checking → downloading → loading → testing → ready or failed. Setup's
 * "Load and test" (`voice.test`) goes loading → testing → tested or failed. */
const STAGES = ["checking", "downloading", "loading", "testing", "tested", "ready", "failed"] as const;
/** A switch or test that is still working. */
export const working = (p: SwitchProgress | null) =>
  p !== null && p.stage !== "ready" && p.stage !== "failed" && p.stage !== "tested";
type Stage = (typeof STAGES)[number];
const isStage = (s: string): s is Stage => (STAGES as ReadonlyArray<string>).includes(s);

export interface SwitchProgress {
  engine: string;
  stage: Stage;
  message: string | null;
  /** Download progress, while downloading. */
  percent: number | null;
}

export function useSpeech() {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [choices, setChoices] = useState<SpeechChoices | null>(null);
  const [models, setModels] = useState<ModelItem[]>([]);
  const [recommendation, setRecommendation] = useState<RecommendationItem | null>(null);
  const [progress, setProgress] = useState<Record<Slot, SwitchProgress | null>>({ stt: null, tts: null });
  // Each downloading model's speed, bytes per second over the last few seconds (setup's line).
  const [speeds, setSpeeds] = useState<Record<string, number>>({});
  const samples = useRef<Record<string, Array<[number, number]>>>({});

  const load = useCallback(() => {
    if (!connected) return;
    void request<SpeechChoices>(Method.voiceEngines)
      .then(setChoices)
      .catch(() => {});
    void request<ModelItem[]>(Method.modelsList)
      .then(setModels)
      .catch(() => {});
  }, [connected, request]);

  useEffect(load, [load]);
  useEffect(() => {
    if (!connected) return;
    void request<RecommendationItem>(Method.voiceRecommend)
      .then(setRecommendation)
      .catch(() => setRecommendation(null));
  }, [connected, request]);

  useRuntimeEvents((event) => {
    if (event.group !== "system") return;
    const e = event.event;
    if (e.type === "engineSwitch") {
      const slot = e.slot === "stt" ? "stt" : "tts";
      const stage = e.stage;
      if (!isStage(stage)) return;
      setProgress((p) => ({
        ...p,
        [slot]: {
          engine: e.engine,
          stage,
          message: e.message,
          percent: p[slot]?.engine === e.engine ? (p[slot]?.percent ?? null) : null,
        },
      }));
      if (e.stage === "ready" || e.stage === "failed") load();
    } else if (e.type === "modelChanged") {
      // Each model's own progress (setup's download step).
      const size = models.find((m) => m.id === e.id)?.size ?? 0;
      if (e.percent !== null && e.percent < 100 && size > 0) {
        const now = performance.now();
        const bytes = (size * e.percent) / 100;
        const recent = [
          ...(samples.current[e.id] ?? []).filter(([t]) => now - t < 4000),
          [now, bytes] as [number, number],
        ];
        samples.current[e.id] = recent;
        const [t0, b0] = recent[0] ?? [now, bytes];
        if (now - t0 > 800) setSpeeds((all) => ({ ...all, [e.id]: ((bytes - b0) * 1000) / (now - t0) }));
      } else {
        delete samples.current[e.id];
        setSpeeds(({ [e.id]: _gone, ...rest }) => rest);
      }
      // At 100% the files are being checked (and unpacked) before the model is installed.
      setModels((all) =>
        all.map((m) =>
          m.id === e.id && e.percent !== null
            ? { ...m, downloading: e.percent, state: e.percent >= 100 ? "installing" : "downloading" }
            : m,
        ),
      );
      // A download for a switch in progress shows its percentage on the card.
      setProgress((p) => {
        const next = { ...p };
        for (const slot of ["stt", "tts"] as const) {
          const current = p[slot];
          const engine = choices?.engines.find((x) => x.id === current?.engine);
          if (current && engine?.model === e.id && current.stage === "downloading") {
            next[slot] = { ...current, percent: e.percent };
          }
        }
        return next;
      });
      if (e.percent === null) load();
    } else if (e.type === "speechFallback" || e.type === "modelResidency") {
      load();
    }
  });

  /** Starts a safe switch (VOICE-45); progress arrives as events. Rejects with a message when the
   * choice can't work here (language, privacy), which the caller shows. */
  const choose = useCallback(
    async (slot: Slot, engine: string, voice?: string) => {
      setProgress((p) => ({ ...p, [slot]: { engine, stage: "checking", message: null, percent: null } }));
      try {
        await request(Method.voiceSwitch, { slot, engine, voice: voice ?? null });
      } catch (e) {
        setProgress((p) => ({ ...p, [slot]: null }));
        throw e;
      }
    },
    [request],
  );

  /** Picks another voice of the engine KIVO already speaks with (no test needed). */
  const pickVoice = useCallback(
    async (voice: string) => {
      await request(Method.settingsSet, { voice: { "tts-voice": voice } });
      load();
    },
    [request, load],
  );

  const preview = useCallback(
    (engine: string, voice: string | null) => request(Method.voicePreview, { engine, voice }),
    [request],
  );

  const trySample = useCallback(
    (engine: string) => request<{ said: string; heard: string }>(Method.voiceTrySample, { engine }),
    [request],
  );

  /** "Benchmark this engine" (BENCH-15): measured on this PC, then the cards show it. */
  const benchmark = useCallback(
    async (engine: string) => {
      const reply = await request<BenchmarkReply>(Method.voiceBenchmark, { engine });
      load();
      return reply;
    },
    [request, load],
  );

  return {
    connected,
    choices,
    models,
    speeds,
    recommendation,
    progress,
    choose,
    pickVoice,
    preview,
    trySample,
    benchmark,
    reload: load,
  };
}

export type Speech = ReturnType<typeof useSpeech>;
