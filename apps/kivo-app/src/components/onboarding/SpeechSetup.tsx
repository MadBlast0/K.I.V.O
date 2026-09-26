/**
 * Setup's Listen and Speak pages (UX §4, UX-60), with Think (the brain) between them: KIVO hears
 * you, thinks, and answers, one page each. A page is one calm column: a level for this PC
 * (Recommended, High, Medium, Low, from the recommendation's priorities) and the model it picks
 * as one row with the next thing to do on the right:
 *
 * - Download: size and licence up front, nothing until the button. While it downloads, a ring
 *   (which pauses it) and a line with MB of MB, speed and time left, over the model and the
 *   models it needs together; then "Installing" while the files are checked and unpacked.
 * - Test (`voice.test`): loaded in a separate worker; a recognizer shows what it heard of
 *   KIVO's test sentence, a voice says it aloud. Nothing is chosen yet.
 * - Use this (`voice.switch`, which checks once more and swaps it in safely), then In use.
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
import { Button, Explain, Segmented, Spinner, useToast } from "../ui";
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
        <ModelRow key={engine.id} slot={slot} engine={engine} speech={speech} />
      ) : (
        <div className="k-model k-model--empty">
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

/** How a model's download is going: its own files and the models it needs, together, from the
 * bytes each has written (their `modelChanged` percentages of their sizes), with the speed and
 * time left over the last few seconds, and whether the files are being checked and unpacked. */
function useDownload(speech: Speech, engine: SpeechEngineItem) {
  const main = engine.model ? speech.models.find((m) => m.id === engine.model) : undefined;
  const parts = (main ? [main.id, ...main.requires] : [])
    .map((id) => speech.models.find((m) => m.id === id))
    .filter((m): m is ModelItem => m !== undefined);
  // The models that weren't here when the download started stay in the total, so it doesn't
  // shrink as the small ones finish.
  const [involved, setInvolved] = useState<string[]>([]);
  const missing = parts.filter((m) => m.state !== "ready" && !involved.includes(m.id)).map((m) => m.id);
  if (missing.length > 0) setInvolved([...involved, ...missing]);
  const counted = parts.filter((m) => involved.includes(m.id) || missing.includes(m.id));
  const total = counted.reduce((sum, m) => sum + m.size, 0);
  const done = counted.reduce(
    (sum, m) =>
      sum +
      (m.state === "ready" || m.state === "installing" || m.state === "updateAvailable"
        ? m.size
        : m.state === "downloading" || m.state === "paused"
          ? (m.size * (m.downloading ?? 0)) / 100
          : 0),
    0,
  );
  const active = counted.some((m) => m.state === "downloading" || m.state === "installing");
  const installing = active && counted.every((m) => m.state !== "downloading");
  const speeds = counted.map((m) => speech.speeds[m.id]).filter((v): v is number => v !== undefined);
  const speed = speeds.length > 0 ? speeds.reduce((a, b) => a + b, 0) : null;
  const left = speed && speed > 0 ? (total - done) / speed : null;
  return {
    active,
    installing,
    paused: main?.state === "paused",
    failed: main?.state === "error" ? (main.error ?? "") : null,
    done,
    total,
    percent: total > 0 ? Math.min(100, Math.round((done * 100) / total)) : 0,
    speed,
    left,
  };
}

/** A ring that fills as a download goes; a spinning arc while its files are checked. */
function Ring({ percent, busy = false }: { percent: number; busy?: boolean }) {
  const r = 11;
  const c = 2 * Math.PI * r;
  return (
    <svg className={cn("k-ring", busy && "is-busy")} viewBox="0 0 28 28" aria-hidden>
      <circle className="k-ring__track" cx="14" cy="14" r={r} />
      <circle
        className="k-ring__fill"
        cx="14"
        cy="14"
        r={r}
        strokeDasharray={c}
        strokeDashoffset={busy ? c * 0.72 : c * (1 - percent / 100)}
      />
    </svg>
  );
}

/** The chosen model as one row, App Store–style: what it is, and on the right the one thing to do
 * next (Download → a ring while it downloads, which pauses it → Test → Use this → In use); the
 * line under it says how it's going. */
function ModelRow({ slot, engine, speech }: { slot: Slot; engine: SpeechEngineItem; speech: Speech }) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [result, setResult] = useState<Tested | null>(null);
  // "Use this" was pressed: the switch's own stages (it checks once more) aren't a new test.
  const [using, setUsing] = useState(false);
  const download = useDownload(speech, engine);
  const choices = speech.choices;
  if (!choices) return null;

  const progress = speech.progress[slot];
  const stage = progress?.engine === engine.id ? progress.stage : null;
  const failure = stage === "failed" ? (progress?.message ?? null) : null;
  const current = slot === "stt" ? choices.stt : choices.tts;
  const inUse = current === engine.id && engine.ready && !working(progress);
  const testing = !using && (stage === "loading" || stage === "testing");
  const passed = result?.passed ?? false;
  const switching = using && !inUse && working(progress);

  const mb = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 });
  const rate = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
  const act = (method: Method) => void request(method, { id: engine.model }).catch((e: unknown) => toast(message(e)));
  const test = () => {
    setResult(null);
    setUsing(false);
    request<Tested>(Method.voiceTest, {
      slot,
      engine: engine.id,
      voice: slot === "tts" ? choices.ttsVoice || null : null,
    })
      .then(setResult)
      // A failure shows under the row, from its `failed` stage.
      .catch(() => {});
  };
  const use = () => {
    setUsing(true);
    speech.choose(slot, engine.id).catch((e: unknown) => {
      setUsing(false);
      toast(message(e));
    });
  };

  // The action on the right, and the line under the row.
  let action: ReactNode;
  let line: ReactNode = null;
  let tone: "plain" | "good" | "bad" = "plain";
  if (inUse) {
    action = (
      <span className="k-model__done">
        <Icon name="check" />
        {t("onboarding.speech.inUse")}
      </span>
    );
    line = t(`onboarding.speech.${slot}.ready`, { name: engine.name });
    tone = "good";
  } else if (!engine.ready) {
    if (download.active) {
      action = download.installing ? (
        <span className="k-model__ring" role="img" aria-label={t("onboarding.speech.installing")}>
          <Ring percent={100} busy />
        </span>
      ) : (
        <button
          type="button"
          className="k-model__ring"
          aria-label={t("onboarding.speech.pauseAt", { percent: download.percent / 100 })}
          onClick={() => act(Method.modelsPause)}
        >
          <Ring percent={download.percent} />
          <Icon name="pause" />
        </button>
      );
      line = download.installing ? (
        t("onboarding.speech.installing")
      ) : (
        <>
          {t("onboarding.speech.bytes", {
            done: mb.format(download.done / 1e6),
            total: mb.format(download.total / 1e6),
          })}
          {download.speed !== null &&
            ` · ${t("onboarding.speech.speed", { speed: rate.format(download.speed / 1e6) })}`}
          {download.left !== null && ` · ${leftText(t, download.left)}`}
          <button type="button" className="k-model__link" onClick={() => act(Method.modelsCancel)}>
            {t("onboarding.speech.cancel")}
          </button>
        </>
      );
    } else {
      action = (
        <Button variant="primary" onClick={() => act(Method.modelsInstall)}>
          {download.paused
            ? t("onboarding.speech.resume")
            : download.failed !== null
              ? t("onboarding.speech.retry")
              : t("onboarding.speech.download")}
        </Button>
      );
      if (download.paused) {
        line = (
          <>
            {t("onboarding.speech.paused", {
              done: mb.format(download.done / 1e6),
              total: mb.format(download.total / 1e6),
            })}
            <button type="button" className="k-model__link" onClick={() => act(Method.modelsCancel)}>
              {t("onboarding.speech.cancel")}
            </button>
          </>
        );
      } else if (download.failed !== null) {
        line = download.failed || t("onboarding.speech.downloadFailed");
        tone = "bad";
      } else {
        line = t("onboarding.speech.downloadHint");
      }
    }
  } else if (testing || switching) {
    action = <Spinner label={t(`speech.stage.${stage ?? "loading"}`)} />;
    line = switching ? t("onboarding.speech.switching") : t(`speech.stage.${stage ?? "loading"}`);
  } else if (passed) {
    action = (
      <Button variant="primary" onClick={use}>
        {t("onboarding.speech.useThis")}
      </Button>
    );
    line = (
      <>
        {failure ??
          (slot === "stt" ? t("onboarding.speech.heard", { text: result?.heard ?? "" }) : t("onboarding.speech.spoke"))}
        <button type="button" className="k-model__link" onClick={test}>
          {t("onboarding.speech.testAgain")}
        </button>
      </>
    );
    tone = failure ? "bad" : "good";
  } else {
    action = (
      <Button variant="primary" onClick={test}>
        {result || failure ? t("onboarding.speech.testAgain") : t("onboarding.speech.test")}
      </Button>
    );
    if (failure) {
      line = failure;
      tone = "bad";
    } else if (result) {
      line = t("onboarding.speech.heardWrong", { heard: result.heard || "…", said: result.said });
      tone = "bad";
    } else {
      line = t(`onboarding.speech.${slot}.testHint`);
    }
  }

  return (
    <div className="k-model">
      <div className="k-model__row">
        <span className={cn("k-model__icon", inUse && "is-ready")} aria-hidden>
          <Icon name={slot === "stt" ? "mic" : "volume"} />
        </span>
        <div className="k-model__name">
          <b>{engine.name}</b>
          <span>
            {engine.model
              ? t("onboarding.speech.size", { size: engine.downloadMb, license: engine.license })
              : t("onboarding.speech.builtIn")}
            {!engine.commercialUse && ` · ${t("speech.personalUseTitle")}`}
          </span>
        </div>
        <div className="k-model__action">{action}</div>
      </div>
      <p className={cn("k-model__line", tone === "good" && "is-good", tone === "bad" && "is-bad")} role="status">
        {line}
      </p>
    </div>
  );
}

/** "About 20 seconds left", "About 3 minutes left". */
function leftText(t: (key: string, options?: Record<string, unknown>) => string, seconds: number): string {
  return seconds < 60
    ? t("onboarding.speech.secondsLeft", { count: Math.max(1, Math.round(seconds)) })
    : t("onboarding.speech.minutesLeft", { count: Math.round(seconds / 60) });
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
