/**
 * Setup's Listen and Speak pages (UX §4, UX-60), with Think (the brain) between them: KIVO hears
 * you, thinks, and answers, one page each. A page is one calm column: a level for this PC
 * (Recommended, High, Medium, Low, from the recommendation's priorities) and one card for the
 * model it picks, which the user takes through three steps with one button that changes as it
 * goes:
 *
 * 1. Download: size and licence up front, nothing until the button; Pause and Cancel while it
 *    downloads, then Resume or Try again.
 * 2. Test (`voice.test`): loaded in a separate worker; a recognizer shows what it heard of
 *    KIVO's test sentence, a voice says it aloud. Nothing is chosen yet.
 * 3. Use this (`voice.switch`, which checks once more and swaps it in safely).
 *
 * Speak then shows the voices as tiles, everyday and Anime. Other models are a link away.
 */
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Method,
  type ModelItem,
  type RecommendationItem,
  type SpeechEngineItem,
  type VoiceItem,
} from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Icon } from "../../icons";
import { cn } from "../../lib/cn";
import { Button, Explain, Meter, Segmented, Spinner, useToast } from "../ui";
import { SpeechChooser } from "../voice/SpeechChooser";
import { working, type Slot, type Speech } from "../voice/useSpeech";

/** The levels and what each asks the recommendation for (`kivo_voice::recommend::Priority`). */
const LEVELS = [
  { id: "recommended", priority: "balanced" },
  { id: "high", priority: "accuracy" },
  { id: "medium", priority: "speed" },
  { id: "low", priority: "resources" },
] as const;
type Level = (typeof LEVELS)[number]["id"];

/** What `voice.test` found. */
interface Tested {
  said: string;
  heard: string;
  passed: boolean;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The model for `slot` is here, in use, and nothing is switching: setup can go on. */
export function slotReady(speech: Speech, slot: Slot): boolean {
  const { choices, progress } = speech;
  if (!choices) return false;
  const id = slot === "stt" ? choices.stt : choices.tts;
  return (choices.engines.find((e) => e.id === id)?.ready ?? false) && !working(progress[slot]);
}

/** Listen → Think → Speak, with where setup is. */
export function Pipeline({ at }: { at: "listen" | "brain" | "speak" }) {
  const { t } = useTranslation();
  const stages = [
    { id: "listen", icon: "mic" },
    { id: "brain", icon: "brain" },
    { id: "speak", icon: "volume" },
  ] as const;
  const index = stages.findIndex((s) => s.id === at);
  return (
    <ol className="k-pipe" aria-label={t("onboarding.pipeline.label")}>
      {stages.map((s, i) => (
        <li
          key={s.id}
          className={cn("k-pipe__stage", i < index && "is-done", i === index && "is-on")}
          aria-current={i === index ? "step" : undefined}
        >
          <Icon name={i < index ? "check" : s.icon} />
          {t(`onboarding.pipeline.${s.id}`)}
        </li>
      ))}
    </ol>
  );
}

/** The recommendation for each level, as the runtime gives it for this PC. */
function usePlans() {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [plans, setPlans] = useState<Partial<Record<Level, RecommendationItem>>>({});
  useEffect(() => {
    if (!connected) return;
    for (const l of LEVELS) {
      void request<RecommendationItem>(Method.voiceRecommend, { priority: l.priority })
        .then((r) => setPlans((all) => ({ ...all, [l.id]: r })))
        .catch(() => {});
    }
  }, [connected, request]);
  return plans;
}

/** One page: the level and the model's card; on Speak, the voices once it's in use. */
export function SlotSetup({ slot, speech }: { slot: Slot; speech: Speech }) {
  const { t } = useTranslation();
  const plans = usePlans();
  const [level, setLevel] = useState<Level>("recommended");
  const [others, setOthers] = useState(false);
  const choices = speech.choices;
  if (!choices) return <Spinner label={t("onboarding.speech.loading")} />;
  const plan = plans[level];
  const id = slot === "stt" ? plan?.sttEngine : plan?.ttsEngine;
  const engine = choices.engines.find((e) => e.id === id);

  return (
    <div className="k-slot">
      <Segmented<Level>
        label={t(`onboarding.speech.${slot}.level`)}
        value={level}
        onChange={setLevel}
        options={LEVELS.map((l) => ({ value: l.id, label: t(`onboarding.speech.level.${l.id}`) }))}
      />
      <p className="k-slot__about">
        {t(`onboarding.speech.${slot}.${level}`)}
        {slot === "stt" && level === "recommended" && plan && (
          <>
            {" "}
            <Explain tip={plan.reason}>{t("onboarding.speech.why")}</Explain>
          </>
        )}
      </p>

      {engine ? (
        // Keyed by engine: another level starts its card afresh.
        <ModelCard key={engine.id} slot={slot} engine={engine} speech={speech} />
      ) : (
        <div className="k-slot__card k-slot__card--empty">
          <Spinner label={t("onboarding.speech.checking")} />
        </div>
      )}

      {slot === "tts" && slotReady(speech, "tts") && <VoiceGrid speech={speech} />}

      <button type="button" className="k-slot__more" onClick={() => setOthers((o) => !o)}>
        {t("onboarding.speech.myself")}
        <Icon name={others ? "up" : "chevronDown"} />
      </button>
      {others && <SpeechChooser slot={slot} speech={speech} />}
    </div>
  );
}

/** The chosen model: what it is, where it is on Download → Test → Use, and the one next action. */
function ModelCard({ slot, engine, speech }: { slot: Slot; engine: SpeechEngineItem; speech: Speech }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [result, setResult] = useState<Tested | null>(null);
  // "Use this" was pressed: the switch's own stages belong to the Use step, not to Test.
  const [using, setUsing] = useState(false);
  const choices = speech.choices;
  if (!choices) return null;

  const model: ModelItem | undefined = engine.model ? speech.models.find((m) => m.id === engine.model) : undefined;
  const progress = speech.progress[slot];
  const stage = progress?.engine === engine.id ? progress.stage : null;
  const failure = stage === "failed" ? (progress?.message ?? null) : null;
  const current = slot === "stt" ? choices.stt : choices.tts;
  const inUse = current === engine.id && engine.ready && !working(progress);

  const percent = model?.downloading ?? 0;
  const downloading = model?.state === "downloading" || model?.state === "installing";
  const paused = model?.state === "paused";
  const downloadFailed = model?.state === "error";
  const testing = !using && (stage === "loading" || stage === "testing");
  const passed = inUse || (result?.passed ?? false);
  const switching = using && !inUse && working(progress);

  // Where it is: 0 download, 1 test, 2 use, 3 done.
  const at = inUse ? 3 : passed ? 2 : engine.ready ? 1 : 0;

  const act = (method: Method, params: unknown) =>
    void request(method, params).catch((e: unknown) => toast(message(e)));
  const test = () => {
    setResult(null);
    setUsing(false);
    request<Tested>(Method.voiceTest, {
      slot,
      engine: engine.id,
      voice: slot === "tts" ? choices.ttsVoice || null : null,
    })
      .then(setResult)
      // A failure shows on the card, from its `failed` stage.
      .catch(() => {});
  };
  const use = () => {
    setUsing(true);
    speech.choose(slot, engine.id).catch((e: unknown) => {
      setUsing(false);
      toast(message(e));
    });
  };
  const size = engine.model
    ? t("onboarding.speech.size", { size: engine.downloadMb, license: engine.license })
    : t("onboarding.speech.builtIn");

  // The line under the steps, and the action that comes next.
  let status: string;
  let tone: "plain" | "good" | "bad" = "plain";
  let action: ReactNode = null;
  if (at === 0) {
    if (downloading) {
      status = t("onboarding.speech.downloading", { percent: percent / 100 });
      action = (
        <>
          <Button variant="plain" onClick={() => act(Method.modelsCancel, { id: engine.model })}>
            {t("onboarding.speech.cancel")}
          </Button>
          <Button onClick={() => act(Method.modelsPause, { id: engine.model })}>{t("onboarding.speech.pause")}</Button>
        </>
      );
    } else {
      status = paused
        ? t("onboarding.speech.paused", { percent: percent / 100 })
        : downloadFailed
          ? (model?.error ?? t("onboarding.speech.downloadFailed"))
          : t(`onboarding.speech.${slot}.downloadHint`);
      tone = downloadFailed ? "bad" : "plain";
      action = (
        <>
          {paused && (
            <Button variant="plain" onClick={() => act(Method.modelsCancel, { id: engine.model })}>
              {t("onboarding.speech.cancel")}
            </Button>
          )}
          <Button variant="primary" onClick={() => act(Method.modelsInstall, { id: engine.model })}>
            {paused
              ? t("onboarding.speech.resume")
              : downloadFailed
                ? t("onboarding.speech.retry")
                : t("onboarding.speech.downloadSize", { size: engine.downloadMb })}
          </Button>
        </>
      );
    }
  } else if (at === 1) {
    if (testing) {
      status = t(`speech.stage.${stage ?? "loading"}`);
    } else if (failure) {
      status = failure;
      tone = "bad";
      action = (
        <Button variant="primary" onClick={test}>
          {t("onboarding.speech.testAgain")}
        </Button>
      );
    } else if (result && !result.passed) {
      status = t("onboarding.speech.heardWrong", { heard: result.heard || "…", said: result.said });
      tone = "bad";
      action = (
        <Button variant="primary" onClick={test}>
          {t("onboarding.speech.testAgain")}
        </Button>
      );
    } else {
      status = t(`onboarding.speech.${slot}.testHint`);
      action = (
        <Button variant="primary" onClick={test}>
          {t("onboarding.speech.test")}
        </Button>
      );
    }
  } else if (at === 2) {
    if (switching) {
      status = t("onboarding.speech.switching");
    } else {
      status = failure
        ? failure
        : slot === "stt"
          ? t("onboarding.speech.heard", { text: result?.heard ?? "" })
          : t("onboarding.speech.spoke");
      tone = failure ? "bad" : "good";
      action = (
        <>
          <Button variant="plain" onClick={test}>
            {t("onboarding.speech.testAgain")}
          </Button>
          <Button variant="primary" onClick={use}>
            {t("onboarding.speech.useThis")}
          </Button>
        </>
      );
    }
  } else {
    status = t(`onboarding.speech.${slot}.ready`, { name: engine.name });
    tone = "good";
  }
  const busy = downloading || testing || switching;

  return (
    <div className={cn("k-slot__card", at === 3 && "is-ready")}>
      <div className="k-slot__model">
        <span className="k-slot__icon" aria-hidden>
          <Icon name={at === 3 ? "check" : slot === "stt" ? "mic" : "volume"} />
        </span>
        <div className="k-slot__name">
          <b>{engine.name}</b>
          <span>
            {size}
            {!engine.commercialUse && ` · ${t("speech.personalUseTitle")}`}
          </span>
        </div>
      </div>

      <ol className="k-slot__steps" aria-label={t("onboarding.speech.steps")}>
        {(["download", "test", "use"] as const).map((s, i) => (
          <li key={s} className={cn("k-slot__step", i < at && "is-done", i === at && "is-on")}>
            <span className="k-slot__dot" aria-hidden>
              {i < at ? <Icon name="check" /> : i + 1}
            </span>
            {t(`onboarding.speech.step.${s}`)}
          </li>
        ))}
      </ol>

      {downloading && <Meter value={percent} label={t("onboarding.speech.step.download")} />}

      <div className="k-slot__foot">
        <p className={cn("k-slot__status", tone === "good" && "is-good", tone === "bad" && "is-bad")} role="status">
          {busy && <Spinner label={status} />}
          {status}
        </p>
        {action && <div className="k-slot__actions">{action}</div>}
      </div>
    </div>
  );
}

/** The voices of the engine KIVO speaks with, as tiles: everyday ones, then Anime. Voices whose
 * files aren't here yet (added after the model was installed) come with one small download. */
function VoiceGrid({ speech }: { speech: Speech }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [playing, setPlaying] = useState<string | null>(null);
  const { choices } = speech;
  const engine = choices?.engines.find((e) => e.id === choices.tts);
  if (!choices || !engine || engine.voices.length === 0) return null;
  const model = speech.models.find((m) => m.id === engine.model);
  const missing = engine.voices.filter((v) => !v.ready);
  const fetching = model?.state === "downloading";
  const selected = choices.ttsVoice || engine.voices[0]?.id;
  const play = (voice: string) => {
    setPlaying(voice);
    speech
      .preview(engine.id, voice)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setPlaying(null));
  };
  const tile = (v: VoiceItem) => {
    const on = v.id === selected;
    return (
      <div key={v.id} className={cn("k-voice", on && "is-on", !v.ready && "is-off")}>
        <button
          type="button"
          className="k-voice__pick"
          disabled={!v.ready}
          aria-pressed={on}
          onClick={() => void speech.pickVoice(v.id).catch((e: unknown) => toast(message(e)))}
        >
          <b>{v.name.replace(/\s*\(.*\)$/, "")}</b>
          <span>{[v.style && t(`speech.style.${v.style}`), accent(v.name)].filter(Boolean).join(" · ")}</span>
        </button>
        <button
          type="button"
          className="k-voice__play"
          aria-label={t("speech.preview", { name: v.name })}
          disabled={!v.ready || playing !== null}
          onClick={() => play(v.id)}
        >
          {playing === v.id ? <Spinner label={t("speech.preview", { name: v.name })} /> : <Icon name="play" />}
        </button>
      </div>
    );
  };
  const everyday = engine.voices.filter((v) => v.character !== "anime");
  const anime = engine.voices.filter((v) => v.character === "anime");
  return (
    <div className="k-voices">
      {missing.length > 0 && model && (
        <div className="k-voices__new">
          <Icon name="download" />
          <span>{t("onboarding.speech.newVoices", { count: missing.length })}</span>
          <Button
            size="sm"
            disabled={fetching}
            onClick={() =>
              void request(Method.modelsInstall, { id: model.id }).catch((e: unknown) => toast(message(e)))
            }
          >
            {fetching ? t("speech.stage.downloading") : t("onboarding.speech.getVoices")}
          </Button>
        </div>
      )}
      <h3 className="k-voices__title">{t("onboarding.speech.chooseVoice")}</h3>
      <div className="k-voices__grid">{everyday.map(tile)}</div>
      {anime.length > 0 && (
        <>
          <h3 className="k-voices__title">
            {t("speech.anime")}
            <span>{t("speech.animeHint")}</span>
          </h3>
          <div className="k-voices__grid">{anime.map(tile)}</div>
        </>
      )}
    </div>
  );
}

/** "Heart (American, female)" → "American". */
function accent(name: string): string {
  const inside = /\(([^,)]*)/.exec(name)?.[1] ?? "";
  return inside;
}
