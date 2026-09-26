/**
 * The speech choosers' data (VOICE §11): the engine registry with its profiles and the current
 * choice (`voice.engines`), this PC's recommendation (`voice.recommend`), and the progress of a
 * switch, which the runtime reports as `engineSwitch` events. Everything refreshes on events.
 */
import { useCallback, useEffect, useState } from "react";
import {
  Method,
  type BenchmarkReply,
  type ModelItem,
  type RecommendationItem,
  type SpeechChoices,
} from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";

export type Slot = "stt" | "tts";

/** Where a switch is: checking → downloading → loading → testing → ready or failed. */
const STAGES = ["checking", "downloading", "loading", "testing", "ready", "failed"] as const;
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
